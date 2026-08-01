//! 播放链路的无头集成测试。
//!
//! 语料在测试里现生成（16-bit PCM WAV，Symphonia 默认 feature 就能解），
//! 不提交二进制、不依赖外部编码器，因此在无声卡的 CI 上也能跑——
//! 这正是把引擎与 Tauri 解耦的目的。
//!
//! ALAC / AAC 等「不能预先承诺 gapless」的格式要靠真实语料验证，
//! 那属于阶段 1 的语料测试，此处只覆盖阶段 0 的能力边界。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use shannon_audio::decode::Decoder;
use shannon_audio::engine::{
    Engine, EngineEvent, LoadContext, LoadRequest, NextRequest, PlaybackState,
};
use shannon_audio::layout::ChannelLayout;
use shannon_audio::mix::ChannelAdapt;
use shannon_audio::output::null::{NullDevice, NullOutput};
use shannon_audio::output::DeviceEnumerator;
use shannon_audio::output::{
    fill_from_ring, OutputBackend, OutputConfig, OutputRequest, OutputShared,
};
use shannon_audio::ring::RingConsumer;
use shannon_audio::{EngineError, ErrorKind, Result, Stage};

const RATE: u32 = 44_100;
const FREQ: f64 = 440.0;

/// 第 `i` 帧的理论样本值。解码结果要与它逐点比对。
fn sine(i: usize) -> f64 {
    0.3 * (2.0 * std::f64::consts::PI * FREQ * i as f64 / RATE as f64).sin()
}

/// 扫频信号：频率随时间线性上升，因此**没有周期**。
///
/// seek 一类的位置断言必须用它，不能用定频正弦：正弦每 100 帧就重复一次相位，
/// 位置差整数个周期时波形完全重合——差一帧的 bug 会被伪装成「误差为零」。
/// 这不是假设，是本轮实际踩到的：定频语料让 seek 的整数换算 off-by-one 逃过了诊断。
fn chirp(i: usize) -> f64 {
    let t = i as f64 / RATE as f64;
    let (f0, f1, span) = (200.0, 4000.0, 4.0);
    let phase = 2.0 * std::f64::consts::PI * (f0 * t + (f1 - f0) * t * t / (2.0 * span));
    0.3 * phase.sin()
}

/// 写一个 16-bit PCM WAV。`channels` 路声道内容相同。
fn write_wav(path: &Path, channels: u16, frames: usize, gen: impl Fn(usize) -> f64) {
    let byte_rate = RATE * channels as u32 * 2;
    let data_len = (frames * channels as usize * 2) as u32;
    let mut buf = Vec::with_capacity(44 + data_len as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_len).to_le_bytes());
    buf.extend_from_slice(b"WAVEfmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&channels.to_le_bytes());
    buf.extend_from_slice(&RATE.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&(channels * 2).to_le_bytes());
    buf.extend_from_slice(&16u16.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        let v = (gen(i) * i16::MAX as f64) as i16;
        for _ in 0..channels {
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }
    let mut f = std::fs::File::create(path).expect("写语料失败");
    f.write_all(&buf).expect("写语料失败");
}

/// 每个用例独立的语料文件，避免并行测试互相覆盖。
fn corpus(name: &str, channels: u16, frames: usize) -> PathBuf {
    corpus_with(name, channels, frames, sine)
}

fn corpus_with(name: &str, channels: u16, frames: usize, gen: impl Fn(usize) -> f64) -> PathBuf {
    let dir = std::env::temp_dir().join("shannon-audio-tests");
    std::fs::create_dir_all(&dir).expect("建语料目录失败");
    let path = dir.join(format!("{name}.wav"));
    write_wav(&path, channels, frames, gen);
    path
}

fn decode_all(path: &Path) -> Vec<f32> {
    let mut decoder = Decoder::open(path).expect("打开失败");
    let mut out = Vec::new();
    while decoder.next_frames(&mut out).expect("解码失败") {}
    out
}

#[test]
fn decodes_pcm_to_expected_samples() {
    let path = corpus("decode", 2, 4410);
    let mut decoder = Decoder::open(&path).unwrap();
    let spec = decoder.spec().clone();
    assert_eq!(spec.sample_rate, RATE);
    assert!(spec.layout.is_stereo());
    assert_eq!(spec.container, "wave");

    let mut out = Vec::new();
    while decoder.next_frames(&mut out).unwrap() {}
    assert_eq!(out.len(), 4410 * 2);

    // 16-bit 量化误差上界约 1/32768，放宽一档留给格式转换。
    for (i, frame) in out.chunks(2).enumerate() {
        let expect = sine(i) as f32;
        assert!(
            (frame[0] - expect).abs() < 1e-4,
            "第 {i} 帧偏差过大：得到 {}，期望 {expect}",
            frame[0]
        );
        assert_eq!(frame[0], frame[1], "两路声道内容应当一致");
    }
}

#[test]
fn seek_output_matches_decoding_from_start() {
    // seek 等价性：任意位置 seek 后的输出，等于从头解码的对应后缀。
    // 语料必须无周期，否则差整数个周期的偏移会被波形重合掩盖。
    let path = corpus_with("seek", 2, RATE as usize * 2, chirp);
    let full = decode_all(&path);

    let mut decoder = Decoder::open(&path).unwrap();
    let frames = decoder.seek(1.0).unwrap();
    let mut after = Vec::new();
    while decoder.next_frames(&mut after).unwrap() {}

    let offset = frames as usize * 2;
    assert!(
        offset + after.len() <= full.len() + 2,
        "seek 后不应多解出数据"
    );
    let compare = after.len().min(full.len() - offset);
    assert!(compare > RATE as usize, "seek 后应还剩接近一秒的音频");
    for i in 0..compare {
        assert!(
            (after[i] - full[offset + i]).abs() < 1e-6,
            "seek 后第 {i} 个样本与整段解码不一致"
        );
    }
}

#[test]
fn seek_past_end_does_not_panic() {
    let path = corpus("seek_past_end", 2, RATE as usize / 2);
    let mut decoder = Decoder::open(&path).unwrap();
    // 越界定位要么报错要么落到末尾，但绝不能 panic——进度条能拖到任何位置。
    let _ = decoder.seek(9_999.0);
}

#[test]
fn mono_source_is_upmixed_to_stereo() {
    let path = corpus("mono", 1, 441);
    let spec_layout = Decoder::open(&path).unwrap().spec().layout;
    assert!(spec_layout.is_mono());

    let plan = ChannelAdapt::plan(spec_layout, ChannelLayout::STEREO).unwrap();
    assert_eq!(plan, ChannelAdapt::MonoToStereo);

    let mono = decode_all(&path);
    let mut stereo = vec![0.0; plan.out_samples(mono.len(), 2)];
    plan.apply(&mono, &mut stereo);
    for (i, frame) in stereo.chunks(2).enumerate() {
        assert_eq!(frame[0], mono[i]);
        assert_eq!(frame[1], mono[i]);
    }
}

#[test]
fn multichannel_source_is_routed_to_platform_backend() {
    // 多声道不走立体声路径：下混与空间化都交给系统，应用自己混会把本可被空间化的流
    // 提前拍扁。所以这里要的是明确的**路由**错误，不是静默丢声道，
    // 也不是按猜的系数把声场弄乱。
    let path = corpus("surround", 6, 441);
    let layout = Decoder::open(&path).unwrap().spec().layout;
    assert_eq!(layout.count(), 6);

    let err = ChannelAdapt::plan(layout, ChannelLayout::STEREO).unwrap_err();
    assert_eq!(err.kind, ErrorKind::Unsupported);
    assert!(
        err.message.contains("交由系统"),
        "错误要说清多声道该走哪条路，而不是暗示我们欠一个下混算法：{}",
        err.message
    );
}

#[test]
fn missing_file_reports_io_error_not_panic() {
    let Err(err) = Decoder::open(Path::new("/不存在/的/文件.wav")) else {
        panic!("不存在的文件不该打开成功");
    };
    assert_eq!(err.kind, ErrorKind::Io);
    assert_eq!(err.stage, Stage::Open);
}

/// 采集实际送到设备的样本，用于验证管线里施加的增益。
///
/// 按真实节奏消费（与 `NullOutput` 同样 10 ms 一拍）：抽干式消费会让环形缓冲
/// 频繁见底，`fill_from_ring` 补的零会混进采集结果，两次运行的零还落在不同位置。
struct CapturingOutput {
    config: Option<OutputConfig>,
    captured: Arc<Mutex<Vec<f32>>>,
    /// 模拟只支持单一采样率的设备，用来把采集延伸到重采样路径。
    fixed_rate: Option<u32>,
    /// 可切换的假端点。空表时退化成单一「采集」设备，与引入设备切换前一致。
    devices: Vec<NullDevice>,
    prefer: Option<String>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl CapturingOutput {
    fn new(captured: Arc<Mutex<Vec<f32>>>, fixed_rate: Option<u32>) -> Self {
        Self {
            config: None,
            captured,
            fixed_rate,
            devices: Vec::new(),
            prefer: None,
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
        }
    }

    /// 带一组可切换端点的采集后端。第一台是默认。
    fn with_devices(captured: Arc<Mutex<Vec<f32>>>, devices: Vec<NullDevice>) -> Self {
        let mut out = Self::new(captured, None);
        out.devices = devices;
        out
    }

    fn resolve(&self) -> Result<Option<&NullDevice>> {
        if self.devices.is_empty() {
            return Ok(None);
        }
        let Some(want) = self.prefer.as_deref() else {
            return Ok(self.devices.first());
        };
        self.devices
            .iter()
            .find(|d| d.id == want)
            .map(Some)
            .ok_or(EngineError::new(
                Stage::Output,
                ErrorKind::NoDevice,
                format!("标识为「{want}」的输出设备已不可用"),
            ))
    }
}

impl OutputBackend for CapturingOutput {
    fn name(&self) -> &'static str {
        "capturing-test"
    }

    fn set_preferred_device(&mut self, id: Option<String>) {
        self.prefer = id;
    }

    fn negotiate(&self, request: &OutputRequest) -> Result<OutputConfig> {
        let Some(device) = self.resolve()? else {
            return Ok(OutputConfig {
                sample_rate: self.fixed_rate.unwrap_or(request.sample_rate),
                layout: request.layout,
                sample_format: "f32".into(),
                device_name: "采集".into(),
                device_id: None,
            });
        };
        if device.channels != request.layout.count() {
            return Err(EngineError::new(
                Stage::Output,
                ErrorKind::DeviceConfig,
                format!(
                    "设备「{}」不支持 {} 声道输出",
                    device.label,
                    request.layout.count()
                ),
            ));
        }
        Ok(OutputConfig {
            sample_rate: device.fixed_rate.unwrap_or(request.sample_rate),
            layout: request.layout,
            sample_format: "f32".into(),
            device_name: device.label.clone(),
            device_id: Some(device.id.clone()),
        })
    }

    fn open(
        &mut self,
        request: &OutputRequest,
        mut consumer: RingConsumer,
        shared: Arc<OutputShared>,
    ) -> Result<OutputConfig> {
        let config = self.negotiate(request)?;
        let channels = config.layout.count() as usize;
        let frames_per_tick = config.sample_rate as usize / 100;
        let ramp_step = shannon_audio::output::ramp_step_for(config.sample_rate);
        let stop = Arc::new(AtomicBool::new(false));
        self.stop = stop.clone();
        let captured = self.captured.clone();
        self.worker = Some(std::thread::spawn(move || {
            let mut buf = vec![0.0f32; frames_per_tick * channels];
            let mut gain = 0.0f32;
            while !stop.load(Ordering::Relaxed) {
                shared.begin_callback();
                let got = fill_from_ring(
                    &mut buf,
                    channels,
                    &mut consumer,
                    &shared,
                    &mut gain,
                    ramp_step,
                );
                shared.finish_callback(got);
                captured.lock().unwrap().extend_from_slice(&buf);
                std::thread::sleep(Duration::from_millis(10));
            }
        }));
        self.config = Some(config.clone());
        Ok(config)
    }

    fn close(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.config = None;
    }

    fn config(&self) -> Option<&OutputConfig> {
        self.config.as_ref()
    }
}

impl Drop for CapturingOutput {
    fn drop(&mut self) {
        self.close();
    }
}

/// 放完一首，返回送到设备的样本峰值。
fn peak_through_engine(path: &Path, loudness_gain: Option<f32>, device_rate: Option<u32>) -> f32 {
    let captured: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let ended = Arc::new(AtomicBool::new(false));
    let engine = {
        let ended = ended.clone();
        Engine::spawn(
            Box::new(CapturingOutput::new(captured.clone(), device_rate)),
            move |event| {
                if matches!(event, EngineEvent::TrackEnded) {
                    ended.store(true, Ordering::Relaxed);
                }
            },
        )
    };

    let mut request = LoadRequest::new(
        path,
        true,
        LoadContext::new(Some("gain-track".into()), "gain-load"),
    )
    .with_volume(1.0);
    if let Some(gain) = loudness_gain {
        request = request.with_loudness_gain(gain);
    }
    engine.load_request(request).unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    while !ended.load(Ordering::Relaxed) {
        assert!(Instant::now() < deadline, "播放未在预期时间内结束");
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(engine);

    let samples = captured.lock().unwrap();
    samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
}

#[test]
fn loudness_gain_scales_what_reaches_the_device() {
    // 语料振幅 0.3（见 `sine`），音量固定 1.0，所以峰值差异只能来自响度增益。
    // 比峰值而不是逐样本比：偶发欠载补的零会挪动样本位置，却不会抬高峰值。
    let path = corpus("loudness_gain", 2, RATE as usize);
    let plain = peak_through_engine(&path, None, None);
    let halved = peak_through_engine(&path, Some(0.5), None);
    let boosted = peak_through_engine(&path, Some(2.0), None);

    assert!(
        (plain - 0.3).abs() < 0.01,
        "不加增益应保持源振幅，实际 {plain}"
    );
    assert!(
        (halved / plain - 0.5).abs() < 0.02,
        "0.5 倍增益应让峰值减半：{halved} / {plain}"
    );
    assert!(
        (boosted / plain - 2.0).abs() < 0.05,
        "增益要能双向——有曲目需要提升（实测应用增益范围 -14.8 到 +7.0 dB）：{boosted} / {plain}"
    );
}

#[test]
fn loudness_gain_survives_the_resampling_path() {
    // 设备给不出源采样率是常态（实测本机默认设备只有 24 / 48 kHz），所以归一化不能
    // 只在「刚好对得上」的那条路上成立。两次都过同一个重采样器，比值才只反映增益。
    let path = corpus("loudness_gain_resampled", 2, RATE as usize);
    let plain = peak_through_engine(&path, None, Some(48_000));
    let quartered = peak_through_engine(&path, Some(0.25), Some(48_000));

    assert!(
        (quartered / plain - 0.25).abs() < 0.02,
        "重采样后仍应保持 0.25 倍：{quartered} / {plain}"
    );
}

/// 收集引擎事件，供端到端用例断言。
#[derive(Default)]
struct Recorder {
    states: Mutex<Vec<PlaybackState>>,
    ended: AtomicBool,
    errors: Mutex<Vec<String>>,
    last_position_ms: AtomicU64,
}

/// `open` 内同步执行第一次回调，稳定复现“后端已启动、控制线程还没设暂停”的竞态。
/// 不继续消费即可：这个用例只验证装载边界，ring 留在后端里保持生命周期正确。
struct EagerOpenOutput {
    config: Option<OutputConfig>,
    consumer: Option<RingConsumer>,
}

impl EagerOpenOutput {
    fn new() -> Self {
        Self {
            config: None,
            consumer: None,
        }
    }
}

impl OutputBackend for EagerOpenOutput {
    fn name(&self) -> &'static str {
        "eager-test"
    }

    /// 测试替身只有一台设备，选谁都是它。真后端必须真的实现（见 trait 上的说明）。
    fn set_preferred_device(&mut self, _id: Option<String>) {}

    fn negotiate(&self, request: &OutputRequest) -> Result<OutputConfig> {
        Ok(OutputConfig {
            sample_rate: request.sample_rate,
            layout: request.layout,
            sample_format: "f32".into(),
            device_name: "同步首回调测试后端".into(),
            device_id: None,
        })
    }

    fn open(
        &mut self,
        request: &OutputRequest,
        mut consumer: RingConsumer,
        shared: Arc<OutputShared>,
    ) -> Result<OutputConfig> {
        let config = self.negotiate(request)?;
        let mut out = vec![0.0; 64 * request.layout.count() as usize];
        let mut gain = 0.0;
        shared.begin_callback();
        let got = fill_from_ring(
            &mut out,
            request.layout.count() as usize,
            &mut consumer,
            &shared,
            &mut gain,
            1.0,
        );
        shared.finish_callback(got);
        self.consumer = Some(consumer);
        self.config = Some(config.clone());
        Ok(config)
    }

    fn close(&mut self) {
        self.consumer = None;
        self.config = None;
    }

    fn config(&self) -> Option<&OutputConfig> {
        self.config.as_ref()
    }
}

/// 包一层空后端，并在测试触发时模拟设备断开。
struct FailingOutput {
    inner: NullOutput,
    fail: Arc<AtomicBool>,
}

/// 记录每次 open 看到的目标增益与位置，验证二者是否与 Load 原子生效。
struct LoadRecordingOutput {
    inner: NullOutput,
    gains: Arc<Mutex<Vec<f32>>>,
    positions: Arc<Mutex<Vec<u64>>>,
}

/// 第一轮协商停在测试控制的栅栏上，用来把「更新命令在 Load 进行中到达」稳定放大。
struct BlockingFirstNegotiateOutput {
    inner: NullOutput,
    entered: Arc<AtomicBool>,
    release: Arc<AtomicBool>,
    calls: AtomicU64,
}

impl OutputBackend for BlockingFirstNegotiateOutput {
    fn name(&self) -> &'static str {
        "blocking-negotiate-test"
    }

    /// 测试替身只有一台设备，选谁都是它。真后端必须真的实现（见 trait 上的说明）。
    fn set_preferred_device(&mut self, _id: Option<String>) {}

    fn negotiate(&self, request: &OutputRequest) -> Result<OutputConfig> {
        if self.calls.fetch_add(1, Ordering::Relaxed) == 0 {
            self.entered.store(true, Ordering::Release);
            while !self.release.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
        }
        self.inner.negotiate(request)
    }

    fn open(
        &mut self,
        request: &OutputRequest,
        consumer: RingConsumer,
        shared: Arc<OutputShared>,
    ) -> Result<OutputConfig> {
        self.inner.open(request, consumer, shared)
    }

    fn close(&mut self) {
        self.inner.close();
    }

    fn config(&self) -> Option<&OutputConfig> {
        self.inner.config()
    }
}

