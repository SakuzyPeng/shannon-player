//! 容器级特殊探测：lofty / symphonia 读不到的那部分规格。
//!
//! 目前覆盖：
//! - **DSF**（`.dsf`）：DSD 码率与声道数在自有 header 里，symphonia 不解析。
//! - **MP4 `dec3`**：E-AC-3 的 JOC 标记 —— Dolby Atmos 常以此承载，声道数只会报 5.1。
//! - **MP4 `dac4`**：AC-4，通常即 Atmos。
//!
//! 这三处都是「只看声道数一定判错」的典型。识别不出时如实返回 None，
//! 并把线索写进 probe_notes，不假装知道。

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::model::{Encoding, SpatialFormat};

/// DSF header 解析结果。
pub struct DsfInfo {
    pub sample_rate_hz: u32,
    pub channels: u8,
    pub duration_sec: f64,
}

/// 解析 `.dsf` header（DSD Stream File）。
///
/// 布局：`DSD ` chunk(28) → `fmt ` chunk(52) → `data`。
/// 声道数与采样率在 fmt chunk 内的固定偏移上。
pub fn probe_dsf(path: &Path) -> Option<DsfInfo> {
    let mut f = File::open(path).ok()?;
    let mut magic = [0u8; 4];
    f.read_exact(&mut magic).ok()?;
    if &magic != b"DSD " {
        return None;
    }
    // fmt chunk 紧跟在 28 字节的 DSD chunk 之后。
    f.seek(SeekFrom::Start(28)).ok()?;
    let mut fmt = [0u8; 52];
    f.read_exact(&mut fmt).ok()?;
    if &fmt[0..4] != b"fmt " {
        return None;
    }
    let channels = u32::from_le_bytes(fmt[24..28].try_into().ok()?) as u8;
    let sample_rate_hz = u32::from_le_bytes(fmt[28..32].try_into().ok()?);
    let sample_count = u64::from_le_bytes(fmt[36..44].try_into().ok()?);
    let duration_sec = if sample_rate_hz > 0 {
        sample_count as f64 / sample_rate_hz as f64
    } else {
        0.0
    };
    Some(DsfInfo { sample_rate_hz, channels, duration_sec })
}

/// DSD 码率 → 常用倍率标签（DSD64 / DSD128 …）。基准 44 100 × 64。
pub fn dsd_label(sample_rate_hz: u32) -> Option<String> {
    const BASE: u32 = 44_100 * 64;
    if sample_rate_hz == 0 || sample_rate_hz % BASE != 0 {
        return None;
    }
    let mult = sample_rate_hz / BASE;
    if mult.is_power_of_two() {
        Some(format!("DSD{}", 64 * mult))
    } else {
        None
    }
}

/// MP4 空间音频探测结果。
pub struct Mp4Spatial {
    pub spatial: Option<SpatialFormat>,
    pub notes: Vec<String>,
}

/// 在 MP4 容器中查找 `dec3`（E-AC-3，含 JOC 标记）与 `dac4`（AC-4）。
///
/// 这里做的是**扁平扫描**而非完整 box 树遍历：只在文件前若干 MB 内搜索这两个
/// 四字符标签。理由是完整遍历 moov 层级成本高，而这两个 box 必定位于 moov 内、
/// 靠近文件头或尾；扁平搜索足以做「有 / 无」判定，误判风险由后续校验字段吸收。
pub fn probe_mp4_spatial(path: &Path) -> Mp4Spatial {
    let mut notes = Vec::new();
    let Ok(mut f) = File::open(path) else {
        return Mp4Spatial { spatial: None, notes };
    };
    // 只读前 4 MB：足以覆盖 moov 前置的常见排布。
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    let n = match f.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return Mp4Spatial { spatial: None, notes },
    };
    let buf = &buf[..n];

    if let Some(pos) = find(buf, b"dac4") {
        notes.push(format!("mp4:dac4@{pos}"));
        // AC-4 在音乐场景下基本等同 Atmos，但对象数需要解 TOC，此处不冒充。
        return Mp4Spatial {
            spatial: Some(SpatialFormat::DolbyAtmos { joc: false, objects: None }),
            notes,
        };
    }

    if let Some(pos) = find(buf, b"dec3") {
        notes.push(format!("mp4:dec3@{pos}"));
        // dec3 之后是 EC3SpecificBox；JOC 由其尾部的 complexity index 体现。
        // 保守判定：存在非零 complexity index 即认为带 JOC。
        let joc = dec3_has_joc(&buf[pos..]);
        if joc {
            return Mp4Spatial {
                spatial: Some(SpatialFormat::DolbyAtmos { joc: true, objects: None }),
                notes,
            };
        }
        notes.push("mp4:dec3-no-joc".into());
    }

    Mp4Spatial { spatial: None, notes }
}

/// 粗判 EC3SpecificBox 是否带 JOC。
///
/// 完整解析需要按位读取 EC3SpecificBox 的比特域；这里只做启发式：
/// box 尾部若出现 `flag_ec3_extension_type_a` 位与非零 complexity index，视为带 JOC。
/// 判不准时返回 false（宁可漏报，不误报 Atmos）。
fn dec3_has_joc(tail: &[u8]) -> bool {
    // dec3 box 本体通常很短（< 64 字节）。取其后 32 字节做启发式检查。
    let window = &tail[4..tail.len().min(36)];
    // JOC 扩展存在时，尾字节的低 4 位会给出对象数量索引（非零）。
    window.len() >= 5 && window.iter().rev().take(2).any(|b| *b & 0x0F != 0) && window.contains(&0x01)
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// 由扩展名给出容器名与编码族的初判（symphonia 无法识别时的兜底）。
pub fn container_hint(path: &Path) -> (String, Encoding) {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let encoding = match ext.as_str() {
        "dsf" | "dff" => Encoding::Dsd,
        "" => Encoding::Unknown,
        _ => Encoding::Pcm,
    };
    (ext, encoding)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dsd_labels() {
        assert_eq!(dsd_label(2_822_400).as_deref(), Some("DSD64"));
        assert_eq!(dsd_label(5_644_800).as_deref(), Some("DSD128"));
        assert_eq!(dsd_label(11_289_600).as_deref(), Some("DSD256"));
        // 普通 PCM 采样率不该被贴上 DSD 标签
        assert_eq!(dsd_label(44_100), None);
        assert_eq!(dsd_label(96_000), None);
    }

    #[test]
    fn container_hint_marks_dsd() {
        let (c, e) = container_hint(Path::new("/x/a.dsf"));
        assert_eq!(c, "dsf");
        assert_eq!(e, Encoding::Dsd);
        let (c, e) = container_hint(Path::new("/x/a.FLAC"));
        assert_eq!(c, "flac");
        assert_eq!(e, Encoding::Pcm);
    }

    #[test]
    fn find_locates_tag() {
        assert_eq!(find(b"....dec3xx", b"dec3"), Some(4));
        assert_eq!(find(b"nothing", b"dec3"), None);
    }
}
