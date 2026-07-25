//! 声道掩码 → 具名布局。
//!
//! 掩码是权威，本模块只做「投影」：能对上常见布局就给具名结果，对不上就返回
//! `Other { mask }`。**绝不由声道数反推布局**——6 声道既可能是 5.1 也可能是 6.0，
//! 摆位不同下混系数就不同，猜错比不猜更糟。

use crate::model::ChannelLayout;

// FFmpeg 口径的扬声器位置位（symphonia 的 Channels 与之一致）。
pub const FRONT_LEFT: u32 = 1 << 0;
pub const FRONT_RIGHT: u32 = 1 << 1;
pub const FRONT_CENTRE: u32 = 1 << 2;
pub const LFE1: u32 = 1 << 3;
pub const REAR_LEFT: u32 = 1 << 4;
pub const REAR_RIGHT: u32 = 1 << 5;
pub const SIDE_LEFT: u32 = 1 << 9;
pub const SIDE_RIGHT: u32 = 1 << 10;
pub const TOP_FRONT_LEFT: u32 = 1 << 12;
pub const TOP_FRONT_RIGHT: u32 = 1 << 14;
pub const TOP_REAR_LEFT: u32 = 1 << 16;
pub const TOP_REAR_RIGHT: u32 = 1 << 18;

const HEIGHT_BITS: u32 = TOP_FRONT_LEFT | TOP_FRONT_RIGHT | TOP_REAR_LEFT | TOP_REAR_RIGHT;
const LFE_BITS: u32 = LFE1;

/// 由掩码判定具名布局。掩码缺失时返回 `None`（不猜）。
pub fn layout_from_mask(mask: u32) -> ChannelLayout {
    // 只有一个扬声器位 = 单声道，无论落在哪一位上。
    // （FFmpeg 的 mono 用 FRONT_CENTRE，而 WAVE_FORMAT_EXTENSIBLE 常写 FRONT_LEFT。）
    if mask.count_ones() == 1 {
        return ChannelLayout::Mono;
    }
    match mask {
        m if m == FRONT_LEFT | FRONT_RIGHT => return ChannelLayout::Stereo,
        m if m == FRONT_LEFT | FRONT_RIGHT | REAR_LEFT | REAR_RIGHT => {
            return ChannelLayout::Quad
        }
        _ => {}
    }

    let height = (mask & HEIGHT_BITS).count_ones() as u8;
    let lfe = (mask & LFE_BITS).count_ones() as u8;
    let main = (mask & !HEIGHT_BITS & !LFE_BITS).count_ones() as u8;

    // 只在主声道数落在已知环绕配置时才给具名布局。
    if matches!(main, 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 12) {
        return ChannelLayout::Surround { main, lfe, height };
    }
    ChannelLayout::Other { mask }
}

/// Ambisonics 判定：声道数为 (n+1)² 且没有具名扬声器掩码时的候选解释。
///
/// 仅在容器/标签另有 Ambisonics 线索时才该采用——单看声道数会把 4 声道的
/// quad 误判成一阶 Ambisonics，所以这里只提供「阶数换算」，不做判定。
pub fn ambisonic_order(channels: u8) -> Option<u8> {
    let n = channels as u32;
    let order = (n as f64).sqrt().round() as u32;
    if order * order == n && order >= 1 {
        Some((order - 1) as u8)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_and_stereo() {
        assert_eq!(layout_from_mask(FRONT_CENTRE), ChannelLayout::Mono);
        // WAV 的单声道掩码常是 FRONT_LEFT，也必须判为 Mono
        assert_eq!(layout_from_mask(FRONT_LEFT), ChannelLayout::Mono);
        assert_eq!(layout_from_mask(FRONT_LEFT | FRONT_RIGHT), ChannelLayout::Stereo);
    }

    #[test]
    fn quad_vs_five_one() {
        let quad = FRONT_LEFT | FRONT_RIGHT | REAR_LEFT | REAR_RIGHT;
        assert_eq!(layout_from_mask(quad), ChannelLayout::Quad);

        let five_one =
            FRONT_LEFT | FRONT_RIGHT | FRONT_CENTRE | LFE1 | REAR_LEFT | REAR_RIGHT;
        assert_eq!(
            layout_from_mask(five_one),
            ChannelLayout::Surround { main: 5, lfe: 1, height: 0 }
        );
    }

    /// 同为 6 声道，5.1 与 6.0 必须区分开——这正是「不能只看声道数」的核心案例。
    #[test]
    fn six_channels_five_one_vs_six_zero() {
        let five_one =
            FRONT_LEFT | FRONT_RIGHT | FRONT_CENTRE | LFE1 | REAR_LEFT | REAR_RIGHT;
        let six_zero = FRONT_LEFT
            | FRONT_RIGHT
            | FRONT_CENTRE
            | REAR_LEFT
            | REAR_RIGHT
            | SIDE_LEFT;
        assert_eq!(five_one.count_ones(), six_zero.count_ones());
        assert_ne!(layout_from_mask(five_one), layout_from_mask(six_zero));
        assert_eq!(
            layout_from_mask(six_zero),
            ChannelLayout::Surround { main: 6, lfe: 0, height: 0 }
        );
    }

    #[test]
    fn seven_one_four_has_height() {
        let mask = FRONT_LEFT
            | FRONT_RIGHT
            | FRONT_CENTRE
            | LFE1
            | REAR_LEFT
            | REAR_RIGHT
            | SIDE_LEFT
            | SIDE_RIGHT
            | TOP_FRONT_LEFT
            | TOP_FRONT_RIGHT
            | TOP_REAR_LEFT
            | TOP_REAR_RIGHT;
        assert_eq!(
            layout_from_mask(mask),
            ChannelLayout::Surround { main: 7, lfe: 1, height: 4 }
        );
    }

    #[test]
    fn ambisonic_order_math() {
        assert_eq!(ambisonic_order(4), Some(1));
        assert_eq!(ambisonic_order(9), Some(2));
        assert_eq!(ambisonic_order(16), Some(3));
        assert_eq!(ambisonic_order(6), None);
    }
}
