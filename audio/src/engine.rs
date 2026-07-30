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

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::decode::{Decoder, SourceSpec};
use crate::error::{EngineError, ErrorKind, Result, Stage};
use crate::layout::ChannelLayout;
use crate::mix::ChannelAdapt;
use crate::output::{OutputBackend, OutputConfig, OutputRequest, OutputShared};
use crate::resample::Resampling;
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

/// 一次装载的全部参数。
///
/// 这些值**必须与装载原子生效**，所以是一个结构体而不是几条独立命令：多次 IPC 的
/// 完成顺序没有保证，音量晚到会让第一首偶发以满音量炸出来，位置晚到会先漏出一段曲首
/// PCM，响度增益晚到则是前几百毫秒响一截。参数一多，平铺成函数参数也不好读了。
#[derive(Debug, Clone)]
pub struct LoadRequest {
    pub path: PathBuf,
    pub autoplay: bool,
    /// 不透明上下文，引擎不解释，只在这一代的事件上原样回带。
    pub context: LoadContext,
    /// 与装载原子生效的初始音量。诊断工具传 `None`，沿用引擎当前音量；
    /// 前端传当前有效音量，避免首次 open 仍使用默认的 1.0。
    pub initial_volume: Option<f32>,
    /// 可选的初始播放位置。播放会话续播必须在预缓冲与解除暂停之前完成定位，
    /// 不能等 `Opened` 跨 IPC 回到前端后再补发 `Seek`。
    pub initial_position_sec: Option<f64>,
    /// 响度归一化的**整曲常量**增益（线性倍率）。`None` 与 1.0 同义：不处理。
    ///
    /// 与音量相反，它在管线里施加而不是输出回调里——做 gapless 后环形缓冲会同时躺着
    /// 两首歌的 PCM，回调里那个「当前增益」必然在边界处把前一首的尾巴用后一首的
    /// 增益放出去。写进管线时那段 PCM 属于哪首歌是确定的。
    pub loudness_gain: Option<f32>,
    /// 与装载**原子生效**的「下一首」。
    ///
    /// 不让调用方在装载之后再补一条 `SetNext`，理由与上面那几个字段完全相同：
    /// 命令跨 IPC 的到达顺序没有保证，而显式装载会清掉待接续的下一首（它属于上一个
    /// 处境）。晚到的 SetNext 被 teardown 抹掉、早到的被清掉，两种顺序都可能——
    /// 表现出来就是「无缝换曲时好时坏」，最难查的那一类。
    pub next: Option<NextRequest>,
}

impl LoadRequest {
    pub fn new(path: impl Into<PathBuf>, autoplay: bool, context: LoadContext) -> Self {
        Self {
            path: path.into(),
            autoplay,
            context,
            initial_volume: None,
            initial_position_sec: None,
            loudness_gain: None,
            next: None,
        }
    }

    pub fn with_volume(mut self, volume: f32) -> Self {
        self.initial_volume = Some(volume);
        self
    }

    /// 设初始位置。非有限值与非正数一律当作「从头开始」。
    pub fn with_position(mut self, position_sec: Option<f64>) -> Self {
        self.initial_position_sec = position_sec.filter(|sec| sec.is_finite() && *sec > 0.0);
        self
    }

    pub fn with_loudness_gain(mut self, gain: f32) -> Self {
        self.loudness_gain = Some(gain);
        self
    }

    /// 顺带指定无缝接续的下一首。
    pub fn with_next(mut self, next: Option<NextRequest>) -> Self {
        self.next = next;
        self
    }
}

/// 下一首的装载参数。
///
/// 与 [`LoadRequest`] 分开而不是复用：next 没有 `autoplay`（它接在当前这首后面，
/// 传输状态早已确定），没有初始位置（无缝交接必然从头放起），却多一个队列版本号。
/// 硬塞进同一个结构，就得靠注释解释「这几个字段在 next 语境下无意义」。
#[derive(Debug, Clone)]
pub struct NextRequest {
    pub path: PathBuf,
    /// 与 `Load` 同样的不透明上下文。前端为下一首预先生成装载 ID，
    /// 于是越过边界之后的每个事件都能被它自己的那一代认领。
    pub context: LoadContext,
    /// 响度归一化增益，语义同 [`LoadRequest::loudness_gain`]。
    pub loudness_gain: Option<f32>,
    /// 队列版本号。引擎不解释它，只在切歌事件里回带，让前端认出这次交接依据的是哪一版队列。
    pub queue_revision: u32,
}

impl NextRequest {
    pub fn new(path: impl Into<PathBuf>, context: LoadContext, queue_revision: u32) -> Self {
        Self {
            path: path.into(),
            context,
            loudness_gain: None,
            queue_revision,
        }
    }

    pub fn with_loudness_gain(mut self, gain: f32) -> Self {
        self.loudness_gain = Some(gain);
        self
    }
}

/// 控制命令。经通道投递，调用方不阻塞，结果一律走事件。
#[derive(Debug)]
pub enum PlayerCmd {
    Load(LoadRequest),
    /// 指定（或清空）无缝接续的下一首。
    SetNext(Option<NextRequest>),
    Play,
    Pause,
    Stop,
    Seek(f64),
    SetVolume(f32),
    Shutdown,
}

/// 发送端给命令盖的内部代际。公开的 `PlayerCmd` 保持领域语义，调度所需的序号不泄漏给
/// Tauri 外壳；引擎线程据此跳过已经被更新意图取代的重活。
struct QueuedCmd {
    cmd: PlayerCmd,
    load_generation: Option<u64>,
    transport_generation: Option<u64>,
}

/// 调用方此刻真正想要的传输状态。发送端与引擎线程只在更新两个原子可见动作时短暂持锁，
/// 输出回调仍只读 `OutputShared` 的原子量，不进入这把锁。
#[derive(Default)]
struct TransportIntent {
    generation: u64,
    playing: bool,
}

/// 引擎事件。
#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// 音源已打开，附源规格与实际协商到的输出配置。
    Opened {
        spec: SourceSpec,
        output: OutputConfig,
    },
    /// 无缝交接到了下一首。**由消费端越过边界帧时判定**，不是解码器换源时——
    /// 解码可以领先播放一秒半，按解码时机发事件会让界面提前一秒半切换曲目信息。
    ///
    /// 事件盖的是**新曲**的章；`from` 给出刚放完的那首，便于前端对账。
    TrackChanged {
        from: Option<LoadContext>,
        spec: SourceSpec,
        output: OutputConfig,
        queue_revision: u32,
    },
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