/// 报告固定设备延迟的无声后端。它让测试能区分「ring 已被回调取空」与
/// 「最后一帧已经到达扬声器」这两个时刻。
struct DelayedOutput {
    config: Option<OutputConfig>,
    delay_frames: u64,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl DelayedOutput {
    fn new(delay_frames: u64) -> Self {
        Self {
            config: None,
            delay_frames,
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
        }
    }
}

impl OutputBackend for DelayedOutput {
    fn name(&self) -> &'static str {
        "delayed-test"
    }

    /// 测试替身只有一台设备，选谁都是它。真后端必须真的实现（见 trait 上的说明）。
    fn set_preferred_device(&mut self, _id: Option<String>) {}

    fn negotiate(&self, request: &OutputRequest) -> Result<OutputConfig> {
        Ok(OutputConfig {
            sample_rate: request.sample_rate,
            layout: request.layout,
            sample_format: "f32".into(),
            device_name: "延迟测试后端".into(),
            device_id: None,
        })
    }

    fn open(
        &mut self,
        request: &OutputRequest,
        mut consumer: RingConsumer,
        shared: Arc<OutputShared>,
    ) -> Result<OutputConfig> {
        self.close();
        let config = self.negotiate(request)?;
        let channels = config.layout.count() as usize;
        let frames_per_tick = config.sample_rate as usize / 100;
        let ramp_step = shannon_audio::output::ramp_step_for(config.sample_rate);
        let delay_frames = self.delay_frames;
        let stop = Arc::new(AtomicBool::new(false));
        self.stop = stop.clone();
        self.worker = Some(std::thread::spawn(move || {
            let mut out = vec![0.0; frames_per_tick * channels];
            let mut gain = 0.0;
            while !stop.load(Ordering::Relaxed) {
                shared.begin_callback();
                shared.set_output_delay_frames(delay_frames);
                let got = fill_from_ring(
                    &mut out,
                    channels,
                    &mut consumer,
                    &shared,
                    &mut gain,
                    ramp_step,
                );
                shared.finish_callback(got);
                std::thread::sleep(Duration::from_millis(10));
            }
        }));
        self.config = Some(config.clone());
        Ok(config)
    }

    fn close(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.config = None;
    }

    fn config(&self) -> Option<&OutputConfig> {
        self.config.as_ref()
    }
}

