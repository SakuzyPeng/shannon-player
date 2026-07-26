//! 单文件探测：标签 + 音频规格 + 封面。
//!
//! 分工：
//! - `lofty` —— 标签（标题 / 艺人 / 专辑 / 年份 / 流派 / 碟号音轨号）与内嵌封面；
//! - `symphonia` —— `CodecParameters` 里的声道掩码、采样率、位深（掩码是布局的权威来源）；
//! - `container` —— 前两者读不到的部分：DSF header、MP4 的 JOC / AC-4 标记。
//!
//! 原则：**识别与播放能力解耦**。这里只负责如实记录规格，播不了是播放器的事，
//! 不能因为暂时播不了就在扫描阶段丢掉文件——那等于告诉用户「你没有这些歌」。

pub mod container;
pub mod layout;

use std::path::Path;

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::prelude::ItemKey;
use lofty::tag::Accessor;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::model::{AudioFormat, ChannelLayout, Encoding, SpatialFormat, PROBE_VERSION};

/// 支持扫描的扩展名。**只用于决定「要不要尝试解析」，不代表能播放**。
pub const AUDIO_EXTS: &[&str] = &[
    "flac", "mp3", "m4a", "mp4", "aac", "ogg", "oga", "opus", "wav", "wave", "aiff", "aif",
    "alac", "wma", "wv", "ape", "dsf", "dff", "mka",
];

pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// 文件标签。
pub struct Tags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub track_no: Option<u16>,
    pub disc_no: Option<u16>,
    /// 内嵌封面原始字节（首张）。
    pub picture: Option<Vec<u8>>,
}

pub struct Probed {
    pub tags: Tags,
    pub format: AudioFormat,
    pub duration_sec: f64,
    /// 内嵌封面的内容指纹。
    ///
    /// 用途是专辑归组：合辑的曲目常按曲目艺人散在不同目录、又没有专辑艺人标签，
    /// 目录和音轨号都认不出它们属于同一张专辑，但**它们嵌的是同一张封面图**。
    /// 在这里算是因为探测本身是并行的，聚合阶段再算等于串行重算一遍。
    pub cover_key: Option<String>,
}

/// 封面内容指纹。取前 16 位十六进制足够区分一个曲库内的封面。
fn cover_key_of(tags: &Tags) -> Option<String> {
    tags.picture.as_ref().map(|b| blake3::hash(b).to_hex()[..16].to_string())
}

/// 探测单个文件。返回 Err 表示无法解析（调用方应计入 failed 而非静默丢弃）。
pub fn probe(path: &Path) -> Result<Probed, String> {
    let (container, mut encoding) = container::container_hint(path);

    // ---- DSD 走独立分支：lofty / symphonia 都不解 DSF ----
    if encoding == Encoding::Dsd {
        let info = container::probe_dsf(path)
            .ok_or_else(|| format!("无法解析 DSD header: {}", path.display()))?;
        let mut notes = Vec::new();
        if let Some(label) = container::dsd_label(info.sample_rate_hz) {
            notes.push(format!("dsd:{label}"));
        }
        let format = AudioFormat {
            container,
            codec: "dsd".into(),
            encoding: Encoding::Dsd,
            sample_rate_hz: info.sample_rate_hz,
            bit_depth: Some(1),
            bitrate_kbps: None,
            lossless: Some(true),
            channels: info.channels,
            channel_mask: None,
            // DSF header 只给声道数，没有摆位信息 —— 立体声之外一律不猜。
            channel_layout: (info.channels == 2).then_some(ChannelLayout::Stereo),
            spatial: None,
            probe_notes: notes,
            probe_version: PROBE_VERSION,
        };
        let tags = read_tags(path).unwrap_or_else(empty_tags);
        let cover_key = cover_key_of(&tags);
        return Ok(Probed { tags, format, duration_sec: info.duration_sec, cover_key });
    }

    // ---- 常规 PCM 路径 ----
    let tagged = lofty::read_from_path(path).map_err(|e| format!("lofty: {e}"))?;
    let props = tagged.properties();
    let duration_sec = props.duration().as_secs_f64();

    let mut notes = Vec::new();
    let mut channel_mask = None;
    let mut codec = String::new();
    let mut bit_depth = props.bit_depth();
    let mut sample_rate_hz = props.sample_rate().unwrap_or(0);
    let mut channels = props.channels().unwrap_or(0);

    // symphonia 补齐 lofty 拿不到的声道掩码（布局的权威来源）。
    if let Some(params) = symphonia_params(path) {
        if let Some(m) = params.channels.map(|c| c.bits()) {
            channel_mask = Some(m);
        }
        if let Some(sr) = params.sample_rate {
            sample_rate_hz = sr;
        }
        if let Some(bits) = params.bits_per_sample {
            bit_depth = Some(bits as u8);
        }
        if let Some(n) = params.channels.map(|c| c.count() as u8) {
            channels = n;
        }
        codec = codec_name(params.codec);
    }
    if codec.is_empty() {
        codec = container.clone();
        notes.push("codec:fallback-to-container".into());
    }
    if encoding == Encoding::Unknown {
        encoding = Encoding::Pcm;
    }

    // 空间音频：只有 MP4 系容器才有 dec3 / dac4。
    let mut spatial: Option<SpatialFormat> = None;
    if matches!(container.as_str(), "m4a" | "mp4" | "aac") {
        let found = container::probe_mp4_spatial(path);
        spatial = found.spatial;
        notes.extend(found.notes);
    }

    let channel_layout = channel_mask.map(layout::layout_from_mask).or_else(|| {
        // 无掩码时只认最无争议的两种，其余留空 —— 声道数推不出摆位。
        match channels {
            1 => Some(ChannelLayout::Mono),
            2 => Some(ChannelLayout::Stereo),
            _ => {
                notes.push(format!("layout:unknown-{channels}ch-no-mask"));
                None
            }
        }
    });

    let format = AudioFormat {
        container,
        codec: codec.clone(),
        encoding,
        sample_rate_hz,
        bit_depth,
        bitrate_kbps: props.audio_bitrate(),
        lossless: Some(is_lossless(&codec)),
        channels,
        channel_mask,
        channel_layout,
        spatial,
        probe_notes: notes,
        probe_version: PROBE_VERSION,
    };

    let tags = read_tags(path).unwrap_or_else(empty_tags);
    let cover_key = cover_key_of(&tags);
    Ok(Probed { tags, format, duration_sec, cover_key })
}

