//! 源声道到输出声道的适配。
//!
//! 阶段 0 只覆盖立体声路径，能做的只有两件**无歧义**的事：原样直通，以及单声道复制成双声道。
//! 多声道到立体声的下混**刻意不在这里草率实现**——下混系数依赖布局（5.1 与 6.0 的
//! 6 声道系数完全不同），而布局判不出时（[`LayoutSource::Unknown`](crate::layout::LayoutSource)）
//! 任何系数都是猜的。猜错的表现是声场错乱或人声消失，用户听得出却无从归因。
//! 因此此处返回明确的能力错误，把下混留给阶段 1 连同布局置信度一起做。

use crate::error::{EngineError, ErrorKind, Result, Stage};
use crate::layout::ChannelLayout;

/// 声道适配方案。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelAdapt {
    /// 源与目标一致，直接搬运。
    Passthrough,
    /// 单声道复制到左右两路。无歧义，且是设备普遍不支持 1 声道输出时的必需品。
    MonoToStereo,
}

impl ChannelAdapt {
    /// 选出适配方案；做不到就报能力错误，不静默丢声道。
    pub fn plan(src: ChannelLayout, dst: ChannelLayout) -> Result<Self> {
        if src.count() == dst.count() {
            return Ok(ChannelAdapt::Passthrough);
        }
        if src.is_mono() && dst.is_stereo() {
            return Ok(ChannelAdapt::MonoToStereo);
        }
        Err(EngineError::new(
            Stage::Output,
            ErrorKind::Unsupported,
            format!(
                "暂不支持把 {} 适配到 {}（多声道下混尚未实现）",
                src.describe(),
                dst.describe()
            ),
        ))
    }

    /// 目标帧数对应的样本数（供缓冲区预分配）。
    pub fn out_samples(&self, in_frames: usize, dst_channels: usize) -> usize {
        match self {
            ChannelAdapt::Passthrough | ChannelAdapt::MonoToStereo => in_frames * dst_channels,
        }
    }

    /// 把 `src`（交错）适配写入 `dst`（交错）。`dst` 长度须为
    /// [`out_samples`](Self::out_samples) 的返回值。
    pub fn apply(&self, src: &[f32], dst: &mut [f32]) {
        match self {
            ChannelAdapt::Passthrough => dst.copy_from_slice(src),
            ChannelAdapt::MonoToStereo => {
                for (i, s) in src.iter().enumerate() {
                    dst[i * 2] = *s;
                    dst[i * 2 + 1] = *s;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_to_stereo_is_passthrough() {
        let plan = ChannelAdapt::plan(ChannelLayout::STEREO, ChannelLayout::STEREO).unwrap();
        assert_eq!(plan, ChannelAdapt::Passthrough);
    }

    #[test]
    fn mono_expands_to_both_channels() {
        let plan = ChannelAdapt::plan(ChannelLayout::MONO, ChannelLayout::STEREO).unwrap();
        let src = [0.5, -0.25];
        let mut dst = [0.0; 4];
        plan.apply(&src, &mut dst);
        assert_eq!(dst, [0.5, 0.5, -0.25, -0.25]);
    }

    #[test]
    fn multichannel_downmix_reports_capability_error() {
        // 阶段 0 不下混：宁可明确报错，也不按猜的系数把声场弄乱。
        let err = ChannelAdapt::plan(ChannelLayout::discrete(6), ChannelLayout::STEREO).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Unsupported);
    }
}
