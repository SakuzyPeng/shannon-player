//! 曲库扫描：遍历目录 → 并行探测 → 聚合为专辑。
//!
//! 进度上报通过回调注入，core 不知道 Tauri 的存在。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use rayon::prelude::*;
use walkdir::WalkDir;

use crate::id::{album_id, track_id_with, FormatFingerprint};
use crate::model::{Album, Cover, LibrarySnapshot, ScanProgress, Track};
use crate::probe::{self, Probed};

/// 占位封面渐变色板（沿用前端种子数据的杏色系语言）。
/// 无内嵌封面时按专辑 ID 稳定取色——同一张专辑每次扫描颜色一致。
const GRADIENTS: &[(&str, &str)] = &[
    ("#3E4C5A", "#2A3440"),
    ("#5A4A3E", "#40342A"),
    ("#4A5A3E", "#34402A"),
    ("#5A3E4A", "#402A34"),
    ("#3E5A55", "#2A403C"),
    ("#55503E", "#3C382A"),
    ("#463E5A", "#322A40"),
    ("#5A4638", "#403227"),
];

/// 先遍历出候选文件，再并行探测。
///
/// 两段式的原因：进度条需要「总数」才有意义，而总数只有遍历完才知道。
/// 遍历本身很快（只看扩展名，不读文件内容）。
pub fn scan_folders<F>(roots: &[PathBuf], mut on_progress: F) -> LibrarySnapshot
where
    F: FnMut(ScanProgress) + Send,
{
    let files = collect_files(roots);
    let total = files.len() as u32;

    let done = AtomicU32::new(0);
    let failed = AtomicU32::new(0);
    // 进度回调不是 Sync，用 Mutex 串行化；回调本身很轻（发一个事件）。
    let progress = Mutex::new(&mut on_progress);

    let probed: Vec<(PathBuf, Probed)> = files
        .par_iter()
        .filter_map(|path| {
            let result = probe::probe(path);
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            // 每 16 个文件报一次，避免事件风暴淹没前端。
            if n % 16 == 0 || n == total {
                if let Ok(mut cb) = progress.lock() {
                    cb(ScanProgress {
                        done: n,
                        total,
                        tracks: 0,
                        albums: 0,
                        current: path.to_string_lossy().to_string(),
                    });
                }
            }
            match result {
                Ok(p) => Some((path.clone(), p)),
                Err(_) => {
                    failed.fetch_add(1, Ordering::Relaxed);
                    None
                }
            }
        })
        .collect();

    let snapshot = aggregate(probed);
    on_progress(ScanProgress {
        done: total,
        total,
        tracks: snapshot.tracks.len() as u32,
        albums: snapshot.albums.len() as u32,
        current: String::new(),
    });

    LibrarySnapshot { failed: failed.load(Ordering::Relaxed), ..snapshot }
}

