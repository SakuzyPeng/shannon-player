//! 后端领域模型。
//!
//! 这些结构体经 ts-rs 生成到 `src/types/generated/`，是前后端的唯一契约来源——
//! 与 i18n 的 `Messages` 接口同一理念：结构一漂移，前端就编译不过。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 采样编码族。决定 `sample_rate_hz` 与 `bit_depth` 的解读方式：
/// PCM 下是常规采样率 / 位深；DSD 下 sample_rate 是 DSD 码率（DSD64 = 2 822 400），位深恒为 1。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/audio.ts")]
#[serde(rename_all = "camelCase")]
pub enum Encoding {
    Pcm,
    Dsd,
    /// 探测器无法确定——不猜。
    Unknown,
}

/// 具名声道布局。
///
/// 注意这只是 `channel_mask` 的**具名投影**，掩码才是权威：布局组合太多
/// （5.1 / 7.1.4 / 9.1.6 / Auro-3D…），封闭枚举必然跟不上，所以用
/// 「主声道数 . 低频 . 天空声道数」的公式表达环绕，冷门的落到 `Other`。
///
/// 单声道 / 立体声单独成项是因为界面上它们是词（需要 i18n），不是数字。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/audio.ts")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ChannelLayout {
    Mono,
    Stereo,
    Quad,
    /// 环绕：`{ main: 5, lfe: 1, height: 0 }` → 5.1；`{ 7, 1, 4 }` → 7.1.4。
    Surround { main: u8, lfe: u8, height: u8 },
    /// n 阶 Ambisonics（声道数 = (n+1)²）。
    Ambisonic { order: u8 },
    /// 无法归入具名布局；掩码是权威。
    Other { mask: u32 },
}

/// 空间 / 对象音频标记。
///
/// **与声道维度正交**，不能合并：Atmos 的声道数可能报 5.1 甚至 2，
/// 真正的信息在容器的对象元数据里。合在一起就永远说不清「5.1 的 Atmos」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/audio.ts")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SpatialFormat {
    /// Dolby Atmos。`joc` = 以 JOC 形式承载于 E-AC-3；`objects` 为对象数（读不到则 None）。
    DolbyAtmos { joc: bool, objects: Option<u16> },
    Sony360Ra,
    MpegH,
    Ambisonics { order: u8 },
    /// 检测到空间音频线索但无法归类。
    Unknown,
}

/// 音频规格。
///
/// 设计要点：
/// 1. 所有「判不出」的字段都是 `Option`，宁可留空也不用声道数硬猜；
/// 2. `codec` / `container` 存探测器报告的原始名，不做归一化——归一化会丢信息；
/// 3. `probe_notes` + `probe_version` 是为日后增强探测能力留的口子：
///    探测器升级后可只重扫版本落后的条目，不必全库重扫。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/audio.ts")]
#[serde(rename_all = "camelCase")]
pub struct AudioFormat {
    /// 容器：flac / mp4 / mp3 / ogg / wav / dsf …
    pub container: String,
    /// 编解码：flac / alac / aac-lc / eac3-joc / dsd …
    pub codec: String,
    pub encoding: Encoding,
    pub sample_rate_hz: u32,
    /// PCM 位深；DSD 恒为 1。
    pub bit_depth: Option<u8>,
    pub bitrate_kbps: Option<u32>,
    pub lossless: Option<bool>,
    pub channels: u8,
    /// 扬声器位置位掩码（FFmpeg 口径），布局的权威来源。
    pub channel_mask: Option<u32>,
    pub channel_layout: Option<ChannelLayout>,
    pub spatial: Option<SpatialFormat>,
    /// 探测器没读懂的原始线索，留待回溯。
    pub probe_notes: Vec<String>,
    pub probe_version: u32,
}

/// 当前探测器版本。**增强探测逻辑时必须 +1**，否则无法识别哪些条目需要重扫。
pub const PROBE_VERSION: u32 = 1;

/// 封面：占位渐变（首字母）或真实图片。与前端 `Cover` 对齐。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/library.ts")]
#[serde(rename_all = "camelCase")]
pub struct Cover {
    pub initial: String,
    /// 渐变起止色。
    pub gradient: (String, String),
    /// 真实封面图 URL（有则优先）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
}

/// 曲目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/library.ts")]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub album_id: Option<String>,
    pub cover: Cover,
    pub duration_sec: f64,
    /// 本地文件绝对路径。
    pub path: String,
    /// 碟号 / 音轨号（用于专辑内排序）。
    pub disc_no: Option<u16>,
    pub track_no: Option<u16>,
    pub format: AudioFormat,
}

/// 专辑（由曲目聚合而来，非独立实体）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/library.ts")]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub year: u32,
    pub genre: String,
    pub cover: Cover,
    pub track_count: u32,
    pub duration_sec: f64,
}

/// 一次扫描的产出。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/library.ts")]
#[serde(rename_all = "camelCase")]
pub struct LibrarySnapshot {
    pub albums: Vec<Album>,
    pub tracks: Vec<Track>,
    /// 遍历到但无法解析的文件数（不静默丢弃，如实上报）。
    pub failed: u32,
}

/// 扫描进度事件（Tauri event `library://scan-progress`）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/library.ts")]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    /// 已处理文件数。
    pub done: u32,
    /// 已发现的候选音频文件总数（遍历完成后才确定）。
    pub total: u32,
    pub tracks: u32,
    pub albums: u32,
    /// 当前正在解析的文件路径（首启页滚动展示用）。
    pub current: String,
}
