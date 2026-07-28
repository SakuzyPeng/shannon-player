//! 源声道到输出声道的适配。
//!
//! 作用域仅限**立体声输出路径**，能做的只有两件无歧义的事：原样直通，
//! 以及单声道复制成双声道。
//!
//! ## 下混不在这里，也不在别处——它是系统的职责
//!
//! 多声道到立体声的下混**应用不自己做**（架构约束「输出后端」与「明确不采用」）。
//! 这与「不自己双耳化」是同一条理由的两种表现：自行下混等于把一条本可以被系统空间化的
//! 多声道流提前拍扁，系统看到的只剩两条声道，空间音频开关会显示「不可用」，
//! 头部追踪更无从谈起——**渲染发生在头动之前**。
//!
//! 正确的做法是把布局如实交给平台原生输出后端（macOS `AVSampleBufferAudioRenderer`
//! 附 `AudioChannelLayoutTag`，Windows `ISpatialAudioClient`），由系统决定空间化还是下混。
//! 系统掌握端点特性（耳机 / 扬声器 / HDMI 各不相同），而应用只能猜一套通用系数；
//! 而且用户在系统播放器里听到的下混结果，在这里应当是同一个。
//!
//! 所以本模块遇到多声道时返回的是**路由错误而非能力缺口**：它要走另一条后端，
//! 不是等我们把某个算法补上。

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
    /// 选出适配方案；做不到就报明确的错误，不静默丢声道、也不自行下混。
    pub fn plan(src: ChannelLayout, dst: ChannelLayout) -> Result<Self> {
        // 声道数相同不代表布局相同：两声道也可能是 FC + LFE，六声道既可能是
        // 5.1 也可能是 6.0。只有数量与权威掩码都一致时才可原样搬运。
        if src.count() == dst.count() && src.mask() == dst.mask() {
            return Ok(ChannelAdapt::Passthrough);
        }
        if src.is_mono() && dst.is_stereo() {
            return Ok(ChannelAdapt::MonoToStereo);
        }
        Err(EngineError::new(
            Stage::Output,
            ErrorKind::Unsupported,
            format!(
                "{} 无法安全映射到 {}，需要交由系统输出；平台原生输出后端尚未接入",
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
    fn multichannel_is_routed_to_the_platform_backend_not_downmixed() {
        // 多声道不在立体声路径里解决：自行下混会把本可被系统空间化的流提前拍扁，
        // 是用户可见的能力降级。这里要的是明确的路由错误。
        let err =
            ChannelAdapt::plan(ChannelLayout::discrete(6), ChannelLayout::STEREO).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Unsupported);
        assert!(
            err.message.contains("交由系统"),
            "错误要说清这是路由问题而非缺个算法：{}",
            err.message
        );
    }

    #[test]
    fn same_channel_count_with_different_positions_is_not_passthrough() {
        use symphonia::core::audio::{Channels, Position};

        let center_and_lfe = ChannelLayout::from_symphonia(&Channels::Positioned(
            Position::FRONT_CENTER | Position::LFE1,
        ));
        let err = ChannelAdapt::plan(center_and_lfe, ChannelLayout::STEREO).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Unsupported);
        assert!(
            err.message.contains("无法安全映射"),
            "同为两声道也不能只按数量直通：{}",
            err.message
        );
    }
}
