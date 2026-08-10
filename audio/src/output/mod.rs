//! 输出后端抽象。
//!
//! trait 首版即定，后续的 WASAPI 独占、macOS `AVSampleBufferAudioRenderer`、
//! Windows `ISpatialAudioClient` 都以新实现插入，不改引擎。
//!
//! **接口必须能表达声道布局，而不只是声道数**——这正是 CPAL 不能承担**任何**多声道输出的
//! 原因（不只是空间内容）：它的配置里只有一个数字，而布局标签既是系统判断能否空间化的
//! 依据，也是它正确下混的依据（见 `docs/MACOS_SPATIAL_PLAYBACK_NOTES.md`）。
//! 所以协商的输入输出都用 [`ChannelLayout`](crate::layout::ChannelLayout)，
//! 哪怕当前唯一的实现（CPAL 立体声）自己只用得上其中的声道数。

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::error::{EngineError, Result};
use crate::layout::ChannelLayout;
use crate::ring::RingConsumer;

pub mod cpal_out;
pub mod null;

/// 向输出后端提出的配置请求。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputRequest {
    pub sample_rate: u32,
    pub layout: ChannelLayout,
}

/// 协商结果。与请求不一致的部分必须能被上层看见——**不静默降级**是架构约束的硬要求，
/// 例如采样率被改动就意味着链路里发生了重采样，`stats` 要如实标记。
#[derive(Debug, Clone)]
pub struct OutputConfig {
    pub sample_rate: u32,
    pub layout: ChannelLayout,
    /// 设备实际使用的采样格式名，仅用于诊断展示。
    pub sample_format: String,
    /// 设备名，用于诊断与设备切换提示。
    pub device_name: String,
    /// 实际打开的端点标识；跟随系统默认时是**当下解析到的那一台**的标识，不是 `None`。
    /// 界面要显示「现在到底在哪台设备上出声」，靠的就是它。
    pub device_id: Option<String>,
}

/// 一个可选的输出端点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// 持久化用的标识。
    ///
    /// **不用设备名当键**：同型号的两只 USB DAC 名字一模一样，用名字选会随机打中一只。
    /// cpal 的 `DeviceId` 就是为持久化设计的（macOS 上取的是 CoreAudio 的设备 UID，
    /// 跨重启与重新插拔都不变），所以这里存它的字符串形式。
    pub id: String,
    /// 给人看的名字。
    pub label: String,
    /// 是否为系统当前的默认输出。
    pub is_default: bool,
}

/// 列举输出端点。
///
/// 与 [`OutputBackend`] 分开建模，因为它是**查询**而不是命令：菜单弹开时要立刻有答案，
/// 而 `OutputBackend` 归引擎线程独占，从外面问它就得为一次只读查询搭一套请求/回执。
/// 枚举既不打开设备也不碰播放状态，没有理由排进那条串行命令队列。
///
/// 分开还有第二层好处：将来平台原生后端接进来时，「有哪些端点」与「用哪个后端放」
/// 可以分别演进——多声道端点在列表里存在，但要等对应后端就位才可选。
pub trait DeviceEnumerator: Send + Sync {
    /// 列出当前可用的输出端点。**每次调用都重新问系统**：设备会插拔，缓存一份
    /// 只会让菜单显示已经拔掉的耳机。
    fn devices(&self) -> Result<Vec<DeviceInfo>>;
}