impl Drop for DelayedOutput {
    fn drop(&mut self) {
        self.close();
    }
}

impl OutputBackend for LoadRecordingOutput {
    fn name(&self) -> &'static str {
        "load-recording-test"
    }

    /// 测试替身只有一台设备，选谁都是它。真后端必须真的实现（见 trait 上的说明）。
    fn set_preferred_device(&mut self, _id: Option<String>) {}

    fn negotiate(&self, request: &OutputRequest) -> Result<OutputConfig> {
        self.inner.negotiate(request)
    }

    fn open(
        &mut self,
        request: &OutputRequest,
        consumer: RingConsumer,
        shared: Arc<OutputShared>,
    ) -> Result<OutputConfig> {
        self.gains.lock().unwrap().push(shared.gain());
        self.positions
            .lock()
            .unwrap()
            .push(shared.position_frames());
        self.inner.open(request, consumer, shared)
    }

    fn close(&mut self) {
        self.inner.close();
    }

    fn config(&self) -> Option<&OutputConfig> {
        self.inner.config()
    }
}

impl OutputBackend for FailingOutput {
    fn name(&self) -> &'static str {
        "failing-test"
    }

    /// 测试替身只有一台设备，选谁都是它。真后端必须真的实现（见 trait 上的说明）。
    fn set_preferred_device(&mut self, _id: Option<String>) {}

    fn negotiate(&self, request: &OutputRequest) -> Result<OutputConfig> {
        self.inner.negotiate(request)
    }

    fn open(
        &mut self,
        request: &OutputRequest,
        consumer: RingConsumer,
        shared: Arc<OutputShared>,
    ) -> Result<OutputConfig> {
        self.inner.open(request, consumer, shared)
    }

    fn take_error(&mut self) -> Option<EngineError> {
        self.fail
            .swap(false, Ordering::Relaxed)
            .then(|| EngineError::new(Stage::Output, ErrorKind::Stream, "测试设备已断开"))
    }

    fn close(&mut self) {
        self.inner.close();
    }

    fn config(&self) -> Option<&OutputConfig> {
        self.inner.config()
    }
}

#[test]
fn output_is_quiescent_before_open_can_invoke_its_first_callback() {
    let path = corpus("eager_open", 2, RATE as usize);
    let opened = Arc::new(AtomicBool::new(false));
    let engine = {
        let opened = opened.clone();
        Engine::spawn(Box::new(EagerOpenOutput::new()), move |event| {
            if matches!(event, EngineEvent::Opened { .. }) {
                opened.store(true, Ordering::Relaxed);
            }
        })
    };

    engine.load(&path, false).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !opened.load(Ordering::Relaxed) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }

    assert!(opened.load(Ordering::Relaxed), "音源应完成装载");
    assert_eq!(
        engine.stats().underruns,
        0,
        "open 内立即发生的首回调必须先看到暂停/重缓冲状态"
    );
}

#[test]
fn queued_loads_coalesce_to_the_latest_context_and_initial_gain() {
    // 同一首连续装载两次，track_id 刻意相同：若只按曲目 ID 过滤，上一代事件仍会漏过。
    // 两条命令紧挨着投递也复现了外壳“共享最新 ID”曾经会盖错章的窗口。
    let path = corpus("load_context", 2, RATE as usize);
    let gains = Arc::new(Mutex::new(Vec::new()));
    let positions = Arc::new(Mutex::new(Vec::new()));
    let opened = Arc::new(Mutex::new(Vec::<LoadContext>::new()));
    let engine = {
        let (gains_for_backend, positions_for_backend, opened) =
            (gains.clone(), positions.clone(), opened.clone());
        Engine::spawn_stamped(
            Box::new(LoadRecordingOutput {
                inner: NullOutput::new(),
                gains: gains_for_backend,
                positions: positions_for_backend,
            }),
            move |stamped| {
                if matches!(stamped.event, EngineEvent::Opened { .. }) {
                    opened.lock().unwrap().push(stamped.context);
                }
            },
        )
    };

    let first = LoadContext::new(Some("track-same".into()), "load-1");
    let second = LoadContext::new(Some("track-same".into()), "load-2");
    engine
        .load_request(LoadRequest::new(&path, false, first.clone()).with_volume(0.18))
        .unwrap();
    engine
        .load_request(LoadRequest::new(&path, false, second.clone()).with_volume(0.42))
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(2);
    while opened.lock().unwrap().is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }

    assert_eq!(
        *opened.lock().unwrap(),
        vec![second],
        "同一批积压的旧 Load 不该再打开设备或发事件"
    );
    let actual_gains = gains.lock().unwrap();
    assert_eq!(actual_gains.len(), 1);
    assert!((actual_gains[0] - 0.42).abs() < f32::EPSILON);
    assert_eq!(*positions.lock().unwrap(), vec![0]);
}

#[test]
fn a_new_load_cancels_one_already_inside_negotiation() {
    let path = corpus("load_generation", 2, RATE as usize);
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let opened = Arc::new(Mutex::new(Vec::<LoadContext>::new()));
    let engine = {
        let opened = opened.clone();
        Engine::spawn_stamped(
            Box::new(BlockingFirstNegotiateOutput {
                inner: NullOutput::new(),
                entered: entered.clone(),
                release: release.clone(),
                calls: AtomicU64::new(0),
            }),
            move |stamped| {
                if matches!(stamped.event, EngineEvent::Opened { .. }) {
                    opened.lock().unwrap().push(stamped.context);
                }
            },
        )
    };

    engine
        .load_request(LoadRequest::new(
            &path,
            true,
            LoadContext::new(Some("old".into()), "load-old"),
        ))
        .unwrap();
    let entered_deadline = Instant::now() + Duration::from_secs(2);
    while !entered.load(Ordering::Acquire) && Instant::now() < entered_deadline {
        std::thread::yield_now();
    }
    assert!(entered.load(Ordering::Acquire), "第一代应已进入耗时协商");

    let latest = LoadContext::new(Some("latest".into()), "load-latest");
    engine
        .load_request(LoadRequest::new(&path, false, latest.clone()))
        .unwrap();
    release.store(true, Ordering::Release);

    let deadline = Instant::now() + Duration::from_secs(2);
    while opened.lock().unwrap().is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        *opened.lock().unwrap(),
        vec![latest],
        "协商途中失效的旧代际不能再打开输出或短暂出声"
    );
}

#[test]
fn pause_sent_during_load_cannot_be_overwritten_by_autoplay() {
    let path = corpus("pause_during_load", 2, RATE as usize * 2);
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let opened = Arc::new(AtomicBool::new(false));
    let states = Arc::new(Mutex::new(Vec::new()));
    let engine = {
        let (opened, states) = (opened.clone(), states.clone());
        Engine::spawn(
            Box::new(BlockingFirstNegotiateOutput {
                inner: NullOutput::new(),
                entered: entered.clone(),
                release: release.clone(),
                calls: AtomicU64::new(0),
            }),
            move |event| match event {
                EngineEvent::Opened { .. } => opened.store(true, Ordering::Relaxed),
                EngineEvent::StateChanged(state) => states.lock().unwrap().push(state),
                _ => {}
            },
        )
    };

    engine.load(&path, true).unwrap();
    let entered_deadline = Instant::now() + Duration::from_secs(2);
    while !entered.load(Ordering::Acquire) && Instant::now() < entered_deadline {
        std::thread::yield_now();
    }
    assert!(entered.load(Ordering::Acquire));
    engine.pause().unwrap();
    release.store(true, Ordering::Release);

    let deadline = Instant::now() + Duration::from_secs(2);
    while !opened.load(Ordering::Relaxed) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(opened.load(Ordering::Relaxed), "暂停不应取消装载本身");
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        engine.stats().position_frames,
        0,
        "加载中发出的暂停必须在首帧前生效"
    );
    assert!(
        !states.lock().unwrap().contains(&PlaybackState::Playing),
        "旧 Load 的 autoplay 不能覆盖稍后到达的 Pause"
    );
}

#[test]
fn stop_sent_during_load_cancels_the_open() {
    let path = corpus("stop_during_load", 2, RATE as usize);
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let opened = Arc::new(AtomicBool::new(false));
    let engine = {
        let opened = opened.clone();
        Engine::spawn(
            Box::new(BlockingFirstNegotiateOutput {
                inner: NullOutput::new(),
                entered: entered.clone(),
                release: release.clone(),
                calls: AtomicU64::new(0),
            }),
            move |event| {
                if matches!(event, EngineEvent::Opened { .. }) {
                    opened.store(true, Ordering::Relaxed);
                }
            },
        )
    };

    engine.load(&path, true).unwrap();
    let entered_deadline = Instant::now() + Duration::from_secs(2);
    while !entered.load(Ordering::Acquire) && Instant::now() < entered_deadline {
        std::thread::yield_now();
    }
    assert!(entered.load(Ordering::Acquire));
    engine.stop().unwrap();
    release.store(true, Ordering::Release);
    std::thread::sleep(Duration::from_millis(100));

    assert!(
        !opened.load(Ordering::Relaxed),
        "Stop 后旧 Load 不得继续打开输出"
    );
    assert_eq!(engine.stats().position_frames, 0);
}

