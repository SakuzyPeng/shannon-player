//! 响度测量与增益计算（ReplayGain 2.0）。
//!
//! 设计与取舍见 `docs/AUDIO_BACKEND_IMPLEMENTATION_PLAN.md` 的「响度归一化」。
//! 本模块放两件事：把 PCM 喂给 EBU R128 测量器，以及由测量结果算出该施加的增益。
//! 测量器本身无 I/O、无状态共享，因此能被无头测试完整覆盖；结果的**存储**在
//! [`store`] 子模块，**分析的调度**与**增益施加到哪一级**都还不在这里。
//!
//! ## 为什么必须自己测，而不是读标签
//!
//! 实测本机曲库 954 首带 `REPLAYGAIN_TRACK_GAIN` 的 0 首、带 `R128_TRACK_GAIN` 的 0 首
//! ——该库 87.5% 是 ALAC，多经 Apple Music / iTunes 链路导入，那条链路从不写这些标签。
//! 只读标签的方案在这个库上覆盖率为零。标签因此只能是可选的加速路径。
//!
//! ## 目标是 -18 LUFS，不是广播的 -23
//!
//! -18 LUFS 来自面向音乐播放的 ReplayGain 2.0 规范；EBU R128 的 -23 LUFS 是广播交付
//! 用的，拿来放音乐会整体偏轻。-1 dBTP 则是本项目为后续重采样与设备转换留的安全余量。

pub mod store;

use std::path::Path;

use ebur128::{Channel, EbuR128, Mode};
use serde::{Deserialize, Serialize};

use crate::decode::Decoder;
use crate::error::{EngineError, ErrorKind, Result, Stage};
use crate::layout::ChannelLayout;

/// ReplayGain 2.0 的参考响度。
pub const TARGET_LUFS: f64 = -18.0;

/// 真峰值上限。增益再大也不许把峰值顶过这条线。
pub const TRUE_PEAK_CEILING_DBTP: f64 = -1.0;

/// 分析版本。
///
/// **任何会改变测量结果的因素变动都要 +1**：`ebur128` 的版本或 feature、`Mode` 组合、
/// 真峰值算法、声道映射规则。版本不符即视为没有结果、需要重测。
///
/// 只靠 `track_id`（内容哈希）复用是不够的——文件一个字节没变，换个测量器版本
/// 真峰值也可能变，上游对此有明确提醒。
pub const ANALYSIS_VERSION: u32 = 1;

/// 一次分析的结论。
///
/// 三种状态都是**确定**的，可以缓存；I/O 与解码失败属于瞬态错误，走 `Err` 而不进这里
/// ——把一次网络盘掉线写成永久结论，等于让那首歌再也不会被分析。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum LoudnessOutcome {
    Measured {
        integrated_lufs: f64,
        /// 各声道真峰值的最大者。全静音时为 `f64::NEG_INFINITY`。
        #[serde(with = "finite_or_null")]
        true_peak_dbtp: f64,
    },
    /// 测不出积分响度：全静音，或短于 R128 的第一个 400 ms 门限块。
    ///
    /// 实测 250 首真实曲库中有 10 首落进这一档，**全部是 Logic 采样库里的单周期波形与
    /// 打击乐短音**（一个波形周期只有几毫秒），239 首真实音乐无一不可测。
    Unmeasurable,
    /// 声道布局超出当前支持范围。阶段 1 只做单/立体声，多声道要等平台原生后端，
    /// 且必须给出经过验证的显式映射——**绝不按声道数猜**。
    UnsupportedLayout,
}

impl LoudnessOutcome {
    /// 该施加的增益（dB）。测不出或不支持时一律 0 dB——不归一化好过归错。
    pub fn gain_db(&self) -> f64 {
        match *self {
            Self::Measured {
                integrated_lufs,
                true_peak_dbtp,
            } => applied_gain_db(integrated_lufs, true_peak_dbtp),
            Self::Unmeasurable | Self::UnsupportedLayout => 0.0,
        }
    }