/// 回调与控制线程之间的共享状态。
///
/// 只放原子量：这些字段全部会被输出回调触碰，而回调内禁止锁、分配与 I/O
/// （架构约束不变量第 3 条）。
#[derive(Debug)]
pub struct OutputShared {
    /// 目标线性增益（f32 的位模式）。变化由回调做斜坡，不在此处突变。
    gain_bits: AtomicU32,
    /// 暂停标志。暂停 = 斜坡到零后停止消费，但**继续向设备写零帧**，
    /// 输出流与设备时钟保持活跃；否则恢复播放要经历设备重启延迟，
    /// 基于消费帧数的位置时钟也会失去参照。
    paused: AtomicBool,
    /// **当前曲目**的位置基准（帧）。播放位置以它为准，不用 JavaScript 定时器
    /// （不变量第 6 条）；seek 与切歌时重置。
    position_frames: AtomicU64,
    /// 输出回调**累计**消费的帧数，跨曲目单调递增、永不重置。
    ///
    /// 与 `position_frames` 分开是必须的：位置每首要归零，而诊断要的是「一共送出去多少」。
    /// 合成一个字段的话，「这首播了多少帧」只能靠前后差值算，而差值会被归零抹平——
    /// 实测表现为歌单里第二首起每首都显示消费 0 帧。
    total_frames: AtomicU64,
    /// 欠载次数。原子累加，经 stats 暴露（验收条件第 5 条）。
    underruns: AtomicU64,
    /// 重缓冲中。seek 与切歌后缓冲被清空，此时的「取不到数据」是**预期**的，
    /// 计进欠载会让这项指标失去诊断价值——它要回答的是「实时性够不够」，
    /// 而不是「用户拖过几次进度条」。
    rebuffering: AtomicBool,
    /// 源数据已经全部写进环形缓冲，后续不会再生产样本。尾帧不足一个设备回调时
    /// 补零是正常收尾，不是解码线程跟不上；这个标志让欠载统计能区分两者。
    source_drained: AtomicBool,
    /// 设备输出延迟（帧）。播放位置 = 消费帧数 − 该值。
    output_delay_frames: AtomicU64,
    /// 输出回调正在搬运一块数据。控制线程看到 ring 已空时还必须等这次回调发布完
    /// 尾部时序，不能在它读下标刚推进、时间戳尚未写回的窗口里误报播完。
    callback_in_progress: AtomicBool,
    /// 最近一次回调从 ring 取走的有效音频帧数。最后一帧真正发声的时刻约为
    /// `回调时刻 + output_delay + 此帧数`，播完判定据此等待设备域排空。
    last_callback_audio_frames: AtomicU64,
    /// 当前链路是否插入了重采样。回调不读它，放这里只是因为它与输出配置同生命周期，
    /// 且要跨线程被 stats 查询。
    resampled: AtomicBool,
}

impl Default for OutputShared {
    fn default() -> Self {
        Self {
            gain_bits: AtomicU32::new(1.0f32.to_bits()),
            // 没有装载音源时天然是暂停态。后端 open 会立即启动回调，默认 false 会在
            // 控制线程来得及设状态之前把空缓冲误记成一次欠载。
            paused: AtomicBool::new(true),
            position_frames: AtomicU64::new(0),
            total_frames: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
            rebuffering: AtomicBool::new(false),
            source_drained: AtomicBool::new(false),
            output_delay_frames: AtomicU64::new(0),
            callback_in_progress: AtomicBool::new(false),
            last_callback_audio_frames: AtomicU64::new(0),
            resampled: AtomicBool::new(false),
        }
    }
}