#[test]
fn track_ended_waits_for_the_reported_device_tail() {
    let path = corpus("device_tail", 2, RATE as usize / 20); // 50 ms
    let ended = Arc::new(AtomicBool::new(false));
    let engine = {
        let ended = ended.clone();
        Engine::spawn(
            Box::new(DelayedOutput::new((RATE / 5) as u64)), // 200 ms 设备延迟
            move |event| {
                if matches!(event, EngineEvent::TrackEnded) {
                    ended.store(true, Ordering::Relaxed);
                }
            },
        )
    };

    let started = Instant::now();
    engine.load(&path, true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ended.load(Ordering::Relaxed) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(ended.load(Ordering::Relaxed), "短曲应正常结束");
    assert!(
        started.elapsed() >= Duration::from_millis(200),
        "TrackEnded 不能在设备报告的尾部延迟之前发出"
    );
}

#[test]
fn load_context_accepts_the_frontend_camel_case_shape() {
    let context: LoadContext =
        serde_json::from_str(r#"{"trackId":"track-7","loadId":"load-9"}"#).unwrap();
    assert_eq!(context, LoadContext::new(Some("track-7".into()), "load-9"));
}

#[test]
fn initial_position_is_applied_before_output_opens() {
    // 会话续播不能先 autoplay 曲首、等 Opened 跨 IPC 回到前端后再 Seek。
    // 输出后端 open 时就看到目标位置，证明定位发生在预缓冲与解除暂停之前。
    let path = corpus_with("initial_position", 2, RATE as usize * 3, chirp);
    let positions = Arc::new(Mutex::new(Vec::new()));
    let opened = Arc::new(AtomicBool::new(false));
    let engine = {
        let (positions_for_backend, opened) = (positions.clone(), opened.clone());
        Engine::spawn_stamped(
            Box::new(LoadRecordingOutput {
                // 顺带覆盖源域定位帧到输出域位置的换算。
                inner: NullOutput::with_fixed_rate(48_000),
                gains: Arc::new(Mutex::new(Vec::new())),
                positions: positions_for_backend,
            }),
            move |stamped| {
                if matches!(stamped.event, EngineEvent::Opened { .. }) {
                    opened.store(true, Ordering::Relaxed);
                }
            },
        )
    };

    let initial_position_sec = 1.25;
    let expected = Decoder::open(&path)
        .unwrap()
        .seek(initial_position_sec)
        .unwrap();
    engine
        .load_request(
            LoadRequest::new(
                &path,
                true,
                LoadContext::new(Some("resume-track".into()), "resume-load"),
            )
            .with_volume(0.5)
            .with_position(Some(initial_position_sec)),
        )
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(2);
    while !opened.load(Ordering::Relaxed) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(opened.load(Ordering::Relaxed), "续播音源应完成装载");

    let actual = positions.lock().unwrap();
    assert_eq!(actual.len(), 1);
    let expected_output = (expected as u128 * 48_000u128 / RATE as u128) as u64;
    assert_eq!(
        actual[0], expected_output,
        "输出 open 前应把解码器实际定位帧换算到输出域"
    );
}

#[test]
fn seek_timeout_reports_stream_error_instead_of_stealing_consumer_index() {
    let path = corpus("seek_timeout", 2, RATE as usize);
    let opened = Arc::new(AtomicBool::new(false));
    let saw_error = Arc::new(AtomicBool::new(false));
    let engine = {
        let (opened, saw_error) = (opened.clone(), saw_error.clone());
        Engine::spawn(Box::new(EagerOpenOutput::new()), move |event| match event {
            EngineEvent::Opened { .. } => opened.store(true, Ordering::Relaxed),
            EngineEvent::Error(err) => {
                assert_eq!(err.kind, ErrorKind::Stream);
                saw_error.store(true, Ordering::Relaxed);
            }
            _ => {}
        })
    };

    engine.load(&path, false).unwrap();
    let open_deadline = Instant::now() + Duration::from_secs(2);
    while !opened.load(Ordering::Relaxed) && Instant::now() < open_deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(opened.load(Ordering::Relaxed), "音源应完成装载");

    engine.seek(0.5).unwrap();
    let error_deadline = Instant::now() + Duration::from_secs(2);
    while !saw_error.load(Ordering::Relaxed) && Instant::now() < error_deadline {
        std::thread::sleep(Duration::from_millis(5));
    }

    assert!(saw_error.load(Ordering::Relaxed), "flush 超时必须明确报错");
}

#[test]
fn runtime_output_error_is_forwarded_and_stops_playback() {
    let path = corpus("runtime_output_error", 2, RATE as usize * 3);
    let fail = Arc::new(AtomicBool::new(false));
    let saw_error = Arc::new(AtomicBool::new(false));
    let ended = Arc::new(AtomicBool::new(false));
    let states = Arc::new(Mutex::new(Vec::new()));
    let engine = {
        let (saw_error, ended, states) = (saw_error.clone(), ended.clone(), states.clone());
        Engine::spawn(
            Box::new(FailingOutput {
                inner: NullOutput::new(),
                fail: fail.clone(),
            }),
            move |event| match event {
                EngineEvent::Error(err) => {
                    assert_eq!(err.kind, ErrorKind::Stream);
                    saw_error.store(true, Ordering::Relaxed);
                }
                EngineEvent::StateChanged(state) => states.lock().unwrap().push(state),
                EngineEvent::TrackEnded => ended.store(true, Ordering::Relaxed),
                _ => {}
            },
        )
    };

    engine.load(&path, true).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    fail.store(true, Ordering::Relaxed);

    let deadline = Instant::now() + Duration::from_secs(2);
    while !saw_error.load(Ordering::Relaxed) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }

    assert!(saw_error.load(Ordering::Relaxed), "运行期设备错误必须上报");
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !ended.load(Ordering::Relaxed),
        "失败代际不能随后再冒充自然结束，否则前端会自动跳下一首"
    );
    assert_eq!(
        states.lock().unwrap().last(),
        Some(&PlaybackState::Error),
        "设备已经无声时状态不能继续停在 Playing"
    );
}

#[test]
fn plays_to_completion_through_null_backend() {
    let seconds = 0.6;
    let path = corpus("e2e", 2, (RATE as f64 * seconds) as usize);
    let rec = Arc::new(Recorder::default());

    let engine = {
        let rec = rec.clone();
        Engine::spawn(Box::new(NullOutput::new()), move |event| match event {
            EngineEvent::StateChanged(s) => rec.states.lock().unwrap().push(s),
            EngineEvent::TrackEnded => rec.ended.store(true, Ordering::Relaxed),
            EngineEvent::Error(e) => rec.errors.lock().unwrap().push(e.to_string()),
            EngineEvent::Progress { position_sec, .. } => {
                rec.last_position_ms
                    .store((position_sec * 1000.0) as u64, Ordering::Relaxed);
            }
            EngineEvent::Opened { .. }
            | EngineEvent::TrackChanged { .. }
            | EngineEvent::OutputChanged { .. }
            | EngineEvent::DeviceRejected { .. } => {}
        })
    };

    engine.load(&path, true).unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    while !rec.ended.load(Ordering::Relaxed) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }

    assert!(
        rec.errors.lock().unwrap().is_empty(),
        "不该有错误：{:?}",
        rec.errors.lock().unwrap()
    );
    assert!(rec.ended.load(Ordering::Relaxed), "应当播放到自然结束");

    let states = rec.states.lock().unwrap().clone();
    assert!(
        states.contains(&PlaybackState::Playing),
        "状态序列应经过 Playing：{states:?}"
    );
    assert_eq!(states.last(), Some(&PlaybackState::Ended));

    let stats = engine.stats();
    let played = stats.frames_consumed as f64 / RATE as f64;
    assert!(
        (played - seconds).abs() < 0.05,
        "消费帧数应对应音频时长：播了 {played:.3} 秒，语料 {seconds} 秒"
    );
    assert_eq!(stats.underruns, 0, "正常播放不该欠载");
}

#[test]
fn pause_stops_position_from_advancing() {
    // 暂停期间输出流仍在跑（继续写零帧），但位置不得推进——
    // 位置的事实来源是「消费了多少帧」，不是墙上时钟。
    let path = corpus("pause", 2, RATE as usize * 3);
    let position = Arc::new(AtomicU64::new(0));

    let engine = {
        let position = position.clone();
        Engine::spawn(Box::new(NullOutput::new()), move |event| {
            if let EngineEvent::Progress { position_sec, .. } = event {
                position.store((position_sec * 1000.0) as u64, Ordering::Relaxed);
            }
        })
    };

    engine.load(&path, true).unwrap();
    std::thread::sleep(Duration::from_millis(600));
    engine.pause().unwrap();
    // 等音量斜坡走完并让位置事件刷新一轮。
    std::thread::sleep(Duration::from_millis(400));

    let before = position.load(Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(500));
    let after = position.load(Ordering::Relaxed);

    assert!(before > 0, "暂停前位置应已推进");
    // 斜坡期间还会消费少量帧，留出一档余量。
    assert!(
        after.saturating_sub(before) < 100,
        "暂停后位置不应继续推进：{before} ms → {after} ms"
    );
}

#[test]
fn seek_does_not_count_as_underrun() {
    // seek 后缓冲被清空，此时的「取不到数据」是预期的重缓冲，
    // 计进欠载会让这项指标失去诊断价值。
    let path = corpus("seek_stats", 2, RATE as usize * 3);
    let engine = Engine::spawn(Box::new(NullOutput::new()), |_| {});

    engine.load(&path, true).unwrap();
    std::thread::sleep(Duration::from_millis(400));
    for target in [2.0, 0.5, 1.5] {
        engine.seek(target).unwrap();
        std::thread::sleep(Duration::from_millis(150));
    }

    assert_eq!(engine.stats().underruns, 0, "seek 引起的重缓冲不算欠载");
}

#[test]
fn resamples_when_device_rate_differs() {
    // 复现实测场景：设备只给得出 48 kHz，而曲库主力是 44.1 kHz。
    // 早先这里是直接报错「不支持 44100 Hz」——一首歌都放不了。
    let seconds = 0.6;
    let path = corpus("resample_e2e", 2, (RATE as f64 * seconds) as usize);
    let ended = Arc::new(AtomicBool::new(false));
    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let out_rate = Arc::new(AtomicU64::new(0));

    let engine = {
        let (ended, errors, out_rate) = (ended.clone(), errors.clone(), out_rate.clone());
        Engine::spawn(
            Box::new(NullOutput::with_fixed_rate(48_000)),
            move |event| match event {
                EngineEvent::TrackEnded => ended.store(true, Ordering::Relaxed),
                EngineEvent::Error(e) => errors.lock().unwrap().push(e.to_string()),
                EngineEvent::Opened { output, .. } => {
                    out_rate.store(output.sample_rate as u64, Ordering::Relaxed)
                }
                _ => {}
            },
        )
    };

    engine.load(&path, true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ended.load(Ordering::Relaxed) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }

    assert!(
        errors.lock().unwrap().is_empty(),
        "不该有错误：{:?}",
        errors.lock().unwrap()
    );
    assert!(ended.load(Ordering::Relaxed), "重采样后仍应播放到自然结束");
    assert_eq!(out_rate.load(Ordering::Relaxed), 48_000);

    let stats = engine.stats();
    assert!(stats.resampled, "插了重采样就必须如实标记");

    // 位置计数记的是输出帧，所以按 48 kHz 换算才对得上音频时长。
    // 若误用源采样率，0.6 秒会被算成 0.653 秒（快 8.8%）。
    let played = stats.frames_consumed as f64 / 48_000.0;
    assert!(
        (played - seconds).abs() < 0.05,
        "重采样后的时长应保持不变：算得 {played:.3} 秒，语料 {seconds} 秒"
    );
    assert_eq!(stats.underruns, 0, "重采样不该造成欠载");
}