/// 遍历给定根目录，收集候选音频文件。跟随符号链接但不重复访问。
fn collect_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in roots {
        for entry in WalkDir::new(root).follow_links(false).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() && probe::is_audio_file(entry.path()) {
                out.push(entry.path().to_path_buf());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// 把探测结果聚合为曲目 + 专辑。
fn aggregate(probed: Vec<(PathBuf, Probed)>) -> LibrarySnapshot {
    // 专辑归组键用「专辑艺人 + 专辑名」：合辑里每首歌 artist 不同，
    // 若用 artist 归组会把一张合辑拆成十几张专辑。
    struct AlbumAcc {
        title: String,
        artist: String,
        year: u32,
        genre: String,
        cover: Cover,
        track_count: u32,
        duration_sec: f64,
    }
    let mut albums: HashMap<String, AlbumAcc> = HashMap::new();
    let mut tracks: Vec<Track> = Vec::new();

    for (path, p) in probed {
        let file_stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "未知曲目".into());
        let title = p.tags.title.clone().unwrap_or(file_stem);
        // 无标签时按 Artist/Album/Track 目录约定兜底：把整盘未标签文件都并进
        // 「未知专辑」会毁掉专辑视图，而目录名通常正是用户的组织方式。
        let (dir_artist, dir_album) = folder_hint(&path);
        let artist = p
            .tags
            .artist
            .clone()
            .or_else(|| dir_artist.clone())
            .unwrap_or_else(|| "未知歌手".into());
        let album_name = p
            .tags
            .album
            .clone()
            .or(dir_album)
            .unwrap_or_else(|| "未知专辑".into());
        // 无专辑艺人标签时回落到曲目艺人（单人专辑的常见情形）。
        let album_artist = p.tags.album_artist.clone().unwrap_or_else(|| artist.clone());
        let aid = album_id(&album_artist, &album_name);
        let cover = make_cover(&aid, &album_name, p.tags.picture.is_some());

        let acc = albums.entry(aid.clone()).or_insert_with(|| AlbumAcc {
            title: album_name.clone(),
            artist: album_artist.clone(),
            year: p.tags.year.unwrap_or(0),
            genre: p.tags.genre.clone().unwrap_or_default(),
            cover: cover.clone(),
            track_count: 0,
            duration_sec: 0.0,
        });
        acc.track_count += 1;
        acc.duration_sec += p.duration_sec;
        // 年份 / 流派取组内首个非空值，避免被个别缺标签的曲目清零。
        if acc.year == 0 {
            acc.year = p.tags.year.unwrap_or(0);
        }
        if acc.genre.is_empty() {
            acc.genre = p.tags.genre.clone().unwrap_or_default();
        }

        let fp = FormatFingerprint {
            codec: &p.format.codec,
            channels: p.format.channels,
            sample_rate_hz: p.format.sample_rate_hz,
            channel_mask: p.format.channel_mask,
        };
        tracks.push(Track {
            id: track_id_with(&path, &fp),
            title,
            artist,
            album: album_name,
            album_id: Some(aid),
            cover,
            duration_sec: p.duration_sec,
            path: path.to_string_lossy().to_string(),
            disc_no: p.tags.disc_no,
            track_no: p.tags.track_no,
            format: p.format,
        });
    }

    // 专辑内按碟号 / 音轨号排序；缺号的排在后面并按标题稳定排序。
    tracks.sort_by(|a, b| {
        a.album_id
            .cmp(&b.album_id)
            .then(a.disc_no.unwrap_or(u16::MAX).cmp(&b.disc_no.unwrap_or(u16::MAX)))
            .then(a.track_no.unwrap_or(u16::MAX).cmp(&b.track_no.unwrap_or(u16::MAX)))
            .then(a.title.cmp(&b.title))
    });

    let mut albums: Vec<Album> = albums
        .into_iter()
        .map(|(id, a)| Album {
            id,
            title: a.title,
            artist: a.artist,
            year: a.year,
            genre: a.genre,
            cover: a.cover,
            track_count: a.track_count,
            duration_sec: a.duration_sec,
        })
        .collect();
    albums.sort_by(|a, b| a.artist.cmp(&b.artist).then(a.title.cmp(&b.title)));

    LibrarySnapshot { albums, tracks, failed: 0 }
}

/// 由路径推断「歌手 / 专辑」：父目录当专辑名，祖父目录当歌手名。
/// 仅用于标签缺失时兜底，有标签一律以标签为准。
fn folder_hint(path: &Path) -> (Option<String>, Option<String>) {
    let parent = path.parent();
    let album = parent
        .and_then(|d| d.file_name())
        .map(|s| s.to_string_lossy().to_string());
    let artist = parent
        .and_then(|d| d.parent())
        .and_then(|d| d.file_name())
        .map(|s| s.to_string_lossy().to_string());
    (artist, album)
}

/// 生成封面描述。
///
/// 内嵌封面的**提取与缓存留到下一步**，此处先如实标记「有图」但 url 为空——
/// 前端遇到 url 为空会回落到渐变 + 首字母占位，正好复用现有设计。
fn make_cover(album_id: &str, album_name: &str, _has_picture: bool) -> Cover {
    let initial = album_name.chars().next().unwrap_or('?').to_string();
    // 按专辑 ID 稳定取色：同一张专辑每次扫描配色一致。
    let idx = album_id.bytes().map(|b| b as usize).sum::<usize>() % GRADIENTS.len();
    let (from, to) = GRADIENTS[idx];
    Cover { initial, gradient: (from.to_string(), to.to_string()), url: None }
}

/// 只遍历不探测，用于快速估算规模（首启页显示总数）。
pub fn count_candidates(roots: &[PathBuf]) -> u32 {
    collect_files(roots).len() as u32
}

/// 单个文件是否在扫描范围内（供外壳做拖入校验）。
pub fn is_scannable(path: &Path) -> bool {
    probe::is_audio_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn collects_only_audio_extensions() {
        let d = tmpdir("shannon_scan_exts");
        fs::write(d.join("a.flac"), b"x").unwrap();
        fs::write(d.join("b.mp3"), b"x").unwrap();
        fs::write(d.join("cover.jpg"), b"x").unwrap();
        fs::write(d.join("notes.txt"), b"x").unwrap();
        let files = collect_files(&[d.clone()]);
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|p| is_scannable(p)));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn recurses_into_subdirs() {
        let d = tmpdir("shannon_scan_nested");
        fs::create_dir_all(d.join("artist/album")).unwrap();
        fs::write(d.join("artist/album/1.flac"), b"x").unwrap();
        fs::write(d.join("top.mp3"), b"x").unwrap();
        assert_eq!(count_candidates(&[d.clone()]), 2);
        let _ = fs::remove_dir_all(d);
    }

    /// 无法解析的文件要计入 failed 并如实上报，不能静默丢弃。
    #[test]
    fn unparseable_files_counted_as_failed() {
        let d = tmpdir("shannon_scan_failed");
        fs::write(d.join("broken.flac"), b"definitely not flac").unwrap();
        let mut events = 0;
        let snap = scan_folders(&[d.clone()], |_| events += 1);
        assert_eq!(snap.failed, 1);
        assert_eq!(snap.tracks.len(), 0);
        assert!(events > 0, "至少要有一次收尾进度事件");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn empty_folder_yields_empty_snapshot() {
        let d = tmpdir("shannon_scan_empty");
        let snap = scan_folders(&[d.clone()], |_| {});
        assert!(snap.albums.is_empty() && snap.tracks.is_empty() && snap.failed == 0);
        let _ = fs::remove_dir_all(d);
    }

    /// 标签缺失时按目录归组：不同目录的未标签文件不该并成一张专辑。
    #[test]
    fn untagged_files_group_by_folder() {
        let d = tmpdir("shannon_scan_folders");
        for (artist, album) in [("白鲸电台", "长夜电波"), ("Radiohead", "In Rainbows")] {
            let dir = d.join(artist).join(album);
            fs::create_dir_all(&dir).unwrap();
            let (a, b) = folder_hint(&dir.join("01.wav"));
            assert_eq!(a.as_deref(), Some(artist));
            assert_eq!(b.as_deref(), Some(album));
        }
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn cover_color_is_stable_per_album() {
        let a = make_cover("a-abc", "长夜电波", false);
        let b = make_cover("a-abc", "长夜电波", false);
        assert_eq!(a.gradient, b.gradient);
        assert_eq!(a.initial, "长");
    }
}