fn symphonia_params(path: &Path) -> Option<CodecParameters> {
    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .ok()?;
    probed
        .format
        .tracks()
        .iter()
        .find(|t| t.codec_params.sample_rate.is_some() || t.codec_params.channels.is_some())
        .map(|t| t.codec_params.clone())
}

fn codec_name(codec: symphonia::core::codecs::CodecType) -> String {
    use symphonia::core::codecs::*;
    match codec {
        CODEC_TYPE_FLAC => "flac",
        CODEC_TYPE_MP3 => "mp3",
        CODEC_TYPE_AAC => "aac",
        CODEC_TYPE_ALAC => "alac",
        CODEC_TYPE_VORBIS => "vorbis",
        CODEC_TYPE_OPUS => "opus",
        CODEC_TYPE_PCM_S16LE | CODEC_TYPE_PCM_S24LE | CODEC_TYPE_PCM_S32LE
        | CODEC_TYPE_PCM_F32LE | CODEC_TYPE_PCM_F64LE => "pcm",
        CODEC_TYPE_ADPCM_IMA_WAV | CODEC_TYPE_ADPCM_MS => "adpcm",
        _ => "",
    }
    .to_string()
}

/// 无损判定由 codec 决定；未知 codec 不猜（返回 false 但 probe_notes 已记录）。
fn is_lossless(codec: &str) -> bool {
    matches!(codec, "flac" | "alac" | "pcm" | "dsd" | "wav" | "wave" | "aiff" | "wv" | "ape")
}

fn empty_tags() -> Tags {
    Tags {
        title: None,
        artist: None,
        album_artist: None,
        album: None,
        year: None,
        genre: None,
        track_no: None,
        disc_no: None,
        picture: None,
    }
}

fn read_tags(path: &Path) -> Option<Tags> {
    let tagged = lofty::read_from_path(path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    Some(Tags {
        title: tag.title().map(|s| s.to_string()),
        artist: tag.artist().map(|s| s.to_string()),
        album_artist: tag.get_string(&ItemKey::AlbumArtist).map(|s| s.to_string()),
        album: tag.album().map(|s| s.to_string()),
        year: tag.year(),
        genre: tag.genre().map(|s| s.to_string()),
        track_no: tag.track().map(|n| n as u16),
        disc_no: tag.disk().map(|n| n as u16),
        picture: tag.pictures().first().map(|p| p.data().to_vec()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_ext_detection() {
        assert!(is_audio_file(Path::new("/m/a.flac")));
        assert!(is_audio_file(Path::new("/m/a.FLAC")));
        assert!(is_audio_file(Path::new("/m/a.dsf")));
        assert!(!is_audio_file(Path::new("/m/cover.jpg")));
        assert!(!is_audio_file(Path::new("/m/notes.txt")));
        assert!(!is_audio_file(Path::new("/m/noext")));
    }

    #[test]
    fn lossless_classification() {
        assert!(is_lossless("flac"));
        assert!(is_lossless("alac"));
        assert!(is_lossless("dsd"));
        assert!(!is_lossless("mp3"));
        assert!(!is_lossless("aac"));
        assert!(!is_lossless("opus"));
    }

    /// 无法解析的文件必须报错，而不是返回一条伪造的空曲目。
    #[test]
    fn garbage_file_is_rejected() {
        let p = std::env::temp_dir().join("shannon_probe_garbage.flac");
        std::fs::write(&p, b"not actually a flac").unwrap();
        assert!(probe(&p).is_err());
        let _ = std::fs::remove_file(p);
    }
}