#[test]
fn no_resampling_when_rates_match() {
    let path = corpus("no_resample", 2, RATE as usize / 4);
    let engine = Engine::spawn(Box::new(NullOutput::with_fixed_rate(RATE)), |_| {});
    engine.load(&path, true).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(!engine.stats().resampled, "采样率一致时不该插入重采样");
}

#[test]
fn seek_position_is_correct_under_resampling() {
    // seek 返回的是源域帧位置，位置计数器要的是输出域。不换算的话，
    // 44.1 → 48 kHz 时进度会偏 8.8%——拖到 1:00 显示成 1:05。
    let path = corpus("resample_seek", 2, RATE as usize * 3);
    let position = Arc::new(AtomicU64::new(0));
    let engine = {
        let position = position.clone();
        Engine::spawn(
            Box::new(NullOutput::with_fixed_rate(48_000)),
            move |event| {
                if let EngineEvent::Progress { position_sec, .. } = event {
                    position.store((position_sec * 1000.0) as u64, Ordering::Relaxed);
                }
            },
        )
    };

    engine.load(&path, true).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    engine.seek(2.0).unwrap();
    std::thread::sleep(Duration::from_millis(400));

    let ms = position.load(Ordering::Relaxed);
    assert!(
        (2_000..2_400).contains(&ms),
        "定位到 2.0 秒后位置应在 2.0～2.4 秒之间，实际 {ms} ms"
    );
}

