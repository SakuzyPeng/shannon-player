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
///
/// 这条区分正是「识别与播放能力解耦」那条戒律：不在列表里的文件连试都不会试，
/// 用户看到的是「文件明明在，曲库里找不到」；而在列表里但放不了，用户至少能看见它、
/// 也能得到一句明确的能力错误。所以宁可收得宽。
///
/// `ec3` / `eac3` 是 E-AC-3（含 Atmos 的 JOC 承载体）的裸流扩展名。当前引擎放不了它，
/// **这不是把它排除在外的理由**——排除等于让用户以为自己没有这些文件。
///
/// `webm` 收进来是因为 yt-dlp 抓 YouTube 音频的默认产物就是它（Opus 装在 WebM 里），
/// 而那条路径正是引擎已经能放的。它同时也是视频容器，因此音乐目录里的视频会被收进
/// 曲库——`mp4` 早就有同样的性质，取舍与它一致：目录是用户自己指定的，
/// 收多了他看得见也删得掉，收少了他只会以为文件丢了。`mkv` 仍然不收，
/// 音频用的是 `mka`，这与 `mp4` / `m4a` 是同一套约定。
pub const AUDIO_EXTS: &[&str] = &[
    "flac", "mp3", "m4a", "mp4", "aac", "ogg", "oga", "opus", "wav", "wave", "aiff", "aif", "caf",
    "alac", "wma", "wv", "ape", "dsf", "dff", "mka", "webm", "ec3", "eac3",
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
    tags.picture
        .as_ref()
        .map(|b| blake3::hash(b).to_hex()[..16].to_string())
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
            // DSF header 里声道数是必填字段，读到了就是读到了。
            channels: Some(info.channels),
            channel_mask: None,
            // DSF header 只给声道数，没有摆位信息 —— 立体声之外一律不猜。
            channel_layout: (info.channels == 2).then_some(ChannelLayout::Stereo),
            spatial: None,
            probe_notes: notes,
            probe_version: PROBE_VERSION,
        };
        let tags = read_tags(path).unwrap_or_else(empty_tags);
        let cover_key = cover_key_of(&tags);
        return Ok(Probed {
            tags,
            format,
            duration_sec: info.duration_sec,
            cover_key,
        });
    }

    // ---- 常规 PCM 路径 ----
    //
    // lofty 读不了的容器（实测 CAF）**不能就此丢文件**：标签读不到只意味着信息少，
    // 不意味着这不是一首歌。与「识别与播放能力解耦」同一条道理——扫描如实记录能拿到的，
    // 拿不到的留空由文件名兜底，而不是让整个文件从曲库里消失。
    // 早先这里是 `?` 直接返回错误，结果一个合法的 CAF 只会体现为 failed 计数 +1。
    let tagged = lofty::read_from_path(path).ok();
    let sym = symphonia_params(path);

    // 两个都读不出来才是真的「这不是音频」。只有 lofty 失败不算：
    // 它读不了的合法容器确实存在（实测 CAF），那时 symphonia 仍能给出规格。
    if tagged.is_none() && sym.is_none() {
        return Err(format!("既读不到标签也探不出音频规格: {}", path.display()));
    }

    let props = tagged.as_ref().map(|t| t.properties());

    let mut notes = Vec::new();
    if tagged.is_none() {
        // 留下痕迹：「没有标签」与「有标签但是空的」不是一回事，
        // 日后回溯字段为何缺失时要分得清。
        notes.push("tags:unreadable".into());
    }
    let mut channel_mask = None;
    let mut codec = String::new();
    let mut bit_depth = props.and_then(|p| p.bit_depth());
    let mut sample_rate_hz = props.and_then(|p| p.sample_rate()).unwrap_or(0);
    let mut channels = props.and_then(|p| p.channels());
    let mut duration_sec = props.map(|p| p.duration().as_secs_f64()).unwrap_or(0.0);

    // symphonia 补齐 lofty 拿不到的声道掩码（布局的权威来源）。
    // lofty 整个读不了时，它还要顶上时长与其余规格。
    if let Some((params, dur)) = sym {
        if duration_sec <= 0.0 {
            if let Some(d) = dur {
                duration_sec = d;
            }
        }
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
            channels = Some(n);
        }
        codec = codec_name(params.codec);
    }
    if codec.is_empty() {
        // **留空，不拿容器名冒充编码。** 「m4a」是容器，里面可能是 ALAC、AAC、也可能是
        // APAC；把容器名填进 codec，界面上就会言之凿凿地显示一个我们其实没认出来的编码，
        // 而这与「判不出一律留空」和「未经证实的状态不得展示」都相悖。线索记进 notes
        // 留待回溯：日后补上 APAC 之类的识别时，正是靠它捞出需要重扫的条目。
        notes.push("codec:unrecognized".into());
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
            Some(1) => Some(ChannelLayout::Mono),
            Some(2) => Some(ChannelLayout::Stereo),
            Some(n) => {
                notes.push(format!("layout:unknown-{n}ch-no-mask"));
                None
            }
            // 声道数本身都没读出来，连「几声道的布局判不出」都说不上。
            None => {
                notes.push("layout:unknown-no-channel-count".into());
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
        bitrate_kbps: props.and_then(|p| p.audio_bitrate()),
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
    Ok(Probed {
        tags,
        format,
        duration_sec,
        cover_key,
    })
}

/// symphonia 侧的规格与时长。时长单独返回是因为 lofty 读不了的容器要靠它顶上。
fn symphonia_params(path: &Path) -> Option<(CodecParameters, Option<f64>)> {
    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;
    let track = probed
        .format
        .tracks()
        .iter()
        .find(|t| t.codec_params.sample_rate.is_some() || t.codec_params.channels.is_some())?;
    let params = track.codec_params.clone();
    let duration = duration_from_params(&params);
    Some((params, duration))
}

/// 从 Symphonia 的参数换算时长。
///
/// 有时基时必须优先按它解释 `n_frames`：Matroska reader 把这里的值记成容器时间戳刻度
/// （2 秒文件约为 2000），直接除以 48 kHz 会误报成四十多毫秒。没有时基的裸音频才退回
/// 「PCM 帧数 / 采样率」。容器没给长度就留空，不拿 0 冒充。
fn duration_from_params(params: &CodecParameters) -> Option<f64> {
    let frames = params.n_frames?;
    if let Some(time_base) = params.time_base {
        let time = time_base.calc_time(frames);
        Some(time.seconds as f64 + time.frac)
    } else {
        params.sample_rate.map(|rate| frames as f64 / rate as f64)
    }
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
    matches!(
        codec,
        "flac" | "alac" | "pcm" | "dsd" | "wav" | "wave" | "aiff" | "wv" | "ape"
    )
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
        // CAF 是 Apple 生态里的常规音频容器（Logic 导出、系统录音），
        // 漏掉它等于对着一整类文件装作没看见。
        assert!(is_audio_file(Path::new("/m/a.caf")));
        assert!(is_audio_file(Path::new("/m/a.webm")));
        assert!(!is_audio_file(Path::new("/m/cover.jpg")));
        assert!(!is_audio_file(Path::new("/m/notes.txt")));
        assert!(!is_audio_file(Path::new("/m/noext")));
    }

    #[test]
    fn tagless_container_still_yields_a_track() {
        // lofty 读不了标签的合法容器（实测 CAF）不能就此丢文件：读不到标签只意味着
        // 信息少，不意味着这不是一首歌。早先这里是硬失败，一个合法的 CAF 只体现为
        // failed 计数 +1，用户那边就是「文件明明在，曲库里找不到」。
        let dir =
            std::env::temp_dir().join(format!("shannon_probe_tagless_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("bare.caf");

        // 最小的 16-bit little-endian PCM CAF。测试必须真的让 lofty 失败、
        // symphonia 成功；用“没有标签的 WAV”只是在测 tag 为空，根本进不到回退分支。
        let (rate, frames, channels) = (44_100u32, 4_410usize, 2u32);
        let bytes_per_packet = channels * 2;
        let data_len = frames as u64 * u64::from(bytes_per_packet);
        let mut buf = Vec::new();
        buf.extend_from_slice(b"caff");
        buf.extend_from_slice(&1u16.to_be_bytes());
        buf.extend_from_slice(&0u16.to_be_bytes());
        buf.extend_from_slice(b"desc");
        buf.extend_from_slice(&32i64.to_be_bytes());
        buf.extend_from_slice(&(rate as f64).to_be_bytes());
        buf.extend_from_slice(b"lpcm");
        buf.extend_from_slice(&2u32.to_be_bytes()); // little-endian integer PCM
        buf.extend_from_slice(&bytes_per_packet.to_be_bytes());
        buf.extend_from_slice(&1u32.to_be_bytes()); // frames per packet
        buf.extend_from_slice(&channels.to_be_bytes());
        buf.extend_from_slice(&16u32.to_be_bytes());
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&((data_len + 4) as i64).to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes()); // edit count
        buf.resize(buf.len() + data_len as usize, 0);
        std::fs::write(&path, buf).unwrap();

        assert!(
            lofty::read_from_path(&path).is_err(),
            "语料必须确实覆盖 lofty 不可读、symphonia 可读的分支"
        );
        let probed = probe(&path).expect("合法音频不该因为没有标签而被拒绝");
        assert_eq!(probed.format.sample_rate_hz, rate);
        assert!(probed.duration_sec > 0.0, "时长要能从码流推出来，不能是 0");
        assert!(
            probed
                .format
                .probe_notes
                .iter()
                .any(|note| note == "tags:unreadable"),
            "回退必须留下标签不可读的诊断线索"
        );

        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_dir(dir);
    }

    #[test]
    fn duration_uses_container_time_base_before_sample_rate() {
        use symphonia::core::units::TimeBase;

        // Symphonia 0.5 的 Matroska reader 会把 2.008 秒记为 2008 个 1 ms 刻度。
        // 若误当 PCM 帧除以 48 kHz，只会得到 0.041833 秒。
        let mut matroska = CodecParameters::new();
        matroska
            .with_sample_rate(48_000)
            .with_time_base(TimeBase::new(1, 1_000))
            .with_n_frames(2_008);
        assert!((duration_from_params(&matroska).unwrap() - 2.008).abs() < 1e-9);

        // 没有容器时基的裸 PCM 仍按帧数 / 采样率换算。
        let mut pcm = CodecParameters::new();
        pcm.with_sample_rate(48_000).with_n_frames(96_000);
        assert_eq!(duration_from_params(&pcm), Some(2.0));
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