/// 一次装载请求的不透明上下文。
///
/// 引擎不解释曲目 ID 与装载 ID，只保证从 `Load` 起产生的每个事件都原样回带。
/// 两者都要保留：`track_id` 让前端关联队列曲目，`load_id` 则能区分同一首曲目的
/// 连续重载，避免上一代迟到事件污染新一代状态。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadContext {
    pub track_id: Option<String>,
    pub load_id: String,
}

impl LoadContext {
    pub fn new(track_id: Option<String>, load_id: impl Into<String>) -> Self {
        Self {
            track_id,
            load_id: load_id.into(),
        }
    }
}

/// 已盖装载章的引擎事件。外壳应直接使用这里的上下文，不能再读取“最新曲目”共享变量。
#[derive(Debug, Clone)]
pub struct StampedEngineEvent {
    pub context: LoadContext,
    pub event: EngineEvent,
}

/// 运行期统计。欠载计数对应架构约束验收条件第 5 条。
#[derive(Debug, Clone, Copy, Default)]
pub struct EngineStats {
    pub underruns: u64,
    /// 跨曲目累计消费的帧数，单调递增；换曲不清零，因此可用前后差值算单曲用量。
    pub frames_consumed: u64,
    /// 当前曲目的播放位置（帧，未扣设备延迟）。
    pub position_frames: u64,
    pub output_delay_frames: u64,
    /// 链路里是否发生了重采样。`bit-perfect` 一类的措辞必须有据可依，
    /// 悄悄插了一级转换却仍宣称原样输出是这类播放器最常见的失实描述。
    pub resampled: bool,
}

/// 引擎句柄。命令投递到引擎线程，事件经构造时传入的回调送出。
pub struct Engine {
    cmd_tx: Sender<QueuedCmd>,
    shared: Arc<OutputShared>,
    alive: Arc<AtomicBool>,
    /// 最近一次 Load 的代际。旧装载即使已经进入耗时的打开/预缓冲，也会在阶段边界
    /// 看见自己过期并静默退出，不把旧曲的 PCM 交给设备。
    latest_load_generation: Arc<AtomicU64>,
    transport: Arc<Mutex<TransportIntent>>,
    load_sequence: AtomicU64,
    worker: Option<JoinHandle<()>>,
}

impl Engine {
    /// 起一个引擎线程。`on_event` 会在引擎线程上被调用，**不要在里面做重活或回调进引擎**。
    pub fn spawn<F>(backend: Box<dyn OutputBackend>, on_event: F) -> Self
    where
        F: Fn(EngineEvent) + Send + 'static,
    {
        Self::spawn_stamped(backend, move |stamped| on_event(stamped.event))
    }

    /// 起一个会回带装载上下文的引擎线程。Tauri 事件桥使用这一入口；
    /// 诊断工具与只关心音频行为的测试可继续使用 [`Engine::spawn`]。
    pub fn spawn_stamped<F>(backend: Box<dyn OutputBackend>, on_event: F) -> Self
    where
        F: Fn(StampedEngineEvent) + Send + 'static,
    {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let shared = Arc::new(OutputShared::default());
        let alive = Arc::new(AtomicBool::new(true));
        let latest_load_generation = Arc::new(AtomicU64::new(0));
        let transport = Arc::new(Mutex::new(TransportIntent::default()));

        let worker = {
            let shared = shared.clone();
            let alive = alive.clone();
            let latest_load_generation = latest_load_generation.clone();
            let transport = transport.clone();
            std::thread::Builder::new()
                .name("shannon-audio".into())
                .spawn(move || {
                    let mut engine = Worker::new(
                        backend,
                        shared,
                        latest_load_generation,
                        transport,
                        Box::new(on_event),
                    );
                    engine.run(cmd_rx);
                    alive.store(false, Ordering::Relaxed);
                })
                .expect("创建引擎线程失败")
        };

        Self {
            cmd_tx,
            shared,
            alive,
            latest_load_generation,
            transport,
            load_sequence: AtomicU64::new(0),
            worker: Some(worker),
        }
    }

    /// 投递命令。引擎线程已退出时返回错误。
    pub fn send(&self, cmd: PlayerCmd) -> Result<()> {
        let load_generation = match &cmd {
            PlayerCmd::Load(_) => {
                Some(self.latest_load_generation.fetch_add(1, Ordering::AcqRel) + 1)
            }
            PlayerCmd::Stop | PlayerCmd::Shutdown => {
                // Stop 的含义是卸载；若控制线程正卡在打开文件或设备协商，立刻让那一代失效。
                self.latest_load_generation.fetch_add(1, Ordering::AcqRel);
                None
            }
            _ => None,
        };
        let transport_generation = match &cmd {
            PlayerCmd::Load(request) => {
                let mut intent = lock_transport(&self.transport);
                intent.generation = intent.generation.wrapping_add(1);
                intent.playing = request.autoplay;
                // 新装载一经提出，旧曲就不该再继续出声。真正拆流仍由引擎线程完成。
                self.shared.set_paused(true);
                Some(intent.generation)
            }
            PlayerCmd::Play => {
                let mut intent = lock_transport(&self.transport);
                intent.generation = intent.generation.wrapping_add(1);
                intent.playing = true;
                Some(intent.generation)
            }
            PlayerCmd::Pause | PlayerCmd::Stop | PlayerCmd::Shutdown => {
                let mut intent = lock_transport(&self.transport);
                intent.generation = intent.generation.wrapping_add(1);
                intent.playing = false;
                // 与意图代际在同一临界区写入：装载完成端不可能在其后把暂停覆盖回播放。
                self.shared.set_paused(true);
                Some(intent.generation)
            }
            PlayerCmd::Seek(_) => {
                let mut intent = lock_transport(&self.transport);
                intent.generation = intent.generation.wrapping_add(1);
                // 定位不改变「定位后要不要继续播」，但要立刻阻止旧位置 PCM 继续外送。
                self.shared.set_paused(true);
                Some(intent.generation)
            }
            // SetNext 不动传输意图，也不动装载代际：它不改变「现在放的是什么」。
            PlayerCmd::SetVolume(_) | PlayerCmd::SetNext(_) => None,
        };
        self.cmd_tx
            .send(QueuedCmd {
                cmd,
                load_generation,
                transport_generation,
            })
            .map_err(|_| EngineError::new(Stage::Output, ErrorKind::Stream, "引擎线程已停止"))
    }