#[test]
fn per_track_frame_count_survives_track_changes() {
    // 位置与累计消费是两个量：位置每首归零，累计跨曲目单调递增。
    // 合成一个字段的话「这首播了多少帧」只能靠差值算，而差值会被归零抹平——
    // 实测表现为歌单里第二首起每首都显示消费 0 帧。
    let path = corpus("per_track_frames", 2, (RATE as f64 * 0.4) as usize);
    let ended = Arc::new(AtomicBool::new(false));
    let engine = {
        let ended = ended.clone();
        Engine::spawn(Box::new(NullOutput::new()), move |event| {
            if matches!(event, EngineEvent::TrackEnded) {
                ended.store(true, Ordering::Relaxed);
            }
        })
    };

    let mut last_total = 0u64;
    for round in 1..=3 {
        ended.store(false, Ordering::Relaxed);
        engine.load(&path, true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ended.load(Ordering::Relaxed) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(ended.load(Ordering::Relaxed), "第 {round} 遍没播完");

        let stats = engine.stats();
        let this_track = stats.frames_consumed - last_total;
        last_total = stats.frames_consumed;
        assert!(
            this_track > RATE as u64 / 4,
            "第 {round} 遍的单曲消费量算成了 {this_track} 帧——累计计数被换曲清零了"
        );
        // 位置则相反：每首从头开始，不会累加到三倍。
        assert!(
            stats.position_frames < RATE as u64,
            "第 {round} 遍的位置累加到了 {} 帧，换曲应当归零",
            stats.position_frames
        );
    }
}

// ───────────────────────────── 无缝接续（gapless） ─────────────────────────────

/// 把一段连续扫频切成两个文件。
///
/// **gapless 的判据**就是把两段接起来之后，波形与从未被切开的原信号逐样本一致：
/// 中间多出静音、少掉几个样本、或把一段重放一遍，都会在接缝处暴露成相位跳变。
///
/// 语料必须无周期（见 `chirp` 的说明）：定频正弦在接缝处差整数个周期时波形完全重合，
/// 而那恰好是最容易出错的一类偏差，用它等于给自己出一张不会失败的考卷。
fn split_chirp(name: &str, frames: usize) -> (PathBuf, PathBuf) {
    let first = corpus_with(&format!("{name}-a"), 2, frames, chirp);
    let second = corpus_with(&format!("{name}-b"), 2, frames, move |i| chirp(i + frames));
    (first, second)
}

/// 恒定电平的语料。用来在采集结果里一眼认出「此刻响的是哪一首」——
/// 幅度 0.9 与扫频那 0.3 拉开距离，正负号区分两首。
fn flat(name: &str, frames: usize, level: f64) -> PathBuf {
    corpus_with(name, 2, frames, move |_| level)
}

/// 在采集结果里找到音频开始的样本下标。
///
/// 采集包含开播前的静音与 15 ms 音量斜坡，逐样本比对必须先对齐，而且要**精确到样本**
/// ——差一帧的偏移正是这类缺陷最常见的形态，用「大致对齐」去验证等于什么都没验证。
/// 做法是拿参考信号中段（已过斜坡）当模板搜一遍，取误差最小的偏移。
fn align(captured: &[f32], reference: &[f32], probe_frame: usize) -> usize {
    const WINDOW: usize = 2000;
    let probe = probe_frame * 2;
    let template = &reference[probe..probe + WINDOW];
    // 开播前的静音最多是预缓冲那 300 ms 加几拍回调，搜索范围留足即可。
    let limit = captured.len().saturating_sub(probe + WINDOW).min(40_000);
    let mut best = (f64::MAX, 0usize);
    for start in (0..limit).step_by(2) {
        let seg = &captured[start + probe..start + probe + WINDOW];
        let err: f64 = seg
            .iter()
            .zip(template)
            .map(|(a, b)| ((a - b) as f64).powi(2))
            .sum();
        if err < best.0 {
            best = (err, start);
        }
    }
    let mse = best.0 / WINDOW as f64;
    assert!(mse < 1e-8, "采集结果与参考信号对不上（最小均方误差 {mse}）");
    best.1
}

#[derive(Debug, Clone, PartialEq)]
enum ChainEvent {
    Changed {
        from: Option<String>,
        to: Option<String>,
        revision: u32,
    },
    Progress {
        position_sec: f64,
        buffered_sec: f64,
    },
    Ended,
    Failed(String),
    /// 换到了另一台端点：记下新端点名与输出采样率。
    Output {
        device: String,
        rate: u32,
        revision: u64,
    },
    /// 换端点被拒（播放未中断）。
    Rejected(String),
    State(PlaybackState),
}

/// 一条无缝播放链的测试台：采集全部输出样本，并按序记录事件。
struct Chain {
    engine: Engine,
    captured: Arc<Mutex<Vec<f32>>>,
    events: Arc<Mutex<Vec<ChainEvent>>>,
    chain_id: String,
    device_revision: AtomicU64,
}

impl Chain {
    fn start(device_rate: Option<u32>) -> Self {
        let captured: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        Self::with_backend(
            captured.clone(),
            CapturingOutput::new(captured, device_rate),
        )
    }

    /// 带一组可切换端点的测试台。第一台是默认。
    fn start_with_devices(devices: Vec<NullDevice>) -> Self {
        let captured: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        Self::with_backend(
            captured.clone(),
            CapturingOutput::with_devices(captured, devices),
        )
    }

    fn with_backend(captured: Arc<Mutex<Vec<f32>>>, backend: CapturingOutput) -> Self {
        let events: Arc<Mutex<Vec<ChainEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let engine = {
            let events = events.clone();
            Engine::spawn_stamped(Box::new(backend), move |stamped| {
                // 事件盖的是**正在发声**那首的章，交接后自动换成新曲的。
                let to = stamped.context.track_id.clone();
                let mut log = events.lock().unwrap();
                match stamped.event {
                    EngineEvent::TrackChanged {
                        from,
                        queue_revision,
                        ..
                    } => log.push(ChainEvent::Changed {
                        from: from.and_then(|c| c.track_id),
                        to,
                        revision: queue_revision,
                    }),
                    EngineEvent::Progress {
                        position_sec,
                        buffered_sec,
                        ..
                    } => log.push(ChainEvent::Progress {
                        position_sec,
                        buffered_sec,
                    }),
                    EngineEvent::TrackEnded => log.push(ChainEvent::Ended),
                    EngineEvent::Error(err) => log.push(ChainEvent::Failed(err.to_string())),
                    EngineEvent::OutputChanged {
                        ref output,
                        device_revision,
                        ..
                    } => log.push(ChainEvent::Output {
                        device: output.device_name.clone(),
                        rate: output.sample_rate,
                        revision: device_revision,
                    }),
                    EngineEvent::DeviceRejected { error, .. } => {
                        log.push(ChainEvent::Rejected(error.to_string()))
                    }
                    EngineEvent::StateChanged(state) => log.push(ChainEvent::State(state)),
                    EngineEvent::Opened { .. } => {}
                }
            })
        };
        Self {
            engine,
            captured,
            events,
            chain_id: "integration-chain".into(),
            device_revision: AtomicU64::new(0),
        }
    }

    fn load(&self, path: &Path, track: &str) {
        self.engine
            .load_request(LoadRequest::new(
                path,
                true,
                LoadContext::new(Some(track.into()), self.chain_id.clone()),
            ))
            .unwrap();
    }

    fn load_with_next(
        &self,
        path: &Path,
        track: &str,
        next_path: &Path,
        next_track: &str,
        revision: u32,
    ) {
        self.engine
            .load_request(
                LoadRequest::new(
                    path,
                    true,
                    LoadContext::new(Some(track.into()), self.chain_id.clone()),
                )
                .with_next(
                    Some(NextRequest::new(
                        next_path,
                        LoadContext::new(Some(next_track.into()), format!("next-{next_track}")),
                    )),
                    revision,
                ),
            )
            .unwrap();
    }

    fn set_next(&self, path: &Path, track: &str, revision: u32) {
        self.engine
            .set_next(
                &self.chain_id,
                Some(NextRequest::new(
                    path,
                    LoadContext::new(Some(track.into()), format!("next-{track}")),
                )),
                revision,
            )
            .unwrap();
    }

    fn clear_next(&self, revision: u32) {
        self.engine
            .set_next(&self.chain_id, None, revision)
            .unwrap();
    }

    fn set_device(&self, id: Option<&str>) {
        let revision = self.device_revision.fetch_add(1, Ordering::Relaxed) + 1;
        self.set_device_revision(id, revision);
    }

    fn set_device_revision(&self, id: Option<&str>, revision: u64) {
        self.device_revision.fetch_max(revision, Ordering::Relaxed);
        self.engine
            .set_device(id.map(str::to_string), revision)
            .unwrap();
    }

    fn events(&self) -> Vec<ChainEvent> {
        self.events.lock().unwrap().clone()
    }

    fn changes(&self) -> Vec<ChainEvent> {
        self.events()
            .into_iter()
            .filter(|e| matches!(e, ChainEvent::Changed { .. }))
            .collect()
    }

    /// 等到条件成立。超时就带着完整事件序列失败——「等不到」本身就是被测行为的一部分。
    fn wait_for(&self, what: &str, cond: impl Fn(&[ChainEvent]) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            let events = self.events();
            if let Some(ChainEvent::Failed(msg)) = events
                .iter()
                .find(|e| matches!(e, ChainEvent::Failed(_)))
                .cloned()
            {
                panic!("等「{what}」时播放失败：{msg}");
            }
            if cond(&events) {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("等不到「{what}」；事件序列：{:?}", self.events());
    }

    fn wait_ended(&self) {
        self.wait_for("播完", |events| events.contains(&ChainEvent::Ended));
    }

    /// 等到下一首确实躺进了缓冲。
    ///
    /// `threshold` 要**大于第一首的总时长**，否则「缓冲里有这么多数据」可能全是第一首的，
    /// 用例就会在旧 next 还没进缓冲时就去改队列——那测的是另一条（更容易的）路径。
    fn wait_until_buffered(&self, threshold: f64) {
        self.wait_for(&format!("缓冲超过 {threshold} 秒"), |events| {
            events.iter().any(
                |e| matches!(e, ChainEvent::Progress { buffered_sec, .. } if *buffered_sec > threshold),
            )
        });
    }

    fn captured(&self) -> Vec<f32> {
        self.captured.lock().unwrap().clone()
    }
}

fn position_of(event: &ChainEvent) -> Option<f64> {
    match event {
        ChainEvent::Progress { position_sec, .. } => Some(*position_sec),
        _ => None,
    }
}

#[test]
fn a_gapless_handoff_is_sample_continuous() {
    // 一秒一段，接缝落在第 44100 帧。
    let frames = RATE as usize;
    let (first, second) = split_chirp("gapless", frames);
    let whole = decode_all(&corpus_with("gapless-whole", 2, frames * 2, chirp));

    let chain = Chain::start(None);
    chain.load(&first, "A");
    chain.set_next(&second, "B", 1);
    chain.wait_ended();

    let captured = chain.captured();
    let offset = align(&captured, &whole, frames / 2);
    let seam = offset + frames * 2;
    // 接缝前后各 200 帧逐样本比对。这一段没有重采样、增益斜坡也早已到顶，
    // 所以要求的是**逐位一致**，不是「差不多」。
    for i in (seam - 400)..(seam + 400) {
        let got = captured[i];
        let want = whole[i - offset];
        assert!(
            (got - want).abs() < 1e-6,
            "接缝偏移 {} 个样本处不连续：得到 {got}，期望 {want}",
            i as isize - seam as isize
        );
    }
    assert_eq!(chain.changes().len(), 1, "一次交接只该报一次");
    assert_eq!(
        chain.engine.stats().underruns,
        0,
        "无缝交接不该欠载——接缝处正是缓冲最容易见底的地方"
    );
}

#[test]
fn the_position_restarts_at_the_boundary() {
    let first = corpus_with("pos-a", 2, RATE as usize, chirp);
    let second = corpus_with("pos-b", 2, RATE as usize, chirp);
    let chain = Chain::start(None);
    chain.load(&first, "A");
    chain.set_next(&second, "B", 1);
    chain.wait_ended();

    let events = chain.events();
    let at = events
        .iter()
        .position(|e| matches!(e, ChainEvent::Changed { .. }))
        .expect("必须有一次交接");
    let before = events[..at]
        .iter()
        .filter_map(position_of)
        .fold(0.0f64, f64::max);
    let after = events[at + 1..]
        .iter()
        .filter_map(position_of)
        .next()
        .expect("交接之后必须还有进度事件");
    assert!(
        before > 0.7,
        "交接前的进度应当接近第一首末尾，实际 {before}"
    );
    // 缓冲里两首歌的 PCM 之间没有任何分隔，位置能归零只可能是消费端结算了边界。
    assert!(after < 0.4, "交接后进度必须从新曲起算，实际 {after}");
}

#[test]
fn replacing_the_next_track_before_it_sounds_keeps_it_silent() {
    // 用户在最后一秒改了队列：已经预解码进缓冲的旧 next 必须一个样本都不出去，
    // 否则「改队列」对听感完全无效——他明明改了，放出来的还是原来那首。
    let first = corpus_with("swap-a", 2, (RATE as f64 * 0.8) as usize, chirp);
    let stale = flat("swap-stale", RATE as usize / 2, 0.9);
    let fresh = flat("swap-fresh", RATE as usize / 2, -0.9);

    let chain = Chain::start(None);
    chain.load(&first, "A");
    chain.set_next(&stale, "STALE", 1);
    chain.wait_until_buffered(0.9);
    chain.set_next(&fresh, "FRESH", 2);
    chain.wait_ended();

    let captured = chain.captured();
    assert!(
        !captured.iter().any(|s| *s > 0.5),
        "旧队列的下一首一个样本都不许出去"
    );
    assert!(
        captured.iter().any(|s| *s < -0.5),
        "新指定的下一首必须真的放出来"
    );
    let changes = chain.changes();
    assert_eq!(
        changes.len(),
        1,
        "被撤掉的那次交接不该产生事件：{changes:?}"
    );
    assert!(
        matches!(&changes[0], ChainEvent::Changed { to: Some(t), revision: 2, .. } if t == "FRESH"),
        "切歌事件要指向新队列的那首并回带新版本号：{changes:?}"
    );
}

#[test]
fn clearing_a_prefetched_next_still_allows_pause_and_resume() {
    // next 已经进 ring 后再清空会让解码头暂时变成 None；但当前曲目的尾巴仍在发声。
    // Play/Pause 若把「有无解码头」误当成「有无播放链」，暂停后便再也恢复不了。
    let first = corpus_with("clear-next-a", 2, (RATE as f64 * 0.8) as usize, chirp);
    let removed = flat("clear-next-b", RATE as usize / 2, 0.9);

    let chain = Chain::start(None);
    chain.load(&first, "A");
    chain.set_next(&removed, "REMOVED", 1);
    chain.wait_until_buffered(0.9);
    chain.clear_next(2);
    chain.engine.pause().unwrap();
    std::thread::sleep(Duration::from_millis(50));
    chain.engine.play().unwrap();
    chain.wait_ended();

    assert!(chain.changes().is_empty(), "被清掉的 next 不应交接");
    assert!(
        !chain.captured().iter().any(|sample| *sample > 0.5),
        "被清掉的 next 一个样本都不应发声"
    );
}

#[test]
fn stale_next_updates_cannot_cross_an_explicit_load() {
    // 一条旧播放链的 SetNext 晚于新 Load 抵达时，不能把新 Load 原子携带的 next 覆盖掉；
    // 同一条新链里 revision 更旧的更新也同理。两类乱序分别由 chain 与 revision 挡住。
    let first = corpus_with("stale-chain-a", 2, (RATE as f64 * 0.4) as usize, chirp);
    let intended = flat("stale-chain-intended", RATE as usize / 2, -0.9);
    let stale = flat("stale-chain-old", RATE as usize / 2, 0.9);

    let chain = Chain::start(None);
    chain.load_with_next(&first, "A", &intended, "INTENDED", 10);
    chain
        .engine
        .set_next(
            "previous-chain",
            Some(NextRequest::new(
                &stale,
                LoadContext::new(Some("OLD-CHAIN".into()), "old-chain-next"),
            )),
            100,
        )
        .unwrap();
    chain.set_next(&stale, "OLD-REVISION", 9);
    chain.wait_ended();

    let captured = chain.captured();
    assert!(
        captured.iter().any(|sample| *sample < -0.5),
        "新 Load 原子指定的 next 必须发声"
    );
    assert!(
        !captured.iter().any(|sample| *sample > 0.5),
        "旧链或旧 revision 都不能污染新 Load"
    );
    let changes = chain.changes();
    assert!(
        matches!(&changes[..], [ChainEvent::Changed { to: Some(to), revision: 10, .. }] if to == "INTENDED"),
        "只应交接到新 Load 指定的 next：{changes:?}"
    );
}

#[test]
fn next_update_arriving_before_its_load_is_not_lost() {
    // Tauri invoke 之间没有顺序保证：后发的 SetNext 可能先于建立播放链的 Load 进引擎。
    // 它应暂存到同名 chain，等 Load 到达后以较新的 revision 覆盖初始指定。
    let first = corpus_with("future-chain-a", 2, (RATE as f64 * 0.4) as usize, chirp);
    let initial = flat("future-chain-initial", RATE as usize / 2, 0.9);
    let updated = flat("future-chain-updated", RATE as usize / 2, -0.9);

    let chain = Chain::start(None);
    chain.set_next(&updated, "UPDATED", 2);
    chain.load_with_next(&first, "A", &initial, "INITIAL", 1);
    chain.wait_ended();

    let captured = chain.captured();
    assert!(captured.iter().any(|sample| *sample < -0.5));
    assert!(
        !captured.iter().any(|sample| *sample > 0.5),
        "先到的更新不能被后到的 Load 初始值抹掉"
    );
    let changes = chain.changes();
    assert!(
        matches!(&changes[..], [ChainEvent::Changed { to: Some(to), revision: 2, .. }] if to == "UPDATED"),
        "交接应采用暂存的更新：{changes:?}"
    );
}

#[test]
fn a_boundary_already_crossed_is_a_fact() {
    // 越过边界之后才改队列就是晚了：那首歌已经在响，撤不回来。此时引擎既要如实
    // 发出它的切歌事件（丢掉的话界面会停在一首早已放完的歌上），也要把新指定的
    // 那首排在它之后。
    let first = corpus_with("late-a", 2, (RATE as f64 * 0.4) as usize, chirp);
    let second = flat("late-b", (RATE as f64 * 0.6) as usize, 0.9);
    let third = flat("late-c", (RATE as f64 * 0.4) as usize, -0.9);

    let chain = Chain::start(None);
    chain.load(&first, "A");
    chain.set_next(&second, "B", 1);
    chain.wait_for("交接到 B", |events| {
        events
            .iter()
            .any(|e| matches!(e, ChainEvent::Changed { to: Some(t), .. } if t == "B"))
    });
    chain.set_next(&third, "C", 2);
    chain.wait_ended();

    let changes = chain.changes();
    assert_eq!(changes.len(), 2, "两次交接都该报：{changes:?}");
    assert!(
        matches!(&changes[0], ChainEvent::Changed { from: Some(f), to: Some(t), .. } if f == "A" && t == "B")
    );
    assert!(
        matches!(&changes[1], ChainEvent::Changed { from: Some(f), to: Some(t), .. } if f == "B" && t == "C")
    );
    let captured = chain.captured();
    assert!(
        captured.iter().any(|s| *s > 0.5),
        "已经在响的那首不能被撤掉"
    );
    assert!(
        captured.iter().any(|s| *s < -0.5),
        "新指定的那首应当接在它后面"
    );
}

#[test]
fn seeking_after_the_handoff_stays_on_the_sounding_track() {
    // 解码头已经跑到下一首去了，此刻定位的对象仍是**正在发声**的那首——而它的解码器
    // 在交接时就丢掉了，引擎必须按路径把它重新打开。若定位错打在下一首上，
    // 缓冲会被清空、边界随之作废，那一次交接就再也不会发生。
    let first = corpus_with("seekh-a", 2, (RATE as f64 * 0.8) as usize, chirp);
    let second = flat("seekh-b", RATE as usize / 2, 0.9);

    let chain = Chain::start(None);
    chain.load(&first, "A");
    chain.set_next(&second, "B", 1);
    chain.wait_until_buffered(0.9);
    chain.engine.seek(0.2).unwrap();
    chain.wait_ended();

    let events = chain.events();
    let at = events
        .iter()
        .position(|e| matches!(e, ChainEvent::Changed { .. }))
        .expect("定位不该让后面那首消失");
    let before = events[..at]
        .iter()
        .filter_map(position_of)
        .fold(0.0f64, f64::max);
    // 进度只有 5 Hz，边界前最后一条最多会早约 200 ms；这里只证明定位后确实继续
    // 推进了相当一段，而「最终到达边界」已经由上面的 Changed 事件严格证明。
    assert!(
        before > 0.4,
        "定位后第一首应当继续放到自己的末尾，实际只到 {before}"
    );
    assert_eq!(chain.changes().len(), 1, "交接既不该丢也不该重复");
}

#[test]
fn repeat_one_hands_off_to_the_same_file() {
    // 单曲循环就是把 next 指向自己。两个解码器同时打开同一个文件必须没问题，
    // 否则「循环一首」会在第二遍开头断掉。
    let path = corpus_with("repeat-one", 2, RATE as usize / 2, chirp);
    let chain = Chain::start(None);
    chain.load(&path, "A");
    chain.set_next(&path, "A-again", 1);
    chain.wait_for("接上第二遍", |events| {
        events
            .iter()
            .any(|e| matches!(e, ChainEvent::Changed { to: Some(t), .. } if t == "A-again"))
    });
    chain.wait_ended();
    assert_eq!(chain.changes().len(), 1);
}

#[test]
fn a_handoff_survives_a_sample_rate_change() {
    // 两首采样率不同：整条输出流的采样率由第一首协商决定，第二首必须重采样到它上面。
    // 这是「无缝优先」的代价，也是必须验证的路径——接缝处换重采样比率最容易出岔子。
    let first = corpus_with("rate-a", 2, RATE as usize / 2, chirp);
    let second = flat("rate-b", RATE as usize / 2, 0.9);
    // 设备只给 48 kHz，于是两首都要重采样，且第二首的比率与第一首相同。
    let chain = Chain::start(Some(48_000));
    chain.load(&first, "A");
    chain.set_next(&second, "B", 1);
    chain.wait_ended();

    assert_eq!(chain.changes().len(), 1, "重采样路径上也要能交接");
    let captured = chain.captured();
    assert!(
        captured.iter().any(|s| *s > 0.5),
        "第二首必须真的放出来（重采样后幅度仍应接近 0.9）"
    );
    assert_eq!(chain.engine.stats().underruns, 0);
}

#[test]
fn hammering_the_next_slot_never_leaves_playback_stuck() {
    // 第一首必须**短于缓冲**（高水位 1.5 秒），它的解码才会立刻吐完，
    // 后面每指定一次下一首都真的走一遍「接上并打点」——否则这个用例连打点都碰不到。
    let first = corpus_with("hammer-a", 2, RATE as usize, chirp);
    // 每次都换一首**不同的**下一首，逼引擎真的重新打点。
    let candidates: Vec<PathBuf> = (0..24)
        .map(|i| flat(&format!("hammer-n{i}"), RATE as usize / 4, 0.9))
        .collect();

    let chain = Chain::start(None);
    chain.load(&first, "A");
    chain.set_next(&candidates[0], "N0", 1);
    // 缓冲超过第一首的总时长（1 秒）= 缓冲里确实已经躺着接上来的那首，打点已经发生。
    chain.wait_until_buffered(1.02);

    for (i, path) in candidates.iter().enumerate().skip(1) {
        chain.set_next(path, &format!("N{i}"), i as u32 + 1);
        // 留出一轮喂料的时间，让引擎真的把这一首接上并打点。
        std::thread::sleep(Duration::from_millis(12));
    }

    // 无论最终接上了哪一首、还是一首都没接上，播放都必须走到终点而不是卡住。
    chain.wait_ended();
}

// ── 设备切换 ──
//
// 架构约束不变量第 8 条把「设备切换、采样率变化和后端失效」列为必须显式的状态机，
// 验收条件第 6 条要求它与 gapless、重采样分别测试。这些用例全部无声卡：真机上验证
// 换端点得插拔硬件，那是进不了 CI 的，而这条路径恰恰会静默出错——切完仍在旧设备上
// 出声、位置按旧时基走、暂停被切成播放，三种都不报任何错。

/// 找出换端点事件之后的第一条进度。
fn position_after_switch(events: &[ChainEvent]) -> Option<f64> {
    let at = events
        .iter()
        .position(|e| matches!(e, ChainEvent::Output { .. }))?;
    events[at..].iter().find_map(|e| match e {
        ChainEvent::Progress { position_sec, .. } => Some(*position_sec),
        _ => None,
    })
}

/// 换端点事件之前的最后一条进度。
fn position_before_switch(events: &[ChainEvent]) -> Option<f64> {
    let at = events
        .iter()
        .position(|e| matches!(e, ChainEvent::Output { .. }))?;
    events[..at].iter().rev().find_map(|e| match e {
        ChainEvent::Progress { position_sec, .. } => Some(*position_sec),
        _ => None,
    })
}

#[test]
fn switching_devices_resumes_where_it_left_off() {
    // 换端点要把输出流整条拆掉重建（新设备的采样率未必相同，环形缓冲与位置时基都得
    // 按它重来）。位置不接回去的表现是这首歌从头再放一遍，而进度条会跟着一起骗人。
    let path = corpus_with("switch-position", 2, (RATE as f64 * 4.0) as usize, chirp);
    let chain = Chain::start_with_devices(vec![
        NullDevice::new("dev-a", "端点 A"),
        NullDevice::new("dev-b", "端点 B"),
    ]);
    chain.load(&path, "A");
    chain.wait_for("放到 1 秒", |events| {
        events
            .iter()
            .any(|e| matches!(e, ChainEvent::Progress { position_sec, .. } if *position_sec > 1.0))
    });

    chain.set_device(Some("dev-b"));
    chain.wait_for("换到端点 B", |events| {
        events
            .iter()
            .any(|e| matches!(e, ChainEvent::Output { device, .. } if device == "端点 B"))
    });
    chain.wait_for("换端点后仍有进度", |events| {
        position_after_switch(events).is_some()
    });

    let events = chain.events();
    let before = position_before_switch(&events).expect("换之前必须有进度");
    let after = position_after_switch(&events).expect("换之后必须有进度");
    // 换端点期间是静音的，所以位置只该原地等待、不该倒退，也不该凭空前进：
    // 上界给的是一个进度间隔（200 ms）加上重建本身的耗时。
    assert!(
        after >= before - 0.05,
        "换端点后位置倒退了：{before} → {after}"
    );
    assert!(
        after < before + 0.6,
        "换端点后位置凭空前进：{before} → {after}"
    );
}

#[test]
fn switching_to_a_device_with_another_rate_rebuilds_the_time_base() {
    // 新端点只吃 48 kHz 而源是 44.1 kHz：整条链路后半段的时基都要按新采样率重建。
    // 漏掉任何一处（环形缓冲容量、重采样比率、位置换算）都不会报错，只会让进度按
    // 比率走偏——44.1 → 48 kHz 快 8.8%，一首四分钟的歌最后差出二十秒。
    let path = corpus_with("switch-rate", 2, (RATE as f64 * 4.0) as usize, chirp);
    let chain = Chain::start_with_devices(vec![
        NullDevice::new("dev-native", "原生 44.1k"),
        NullDevice::new("dev-48k", "只吃 48k").with_fixed_rate(48_000),
    ]);
    chain.load(&path, "A");
    chain.wait_for("放到 1 秒", |events| {
        events
            .iter()
            .any(|e| matches!(e, ChainEvent::Progress { position_sec, .. } if *position_sec > 1.0))
    });
    assert!(
        !chain.engine.stats().resampled,
        "原生采样率的端点上不该有重采样"
    );

    chain.set_device(Some("dev-48k"));
    chain.wait_for("换到 48k 端点", |events| {
        events
            .iter()
            .any(|e| matches!(e, ChainEvent::Output { rate, .. } if *rate == 48_000))
    });
    // 「不静默降级」是硬要求：插了重采样就要能被上层看见。
    assert!(
        chain.engine.stats().resampled,
        "换到 48k 端点后必须标记重采样"
    );

    // 位置继续按秒推进，而不是按输出帧数被 48/44.1 放大。
    chain.wait_for("换端点后位置继续推进", |events| {
        let Some(after) = position_after_switch(events) else {
            return false;
        };
        events.iter().any(
            |e| matches!(e, ChainEvent::Progress { position_sec, .. } if *position_sec > after + 0.5),
        )
    });
    let events = chain.events();
    let before = position_before_switch(&events).expect("换之前必须有进度");
    let after = position_after_switch(&events).expect("换之后必须有进度");
    assert!(
        (after - before).abs() < 0.6,
        "跨采样率换端点后位置对不上：{before} → {after}"
    );
}

#[test]
fn an_unusable_device_is_refused_without_stopping_the_music() {
    // 两种「用不了」都要在**不打断播放**的前提下如实回报：设备不在（拔了 / 存的标识
    // 过期）与能力不够（当前只有立体声路径）。这里的判据不是「报了错」，而是
    // 「报了错、歌还在放、并且仍在原来那台设备上」——把正在听的歌掐掉，比换不成糟得多。
    let path = corpus_with("switch-refused", 2, (RATE as f64 * 2.0) as usize, chirp);
    let chain = Chain::start_with_devices(vec![
        NullDevice::new("dev-a", "端点 A"),
        NullDevice::new("dev-surround", "只有 5.1 口").with_channels(6),
    ]);
    chain.load(&path, "A");
    chain.wait_for("放到 0.5 秒", |events| {
        events
            .iter()
            .any(|e| matches!(e, ChainEvent::Progress { position_sec, .. } if *position_sec > 0.5))
    });

    chain.set_device(Some("dev-gone")); // 根本不存在
    chain.set_device(Some("dev-surround")); // 存在但给不出立体声
    chain.wait_for("两次拒绝都回报", |events| {
        events
            .iter()
            .filter(|e| matches!(e, ChainEvent::Rejected(_)))
            .count()
            >= 2
    });

    // 播放照常走到终点，一次都没换过端点，也没有失败事件。
    chain.wait_ended();
    let events = chain.events();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, ChainEvent::Output { .. })),
        "被拒的端点不该产生换端点事件：{events:?}"
    );
    assert!(
        !events.iter().any(|e| matches!(e, ChainEvent::Failed(_))),
        "换端点被拒不是播放失败：{events:?}"
    );
    let peak = chain
        .captured()
        .iter()
        .fold(0.0f32, |acc, s| acc.max(s.abs()));
    assert!(peak > 0.2, "被拒之后声音必须照常出去，实际峰值 {peak}");
}

