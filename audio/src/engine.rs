//! 播放引擎：状态机、命令与事件。
//!
//! ## 线程模型（阶段 0 的收敛，与实现设计文档的差异）
//!
//! 实现设计里控制线程与解码线程是分开的。阶段 0 把两者合成**一个**引擎线程：
//! 循环内先收命令再喂料，命令响应延迟等于一批解码的时间（几毫秒）。
//!
//! 这么做不触碰任何架构不变量——输出回调仍然只消费环形缓冲，实时纪律不变——
//! 而拆分带来的复杂度（两线程之间的 generation 同步、seek 的三方握手）在阶段 0
//! 没有对应收益。真正需要独立解码线程的是**next 曲目预解码与 current 并行**，
//! 那是阶段 1 的 gapless 才会用到，届时再拆。
//!
//! ## 阶段 0 的能力边界
//!
//! 只打通立体声路径：源为立体声则直通，单声道复制成双声道。
//! **多声道走另一条后端**——下混与空间化都交给系统，应用不自己混（见 `mix` 模块），
//! 而平台原生输出后端尚未接入，因此当前遇到多声道会报明确的路由错误。
//! 采样率不受此限——设备给不出源采样率时插入重采样，并在 stats 里如实标记。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::decode::{Decoder, SourceSpec};
use crate::error::{EngineError, ErrorKind, Result, Stage};
use crate::layout::ChannelLayout;
use crate::mix::ChannelAdapt;
use crate::resample::Resampling;
use crate::output::{OutputBackend, OutputConfig, OutputRequest, OutputShared};
use crate::ring::RingProducer;

/// 环形缓冲容量。2 秒足够吸收调度抖动，又不至于让 seek 后的重填等太久。
const RING_SECONDS: f64 = 2.0;
/// 开始播放前的预缓冲量，沿用概念验证的取值。
const PREBUFFER_MS: f64 = 300.0;
/// 喂料上限：填到这个水位就歇一轮，把 CPU 让出去。
const HIGH_WATER_MS: f64 = 1500.0;
/// 引擎循环的空转间隔。命令响应延迟的上界。
const IDLE_TICK: Duration = Duration::from_millis(3);
/// 进度事件推送间隔（约 5 Hz）。界面在事件之间用 rAF 插值，事件只做重锚定。
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);
/// 等待输出回调回执 flush 的上限。超时说明回调没在跑，生产端自行重置。
const FLUSH_TIMEOUT: Duration = Duration::from_millis(120);

/// 播放状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Idle,
    Loading,
    Playing,
    Paused,
    /// 播完且缓冲已排空。与 `Idle` 分开，前端才能区分「没放过」与「放完了」。
    Ended,
    Error,
}

/// 控制命令。经通道投递，调用方不阻塞，结果一律走事件。
#[derive(Debug)]
pub enum PlayerCmd {
    Load { path: PathBuf, autoplay: bool },
    Play,
    Pause,
    Stop,
    Seek(f64),
    SetVolume(f32),
    Shutdown,
}

/// 引擎事件。
#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// 音源已打开，附源规格与实际协商到的输出配置。
    Opened { spec: SourceSpec, output: OutputConfig },
    StateChanged(PlaybackState),
    Progress {
        position_sec: f64,
        duration_sec: Option<f64>,
        /// 环形缓冲水位换算出的已缓冲时长。
        buffered_sec: f64,
    },
    /// 播放到自然结束。
    TrackEnded,
    Error(EngineError),
}

/// 运行期统计。欠载计数对应架构约束验收条件第 5 条。
#[derive(Debug, Clone, Copy, Default)]
pub struct EngineStats {
    pub underruns: u64,
    pub frames_consumed: u64,
    pub output_delay_frames: u64,
    /// 链路里是否发生了重采样。`bit-perfect` 一类的措辞必须有据可依，
    /// 悄悄插了一级转换却仍宣称原样输出是这类播放器最常见的失实描述。
    pub resampled: bool,
}