    pub fn load(&self, path: impl Into<PathBuf>, autoplay: bool) -> Result<()> {
        let sequence = self.load_sequence.fetch_add(1, Ordering::Relaxed) + 1;
        self.load_request(LoadRequest::new(
            path,
            autoplay,
            LoadContext::new(None, format!("engine-{sequence}")),
        ))
    }

    /// 按完整的装载请求装载。见 [`LoadRequest`]：那些参数必须与装载一起生效。
    pub fn load_request(&self, request: LoadRequest) -> Result<()> {
        self.send(PlayerCmd::Load(request))
    }

    /// 指定无缝接续的下一首；`None` 表示当前这首放完就停。
    ///
    /// 队列的权威在前端，引擎只认「下一个是谁」。每次队列、循环或随机状态变化都该重发，
    /// 已经预解码但尚未发声的旧 next 会被丢弃（见实现计划「队列归属与切歌交接」）。
    pub fn set_next(&self, request: Option<NextRequest>) -> Result<()> {
        self.send(PlayerCmd::SetNext(request))
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
            frames_consumed: self.shared.total_frames(),
            position_frames: self.shared.position_frames(),
            output_delay_frames: self.shared.output_delay_frames(),
            resampled: self.shared.is_resampled(),
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.send(PlayerCmd::Shutdown);
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }
}

/// 输出流侧的资源：一次 `open` 的产物，**跨曲目复用**。
///
/// 与音源分开正是 gapless 的前提。曲目之间那声停顿的来源就是「换曲 = 拆流重开」：
/// 设备要重新协商、缓冲要重新填、位置时基要重建。分开之后，换曲只是往同一个环形缓冲
/// 里接着写，设备时钟一刻没停。
///
/// 代价要说清楚：整条链路的采样率由**第一首**协商决定，后面的曲目若采样率不同，就得
/// 重采样到它上面。反过来（每首都按自己的采样率重开设备）能省掉这级转换，但那样换曲
/// 必然有停顿——两者不可兼得，而这里的取舍是「无缝优先」，且转换本身会如实记进 stats。
struct Stream {
    producer: RingProducer,
    out_channels: usize,
    /// **输出**采样率。环形缓冲、位置计数与进度换算一律用它——
    /// 重采样之后链路里流动的就是输出域的帧，混用源采样率会让进度按比率走偏。
    out_rate: u32,
    config: OutputConfig,
}

/// 一个音源的解码侧状态：从文件到「可以写进环形缓冲的样本」这一段。
struct Source {
    decoder: Decoder,
    spec: SourceSpec,
    /// 留着路径，是为了 seek 到已经交接出去的曲目时能重新打开它（见 `rewind_head_to_sounding`）。
    path: PathBuf,
    context: LoadContext,
    /// 这一首是依据哪一版队列排上来的。引擎不解释，只在切歌事件里回带。
    queue_revision: u32,
    resampler: Resampling,
    adapt: ChannelAdapt,
    /// 源声道数。重采样在声道适配**之前**做，中间缓冲仍是源声道布局。
    src_channels: usize,
    /// 响度归一化增益（线性倍率）。1.0 表示不处理，此时连乘法都不做。
    loudness_gain: f32,
    /// 重采样后、声道适配前的中间缓冲。
    resampled: Vec<f32>,
    /// 已适配但还没写进环形缓冲的样本（缓冲满时的剩余）。
    pending: Vec<f32>,
    pending_pos: usize,
    /// 解码是否已到流末尾。
    eof: bool,
    /// 重采样器的尾部延迟是否已冲刷。只能冲一次。
    flushed: bool,
}

impl Source {
    /// 打开文件并按**给定的输出采样率**配好重采样与声道适配。
    ///
    /// 采样率是参数而不是现场协商：接续的曲目必须服从当前输出流的采样率，
    /// 它自己那份「理想输出配置」在这条流上没有意义。
    fn open(
        path: &Path,
        context: LoadContext,
        loudness_gain: Option<f32>,
        queue_revision: u32,
        out_rate: u32,
    ) -> Result<Self> {
        let decoder = Decoder::open(path)?;
        let spec = decoder.spec().clone();
        // 目标布局恒为立体声。多声道整体走平台原生后端；
        // `ChannelAdapt::plan` 会在当前路径做不到时给出带布局描述的路由错误。
        let adapt = ChannelAdapt::plan(spec.layout, ChannelLayout::STEREO)?;
        let src_channels = spec.layout.count() as usize;
        let resampler = Resampling::new(spec.sample_rate, out_rate, src_channels)?;
        Ok(Self {
            decoder,
            spec,
            path: path.to_path_buf(),
            context,
            queue_revision,
            resampler,
            adapt,
            src_channels,
            loudness_gain: sanitize_gain(loudness_gain),
            resampled: Vec::new(),
            pending: Vec::new(),
            pending_pos: 0,
            eof: false,
            flushed: false,
        })
    }

    /// 这一路音源再也不会产出样本了（流已读完、重采样尾部已冲、残余已写完）。
    fn drained(&self) -> bool {
        self.eof && self.flushed && self.pending_pos >= self.pending.len()
    }

    fn sounding(&self) -> Sounding {
        Sounding {
            context: self.context.clone(),
            spec: self.spec.clone(),
            path: self.path.clone(),
            loudness_gain: self.loudness_gain,
        }
    }

    /// 把自己还原成一条「待接续」请求。seek 到上一首时，已经接上的这首要退回去重排。
    fn as_next_request(&self) -> NextRequest {
        NextRequest {
            path: self.path.clone(),
            context: self.context.clone(),
            loudness_gain: Some(self.loudness_gain),
            queue_revision: self.queue_revision,
        }
    }

    /// 定位后清掉跨块状态。重采样器持有历史样本，不复位会把定位前的尾巴混进开头。
    fn reset_pipeline(&mut self) {
        self.resampler.reset();
        self.resampled.clear();
        self.pending.clear();
        self.pending_pos = 0;
        self.eof = false;
        self.flushed = false;
    }
}