    /// 线性增益倍率，可直接乘到 PCM 上。
    pub fn linear_gain(&self) -> f32 {
        db_to_linear(self.gain_db())
    }
}

/// 由积分响度与真峰值算出该施加的增益。
///
/// **只有一个整曲常量增益，不上 limiter、不压缩动态范围**（`settings.loudnessDesc`
/// 对用户就是这么承诺的）。一首安静但峰值很高的曲目若无法在不削顶的前提下到达目标，
/// 就少加增益——「绝不削顶」优先于「一定到目标」。
pub fn applied_gain_db(integrated_lufs: f64, true_peak_dbtp: f64) -> f64 {
    let requested = TARGET_LUFS - integrated_lufs;
    if true_peak_dbtp.is_finite() {
        // 峰值保护只在需要**提升**时才可能咬住；衰减方向永远更安全。
        requested.min(TRUE_PEAK_CEILING_DBTP - true_peak_dbtp)
    } else {
        requested
    }
}

pub fn db_to_linear(db: f64) -> f32 {
    10f64.powf(db / 20.0) as f32
}

/// 流式响度测量器。喂交错 f32，收一个结论。
pub struct LoudnessAnalyzer {
    inner: EbuR128,
}

impl LoudnessAnalyzer {
    /// 按源布局建测量器；布局不支持时返回 `Ok(None)`。
    ///
    /// **必须显式 `set_channel_map`**：`EbuR128::new` 会按声道数给一套默认布局，
    /// 那与本项目「位掩码是权威、判不出就不猜」的建模冲突——默认值看起来总是合理的，
    /// 于是错误的加权会一路混进结果里而毫无征兆。
    pub fn new(layout: ChannelLayout, sample_rate: u32) -> Result<Option<Self>> {
        let channel_map: &[Channel] = if layout.is_mono() {
            // 播放管线会把单声道复制到左右两路，`DualMono` 正是按两只扬声器的
            // 实际能量计权；用单个 `Left` 会低估约 3 dB。
            &[Channel::DualMono]
        } else if layout.is_stereo() {
            &[Channel::Left, Channel::Right]
        } else {
            return Ok(None);
        };

        // 只开用得到的两项：momentary / short-term / LRA 一个都不读，不为它们付成本。
        let mut inner = EbuR128::new(
            channel_map.len() as u32,
            sample_rate,
            Mode::I | Mode::TRUE_PEAK,
        )
        .map_err(|e| analyze_err(format!("建不出响度测量器：{e}")))?;
        inner
            .set_channel_map(channel_map)
            .map_err(|e| analyze_err(format!("设不了响度声道映射：{e}")))?;
        Ok(Some(Self { inner }))
    }

    /// 喂一段交错 PCM。
    pub fn feed(&mut self, interleaved: &[f32]) -> Result<()> {
        if interleaved.is_empty() {
            return Ok(());
        }
        self.inner
            .add_frames_f32(interleaved)
            .map_err(|e| analyze_err(format!("响度测量失败：{e}")))
    }

    /// 收尾并给出结论。
    pub fn finish(&self) -> Result<LoudnessOutcome> {
        let integrated_lufs = self
            .inner
            .loudness_global()
            .map_err(|e| analyze_err(format!("读不出积分响度：{e}")))?;
        // `-inf` / `NaN` 表示没有任何一块过门限——全静音或太短。
        if !integrated_lufs.is_finite() {
            return Ok(LoudnessOutcome::Unmeasurable);
        }

        let mut peak = 0.0f64;
        for channel in 0..self.inner.channels() {
            let candidate = self
                .inner
                .true_peak(channel)
                .map_err(|e| analyze_err(format!("读不出真峰值：{e}")))?;
            if !candidate.is_finite() {
                return Err(analyze_err("真峰值不是有限数"));
            }
            peak = peak.max(candidate);
        }

        Ok(LoudnessOutcome::Measured {
            integrated_lufs,
            // 库返回线性幅度，转 dBTP。**可能大于 0**——采样点之间的峰值会超过采样峰值，
            // 这正是必须用 true peak 而非 sample peak 的原因（实测某首母带 +3.3 dBTP）。
            true_peak_dbtp: if peak > 0.0 {
                20.0 * peak.log10()
            } else {
                f64::NEG_INFINITY
            },
        })
    }
}