#[test]
fn switching_while_paused_does_not_start_playing() {
    // 换端点走的是「拆流 → 重建 → 预缓冲」，而预缓冲结束时最容易顺手把暂停解除掉。
    // 用户在暂停状态下换耳机，不该因此突然放出声音。
    let path = corpus_with("switch-paused", 2, (RATE as f64 * 3.0) as usize, chirp);
    let chain = Chain::start_with_devices(vec![
        NullDevice::new("dev-a", "端点 A"),
        NullDevice::new("dev-b", "端点 B"),
    ]);
    chain.load(&path, "A");
    chain.wait_for("放到 0.5 秒", |events| {
        events
            .iter()
            .any(|e| matches!(e, ChainEvent::Progress { position_sec, .. } if *position_sec > 0.5))
    });
    chain.engine.pause().unwrap();
    std::thread::sleep(Duration::from_millis(120));

    chain.set_device(Some("dev-b"));
    chain.wait_for("换到端点 B", |events| {
        events
            .iter()
            .any(|e| matches!(e, ChainEvent::Output { device, .. } if device == "端点 B"))
    });

    let baseline = chain.captured().len();
    std::thread::sleep(Duration::from_millis(300));
    let silent_tail = &chain.captured()[baseline..];
    let peak = silent_tail.iter().fold(0.0f32, |acc, s| acc.max(s.abs()));
    assert!(
        peak < 1e-4,
        "暂停状态下换端点后不该出声，实际峰值 {peak}（{} 个样本）",
        silent_tail.len()
    );
    let last_state = chain
        .events()
        .into_iter()
        .rev()
        .find_map(|e| match e {
            ChainEvent::State(state) => Some(state),
            _ => None,
        })
        .expect("必须有状态事件");
    assert_eq!(last_state, PlaybackState::Paused, "换端点不改变传输状态");
}

