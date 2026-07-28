//! 稳定曲目 ID。
//!
//! 收藏与歌单都以曲目 ID 为键，所以这个 ID 必须**扛得住文件被移动、重命名、改标签**——
//! 否则用户整理一次音乐库，收藏和歌单就全部失联。
//!
//! 取舍：
//! - 路径哈希：最省事，但移动即失联 —— 否决。
//! - 整文件内容哈希：最稳，但每首都要读完整个文件，大库扫描慢得无法接受 —— 否决。
//! - **文件大小 + 内容前后各取一段做 blake3** ：改标签只动文件头尾的元数据块，
//!   所以采样点要避开纯元数据区；取「跳过开头 + 中段 + 尾段」三处足以区分不同录音，
//!   又不受重命名 / 移动影响。本模块采用此方案。

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// 每个采样点读取的字节数。
const CHUNK: usize = 64 * 1024;
/// 跳过文件开头这么多字节再采样：避开 ID3 / Vorbis comment 等可变的元数据区。
const HEAD_SKIP: u64 = 128 * 1024;

/// 格式指纹：参与 ID 的规格要素。
///
/// 只包含**不会因改标签或移动文件而变**的项。加它是因为同一段音频的不同规格版本
/// （例如 5.1 与 6.0 下混，采样数据可能完全一致）否则会撞 ID。
#[derive(Clone, Copy)]
pub struct FormatFingerprint<'a> {
    pub codec: &'a str,
    pub channels: u8,
    pub sample_rate_hz: u32,
    pub channel_mask: Option<u32>,
}

/// 计算曲目稳定 ID。失败时回落到路径哈希（至少不会 panic，且仍然唯一）。
pub fn track_id_with(path: &Path, fp: &FormatFingerprint<'_>) -> String {
    match content_id_with(path, Some(fp)) {
        Ok(id) => id,
        Err(_) => {
            let mut h = blake3::Hasher::new();
            h.update(path.to_string_lossy().as_bytes());
            format!("t-{}", &h.finalize().to_hex()[..24])
        }
    }
}

/// 无格式信息时的版本（测试与回落路径用）。
pub fn track_id(path: &Path) -> String {
    match content_id_with(path, None) {
        Ok(id) => id,
        Err(_) => {
            let mut h = blake3::Hasher::new();
            h.update(path.to_string_lossy().as_bytes());
            format!("t-{}", &h.finalize().to_hex()[..24])
        }
    }
}