/// 完整解码一个文件并测量它。
///
/// 分析器**独立解码**，不复用播放链路的 PCM：真峰值要做 4x 过采样，把它塞进播放的
/// 生产线程是拿实时性冒险。等「完整分析与欠载共存」的测试通过后再谈这项优化。
pub fn analyze_file(path: &Path) -> Result<LoudnessOutcome> {
    let mut decoder = Decoder::open(path)?;
    let spec = decoder.spec().clone();

    let Some(mut analyzer) = LoudnessAnalyzer::new(spec.layout, spec.sample_rate)? else {
        return Ok(LoudnessOutcome::UnsupportedLayout);
    };

    let mut buf = Vec::new();
    loop {
        buf.clear();
        if !decoder.next_frames(&mut buf)? {
            break;
        }
        analyzer.feed(&buf)?;
    }
    analyzer.finish()
}

fn analyze_err(msg: impl Into<String>) -> EngineError {
    EngineError::new(Stage::Decode, ErrorKind::Decode, msg)
}

/// dBTP 的 JSON 表示：非有限值写成 `null`。
///
/// `serde_json` 会把 `f64::NEG_INFINITY` 静默序列化成 `null`，再读回来却是解析错误
/// ——一条记录能把整份分析结果拖垮，而重建它要把全库解码一遍。宁可在这里把两个方向
/// 都写明：`null` 就是「没有峰值」。
mod finite_or_null {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &f64, ser: S) -> Result<S::Ok, S::Error> {
        match value.is_finite() {
            true => ser.serialize_some(value),
            false => ser.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<f64, D::Error> {
        Ok(Option::<f64>::deserialize(de)?
            .filter(|v| v.is_finite())
            .unwrap_or(f64::NEG_INFINITY))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 生成一段交错立体声正弦。
    fn sine(rate: u32, seconds: f64, freq: f64, amplitude: f32) -> Vec<f32> {
        let frames = (rate as f64 * seconds) as usize;
        let mut out = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let t = i as f64 / rate as f64;
            let v = (amplitude as f64 * (2.0 * std::f64::consts::PI * freq * t).sin()) as f32;
            out.push(v);
            out.push(v);
        }
        out
    }

    #[test]
    fn gain_moves_loudness_toward_the_target() {
        // 比目标响的要压低，比目标轻的要提升——增益方向必须由两侧各验一次，
        // 只验一侧的话把减号写反也照样通过。
        assert!(
            (applied_gain_db(-8.0, -6.0) - -10.0).abs() < 1e-9,
            "响的要衰减"
        );
        assert!(
            (applied_gain_db(-28.0, -20.0) - 10.0).abs() < 1e-9,
            "轻的要提升"
        );
        assert_eq!(applied_gain_db(TARGET_LUFS, -30.0), 0.0, "正好在目标上不动");
    }

    #[test]
    fn peak_protection_caps_the_boost_instead_of_clipping() {
        // 很安静（-30 LUFS）却已经贴顶（-0.5 dBTP）的曲目：想要 +12 dB 才到目标，
        // 但那会把峰值推到 +11.5 dBTP。宁可少加增益也不削顶。
        let gain = applied_gain_db(-30.0, -0.5);
        assert!((gain - -0.5).abs() < 1e-9, "应被峰值保护咬住，实际 {gain}");
        assert!(gain < TARGET_LUFS - -30.0, "必须小于纯目标增益");
    }

    #[test]
    fn peak_protection_never_blocks_attenuation() {
        // 已经削顶的响曲目（+3.3 dBTP 是实测值）：需要衰减，峰值保护不该反过来
        // 变成「必须再降 4.3 dB」的额外约束——衰减方向本来就更安全。
        let gain = applied_gain_db(-6.7, 3.3);
        assert!(
            (gain - -11.3).abs() < 1e-9,
            "衰减量应由目标响度决定，实际 {gain}"
        );
    }

    #[test]
    fn silence_is_unmeasurable_not_infinite_gain() {
        // 全静音的积分响度是 -inf，若不拦住，requested = -18 - (-inf) = +inf。
        let mut a = LoudnessAnalyzer::new(ChannelLayout::STEREO, 48_000)
            .expect("立体声必然支持")
            .expect("立体声必然支持");
        a.feed(&vec![0.0; 48_000 * 2]).unwrap();
        assert_eq!(a.finish().unwrap(), LoudnessOutcome::Unmeasurable);
        assert_eq!(LoudnessOutcome::Unmeasurable.gain_db(), 0.0);
    }

    #[test]
    fn multichannel_is_refused_rather_than_guessed() {
        // 6 声道可能是 5.1 也可能是 6.0，摆位不同加权就不同。判不出一律拒绝。
        let six = ChannelLayout::discrete(6);
        assert!(
            LoudnessAnalyzer::new(six, 48_000).unwrap().is_none(),
            "多声道必须拒绝，不能按声道数猜布局"
        );
        assert_eq!(LoudnessOutcome::UnsupportedLayout.gain_db(), 0.0);
    }

    #[test]
    fn mono_is_measured_as_dual_mono() {
        // 播放管线把单声道复制到左右两路。若按单个 Left 计权会低估约 3 dB，
        // 于是单声道曲目会被系统性地放得过响。
        let mono = ChannelLayout::MONO;
        let mut a = LoudnessAnalyzer::new(mono, 48_000)
            .expect("单声道支持")
            .expect("单声道支持");
        let frames: Vec<f32> = (0..48_000)
            .map(|i| {
                (0.5 * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / 48_000.0).sin()) as f32
            })
            .collect();
        a.feed(&frames).unwrap();
        let LoudnessOutcome::Measured {
            integrated_lufs, ..
        } = a.finish().unwrap()
        else {
            panic!("1 秒 1 kHz 正弦应当可测");
        };

        // 同一波形按立体声（两路相同）测，结果应当一致——这正是 DualMono 的含义。
        let mut stereo = LoudnessAnalyzer::new(ChannelLayout::STEREO, 48_000)
            .unwrap()
            .unwrap();
        stereo.feed(&sine(48_000, 1.0, 1000.0, 0.5)).unwrap();
        let LoudnessOutcome::Measured {
            integrated_lufs: st,
            ..
        } = stereo.finish().unwrap()
        else {
            panic!("立体声参照应当可测");
        };
        assert!(
            (integrated_lufs - st).abs() < 0.1,
            "单声道 {integrated_lufs} 应与双声道同波形 {st} 一致"
        );
    }

    #[test]
    fn true_peak_can_exceed_sample_peak() {
        // 采样峰值 0.5 的信号，真峰值可以更高——采样点之间的波形会冲过去。
        // 这条钉住「必须用 true peak」：换成 sample peak 会低估削顶风险。
        let mut a = LoudnessAnalyzer::new(ChannelLayout::STEREO, 48_000)
            .unwrap()
            .unwrap();
        // 挑一个与采样率不整除的频率，让峰值落在采样点之间。
        a.feed(&sine(48_000, 2.0, 7993.0, 0.5)).unwrap();
        let LoudnessOutcome::Measured { true_peak_dbtp, .. } = a.finish().unwrap() else {
            panic!("正弦应当可测");
        };
        let sample_peak_dbtp = 20.0 * 0.5f64.log10();
        assert!(
            true_peak_dbtp > sample_peak_dbtp,
            "真峰值 {true_peak_dbtp} 应高于采样峰值 {sample_peak_dbtp}"
        );
    }
}