#[test]
fn a_stale_device_request_cannot_override_the_latest_choice() {
    // Tauri 命令会并发执行，“较早点击的请求较晚进引擎”是合法时序。版本 2 已经选中 B
    // 之后，迟到的版本 1 绝不能再把输出切回 A；否则设置里显示 B，实际却从 A 出声。
    let path = corpus_with("switch-stale", 2, (RATE as f64 * 2.0) as usize, chirp);
    let chain = Chain::start_with_devices(vec![
        NullDevice::new("dev-a", "端点 A"),
        NullDevice::new("dev-b", "端点 B"),
    ]);
    chain.load(&path, "A");
    chain.wait_for("开始播放", |events| {
        events
            .iter()
            .any(|e| matches!(e, ChainEvent::State(PlaybackState::Playing)))
    });

    chain.set_device_revision(Some("dev-b"), 2);
    chain.set_device_revision(Some("dev-a"), 1);
    chain.wait_for("新版请求换到端点 B", |events| {
        events
            .iter()
            .any(|e| matches!(e, ChainEvent::Output { device, .. } if device == "端点 B"))
    });
    std::thread::sleep(Duration::from_millis(150));

    let outputs: Vec<_> = chain
        .events()
        .into_iter()
        .filter_map(|event| match event {
            ChainEvent::Output { device, .. } => Some(device),
            _ => None,
        })
        .collect();
    assert_eq!(outputs, ["端点 B"], "迟到的旧请求不该再切一次端点");
}

#[test]
fn the_latest_revision_is_acknowledged_even_for_the_same_device() {
    // StrictMode 会重挂载同步 hook，同一偏好因而可能连续下发两版。前端在发出版本 2 后
    // 会丢掉版本 1 的回执；若“设备没变”就不回新版确认，实际输出与 effectiveDeviceId
    // 会永久失配。
    let path = corpus_with(
        "switch-same-revision",
        2,
        (RATE as f64 * 2.0) as usize,
        chirp,
    );
    let chain = Chain::start_with_devices(vec![
        NullDevice::new("dev-a", "端点 A"),
        NullDevice::new("dev-b", "端点 B"),
    ]);
    chain.load(&path, "A");
    chain.wait_for("开始播放", |events| {
        events
            .iter()
            .any(|e| matches!(e, ChainEvent::State(PlaybackState::Playing)))
    });

    chain.set_device_revision(Some("dev-b"), 1);
    chain.set_device_revision(Some("dev-b"), 2);
    chain.wait_for("同一端点的新版请求也有确认", |events| {
        events.iter().any(
            |event| matches!(event, ChainEvent::Output { device, revision: 2, .. } if device == "端点 B"),
        )
    });
}

#[test]
fn switching_device_after_natural_end_keeps_the_ended_state() {
    // 自然结束必须同时收束“播放意图”。若只把 shared 暂停、却把 intent.playing 留在 true，
    // 换设备重建尾部后会按旧意图解除暂停，Ended 短暂跳回 Playing，随后还会再报一次结束。
    let path = corpus_with("switch-ended", 2, (RATE as f64 * 0.25) as usize, chirp);
    let chain = Chain::start_with_devices(vec![
        NullDevice::new("dev-a", "端点 A"),
        NullDevice::new("dev-b", "端点 B"),
    ]);
    chain.load(&path, "A");
    chain.wait_ended();
    let first_ended = chain
        .events()
        .iter()
        .position(|event| matches!(event, ChainEvent::Ended))
        .expect("必须先自然结束");

    chain.set_device(Some("dev-b"));
    chain.wait_for("结束后仍能换到端点 B", |events| {
        events
            .iter()
            .any(|e| matches!(e, ChainEvent::Output { device, .. } if device == "端点 B"))
    });
    std::thread::sleep(Duration::from_millis(200));

    let events = chain.events();
    assert!(
        !events[first_ended + 1..]
            .iter()
            .any(|event| matches!(event, ChainEvent::State(PlaybackState::Playing))),
        "结束后换端点不该复活播放态：{events:?}"
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, ChainEvent::Ended))
            .count(),
        1,
        "一次自然结束只能上报一次：{events:?}"
    );
    let last_state = events.iter().rev().find_map(|event| match event {
        ChainEvent::State(state) => Some(*state),
        _ => None,
    });
    assert_eq!(last_state, Some(PlaybackState::Ended));
}

#[test]
fn the_next_track_survives_a_device_switch() {
    // 换端点不改变「下一首是谁」。用 teardown 一把拆干净最省事，但那会把待接续的
    // 下一首一并清掉——表现为用户每换一次设备就丢一次无缝接续，而且只在换过设备
    // 的那一次曲目边界上才看得出来。
    let first = corpus_with("switch-next-a", 2, (RATE as f64 * 2.0) as usize, chirp);
    let second = flat("switch-next-b", RATE as usize / 2, 0.9);
    let chain = Chain::start_with_devices(vec![
        NullDevice::new("dev-a", "端点 A"),
        NullDevice::new("dev-b", "端点 B"),
    ]);
    chain.load(&first, "A");
    chain.set_next(&second, "B", 1);
    chain.wait_for("放到 0.5 秒", |events| {
        events
            .iter()
            .any(|e| matches!(e, ChainEvent::Progress { position_sec, .. } if *position_sec > 0.5))
    });

    chain.set_device(Some("dev-b"));
    chain.wait_for("换到端点 B", |events| {
        events
            .iter()
            .any(|e| matches!(e, ChainEvent::Output { device, .. } if device == "端点 B"))
    });
    chain.wait_ended();

    let changes = chain.changes();
    assert!(
        matches!(&changes[..], [ChainEvent::Changed { to: Some(to), .. }] if to == "B"),
        "换端点后仍应无缝接上原先指定的下一首：{changes:?}"
    );
    assert!(
        chain.captured().iter().any(|s| *s > 0.5),
        "接上的那首必须真的发声"
    );
}

#[test]
fn the_device_list_marks_the_system_default() {
    // 「跟随系统默认」与「恰好选中了当前的默认设备」是两回事，但界面上要能看出
    // 哪一台是系统默认——否则用户无从判断自己那台耳机是不是已经被系统接管了。
    let backend = NullOutput::with_devices([
        NullDevice::new("dev-a", "端点 A"),
        NullDevice::new("dev-b", "端点 B"),
    ]);
    let listed = backend.enumerator().devices().unwrap();
    assert_eq!(listed.len(), 2);
    assert!(listed[0].is_default, "第一台应标为系统默认");
    assert!(!listed[1].is_default);
    assert_eq!(listed[1].id, "dev-b");
}

#[test]
fn an_unusable_device_is_refused_even_before_anything_plays() {
    // 用户完全可以在暂停（甚至还没放过任何东西）时去设置里改设备。这时若只记下偏好
    // 不作校验，错误要等到他下次按播放才冒出来——那时他已经忘了自己改过设备，
    // 而报出来的是一条「播放失败」，看上去像是这首歌的问题。
    let chain = Chain::start_with_devices(vec![
        NullDevice::new("dev-a", "端点 A"),
        NullDevice::new("dev-surround", "只有 5.1 口").with_channels(6),
    ]);
    chain.set_device(Some("dev-surround"));
    chain.wait_for("空闲时也要当场回报", |events| {
        events.iter().any(|e| matches!(e, ChainEvent::Rejected(_)))
    });

    // 被拒之后偏好没有落下：随后真的开始放，走的仍是原来那台，而不是那台 5.1 口。
    let path = corpus_with("idle-refused", 2, (RATE as f64 * 0.4) as usize, chirp);
    chain.load(&path, "A");
    chain.wait_ended();
    assert!(
        !chain
            .events()
            .iter()
            .any(|e| matches!(e, ChainEvent::Failed(_))),
        "被拒的选择不该留下来污染下一次装载：{:?}",
        chain.events()
    );
}