/// 正在**发声**的曲目。
///
/// 与解码头（`Worker::head`）分开是 gapless 带来的：解码领先播放最多一秒半，那段时间
/// 里两者根本不是同一首。事件盖章、进度里的时长、以及 seek 该定位到哪个文件，全部以
/// 这里为准；按解码头来会让界面提前一秒半切歌，也会让拖动进度条定位到下一首上。
struct Sounding {
    context: LoadContext,
    spec: SourceSpec,
    path: PathBuf,
    loudness_gain: f32,
}

/// 已打点、尚未被消费端越过的边界。与环形缓冲里的打点一一对应、同序。
struct PendingMark {
    boundary: usize,
    /// 越过之后正在发声的是谁。
    sounding: Sounding,
    queue_revision: u32,
    /// 已被新队列作废：打点仍留在缓冲里（回收它会与消费端打架，见 `ring` 模块），
    /// 但不该产生切歌事件——那段音频一个样本都没出去过。
    stale: bool,
}

struct Worker {
    backend: Box<dyn OutputBackend>,
    shared: Arc<OutputShared>,
    latest_load_generation: Arc<AtomicU64>,
    transport: Arc<Mutex<TransportIntent>>,
    on_event: Box<dyn Fn(StampedEngineEvent) + Send>,
    /// 事件盖章用的上下文。装载途中是正在装载的那首，此后与 `sounding` 同一身份。
    context: Option<LoadContext>,
    state: PlaybackState,
    /// 输出流。跨曲目复用，只有显式装载、停止与失败才拆。
    stream: Option<Stream>,
    /// 解码头：正在往环形缓冲里写的那首。可能已经领先发声曲目一整首。
    head: Option<Source>,
    /// 正在发声的曲目。由消费端越过边界时推进。
    sounding: Option<Sounding>,
    /// 前端指定的下一首。收到时只记请求，等缓冲喂满、控制线程本来就要休息时才打开文件。
    next_request: Option<NextRequest>,
    /// 已打开、尚未开始写入缓冲的下一首。
    staged: Option<Source>,
    /// 已打点未越过的边界，与缓冲里的打点同序。
    marks: VecDeque<PendingMark>,
    /// `TrackEnded` 是否已发出，避免排空后反复上报。
    ended_reported: bool,
    /// ring 被最后一次设备回调取空之后，仍需等待「设备延迟 + 本回调有效帧」才能
    /// 确认耳朵真正听完。`None` 表示尚未观察到稳定的排空时刻。
    drain_deadline: Option<Instant>,
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
        latest_load_generation: Arc<AtomicU64>,
        transport: Arc<Mutex<TransportIntent>>,
        on_event: Box<dyn Fn(StampedEngineEvent) + Send>,
    ) -> Self {
        Self {
            backend,
            shared,
            latest_load_generation,
            transport,
            on_event,
            context: None,
            state: PlaybackState::Idle,
            stream: None,
            head: None,
            sounding: None,
            next_request: None,
            staged: None,
            marks: VecDeque::new(),
            ended_reported: false,
            drain_deadline: None,
            volume: 1.0,
            last_progress: Instant::now(),
            decode_scratch: Vec::new(),
        }
    }

    fn run(&mut self, rx: Receiver<QueuedCmd>) {
        loop {
            // 一轮里把积压的命令收干净，避免连续拖动进度条时逐条滞后。
            loop {
                match rx.try_recv() {
                    Ok(queued) if matches!(&queued.cmd, PlayerCmd::Shutdown) => {
                        self.teardown();
                        return;
                    }
                    Ok(queued) => self.handle(queued),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.teardown();
                        return;
                    }
                }
            }

            // 设备断开、被其它应用独占等错误从输出回调旁路投递；控制线程负责
            // 关闭整条链路并发出结构化事件，不能让状态停在一个实际已无声的 Playing。
            if self.poll_backend_error() {
                std::thread::sleep(IDLE_TICK);
                continue;
            }

            let fed = match self.pump() {
                Ok(fed) => fed,
                Err(err) => {
                    self.fail(err);
                    false
                }
            };
            if self.state != PlaybackState::Error {
                // 先结算边界再报进度：同一轮里越过边界的话，进度该按新曲的时长发。
                self.settle_crossings();
                self.check_ended();
                self.emit_progress();
            }

            // 喂满了或没在播就让出 CPU；刚喂过料说明还欠着数据，立刻进下一轮。
            if !fed {
                std::thread::sleep(IDLE_TICK);
            }
        }
    }

    fn teardown(&mut self) {
        // 先让仍可能执行的最后一次回调只写零，并把其空读排除在欠载之外；
        // close 返回后消费端已销毁，随后才能安全丢生产端与共享状态。
        self.shared.set_paused(true);
        self.shared.set_rebuffering(true);
        self.backend.close();
        self.stream = None;
        self.head = None;
        self.staged = None;
        // 待接续的下一首同样作废：显式装载之后「下一个是谁」要由前端按新处境重新指定，
        // 沿用旧的等于让引擎自作主张接上一个前端已经不这么想的曲目。
        self.next_request = None;
        self.marks.clear();
        self.ended_reported = false;
        self.drain_deadline = None;
        self.shared.set_source_drained(false);
        self.shared.set_rebuffering(false);
    }

    fn poll_backend_error(&mut self) -> bool {
        let Some(err) = self.backend.take_error() else {
            return false;
        };
        self.fail(err);
        true
    }

    fn emit(&self, event: EngineEvent) {
        let context = self
            .context
            .clone()
            .expect("引擎事件必须隶属于一次装载请求");
        (self.on_event)(StampedEngineEvent { context, event });
    }

    /// 终止当前装载代际并上报错误。失败不是 EOF：拆掉链路后不会再产生自然结束事件，
    /// 上层也就不会把损坏文件误当成播完并自动跳到下一首。
    fn fail(&mut self, err: EngineError) {
        self.teardown();
        self.set_state(PlaybackState::Error);
        self.emit(EngineEvent::Error(err));
    }

    fn set_state(&mut self, state: PlaybackState) {
        if self.state != state {
            self.state = state;
            self.emit(EngineEvent::StateChanged(state));
        }
    }

    fn handle(&mut self, queued: QueuedCmd) {
        let QueuedCmd {
            cmd,
            load_generation,
            transport_generation,
        } = queued;
        match cmd {
            PlayerCmd::Load(request) => {
                let load_generation = load_generation.expect("Load 必须带装载代际");
                let transport_generation = transport_generation.expect("Load 必须带传输意图代际");
                // 同一轮里已经排了更新的 Load，旧请求连文件都不打开。若更新请求是在
                // 打开途中到达，`load` 还会在各阶段边界再次检查。
                if !self.is_current_load(load_generation) {
                    return;
                }
                self.context = Some(request.context.clone());
                if let Some(volume) = request.initial_volume {
                    self.volume = volume.clamp(0.0, 1.0);
                    self.shared.set_gain(self.volume);
                }
                self.set_state(PlaybackState::Loading);
                match self.load(&request, load_generation) {
                    Ok(true) => self.finish_transport(transport_generation),
                    Ok(false) => self.discard_stale_load(),
                    Err(err) if self.is_current_load(load_generation) => self.fail(err),
                    Err(_) => self.discard_stale_load(),
                }
            }
            PlayerCmd::SetNext(request) => self.set_next(request),
            PlayerCmd::Play => {
                let generation = transport_generation.expect("Play 必须带传输意图代际");
                if self.head.is_some() {
                    if let Some(playing) = self.apply_transport(generation) {
                        self.set_state(if playing {
                            PlaybackState::Playing
                        } else {
                            PlaybackState::Paused
                        });
                    }
                }
            }
            PlayerCmd::Pause => {
                let generation = transport_generation.expect("Pause 必须带传输意图代际");
                if self.head.is_some() && self.apply_transport(generation).is_some() {
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
                    self.fail(err);
                } else if self.head.is_some() {
                    let generation = transport_generation.expect("Seek 必须带传输意图代际");
                    if let Some(playing) = self.apply_transport(generation) {
                        self.set_state(if playing {
                            PlaybackState::Playing
                        } else {
                            PlaybackState::Paused
                        });
                    }
                }
            }
            PlayerCmd::SetVolume(v) => {
                self.volume = v.clamp(0.0, 1.0);
                self.shared.set_gain(self.volume);
            }
            PlayerCmd::Shutdown => unreachable!("在 run 里已处理"),
        }
    }

    fn is_current_load(&self, generation: u64) -> bool {
        self.latest_load_generation.load(Ordering::Acquire) == generation
    }

    /// 长装载结束时才提交 autoplay。发送端与这里共用一把极短的控制锁，因此加载中
    /// 到达的 Pause / Stop / 新 Load 不会被结尾那句 set_paused(false) 反向覆盖。
    fn finish_transport(&mut self, generation: u64) {
        match self.apply_transport(generation) {
            Some(true) => self.set_state(PlaybackState::Playing),
            Some(false) => self.set_state(PlaybackState::Paused),
            None => {
                // 更新意图已经在队列里；发送端对 Pause / Stop / Load / Seek 已同步静音，
                // 当前装载只保持安静，等下一条命令决定最终状态。
                self.shared.set_paused(true);
                self.set_state(PlaybackState::Paused);
            }
        }
    }

    /// 若命令仍是最新传输意图，就把它原子提交到输出共享状态，并返回目标是否为播放。
    fn apply_transport(&self, generation: u64) -> Option<bool> {
        let intent = lock_transport(&self.transport);
        if intent.generation != generation {
            return None;
        }
        self.shared.set_paused(!intent.playing);
        Some(intent.playing)
    }

    fn discard_stale_load(&mut self) {
        self.teardown();
        // 不给已经失效的上下文发事件，但让下一代 Load 能重新发出 Loading。
        self.state = PlaybackState::Idle;
    }

    fn load(&mut self, request: &LoadRequest, generation: u64) -> Result<bool> {
        // 显式装载先拆旧流：设备配置可能不同（采样率、声道数），沿用旧流会放出错误的音高。
        // 无缝交接走的是另一条路（`advance_head`），那里恰恰不拆流。
        self.teardown();
        // teardown 之后再装：它刚把上一个处境的「下一首」清掉了。
        self.next_request = request.next.clone();

        // 先协商，再按**协商结果**建缓冲与重采样器：设备给不出源采样率时，
        // 输出域的采样率才是链路后半段的基准。协商只是预演，不碰设备。
        let out_layout = ChannelLayout::STEREO;
        let out_channels = out_layout.count() as usize;
        let probe_spec = Decoder::open(&request.path)?.spec().clone();
        if !self.is_current_load(generation) {
            return Ok(false);
        }
        let output_request = OutputRequest {
            sample_rate: probe_spec.sample_rate,
            layout: out_layout,
        };
        let probe = self.backend.negotiate(&output_request)?;
        if !self.is_current_load(generation) {
            return Ok(false);
        }
        let out_rate = probe.sample_rate;

        let mut source = Source::open(
            &request.path,
            request.context.clone(),
            request.loudness_gain,
            0,
            out_rate,
        )?;
        let spec = source.spec.clone();
        // 新装载还没有任何在途 PCM，直接在解码器上定位即可；随后按返回的**源域帧**
        // 换算输出域位置。这样 open、预缓冲与 autoplay 从一开始看到的就是同一位置，
        // 不会先放出曲首再由一条迟到的 Seek 把它冲掉。
        let initial_source_frame = match request.initial_position_sec {
            Some(seconds) => source.decoder.seek(seconds)?,
            None => 0,
        };
        if !self.is_current_load(generation) {
            return Ok(false);
        }
        let capacity_frames = (out_rate as f64 * RING_SECONDS) as usize;
        let (producer, consumer) = crate::ring::ring(capacity_frames, out_channels);

        // `open` 会立即启动设备回调，所有共享状态必须在它之前就准备好。
        // 早先顺序相反，回调能抢在 set_paused 前读空 ring，把正常启动误记成欠载。
        self.shared
            .reset_position(source.resampler.src_frames_to_out(initial_source_frame));
        self.shared.set_gain(self.volume);
        self.shared.set_resampled(source.resampler.is_active());
        self.shared.set_source_drained(false);
        self.shared.set_rebuffering(true);
        self.shared.set_paused(true);
        self.shared.reset_callback_timing();

        let output = self
            .backend
            .open(&output_request, consumer, self.shared.clone())?;
        if output.sample_rate != out_rate {
            // 协商预演与实际打开给出不同结果说明后端实现自相矛盾，
            // 继续下去链路里的采样率就对不上了——宁可明确报错。
            self.backend.close();
            return Err(EngineError::new(
                Stage::Output,
                ErrorKind::DeviceConfig,
                format!(
                    "设备协商结果不一致：预演 {out_rate} Hz，实际 {} Hz",
                    output.sample_rate
                ),
            ));
        }
        if !self.is_current_load(generation) {
            self.backend.close();
            return Ok(false);
        }
        self.stream = Some(Stream {
            producer,
            out_channels,
            out_rate,
            config: output.clone(),
        });
        self.sounding = Some(source.sounding());
        self.head = Some(source);

        if !self.prebuffer(Some(generation))? || !self.is_current_load(generation) {
            return Ok(false);
        }
        self.shared.set_rebuffering(false);
        self.emit(EngineEvent::Opened { spec, output });
        Ok(true)
    }

    /// 指定（或清空）无缝接续的下一首。
    ///
    /// 三种处境要分开处理，差别在于那首歌的音频走到了哪一步：
    ///
    /// 1. 还没打开 —— 换掉请求即可；
    /// 2. 已经打开、还没写进缓冲 —— 直接丢掉解码器（在控制线程上释放，不在回调里）；
    /// 3. 已经写进缓冲、但还没发声 —— 必须从缓冲里撤掉，否则改队列对听感无效，
    ///    用户仍会听到旧队列的下一首。撤不掉（已经越过边界）就是既成事实，
    ///    此时新请求自然成为**它**之后的下一首。
    fn set_next(&mut self, request: Option<NextRequest>) {
        self.staged = None;
        self.invalidate_unheard_next();
        self.next_request = request;
    }

    /// 把已经写进缓冲、但还没发声的下一首撤掉。
    fn invalidate_unheard_next(&mut self) {
        let Some(mark) = self.marks.back() else {
            return;
        };
        // 已经撤过一次：缓冲里那段就只剩当前这首的尾巴，没有可撤的了。
        if mark.stale {
            return;
        }
        let boundary = mark.boundary;
        let Some(stream) = self.stream.as_mut() else {
            return;
        };
        if !stream.producer.truncate_after(boundary, FLUSH_TIMEOUT) {
            return;
        }
        if let Some(mark) = self.marks.back_mut() {
            mark.stale = true;
        }
        // 解码头正是被撤掉的那首，连同它在管线里的残余一起丢弃。
        self.head = None;
    }

    /// 打开待接续的下一首。
    ///
    /// **挑缓冲喂满的时候做**：打开文件是几毫秒的阻塞 I/O，而那一刻控制线程本来就要
    /// 休息，缓冲也正处在最厚的位置。打不开就清掉请求退回非 gapless 路径（见
    /// `advance_head`），不在这里报错——上一首还在放，此刻弹「播放失败」只会让人莫名其妙。
    fn stage_next(&mut self) {
        if self.staged.is_some() {
            return;
        }
        let (Some(request), Some(stream)) = (self.next_request.as_ref(), self.stream.as_ref())
        else {
            return;
        };
        match Source::open(
            &request.path,
            request.context.clone(),
            request.loudness_gain,
            request.queue_revision,
            stream.out_rate,
        ) {
            Ok(source) => self.staged = Some(source),
            Err(_) => self.next_request = None,
        }
    }

    /// 解码头吐干净（或被截断丢弃）后接上下一首，并在缓冲当前写位置打点。
    ///
    /// 接不上时**不报错**：那首歌的音频一个字节都还没写进缓冲，当前这首照常放完，
    /// 随后走 `TrackEnded` 那条老路——前端会显式装载它并拿到真正的错误说明。
    fn advance_head(&mut self) {
        // 播完之后不再自动接续：此时传输状态已经停下，接上去只会在暂停态里悄悄填满缓冲，
        // 下次按播放就从一首用户没选的歌中间开始。
        if self.state == PlaybackState::Ended {
            return;
        }
        self.stage_next();
        let Some(source) = self.staged.take() else {
            return;
        };
        let Some(stream) = self.stream.as_mut() else {
            self.staged = Some(source);
            return;
        };
        let boundary = stream.producer.write_index();
        // 打点槽位用尽（用户在一首歌里反复改队列）。宁可让这次交接退回「放完再装载」，
        // 也不能丢掉打点——那会让位置计数与切歌事件永久错位。
        if !stream.producer.mark_boundary(0) {
            self.staged = Some(source);
            return;
        }
        self.marks.push_back(PendingMark {
            boundary,
            sounding: source.sounding(),
            queue_revision: source.queue_revision,
            stale: false,
        });
        // 接上的是同一条输出流，重采样标记要按新曲更新（它未必与前一首同采样率）。
        self.shared.set_resampled(source.resampler.is_active());
        self.head = Some(source);
        self.next_request = None;
        self.ended_reported = false;
        self.drain_deadline = None;
    }

    /// 后面还有没有东西可放。有的话，缓冲里暂时的空不是「放完了」而是交接慢了一步。
    fn has_successor(&self) -> bool {
        self.staged.is_some() || self.next_request.is_some()
    }

    /// 结算消费端越过的边界，把「正在发声的是谁」推进到新曲并发出切歌事件。
    ///
    /// 判定放在这里而不是 `advance_head`，是因为解码领先播放最多一秒半：按解码时机
    /// 发事件，界面会在上一首还在响的时候就换成下一首的标题。
    fn settle_crossings(&mut self) {
        let Some(stream) = self.stream.as_mut() else {
            return;
        };
        let crossed = stream.producer.take_crossed();
        let output = stream.config.clone();
        for _ in 0..crossed {
            let Some(mark) = self.marks.pop_front() else {
                debug_assert!(false, "越界数多于打点数：两侧记录已经不同步");
                break;
            };
            // 被新队列作废的打点：那段音频从未发声，不该产生切歌事件。
            if mark.stale {
                continue;
            }
            let from = self.sounding.as_ref().map(|s| s.context.clone());
            let spec = mark.sounding.spec.clone();
            self.context = Some(mark.sounding.context.clone());
            self.sounding = Some(mark.sounding);
            self.emit(EngineEvent::TrackChanged {
                from,
                spec,
                output: output.clone(),
                queue_revision: mark.queue_revision,
            });
        }
    }

    /// 填到预缓冲阈值。填不满（短文件）也返回，由 `check_ended` 处理结束。
    /// 传入装载代际时，每批解码前检查是否已被更新的 Load 取代。
    fn prebuffer(&mut self, load_generation: Option<u64>) -> Result<bool> {
        let Some(stream) = self.stream.as_ref() else {
            return Ok(true);
        };
        let target = (stream.out_rate as f64 * PREBUFFER_MS / 1000.0) as usize;
        for _ in 0..4096 {
            if load_generation.is_some_and(|generation| !self.is_current_load(generation)) {
                return Ok(false);
            }
            let (Some(stream), Some(head)) = (self.stream.as_ref(), self.head.as_ref()) else {
                return Ok(true);
            };
            if stream.producer.queued_frames() >= target || head.eof {
                return Ok(true);
            }
            if !self.pump()? {
                return Ok(true);
            }
        }
        Ok(true)
    }

    /// 解码并向环形缓冲喂料。返回是否推进了工作（用于决定要不要休眠）。
    ///
    /// 管线：解码 → 响度增益 → 重采样（源声道数）→ 声道适配 → 环形缓冲。
    fn pump(&mut self) -> Result<bool> {
        if self.stream.is_none() {
            return Ok(false);
        }
        // 解码头吐干净了（或刚被截断丢弃）：把备好的下一首接上，缓冲不断流。
        if self.head.as_ref().is_none_or(Source::drained) {
            self.advance_head();
        }
        let no_successor = !self.has_successor();

        let Some(stream) = self.stream.as_mut() else {
            return Ok(false);
        };
        let Some(head) = self.head.as_mut() else {
            if no_successor {
                self.shared.set_source_drained(true);
            }
            return Ok(false);
        };

        if head.drained() {
            if no_successor {
                self.shared.set_source_drained(true);
            }
            return Ok(false);
        }

        let high_water = (stream.out_rate as f64 * HIGH_WATER_MS / 1000.0) as usize;
        if stream.producer.queued_frames() >= high_water {
            // 缓冲喂满，控制线程本来就要歇一轮——正好用来打开下一首。
            self.stage_next();
            return Ok(false);
        }

        // 先把上一轮没写完的残余送进去。
        if head.pending_pos < head.pending.len() {
            let remaining = head.pending.len() - head.pending_pos;
            if no_successor && head.eof && head.flushed && remaining <= stream.producer.writable() {
                // 先发布“不会再生产”，再发布最后一批样本；回调看到尾帧不足整块时
                // 才能稳定地把补零识别为自然收尾，而不是偶发欠载。
                self.shared.set_source_drained(true);
            }
            let written = stream.producer.write(&head.pending[head.pending_pos..]);
            head.pending_pos += written;
            if head.pending_pos < head.pending.len() {
                // 缓冲满了，下轮继续。
                return Ok(written > 0);
            }
        }

        head.resampled.clear();
        if head.eof {
            // 流已读完但重采样器里还压着尾部延迟，冲出来再收工，
            // 否则结尾会缺几十毫秒——单曲不易察觉，gapless 拼接时正好丢在接缝上。
            if head.flushed {
                if no_successor {
                    self.shared.set_source_drained(true);
                }
                return Ok(false);
            }
            head.flushed = true;
            head.resampler.flush(&mut head.resampled);
        } else {
            self.decode_scratch.clear();
            let more = head.decoder.next_frames(&mut self.decode_scratch)?;
            if more {
                // ReplayGain 在**重采样之前**施加（管线顺序见实现计划）：重采样器里
                // 压着上一轮的尾部延迟，中途换增益会让那段尾巴用错倍率；放在前面则
                // 连 flush 冲出来的尾部都是已经归一化过的。
                apply_gain(&mut self.decode_scratch, head.loudness_gain);
                head.resampler
                    .process(&self.decode_scratch, &mut head.resampled);
            } else {
                head.eof = true;
                head.flushed = true;
                head.resampler.flush(&mut head.resampled);
            }
        }

        if head.resampled.is_empty() {
            // 重采样器还没攒够一整块，本轮没有可送的数据。
            if head.eof && no_successor {
                self.shared.set_source_drained(true);
            }
            return Ok(!head.eof);
        }

        let in_frames = head.resampled.len() / head.src_channels;
        let needed = head.adapt.out_samples(in_frames, stream.out_channels);
        head.pending.resize(needed, 0.0);
        head.adapt.apply(&head.resampled, &mut head.pending);
        if no_successor
            && head.eof
            && head.flushed
            && head.pending.len() <= stream.producer.writable()
        {
            self.shared.set_source_drained(true);
        }
        head.pending_pos = stream.producer.write(&head.pending);
        if no_successor && head.eof && head.flushed && head.pending_pos >= head.pending.len() {
            self.shared.set_source_drained(true);
        }
        Ok(true)
    }

    fn seek(&mut self, seconds: f64) -> Result<()> {
        if self.stream.is_none() || self.sounding.is_none() {
            return Ok(());
        }

        // 准确定位可能触发容器 I/O；先让输出回调进入静音/重缓冲态，避免它在定位期间
        // 继续把旧 PCM 往外送。回调仍保持运行，所以能处理下面的 flush 请求。
        self.shared.set_source_drained(false);
        self.shared.set_rebuffering(true);
        self.shared.set_paused(true);

        // 解码头可能已经跑到下一首去了（缓冲里同时躺着两首歌）。定位的对象是**正在
        // 发声**的那首，得先把头拉回来——它的解码器在交接时已经丢掉，只能重开。
        if self.head.is_none() || !self.marks.is_empty() {
            self.rewind_head_to_sounding()?;
        }

        let Some(head) = self.head.as_mut() else {
            return Ok(());
        };
        let frames = head.decoder.seek(seconds)?;

        // 顺序不能反：先丢弃在途 PCM，再重设位置锚点。反过来的话，
        // 旧音频仍会被消费并把位置往前推，界面会看到进度先跳回再乱跳。
        let Some(stream) = self.stream.as_mut() else {
            return Ok(());
        };
        if !stream.producer.flush(FLUSH_TIMEOUT) {
            return Err(EngineError::new(
                Stage::Output,
                ErrorKind::Stream,
                format!(
                    "输出回调未在 {} ms 内确认清空定位前的缓冲",
                    FLUSH_TIMEOUT.as_millis()
                ),
            ));
        }
        // flush 已让消费端把打点一并作废，这边跟着清掉。
        self.marks.clear();
        self.ended_reported = false;
        self.drain_deadline = None;

        let head = self.head.as_mut().expect("刚确认过有解码头");
        head.reset_pipeline();
        // 位置计数器记的是输出帧，源帧要按比率换算。
        self.shared
            .reset_position(head.resampler.src_frames_to_out(frames));

        let _ = self.prebuffer(None)?;
        self.shared.set_rebuffering(false);
        Ok(())
    }

    /// 把解码头退回正在发声的那首。
    ///
    /// 只在 seek 跨越了已经完成的交接时用到：那首歌的解码器在交接时就丢掉了，
    /// 唯一的办法是按路径重开。已经接上的那首退回待接续队列，交接稍后重来一遍。
    fn rewind_head_to_sounding(&mut self) -> Result<()> {
        let (Some(sounding), Some(stream)) = (self.sounding.as_ref(), self.stream.as_ref()) else {
            return Ok(());
        };
        let source = Source::open(
            &sounding.path,
            sounding.context.clone(),
            Some(sounding.loudness_gain),
            0,
            stream.out_rate,
        )?;
        if let Some(head) = self.head.take() {
            self.next_request = Some(head.as_next_request());
        }
        self.staged = None;
        self.shared.set_resampled(source.resampler.is_active());
        self.head = Some(source);
        Ok(())
    }

    /// 解码到末尾、缓冲排空且设备域尾部已发声才算播完。
    ///
    /// 判据里必须带上「缓冲排空」：解码可以领先播放一秒以上，只看 EOF 会在
    /// 最后一秒还在发声时就报播完。后面还有待接续的曲目时更不能报——那不是播完，
    /// 是交接慢了一步。
    fn check_ended(&mut self) {
        let Some(stream) = self.stream.as_ref() else {
            return;
        };
        if self.ended_reported || self.has_successor() {
            return;
        }
        if self.head.as_ref().is_some_and(|head| !head.drained()) {
            return;
        }
        if stream.producer.queued_frames() > 0 {
            self.drain_deadline = None;
            return;
        }
        // 回调可能刚推进 ring 的读下标、还没来得及发布设备时间戳与本块帧数。
        // 先等它退出，避免把默认的 0 延迟误当成设备已经排空。
        if self.shared.callback_in_progress() {
            return;
        }
        let out_rate = stream.out_rate;
        let deadline = *self.drain_deadline.get_or_insert_with(|| {
            let tail_frames = self
                .shared
                .output_delay_frames()
                .saturating_add(self.shared.last_callback_audio_frames());
            Instant::now() + Duration::from_secs_f64(tail_frames as f64 / out_rate as f64)
        });
        if Instant::now() < deadline {
            return;
        }
        self.ended_reported = true;
        self.shared.set_paused(true);
        self.set_state(PlaybackState::Ended);
        self.emit(EngineEvent::TrackEnded);
    }

    fn emit_progress(&mut self) {
        if self.last_progress.elapsed() < PROGRESS_INTERVAL {
            return;
        }
        self.last_progress = Instant::now();
        let (Some(stream), Some(sounding)) = (self.stream.as_ref(), self.sounding.as_ref()) else {
            return;
        };

        let rate = stream.out_rate as f64;
        // 位置来自输出回调累计消费的帧数并扣除设备延迟，不用定时器估算。
        let position_sec = self.shared.played_frames() as f64 / rate;
        let buffered_sec = stream.producer.queued_frames() as f64 / rate;
        // 时长取**正在发声**那首的：解码头可能已经是下一首了，用它会让进度条量程
        // 在上一首还没放完时就换成下一首的。
        let duration_sec = sounding.spec.duration_sec();
        self.emit(EngineEvent::Progress {
            position_sec,
            duration_sec,
            buffered_sec,
        });
    }
}

