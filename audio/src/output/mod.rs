//! 输出后端抽象。
//!
//! trait 首版即定，后续的 WASAPI 独占、macOS `AVSampleBufferAudioRenderer`、
//! Windows `ISpatialAudioClient` 都以新实现插入，不改引擎。
//!
//! **接口必须能表达声道布局，而不只是声道数**——这正是 CPAL 不能承担空间输出的原因：
//! 它的配置里只有一个数字，而布局标签才是系统判断能否空间化的依据（见
//! `docs/MACOS_SPATIAL_PLAYBACK_NOTES.md`）。所以协商的输入输出都用
//! [`ChannelLayout`](crate::layout::ChannelLayout)，哪怕 CPAL 实现自己只用得上其中的声道数。

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::error::Result;
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
    /// 累计消费帧数。播放位置以它为准，不用 JavaScript 定时器（不变量第 6 条）。
    frames_consumed: AtomicU64,
    /// 欠载次数。原子累加，经 stats 暴露（验收条件第 5 条）。
    underruns: AtomicU64,
    /// 重缓冲中。seek 与切歌后缓冲被清空，此时的「取不到数据」是**预期**的，
    /// 计进欠载会让这项指标失去诊断价值——它要回答的是「实时性够不够」，
    /// 而不是「用户拖过几次进度条」。
    rebuffering: AtomicBool,
    /// 设备输出延迟（帧）。播放位置 = 消费帧数 − 该值。
    output_delay_frames: AtomicU64,
}

impl Default for OutputShared {
    fn default() -> Self {
        Self {
            gain_bits: AtomicU32::new(1.0f32.to_bits()),
            paused: AtomicBool::new(false),
            frames_consumed: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
            rebuffering: AtomicBool::new(false),
            output_delay_frames: AtomicU64::new(0),
        }
    }
}

impl OutputShared {
    pub fn set_gain(&self, gain: f32) {
        self.gain_bits.store(gain.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
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

    pub fn frames_consumed(&self) -> u64 {
        self.frames_consumed.load(Ordering::Relaxed)
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

    pub fn output_delay_frames(&self) -> u64 {
        self.output_delay_frames.load(Ordering::Relaxed)
    }

    /// 已发声的位置（帧）。扣除设备延迟——共享模式的输出延迟普遍达数十毫秒，
    /// 不补偿会让歌词逐字高亮系统性偏早。
    pub fn played_frames(&self) -> u64 {
        self.frames_consumed().saturating_sub(self.output_delay_frames())
    }

    /// 重置位置计数（seek 与切歌后调用）。
    pub fn reset_position(&self, frames: u64) {
        self.frames_consumed.store(frames, Ordering::Relaxed);
    }
}

/// 输出后端。
pub trait OutputBackend: Send {
    /// 后端名（诊断用）。
    fn name(&self) -> &'static str;

    /// 按请求协商并打开输出流。返回实际生效的配置。
    ///
    /// 打开后流处于**已启动**状态：暂停靠 [`OutputShared::set_paused`] 表达，
    /// 而不是拆流（见该字段的说明）。
    fn open(
        &mut self,
        request: &OutputRequest,
        consumer: RingConsumer,
        shared: std::sync::Arc<OutputShared>,
    ) -> Result<OutputConfig>;

    /// 关闭输出流并释放设备。
    fn close(&mut self);

    /// 当前生效的配置；未打开时为 `None`。
    fn config(&self) -> Option<&OutputConfig>;
}

/// 音量斜坡的时长。变化不做斜坡会产生可闻爆音，斜坡行为纳入验收。
pub const GAIN_RAMP_MS: f32 = 15.0;

/// 回调内的公共填充逻辑：斜坡增益 + 消费环形缓冲 + 欠载补零与计数。
///
/// 抽出来是因为每个输出后端都要重复这段，而它是**唯一**允许在实时线程里跑的代码路径，
/// 分散实现意味着实时纪律要在每处重新审一遍。
///
/// `current_gain` 由调用方在回调之间保持（回调是唯一的读写者，不需要原子）。
#[inline]
pub fn fill_from_ring(
    out: &mut [f32],
    channels: usize,
    consumer: &mut RingConsumer,
    shared: &OutputShared,
    current_gain: &mut f32,
    ramp_step: f32,
) {
    // 无条件处理 flush：暂停期间不消费数据，但 seek 的回执不能因此卡住。
    consumer.poll_flush();

    let target = if shared.is_paused() { 0.0 } else { shared.gain() };

    // 已经静音且处于暂停：只写零帧维持设备时钟，不推进位置。
    if shared.is_paused() && *current_gain <= f32::EPSILON {
        *current_gain = 0.0;
        out.fill(0.0);
        return;
    }

    let got = consumer.read(out);
    if got < out.len() {
        out[got..].fill(0.0);
        // 只有正在播放、且不处于重缓冲时的填不满才算欠载；
        // 暂停、播完与 seek 后的重填都是正常状态。
        if !shared.is_paused() && !shared.is_rebuffering() && got == 0 {
            shared.underruns.fetch_add(1, Ordering::Relaxed);
        }
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

    shared.frames_consumed.fetch_add((got / channels) as u64, Ordering::Relaxed);
}

/// 按采样率算出每帧的增益步进。
pub fn ramp_step_for(sample_rate: u32) -> f32 {
    1.0 / (sample_rate as f32 * GAIN_RAMP_MS / 1000.0).max(1.0)
}