/// 引擎句柄。命令投递到引擎线程，事件经构造时传入的回调送出。
pub struct Engine {
    cmd_tx: Sender<PlayerCmd>,
    shared: Arc<OutputShared>,
    alive: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Engine {
    /// 起一个引擎线程。`on_event` 会在引擎线程上被调用，**不要在里面做重活或回调进引擎**。
    pub fn spawn<F>(backend: Box<dyn OutputBackend>, on_event: F) -> Self
    where
        F: Fn(EngineEvent) + Send + 'static,
    {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let shared = Arc::new(OutputShared::default());
        let alive = Arc::new(AtomicBool::new(true));

        let worker = {
            let shared = shared.clone();
            let alive = alive.clone();
            std::thread::Builder::new()
                .name("shannon-audio".into())
                .spawn(move || {
                    let mut engine = Worker::new(backend, shared, Box::new(on_event));
                    engine.run(cmd_rx);
                    alive.store(false, Ordering::Relaxed);
                })
                .expect("创建引擎线程失败")
        };

        Self { cmd_tx, shared, alive, worker: Some(worker) }
    }

    /// 投递命令。引擎线程已退出时返回错误。
    pub fn send(&self, cmd: PlayerCmd) -> Result<()> {
        self.cmd_tx.send(cmd).map_err(|_| {
            EngineError::new(Stage::Output, ErrorKind::Stream, "引擎线程已停止")
        })
    }

    pub fn load(&self, path: impl Into<PathBuf>, autoplay: bool) -> Result<()> {
        self.send(PlayerCmd::Load { path: path.into(), autoplay })
    }

    pub fn play(&self) -> Result<()> {
        self.send(PlayerCmd::Play)
    }

    pub fn pause(&self) -> Result<()> {
        self.send(PlayerCmd::Pause)
    }

    pub fn stop(&self) -> Result<()> {
        self.send(PlayerCmd::Stop)
    }

    pub fn seek(&self, seconds: f64) -> Result<()> {
        self.send(PlayerCmd::Seek(seconds))
    }

    pub fn set_volume(&self, volume: f32) -> Result<()> {
        self.send(PlayerCmd::SetVolume(volume))
    }

    pub fn is_running(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    pub fn stats(&self) -> EngineStats {
        EngineStats {
            underruns: self.shared.underruns(),
            frames_consumed: self.shared.frames_consumed(),
            output_delay_frames: self.shared.output_delay_frames(),
            resampled: self.shared.is_resampled(),
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(PlayerCmd::Shutdown);
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }
}

/// 当前装载的音源及其配套资源。
struct Loaded {
    decoder: Decoder,
    producer: RingProducer,
    resampler: Resampling,
    adapt: ChannelAdapt,
    /// 源声道数。重采样在声道适配**之前**做，中间缓冲仍是源声道布局。
    src_channels: usize,
    out_channels: usize,
    /// **输出**采样率。环形缓冲、位置计数与进度换算一律用它——
    /// 重采样之后链路里流动的就是输出域的帧，混用源采样率会让进度按比率走偏。
    out_rate: u32,
    /// 重采样后、声道适配前的中间缓冲。
    resampled: Vec<f32>,
    /// 已适配但还没写进环形缓冲的样本（缓冲满时的剩余）。
    pending: Vec<f32>,
    pending_pos: usize,
    /// 解码是否已到流末尾。
    eof: bool,
    /// 重采样器的尾部延迟是否已冲刷。只能冲一次。
    flushed: bool,
    /// `TrackEnded` 是否已发出，避免排空后反复上报。
    ended_reported: bool,
}

struct Worker {
    backend: Box<dyn OutputBackend>,
    shared: Arc<OutputShared>,
    on_event: Box<dyn Fn(EngineEvent) + Send>,
    state: PlaybackState,
    loaded: Option<Loaded>,
    /// 音量与暂停都记在 shared 里，这里只留下用户设定的音量，
    /// 以便换曲后仍按同一音量播放。
    volume: f32,
    last_progress: Instant,
    decode_scratch: Vec<f32>,
}

impl Worker {
    fn new(
        backend: Box<dyn OutputBackend>,
        shared: Arc<OutputShared>,
        on_event: Box<dyn Fn(EngineEvent) + Send>,
    ) -> Self {
        Self {
            backend,
            shared,
            on_event,
            state: PlaybackState::Idle,
            loaded: None,
            volume: 1.0,
            last_progress: Instant::now(),
            decode_scratch: Vec::new(),
        }
    }

    fn run(&mut self, rx: Receiver<PlayerCmd>) {
        loop {
            // 一轮里把积压的命令收干净，避免连续拖动进度条时逐条滞后。
            loop {
                match rx.try_recv() {
                    Ok(PlayerCmd::Shutdown) => {
                        self.teardown();
                        return;
                    }
                    Ok(cmd) => self.handle(cmd),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.teardown();
                        return;
                    }
                }
            }

            let fed = self.pump();
            self.check_ended();
            self.emit_progress();

            // 喂满了或没在播就让出 CPU；刚喂过料说明还欠着数据，立刻进下一轮。
            if !fed {
                std::thread::sleep(IDLE_TICK);
            }
        }
    }

    fn teardown(&mut self) {
        self.backend.close();
        self.loaded = None;
    }

    fn emit(&self, event: EngineEvent) {
        (self.on_event)(event);
    }

    fn set_state(&mut self, state: PlaybackState) {
        if self.state != state {
            self.state = state;
            self.emit(EngineEvent::StateChanged(state));
        }
    }

    fn handle(&mut self, cmd: PlayerCmd) {
        match cmd {
            PlayerCmd::Load { path, autoplay } => {
                self.set_state(PlaybackState::Loading);
                if let Err(err) = self.load(&path, autoplay) {
                    self.loaded = None;
                    self.backend.close();
                    self.set_state(PlaybackState::Error);
                    self.emit(EngineEvent::Error(err));
                }
            }
            PlayerCmd::Play => {
                if self.loaded.is_some() {
                    self.shared.set_paused(false);
                    self.set_state(PlaybackState::Playing);
                }
            }
            PlayerCmd::Pause => {
                if self.loaded.is_some() {
                    self.shared.set_paused(true);
                    self.set_state(PlaybackState::Paused);
                }
            }
            PlayerCmd::Stop => {
                self.teardown();
                self.shared.reset_position(0);
                self.set_state(PlaybackState::Idle);
            }
            PlayerCmd::Seek(sec) => {
                if let Err(err) = self.seek(sec) {
                    self.set_state(PlaybackState::Error);
                    self.emit(EngineEvent::Error(err));
                }
            }
            PlayerCmd::SetVolume(v) => {
                self.volume = v.clamp(0.0, 1.0);
                self.shared.set_gain(self.volume);
            }
            PlayerCmd::Shutdown => unreachable!("在 run 里已处理"),
        }
    }

    fn load(&mut self, path: &Path, autoplay: bool) -> Result<()> {
        // 换曲先拆旧流：设备配置可能不同（采样率、声道数），沿用旧流会放出错误的音高。
        self.backend.close();
        self.loaded = None;

        let decoder = Decoder::open(path)?;
        let spec = decoder.spec().clone();

        // 阶段 0 的目标布局恒为立体声。多声道要等阶段 1 的下混，
        // `ChannelAdapt::plan` 会在做不到时给出带布局描述的能力错误。
        let out_layout = ChannelLayout::STEREO;
        let adapt = ChannelAdapt::plan(spec.layout, out_layout)?;

        let out_channels = out_layout.count() as usize;

        // 先协商，再按**协商结果**建缓冲与重采样器：设备给不出源采样率时，
        // 输出域的采样率才是链路后半段的基准。
        let request = OutputRequest { sample_rate: spec.sample_rate, layout: out_layout };
        let probe = self.backend.negotiate(&request)?;
        let out_rate = probe.sample_rate;

        let resampler = Resampling::new(spec.sample_rate, out_rate, spec.layout.count() as usize)?;
        let capacity_frames = (out_rate as f64 * RING_SECONDS) as usize;
        let (producer, consumer) = crate::ring::ring(capacity_frames, out_channels);

        let output = self.backend.open(&request, consumer, self.shared.clone())?;
        if output.sample_rate != out_rate {
            // 协商预演与实际打开给出不同结果说明后端实现自相矛盾，
            // 继续下去链路里的采样率就对不上了——宁可明确报错。
            self.backend.close();
            return Err(EngineError::new(
                Stage::Output,
                ErrorKind::DeviceConfig,
                format!("设备协商结果不一致：预演 {out_rate} Hz，实际 {} Hz", output.sample_rate),
            ));
        }
        self.shared.set_resampled(resampler.is_active());

        self.shared.reset_position(0);
        self.shared.set_gain(self.volume);
        // 先按暂停打开：预缓冲没到阈值就发声会立刻欠载，开头听起来像卡了一下。
        self.shared.set_paused(true);

        self.loaded = Some(Loaded {
            src_channels: spec.layout.count() as usize,
            decoder,
            producer,
            resampler,
            adapt,
            out_channels,
            out_rate,
            resampled: Vec::new(),
            pending: Vec::new(),
            pending_pos: 0,
            eof: false,
            flushed: false,
            ended_reported: false,
        });

        self.prebuffer();
        self.emit(EngineEvent::Opened { spec, output });

        if autoplay {
            self.shared.set_paused(false);
            self.set_state(PlaybackState::Playing);
        } else {
            self.set_state(PlaybackState::Paused);
        }
        Ok(())
    }

    /// 填到预缓冲阈值。填不满（短文件）也返回，由 `check_ended` 处理结束。
    fn prebuffer(&mut self) {
        let Some(loaded) = &self.loaded else { return };
        let target = (loaded.out_rate as f64 * PREBUFFER_MS / 1000.0) as usize;
        for _ in 0..4096 {
            let Some(loaded) = &self.loaded else { return };
            if loaded.producer.queued_frames() >= target || loaded.eof {
                return;
            }
            if !self.pump() {
                return;
            }
        }
    }

    /// 解码并向环形缓冲喂料。返回是否推进了工作（用于决定要不要休眠）。
    ///
    /// 管线：解码 → 重采样（源声道数）→ 声道适配 → 环形缓冲。
    fn pump(&mut self) -> bool {
        let Some(loaded) = self.loaded.as_mut() else { return false };
        if loaded.eof && loaded.flushed && loaded.pending_pos >= loaded.pending.len() {
            return false;
        }

        let high_water = (loaded.out_rate as f64 * HIGH_WATER_MS / 1000.0) as usize;
        if loaded.producer.queued_frames() >= high_water {
            return false;
        }

        // 先把上一轮没写完的残余送进去。
        if loaded.pending_pos < loaded.pending.len() {
            let written = loaded.producer.write(&loaded.pending[loaded.pending_pos..]);
            loaded.pending_pos += written;
            if loaded.pending_pos < loaded.pending.len() {
                // 缓冲满了，下轮继续。
                return written > 0;
            }
        }

        loaded.resampled.clear();
        if loaded.eof {
            // 流已读完但重采样器里还压着尾部延迟，冲出来再收工，
            // 否则结尾会缺几十毫秒——单曲不易察觉，gapless 拼接时正好丢在接缝上。
            if loaded.flushed {
                return false;
            }
            loaded.flushed = true;
            loaded.resampler.flush(&mut loaded.resampled);
        } else {
            self.decode_scratch.clear();
            let more = match loaded.decoder.next_frames(&mut self.decode_scratch) {
                Ok(more) => more,
                Err(err) => {
                    // 解码中途失败：报结构化错误并按流结束处理，由上层决定跳不跳下一首。
                    loaded.eof = true;
                    loaded.flushed = true;
                    self.emit(EngineEvent::Error(err));
                    return false;
                }
            };
            if more {
                loaded.resampler.process(&self.decode_scratch, &mut loaded.resampled);
            } else {
                loaded.eof = true;
                loaded.flushed = true;
                loaded.resampler.flush(&mut loaded.resampled);
            }
        }

        if loaded.resampled.is_empty() {
            // 重采样器还没攒够一整块，本轮没有可送的数据。
            return !loaded.eof;
        }

        let in_frames = loaded.resampled.len() / loaded.src_channels;
        let needed = loaded.adapt.out_samples(in_frames, loaded.out_channels);
        loaded.pending.resize(needed, 0.0);
        loaded.adapt.apply(&loaded.resampled, &mut loaded.pending);
        loaded.pending_pos = loaded.producer.write(&loaded.pending);
        true
    }

    fn seek(&mut self, seconds: f64) -> Result<()> {
        let Some(loaded) = self.loaded.as_mut() else { return Ok(()) };

        let frames = loaded.decoder.seek(seconds)?;

        // 顺序不能反：先丢弃在途 PCM，再重设位置锚点。反过来的话，
        // 旧音频仍会被消费并把位置往前推，界面会看到进度先跳回再乱跳。
        self.shared.set_rebuffering(true);
        let loaded = self.loaded.as_mut().expect("刚确认过已装载");
        loaded.producer.flush(FLUSH_TIMEOUT);
        // 重采样器持有跨块的历史样本，不复位会把定位前的尾巴混进定位后的开头。
        loaded.resampler.reset();
        loaded.resampled.clear();
        loaded.pending.clear();
        loaded.pending_pos = 0;
        loaded.eof = false;
        loaded.flushed = false;
        loaded.ended_reported = false;
        // 位置计数器记的是输出帧，源帧要按比率换算。
        self.shared.reset_position(loaded.resampler.src_frames_to_out(frames));

        self.prebuffer();
        self.shared.set_rebuffering(false);
        if self.state == PlaybackState::Ended {
            self.set_state(PlaybackState::Paused);
        }
        Ok(())
    }

    /// 解码到末尾且缓冲排空即为播完。
    ///
    /// 判据里必须带上「缓冲排空」：解码可以领先播放一秒以上，只看 EOF 会在
    /// 最后一秒还在发声时就报播完。
    fn check_ended(&mut self) {
        let Some(loaded) = self.loaded.as_mut() else { return };
        if !loaded.eof || !loaded.flushed || loaded.ended_reported {
            return;
        }
        if loaded.pending_pos < loaded.pending.len() || loaded.producer.queued_frames() > 0 {
            return;
        }
        loaded.ended_reported = true;
        self.shared.set_paused(true);
        self.set_state(PlaybackState::Ended);
        self.emit(EngineEvent::TrackEnded);
    }

    fn emit_progress(&mut self) {
        if self.last_progress.elapsed() < PROGRESS_INTERVAL {
            return;
        }
        self.last_progress = Instant::now();
        let Some(loaded) = &self.loaded else { return };

        let rate = loaded.out_rate as f64;
        // 位置来自输出回调累计消费的帧数并扣除设备延迟，不用定时器估算。
        let position_sec = self.shared.played_frames() as f64 / rate;
        let buffered_sec = loaded.producer.queued_frames() as f64 / rate;
        let duration_sec = loaded.decoder.spec().duration_sec();
        self.emit(EngineEvent::Progress { position_sec, duration_sec, buffered_sec });
    }
}