fn lock_transport(mutex: &Mutex<TransportIntent>) -> MutexGuard<'_, TransportIntent> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// 归一化增益的取值过滤。
///
/// 非有限值、负数一律当作「不处理」而不是钳到 0：那意味着静音，而一个算歪了的增益
/// 让整首歌没声音，比不归一化糟糕得多。上限 8 倍（+18 dB）是给「安静但峰值也低」的
/// 曲目留的余量，同时挡住离谱值——真峰值保护已经在增益公式里咬住了削顶。
fn sanitize_gain(gain: Option<f32>) -> f32 {
    match gain {
        Some(g) if g.is_finite() && g > 0.0 => g.min(8.0),
        _ => 1.0,
    }
}

/// 就地施加整曲常量增益。1.0 时不做任何事——绝大多数曲目都要走这条路径。
fn apply_gain(samples: &mut [f32], gain: f32) {
    if gain == 1.0 {
        return;
    }
    for sample in samples {
        *sample *= gain;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_broken_gain_means_no_processing_not_silence() {
        // 算歪的增益让整首歌没声音，比不归一化糟糕得多——所以异常值一律回落到 1.0，
        // 而不是钳到 0。
        assert_eq!(sanitize_gain(None), 1.0);
        assert_eq!(sanitize_gain(Some(f32::NAN)), 1.0);
        assert_eq!(sanitize_gain(Some(f32::INFINITY)), 1.0);
        assert_eq!(sanitize_gain(Some(-2.0)), 1.0);
        assert_eq!(sanitize_gain(Some(0.0)), 1.0);
        assert_eq!(sanitize_gain(Some(0.5)), 0.5);
        assert_eq!(sanitize_gain(Some(100.0)), 8.0, "离谱的提升要有上限");
    }

    #[test]
    fn unit_gain_leaves_samples_untouched() {
        // 绝大多数曲目最终都落在某个具体倍率上，但「不处理」这条路必须是逐位不变的：
        // 关掉响度归一化时不该有任何浮点尾数上的差异。
        let original = vec![0.1f32, -0.25, 0.5, -1.0];
        let mut samples = original.clone();
        apply_gain(&mut samples, 1.0);
        assert_eq!(samples, original);

        apply_gain(&mut samples, 0.5);
        assert_eq!(samples, vec![0.05, -0.125, 0.25, -0.5]);
    }
}
