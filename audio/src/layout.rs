//! 播放期的声道布局。
//!
//! **为什么不复用 `shannon-core` 的 `ChannelLayout`**：那个是给界面看的**具名投影**
//! （「5.1」是一个需要 i18n 的词），回答的是「这张专辑是什么规格」；这里要回答的是
//! 「缓冲区第 k 个样本是哪只音箱的」——下混系数、声道重排都按位掩码算，具名与否无关。
//! 两者职责不同，硬合并会让播放链去依赖曲库层。
//!
//! 沿用曲库侧同一条戒律：**位掩码是权威**，判不出一律留空，不用声道数硬猜。
//! 区别在于播放期不能「留空就算了」——总得往设备里写点什么，所以多了
//! [`LayoutSource`]：猜可以，但必须留下「这是猜的」的痕迹，让下混与诊断能分辨。

use symphonia::core::audio::{Channels, Position};

/// 布局的判定依据。
///
/// 存在的理由与曲库侧的 `FieldSource` 相同：兜底推断一定会有猜错的时候，
/// 下游（下混系数、能力协商、诊断输出）必须能区分「码流里写明的」与「按声道数猜的」。
/// 事后补录代价大——一旦 PCM 进了环形缓冲就再没有源头信息了——所以自解码起点即进类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutSource {
    /// 容器或码流给出了明确的声道位置。
    Explicit,
    /// 只知道声道数，按惯例推断（1 → 单声道，2 → 立体声）。
    InferredFromCount,
    /// 连推断都做不到，只当作 n 条无位置的离散声道。
    Unknown,
}

/// 声道布局：掩码 + 声道数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelLayout {
    count: u16,
    /// 扬声器位置掩码，`None` 表示摆位未知（离散声道）。
    mask: Option<u64>,
    source: LayoutSource,
}

impl ChannelLayout {
    pub const MONO: Self =
        Self { count: 1, mask: Some(Position::FRONT_CENTER.bits()), source: LayoutSource::Explicit };

    pub const STEREO: Self = Self {
        count: 2,
        mask: Some(Position::FRONT_LEFT.bits() | Position::FRONT_RIGHT.bits()),
        source: LayoutSource::Explicit,
    };

    /// 摆位未知的 n 条离散声道。
    pub fn discrete(count: u16) -> Self {
        Self { count, mask: None, source: LayoutSource::Unknown }
    }

    /// 从 Symphonia 的声道集转换。
    ///
    /// `Discrete` 分支是本函数唯一做推断的地方：1 / 2 声道按惯例认作单声道 / 立体声，
    /// 标记为 [`LayoutSource::InferredFromCount`]。3 声道以上不猜——6 声道可能是 5.1
    /// 也可能是 6.0，摆位不同下混系数就不同，猜错比不猜更糟。
    pub fn from_symphonia(channels: &Channels) -> Self {
        match channels {
            Channels::Positioned(pos) => Self {
                count: pos.bits().count_ones() as u16,
                mask: Some(pos.bits()),
                source: LayoutSource::Explicit,
            },
            Channels::Discrete(count) => match count {
                1 => Self { source: LayoutSource::InferredFromCount, ..Self::MONO },
                2 => Self { source: LayoutSource::InferredFromCount, ..Self::STEREO },
                _ => Self::discrete(*count),
            },
            // Ambisonic 与 Custom 都不是扬声器位置模型，掩码无从表达。
            other => Self::discrete(other.count() as u16),
        }
    }

    pub fn count(&self) -> u16 {
        self.count
    }

    pub fn mask(&self) -> Option<u64> {
        self.mask
    }

    pub fn source(&self) -> LayoutSource {
        self.source
    }

    /// 是否为常规立体声（左 + 右）。
    ///
    /// 判据是掩码而非声道数：2 声道也可能是 Lt/Rt 之外的其它组合，
    /// 而掩码未知的 2 声道由 [`from_symphonia`](Self::from_symphonia) 归到立体声时
    /// 已经打上了 `InferredFromCount`，此处不再重复推断。
    pub fn is_stereo(&self) -> bool {
        self.mask == ChannelLayout::STEREO.mask
    }

    /// 是否为单声道。
    ///
    /// 判据是**声道数**而非掩码位置：实测 Symphonia 的 WAV 读取器把单声道标为
    /// `FRONT_LEFT`（0x1）而非 `FRONT_CENTER`（0x4），按掩码比对会把它判成非单声道，
    /// 进而拒播一个本该最简单的文件。这也符合语义——只有一条声道时它就是全部内容，
    /// 标在哪个位置都不改变「复制到左右两路」这个唯一无歧义的处理。
    pub fn is_mono(&self) -> bool {
        self.count == 1
    }

    /// 给诊断与错误信息用的简短描述。不做 i18n——这是日志，不是界面。
    pub fn describe(&self) -> String {
        if self.is_mono() {
            return "mono".into();
        }
        if self.is_stereo() {
            return "stereo".into();
        }
        match self.mask {
            Some(mask) => format!("{} ch (mask 0x{mask:x})", self.count),
            None => format!("{} ch (摆位未知)", self.count),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positioned_layout_keeps_mask() {
        let ch = Channels::Positioned(Position::FRONT_LEFT | Position::FRONT_RIGHT);
        let layout = ChannelLayout::from_symphonia(&ch);
        assert!(layout.is_stereo());
        assert_eq!(layout.count(), 2);
        assert_eq!(layout.source(), LayoutSource::Explicit);
    }

    #[test]
    fn discrete_stereo_is_inferred_not_explicit() {
        let layout = ChannelLayout::from_symphonia(&Channels::Discrete(2));
        assert!(layout.is_stereo());
        // 摆位是猜的，来源必须如实标记，否则下游无从分辨。
        assert_eq!(layout.source(), LayoutSource::InferredFromCount);
    }

    #[test]
    fn discrete_multichannel_is_not_guessed() {
        // 6 声道可能是 5.1 也可能是 6.0，摆位不同下混系数就不同——不猜。
        let layout = ChannelLayout::from_symphonia(&Channels::Discrete(6));
        assert_eq!(layout.count(), 6);
        assert_eq!(layout.mask(), None);
        assert_eq!(layout.source(), LayoutSource::Unknown);
    }

    #[test]
    fn mono_is_recognised_regardless_of_marked_position() {
        // 实测 Symphonia 的 WAV 读取器把单声道标成 FRONT_LEFT 而非 FRONT_CENTER，
        // 按掩码比对会把它判成非单声道，进而拒播——判据必须是声道数。
        let as_left = ChannelLayout::from_symphonia(&Channels::Positioned(Position::FRONT_LEFT));
        let as_center = ChannelLayout::from_symphonia(&Channels::Positioned(Position::FRONT_CENTER));
        assert!(as_left.is_mono());
        assert!(as_center.is_mono());
    }

    #[test]
    fn surround_layout_is_not_mistaken_for_stereo() {
        let ch = Channels::Positioned(
            Position::FRONT_LEFT
                | Position::FRONT_RIGHT
                | Position::FRONT_CENTER
                | Position::LFE1
                | Position::REAR_LEFT
                | Position::REAR_RIGHT,
        );
        let layout = ChannelLayout::from_symphonia(&ch);
        assert_eq!(layout.count(), 6);
        assert!(!layout.is_stereo());
    }
}