fn content_id_with(path: &Path, fp: Option<&FormatFingerprint<'_>>) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let len = f.metadata()?.len();

    let mut h = blake3::Hasher::new();
    // 文件长度参与哈希：同一段音频的不同编码版本长度必然不同。
    h.update(&len.to_le_bytes());
    if let Some(fp) = fp {
        h.update(fp.codec.as_bytes());
        h.update(&[fp.channels]);
        h.update(&fp.sample_rate_hz.to_le_bytes());
        h.update(&fp.channel_mask.unwrap_or(0).to_le_bytes());
    }

    // 三个采样点：跳过元数据区后的开头、中段、尾段。小文件自动退化为读一次。
    let offsets = sample_offsets(len);
    let mut buf = vec![0u8; CHUNK];
    for off in offsets {
        f.seek(SeekFrom::Start(off))?;
        let n = read_up_to(&mut f, &mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("t-{}", &h.finalize().to_hex()[..24]))
}

/// 采样偏移。文件太小时只取一个点（且不跳过头部，否则可能什么都读不到）。
fn sample_offsets(len: u64) -> Vec<u64> {
    let chunk = CHUNK as u64;
    if len <= HEAD_SKIP + chunk {
        return vec![0];
    }
    let mid = len / 2;
    let tail = len.saturating_sub(chunk);
    let mut v = vec![HEAD_SKIP, mid, tail];
    v.dedup();
    v
}

/// `Read::read` 可能短读，这里循环填满或读到 EOF。
fn read_up_to(f: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match f.read(&mut buf[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

/// 专辑 ID：由归组键派生。
///
/// 归组键含所在目录（见 `scan::album_group_key`），不是「专辑艺人 + 专辑名」——
/// 否则两位不同歌手各自的《Greatest Hits》会撞成一张。专辑不是持久化实体
/// （收藏与歌单都用曲目 ID），所以这个 ID 随重扫变化是可以接受的。
pub fn album_id(group_key: &str) -> String {
    let mut h = blake3::Hasher::new();
    h.update(group_key.as_bytes());
    format!("a-{}", &h.finalize().to_hex()[..20])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(name);
        let mut f = File::create(&p).unwrap();
        f.write_all(bytes).unwrap();
        p
    }

    #[test]
    fn id_survives_rename() {
        let data: Vec<u8> = (0..400_000u32).map(|i| (i % 251) as u8).collect();
        let a = write_temp("shannon_id_a.bin", &data);
        let b = write_temp("shannon_id_b.bin", &data);
        // 同内容不同文件名 → 同 ID（等价于「文件被移动 / 重命名」）
        assert_eq!(track_id(&a), track_id(&b));
        let _ = std::fs::remove_file(a);
        let _ = std::fs::remove_file(b);
    }

    #[test]
    fn id_differs_for_different_audio() {
        let a_data: Vec<u8> = (0..400_000u32).map(|i| (i % 251) as u8).collect();
        let b_data: Vec<u8> = (0..400_000u32).map(|i| (i % 249) as u8).collect();
        let a = write_temp("shannon_id_c.bin", &a_data);
        let b = write_temp("shannon_id_d.bin", &b_data);
        assert_ne!(track_id(&a), track_id(&b));
        let _ = std::fs::remove_file(a);
        let _ = std::fs::remove_file(b);
    }

    #[test]
    fn id_ignores_leading_metadata_changes() {
        // 前 128 KB 视作元数据区：只改这里，ID 不应变化（改标签不应断开收藏）。
        let mut base: Vec<u8> = (0..400_000u32).map(|i| (i % 251) as u8).collect();
        let a = write_temp("shannon_id_e.bin", &base);
        let id_a = track_id(&a);
        for b in base.iter_mut().take(100_000) {
            *b = 0xAB;
        }
        let b = write_temp("shannon_id_f.bin", &base);
        assert_eq!(id_a, track_id(&b));
        let _ = std::fs::remove_file(a);
        let _ = std::fs::remove_file(b);
    }

    #[test]
    fn small_files_still_hash() {
        let a = write_temp("shannon_id_g.bin", b"tiny");
        let b = write_temp("shannon_id_h.bin", b"tiny");
        assert_eq!(track_id(&a), track_id(&b));
        assert!(track_id(&a).starts_with("t-"));
        let _ = std::fs::remove_file(a);
        let _ = std::fs::remove_file(b);
    }

    /// 同一段采样数据、不同声道布局，必须得到不同 ID（否则 5.1 与 6.0 会互相覆盖）。
    #[test]
    fn format_fingerprint_breaks_ties() {
        let data: Vec<u8> = (0..400_000u32).map(|i| (i % 251) as u8).collect();
        let p = write_temp("shannon_id_fp.bin", &data);
        let five_one = FormatFingerprint {
            codec: "pcm",
            channels: 6,
            sample_rate_hz: 48_000,
            channel_mask: Some(0x3F),
        };
        let six_zero = FormatFingerprint {
            channel_mask: Some(0x37),
            ..five_one
        };
        assert_ne!(track_id_with(&p, &five_one), track_id_with(&p, &six_zero));
        // 同指纹仍然稳定
        assert_eq!(track_id_with(&p, &five_one), track_id_with(&p, &five_one));
        let _ = std::fs::remove_file(p);
    }

    /// 归一化（大小写、作用域）是 `scan::album_group_key` 的职责，
    /// 这里只保证同键同 ID、异键异 ID。
    #[test]
    fn album_id_is_stable_per_group_key() {
        assert_eq!(
            album_id("长夜电波\u{1f}aa:白鲸电台"),
            album_id("长夜电波\u{1f}aa:白鲸电台")
        );
        assert_ne!(
            album_id("greatest hits\u{1f}aa:a"),
            album_id("greatest hits\u{1f}aa:b")
        );
    }
}