impl OutputShared {
    pub fn set_gain(&self, gain: f32) {
        self.gain_bits
            .store(gain.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn gain(&self) -> f32 {
        f32::from_bits(self.gain_bits.load(Ordering::Relaxed))
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    /// 当前曲目的位置（帧，未扣设备延迟）。
    pub fn position_frames(&self) -> u64 {
        self.position_frames.load(Ordering::Relaxed)
    }

    /// 跨曲目累计消费的帧数，单调递增。
    pub fn total_frames(&self) -> u64 {
        self.total_frames.load(Ordering::Relaxed)
    }

    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    pub fn set_rebuffering(&self, value: bool) {
        self.rebuffering.store(value, Ordering::Relaxed);
    }

    pub fn is_rebuffering(&self) -> bool {
        self.rebuffering.load(Ordering::Relaxed)
    }

    pub fn set_source_drained(&self, value: bool) {
        self.source_drained.store(value, Ordering::Relaxed);
    }

    pub fn is_source_drained(&self) -> bool {
        self.source_drained.load(Ordering::Relaxed)
    }

    pub fn set_resampled(&self, value: bool) {
        self.resampled.store(value, Ordering::Relaxed);
    }

    pub fn is_resampled(&self) -> bool {
        self.resampled.load(Ordering::Relaxed)
    }

    pub fn output_delay_frames(&self) -> u64 {
        self.output_delay_frames.load(Ordering::Relaxed)
    }

    /// 输出后端在一次设备回调开始时调用。只有原子写，符合实时线程纪律。
    pub fn begin_callback(&self) {
        self.callback_in_progress.store(true, Ordering::Release);
    }

    /// 输出后端在一次设备回调结束前调用，并发布这次实际取走的有效音频帧数。
    pub fn finish_callback(&self, audio_frames: usize) {
        self.last_callback_audio_frames
            .store(audio_frames as u64, Ordering::Relaxed);
        self.callback_in_progress.store(false, Ordering::Release);
    }

    pub fn callback_in_progress(&self) -> bool {
        self.callback_in_progress.load(Ordering::Acquire)
    }

    pub fn last_callback_audio_frames(&self) -> u64 {
        self.last_callback_audio_frames.load(Ordering::Relaxed)
    }

    /// 更新设备报告的回调到发声延迟。输出后端从设备时间戳换算后写入。
    pub fn set_output_delay_frames(&self, frames: u64) {
        self.output_delay_frames.store(frames, Ordering::Relaxed);
    }

    /// 换流时清掉上一台设备留下的回调时序，避免新曲沿用旧延迟。
    pub fn reset_callback_timing(&self) {
        self.output_delay_frames.store(0, Ordering::Relaxed);
        self.last_callback_audio_frames.store(0, Ordering::Relaxed);
        self.callback_in_progress.store(false, Ordering::Release);
    }

    /// 已发声的位置（帧）。扣除设备延迟——共享模式的输出延迟普遍达数十毫秒，
    /// 不补偿会让歌词逐字高亮系统性偏早。
    pub fn played_frames(&self) -> u64 {
        self.position_frames()
            .saturating_sub(self.output_delay_frames())
    }

    /// 重置位置（seek 与切歌后调用）。**只动位置，不动累计量**。
    pub fn reset_position(&self, frames: u64) {
        self.position_frames.store(frames, Ordering::Relaxed);
    }
}

/// 输出后端。
pub trait OutputBackend: Send {
    /// 后端名（诊断用）。
    fn name(&self) -> &'static str;

    /// **预演协商**：只回答「这个请求会落到什么配置上」，不碰设备、不建流。
    ///
    /// 分成两步是因为链路后半段（环形缓冲容量、重采样比率、位置计数的时基）都要
    /// 按**协商结果**而不是请求来搭；先 open 再回头调整意味着缓冲刚建好就得推倒重建。
    fn negotiate(&self, request: &OutputRequest) -> Result<OutputConfig>;

    /// 指定下次打开时使用的端点；`None` = 跟随系统默认。
    ///
    /// **只记偏好，不碰设备**。换端点要重新协商采样率、按新时基重建环形缓冲与重采样器、
    /// 再把播放位置接回去，这一整套是引擎的状态机（架构约束不变量第 8 条要求它是显式的）；
    /// 后端在这里擅自重开流就绕过了它，表现为位置错乱或静默换成另一条质量不同的路径。
    ///
    /// 没有默认实现是故意的：一个忽略该方法的后端会让用户在菜单里选好了设备、
    /// 声音却仍从原来那台出来，而且不报任何错。
    fn set_preferred_device(&mut self, id: Option<String>);

    /// 按请求协商并打开输出流。返回实际生效的配置，须与 [`negotiate`](Self::negotiate) 一致。
    ///
    /// 打开后流处于**已启动**状态：暂停靠 [`OutputShared::set_paused`] 表达，
    /// 而不是拆流（见该字段的说明）。
    fn open(
        &mut self,
        request: &OutputRequest,
        consumer: RingConsumer,
        shared: std::sync::Arc<OutputShared>,
    ) -> Result<OutputConfig>;

    /// 取出一个输出流运行期错误（设备断开、被独占等）。默认后端没有异步错误源。
    /// 控制线程每轮轮询并把它转换成 `EngineEvent::Error`，错误回调本身不做重活。
    fn take_error(&mut self) -> Option<EngineError> {
        None
    }

    /// 关闭输出流并释放设备。
    fn close(&mut self);

    /// 当前生效的配置；未打开时为 `None`。
    fn config(&self) -> Option<&OutputConfig>;
}

/// 音量斜坡的时长。变化不做斜坡会产生可闻爆音，斜坡行为纳入验收。
pub const GAIN_RAMP_MS: f32 = 15.0;

/// 一次填充的结果。
///
/// `starved` 是**报告**而不是记账：一次回调可能被拆成多块填充，谁都不该在块这一层
/// 往计数器上加——那样同一次掉音会随设备缓冲大小与 scratch 大小得出不同的数字。
/// 由 [`render_output_callback`] 汇总后每次回调至多记一次。
pub struct FillOutcome {
    /// 本次真正来自缓冲的帧数（补的零不算）。
    pub frames: usize,
    /// 这一块没被填满，且当时确实处于「本该有数据」的状态。
    pub starved: bool,
}

/// 回调内的公共填充逻辑：斜坡增益 + 消费环形缓冲 + 欠载补零。
///
/// **不对外公开**：唯一该被后端调用的是 [`render_output_callback`]。把这一层暴露出去，
/// 后端就会各自在外面再包一圈打点、分块与样本转换，那一圈同样跑在实时线程上却不受
/// 任何测试保护——三个测试后端曾经就是这么各写了一遍。
///
/// `current_gain` 由调用方在回调之间保持（回调是唯一的读写者，不需要原子）。
#[inline]
fn fill_from_ring(
    out: &mut [f32],
    channels: usize,
    consumer: &mut RingConsumer,
    shared: &OutputShared,
    current_gain: &mut f32,
    ramp_step: f32,
) -> FillOutcome {
    // 无条件处理 flush 与截断：暂停期间不消费数据，但那两条协议的回执不能因此卡住。
    consumer.poll_control();

    let target = if shared.is_paused() {
        0.0
    } else {
        shared.gain()
    };

    // 已经静音且处于暂停：只写零帧维持设备时钟，不推进位置。
    if shared.is_paused() && *current_gain <= f32::EPSILON {
        *current_gain = 0.0;
        out.fill(0.0);
        return FillOutcome {
            frames: 0,
            starved: false,
        };
    }

    let outcome = consumer.read(out);
    let got = outcome.samples;
    let mut starved = false;
    if got < out.len() {
        out[got..].fill(0.0);
        // 只有正在播放、且不处于重缓冲时的填不满才算欠载；
        // 暂停、播完与 seek 后的重填都是正常状态。
        starved = !shared.is_paused() && !shared.is_rebuffering() && !shared.is_source_drained();
    }

    // 逐帧向目标增益靠拢，避免音量突变与暂停/恢复的爆音。
    let frames = out.len() / channels;
    let mut g = *current_gain;
    for f in 0..frames {
        if g < target {
            g = (g + ramp_step).min(target);
        } else if g > target {
            g = (g - ramp_step).max(target);
        }
        let base = f * channels;
        for s in &mut out[base..base + channels] {
            *s *= g;
        }
    }
    *current_gain = g;

    let frames = (got / channels) as u64;
    match outcome.crossed {
        // 越过了曲目边界：位置不再是累加而是**改写**——新曲从它自己的基准起算。
        // 缓冲里两首歌的 PCM 之间没有任何分隔，只有这里知道读下标跨了过去。
        Some(crossing) => shared.position_frames.store(
            crossing.position_base + crossing.frames_after,
            Ordering::Relaxed,
        ),
        None => {
            shared.position_frames.fetch_add(frames, Ordering::Relaxed);
        }
    }
    // 累计量跨曲目单调递增，边界处照加不误：它回答的是「一共送出去多少帧」。
    shared.total_frames.fetch_add(frames, Ordering::Relaxed);
    FillOutcome {
        frames: frames as usize,
        starved,
    }
}

/// 按采样率算出每帧的增益步进。
pub fn ramp_step_for(sample_rate: u32) -> f32 {
    1.0 / (sample_rate as f32 * GAIN_RAMP_MS / 1000.0).max(1.0)
}

/// 一次输出回调里**我们自己写的那一整段**：打点、设备延迟、分块填充、样本格式转换。
///
/// 从 CPAL 的闭包里提出来，是为了让它能被直接调用——[`fill_from_ring`] 只是其中的填充
/// 核心，把它的边界当成整个回调会漏掉外面这一圈（时间戳换算、分块、`from_sample` 转换），
/// 而那一圈同样跑在实时线程上。测试测的必须是这个生产函数本身，复制一份近似实现去测，
/// 测的就是那份复制品。真实 CPAL 闭包因此只剩两件事：把设备缓冲和时间戳递进来。
///
/// `scratch` 由调用方在**建流时**预分配（回调内禁止分配）；回调请求超过它时分块处理，
/// 所以它的大小只影响拷贝次数，不影响正确性。
///
/// `delay_frames` 是取设备延迟的闭包，按泛型传入而不是 `Box<dyn Fn>`：装箱要在建流时
/// 分配一次，更要紧的是它会把「回调里能调什么」这件事藏进一个动态类型背后。它在
/// `begin_callback` **之后**调用——打点必须先于读时间戳，控制线程据此判断这次回调是否
/// 已经开始搬运数据。
///
/// 样本转换沿用 `cpal::FromSample`，不另立一个 trait：后端支持 f32 / i32 / i16 / u16 / u8
/// 五种格式，自己实现一遍等于维护一份可能与 cpal 不一致的近似转换，而 `shannon-audio`
/// 本来就无条件依赖 cpal，换个 trait 省不掉这个依赖。
#[inline]
pub fn render_output_callback<T, F>(
    out: &mut [T],
    state: &mut CallbackState,
    consumer: &mut RingConsumer,
    shared: &OutputShared,
    delay_frames: F,
) where
    T: Copy + cpal::FromSample<f32>,
    F: FnOnce() -> u64,
{
    shared.begin_callback();
    // 设备延迟：播放时刻与回调时刻之差。播放位置要扣掉它，
    // 否则歌词逐字高亮会系统性偏早（共享模式延迟普遍数十毫秒）。
    shared.set_output_delay_frames(delay_frames());

    let chunk_samples = state.scratch.len();
    let mut written = 0;
    let mut audio_frames = 0usize;
    let mut starved = false;
    while written < out.len() {
        let n = (out.len() - written).min(chunk_samples);
        let block = &mut state.scratch[..n];
        let filled = fill_from_ring(
            block,
            state.channels,
            consumer,
            shared,
            &mut state.gain,
            state.ramp_step,
        );
        audio_frames += filled.frames;
        starved |= filled.starved;
        for (dst, src) in out[written..written + n].iter_mut().zip(block.iter()) {
            *dst = T::from_sample_(*src);
        }
        written += n;
    }

    // **每次回调至多记一次欠载。** 分块只是拷贝策略，设备一次要多少样本、scratch 有多大
    // 都不该改变「掉了几次音」这个数字；按块记会让同一次掉音在缓冲更大的设备上被报成
    // 好几次，也会让调小 scratch 凭空抬高这个指标。它要回答的是「实时性够不够」，
    // 那个问题的单位就是回调。
    if starved {
        shared.underruns.fetch_add(1, Ordering::Relaxed);
    }
    shared.finish_callback(audio_frames);
}

/// 一条输出流在回调之间保持的状态。
///
/// 单独立成一个类型，是因为这几项恰好共享同一条性质：**建流时就得备好，回调里一个都不能
/// 现造**。散成参数时它只是「函数签名有点长」，聚起来才看得出它就是被验证的那条不变量
/// （见 `tests/realtime_discipline.rs`）——往回调里加东西时，该问的是「它能不能放进这里」。
pub struct CallbackState {
    /// 分块用的中间缓冲。设备一次要的样本可能超过它，那时分块处理，
    /// 所以它的大小只影响拷贝次数，不影响正确性。
    scratch: Vec<f32>,
    channels: usize,
    ramp_step: f32,
    /// 当前增益。回调是它唯一的读写者，因此不必是原子量。
    gain: f32,
}

impl CallbackState {
    /// `scratch_frames` 是分块上限，按后端自己的取值给。
    pub fn new(channels: usize, sample_rate: u32, scratch_frames: usize) -> Self {
        assert!(channels > 0, "输出回调的声道数必须大于 0");
        assert!(scratch_frames > 0, "输出回调的 scratch 帧数必须大于 0");
        let scratch_samples = scratch_frames
            .checked_mul(channels)
            .expect("输出回调的 scratch 样本容量溢出");
        Self {
            scratch: vec![0.0; scratch_samples],
            channels,
            ramp_step: ramp_step_for(sample_rate),
            // 从零起步：新流的第一个回调要把音量斜坡上来，直接给目标值会有爆音。
            gain: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_fill_reports_starvation_while_source_is_active() {
        let (mut producer, mut consumer) = crate::ring::ring(8, 2);
        producer.write(&[0.25, 0.25]);
        let shared = OutputShared::default();
        shared.set_paused(false);
        let mut out = [0.0; 8];
        let mut gain = 1.0;

        let filled = fill_from_ring(&mut out, 2, &mut consumer, &shared, &mut gain, 1.0);

        assert!(filled.starved, "部分补零同样是一次真实欠载");
        assert_eq!(
            shared.underruns(),
            0,
            "填充层只报告不记账——记账在回调那一层，否则分块会把一次掉音记成多次"
        );
    }

    #[test]
    fn partial_final_fill_is_not_starvation_after_source_drains() {
        let (mut producer, mut consumer) = crate::ring::ring(8, 2);
        producer.write(&[0.25, 0.25]);
        let shared = OutputShared::default();
        shared.set_source_drained(true);
        shared.set_paused(false);
        let mut out = [0.0; 8];
        let mut gain = 1.0;

        let filled = fill_from_ring(&mut out, 2, &mut consumer, &shared, &mut gain, 1.0);

        assert!(!filled.starved, "自然收尾的不足帧不是实时欠载");
    }

    #[test]
    #[should_panic(expected = "声道数必须大于 0")]
    fn callback_state_rejects_zero_channels() {
        let _ = CallbackState::new(0, 48_000, 8192);
    }

    #[test]
    #[should_panic(expected = "scratch 帧数必须大于 0")]
    fn callback_state_rejects_an_empty_scratch() {
        let _ = CallbackState::new(2, 48_000, 0);
    }
}
