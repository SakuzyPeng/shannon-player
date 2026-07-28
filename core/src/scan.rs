//! 曲库扫描：遍历目录 → 并行探测 → 聚合为专辑。
//!
//! 进度上报通过回调注入，core 不知道 Tauri 的存在。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use rayon::prelude::*;
use walkdir::WalkDir;

use crate::cache::{RawTags, RawTrack, ScanCache};
use crate::cover;
use crate::id::{album_id, track_id_with, FormatFingerprint};
use crate::model::{
    Album, Cover, Encoding, FieldSource, FieldSources, LibrarySnapshot, ScanProgress,
    SpatialFormat, Track,
};
use crate::overrides::Overrides;
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
/// 产出的是**原始缓存**而非曲库快照：套用用户覆盖、聚合成专辑都是纯内存计算
/// （见 `ScanCache::library`），分开之后改一次元数据不必重扫整库，重启也不必。
pub fn scan_folders<F>(roots: &[PathBuf], cover_dir: Option<&Path>, mut on_progress: F) -> ScanCache
where
    F: FnMut(ScanProgress) + Send,
{
    // 先规范化根目录：目录兜底要判断「这一层是不是扫描根」，路径写法不一致会判错。
    let roots: Vec<PathBuf> = roots
        .iter()
        .map(|r| std::fs::canonicalize(r).unwrap_or_else(|_| r.clone()))
        .collect();
    let files = collect_files(&roots);
    let total = files.len() as u32;

    let done = AtomicU32::new(0);
    let failed = AtomicU32::new(0);
    // 已成功解析的曲目数。进度事件要有它才能边扫边显示数字——专辑数得等聚合，
    // 扫描途中给不出，如实报 0 由前端决定怎么显示。
    let ok = AtomicU32::new(0);
    let cover_failed = AtomicU32::new(0);
    // 已处理过的封面指纹。同一张封面往往被整张专辑的每一首重复内嵌
    // （实测 939 首只对应 33 张唯一封面），去重后解码开销可以忽略。
    let covers_done: Mutex<std::collections::HashSet<String>> = Mutex::new(HashSet::new());
    // 进度回调不是 Sync，用 Mutex 串行化；回调本身很轻（发一个事件）。
    let progress = Mutex::new(&mut on_progress);

    let mut tracks: Vec<RawTrack> = files
        .par_iter()
        .filter_map(|path| {
            let result = probe::probe(path);
            if result.is_ok() {
                ok.fetch_add(1, Ordering::Relaxed);
            }
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            // 每 16 个文件报一次，避免事件风暴淹没前端。
            if n % 16 == 0 || n == total {
                if let Ok(mut cb) = progress.lock() {
                    cb(ScanProgress {
                        done: n,
                        total,
                        tracks: ok.load(Ordering::Relaxed),
                        albums: 0,
                        current: path.to_string_lossy().to_string(),
                    });
                }
            }
            match result {
                Ok(p) => {
                    // 封面字节只在这里还拿得到（缓存里只留指纹），趁机写出缩略图。
                    if let (Some(dir), Some(key), Some(pic)) =
                        (cover_dir, p.cover_key.as_deref(), p.tags.picture.as_deref())
                    {
                        let first = covers_done
                            .lock()
                            .map(|mut seen| seen.insert(key.to_string()))
                            .unwrap_or(false);
                        if first
                            && !cover::thumbs_exist(dir, key)
                            && cover::write_thumbs(dir, key, pic).is_err()
                        {
                            // 封面坏了不该让整次扫描失败，界面回落占位渐变；如实计数上报。
                            cover_failed.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Some(raw_track(path.clone(), p))
                }
                Err(_) => {
                    failed.fetch_add(1, Ordering::Relaxed);
                    None
                }
            }
        })
        .collect();
    tracks.sort_by(|a, b| a.path.cmp(&b.path));

    let cache = ScanCache {
        roots,
        tracks,
        failed: failed.load(Ordering::Relaxed),
        cover_failed: cover_failed.load(Ordering::Relaxed),
    };
    let snapshot = cache.library(&Overrides::default());
    on_progress(ScanProgress {
        done: total,
        total,
        tracks: snapshot.tracks.len() as u32,
        albums: snapshot.albums.len() as u32,
        current: String::new(),
    });
    cache
}

/// 探测结果 → 可落盘的原始记录。封面字节在此丢弃，只留指纹（见 `cache` 模块）。
fn raw_track(path: PathBuf, p: Probed) -> RawTrack {
    let fp = FormatFingerprint {
        codec: &p.format.codec,
        channels: p.format.channels,
        sample_rate_hz: p.format.sample_rate_hz,
        channel_mask: p.format.channel_mask,
    };
    RawTrack {
        id: track_id_with(&path, &fp),
        path,
        tags: RawTags {
            title: p.tags.title,
            artist: p.tags.artist,
            album_artist: p.tags.album_artist,
            album: p.tags.album,
            year: p.tags.year,
            genre: p.tags.genre,
            track_no: p.tags.track_no,
            disc_no: p.tags.disc_no,
        },
        has_cover: p.tags.picture.is_some(),
        cover_key: p.cover_key,
        format: p.format,
        duration_sec: p.duration_sec,
    }
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

/// 合辑的专辑艺人。沿用国际通行的标签惯例，不本地化（属于内容而非界面文案）。
pub const VARIOUS_ARTISTS: &str = "Various Artists";

/// 判为合辑的门槛：组内最高票艺人占比低于此值时，认定没有主艺人。
/// 0.6 的取舍：同人社团专辑里主创常占七八成（应归社团名下），
/// 真正的拼盘合辑则高度分散（应标合辑）。
const MAJORITY_RATIO: f64 = 0.6;

/// 逐文件的中间结果。第一遍定出各字段与来源，第二遍才能做组级判断。
struct Pending<'a> {
    raw: &'a RawTrack,
    title: String,
    title_src: FieldSource,
    artist: String,
    artist_src: FieldSource,
    album: String,
    album_src: FieldSource,
    /// 标签或用户给出的专辑艺人；None 表示要靠组内多数决推断。
    album_artist: Option<String>,
    album_artist_src: FieldSource,
    group: String,
}

/// 把探测结果聚合为曲目 + 专辑。
///
/// **两遍**：第一遍逐文件定出字段与来源并套用用户覆盖，第二遍按组决定专辑艺人。
/// 第二遍不可省——专辑艺人是**组级**结论：单看一首歌无法判断它属于某位歌手的专辑
/// 还是一张合辑，而逐曲回落到曲目艺人正是合辑被拆成十几张的根因。
pub(crate) fn aggregate(
    raw: &[RawTrack],
    roots: &[PathBuf],
    overrides: &Overrides,
) -> LibrarySnapshot {
    // ---- 第一遍：逐文件定字段 + 套用覆盖 ----
    let mut pending: Vec<Pending> = Vec::with_capacity(raw.len());
    for r in raw {
        // 覆盖以曲目 ID 为键，而 ID 只由文件内容与格式决定、与元数据无关——
        // 所以用户改标题不会让覆盖记录自己的键失效。
        let ov = overrides.get(&r.id);

        let file_stem =
            r.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let (dir_artist, dir_album) = folder_hint(&r.path, roots);

        let (title, title_src) = pick(
            ov.and_then(|o| o.title.clone()),
            r.tags.title.clone(),
            if file_stem.is_empty() { None } else { Some((file_stem, FieldSource::FileName)) },
            "未知曲目",
        );
        // 无标签时按 Artist/Album/Track 目录约定兜底：把整盘未标签文件都并进
        // 「未知专辑」会毁掉专辑视图，而目录名通常正是用户的组织方式。
        let (artist, artist_src) = pick(
            ov.and_then(|o| o.artist.clone()),
            r.tags.artist.clone(),
            dir_artist.map(|v| (v, FieldSource::Folder)),
            "未知歌手",
        );
        let (album, album_src) = pick(
            ov.and_then(|o| o.album.clone()),
            r.tags.album.clone(),
            dir_album.map(|v| (v, FieldSource::Folder)),
            "未知专辑",
        );
        let (album_artist, album_artist_src) = match ov.and_then(|o| o.album_artist.clone()) {
            Some(v) => (Some(v), FieldSource::UserEdit),
            None => match r.tags.album_artist.clone() {
                Some(v) => (Some(v), FieldSource::Tag),
                // 留空交给第二遍的组级判断，**不再逐曲回落到曲目艺人**。
                None => (None, FieldSource::Majority),
            },
        };
        let user_aa =
            (album_artist_src == FieldSource::UserEdit).then(|| album_artist.clone()).flatten();
        let group = album_group_key(&album, user_aa.as_deref(), album_artist.as_deref(), &r.path);
        pending.push(Pending {
            raw: r,
            title,
            title_src,
            artist,
            artist_src,
            album,
            album_src,
            album_artist,
            album_artist_src,
            group,
        });
    }

    // ---- 第二遍：合并同名专辑，再按组决定专辑艺人 ----
    let mut buckets: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, it) in pending.iter().enumerate() {
        buckets.entry(it.group.clone()).or_default().push(i);
    }
    let mut infos: Vec<GroupInfo> = buckets
        .into_iter()
        .map(|(key, members)| GroupInfo {
            album_norm: strip_disc_suffix(&pending[members[0]].album).trim().to_lowercase(),
            covers: members
                .iter()
                .filter_map(|&i| pending[i].raw.cover_key.clone())
                .collect(),
            dir_names: members
                .iter()
                .filter_map(|&i| {
                    pending[i]
                        .raw
                        .path
                        .parent()
                        .and_then(|d| d.file_name())
                        .map(|n| n.to_string_lossy().trim().to_lowercase())
                })
                .collect(),
            user_aa: (pending[members[0]].album_artist_src == FieldSource::UserEdit)
                .then(|| pending[members[0]].album_artist.clone())
                .flatten(),
            mergeable: pending[members[0]].album_src != FieldSource::Unknown,
            key,
            members,
        })
        .collect();
    // 固定顺序：并查集按下标合并，顺序不定会让专辑 ID 随机漂移。
    infos.sort_by(|a, b| a.key.cmp(&b.key));
    let repr = merge_groups_by_cover(&infos);

    // 把被合并的组并成一个成员表，代表键取组内最小键（有序 → ID 可重复）。
    let mut merged: HashMap<usize, (String, Vec<usize>)> = HashMap::new();
    for (i, info) in infos.iter().enumerate() {
        let entry = merged.entry(repr[i]).or_insert_with(|| (info.key.clone(), Vec::new()));
        entry.1.extend(info.members.iter().copied());
    }

    let mut albums: Vec<Album> = Vec::with_capacity(merged.len());
    let mut album_of: HashMap<String, String> = HashMap::new(); // 原始组键 -> 专辑 ID
    let mut tracks: Vec<Track> = Vec::with_capacity(pending.len());

    let mut merged: Vec<(String, Vec<usize>)> = merged.into_values().collect();
    merged.sort_by(|a, b| a.0.cmp(&b.0));
    let mut duplicates = 0u32;
    // 被折叠掉的重复曲目不进曲目列表，这里记下保留的下标。
    let mut kept_tracks: HashSet<usize> = HashSet::new();
    for (key, mut idxs) in merged {
        idxs.sort();
        // 折叠同一张专辑里的重复曲目（多份拷贝、整盘版与分碟版并存等）。
        let (kept, dropped) = dedupe_within_album(&pending, &idxs);
        duplicates += dropped;
        kept_tracks.extend(kept.iter().copied());
        let idxs = &kept;
        let aid = album_id(&key);
        for &i in idxs {
            album_of.insert(pending[i].group.clone(), aid.clone());
        }

        let tagged: Vec<&str> = idxs
            .iter()
            .filter_map(|&i| pending[i].album_artist.as_deref())
            .filter(|s| !s.trim().is_empty())
            .collect();
        let (artist, artist_source, compilation) = if !tagged.is_empty() {
            // 有专辑艺人标签（或用户指定）：取多数，来源取该值对应的最高优先级来源。
            let value = majority(&tagged).unwrap_or_else(|| tagged[0].to_string());
            let user_set = idxs.iter().any(|&i| {
                pending[i].album_artist_src == FieldSource::UserEdit
                    && pending[i].album_artist.as_deref() == Some(value.as_str())
            });
            let src = if user_set { FieldSource::UserEdit } else { FieldSource::Tag };
            (value, src, false)
        } else {
            let artists: Vec<&str> = idxs.iter().map(|&i| pending[i].artist.as_str()).collect();
            let distinct: std::collections::HashSet<&str> = artists.iter().copied().collect();
            if distinct.len() == 1 {
                // 单人专辑：专辑艺人就是这位歌手，来源沿用曲目艺人的来源。
                (artists[0].to_string(), pending[idxs[0]].artist_src, false)
            } else {
                let top = majority(&artists).unwrap_or_default();
                let ratio = artists.iter().filter(|a| **a == top).count() as f64
                    / artists.len().max(1) as f64;
                if ratio >= MAJORITY_RATIO {
                    (top, FieldSource::Majority, false)
                } else {
                    // 高度分散：这是拼盘合辑，不该挂在任何一位歌手名下。
                    (VARIOUS_ARTISTS.to_string(), FieldSource::Majority, true)
                }
            }
        };

        // 年份 / 流派取组内首个非空值，避免被个别缺标签的曲目清空。
        let year = idxs.iter().find_map(|&i| pending[i].raw.tags.year);
        let genre = idxs
            .iter()
            .find_map(|&i| pending[i].raw.tags.genre.clone().filter(|g| !g.is_empty()))
            .unwrap_or_default();
        // 显示名取组内剥离碟号后的多数名：各碟合并后该叫主专辑名，而不是某一碟的名字。
        let names: Vec<&str> = idxs.iter().map(|&i| strip_disc_suffix(&pending[i].album)).collect();
        let title = majority(&names).unwrap_or_else(|| names[0].to_string());
        // 专辑封面取组内首个有内嵌封面的曲目（按曲目顺序，结果稳定）。
        let album_cover_key = idxs.iter().find_map(|&i| pending[i].raw.cover_key.clone());
        let cover = make_cover(&aid, &title, album_cover_key.clone());

        albums.push(Album {
            id: aid,
            title,
            artist,
            year,
            genre,
            cover,
            track_count: idxs.len() as u32,
            duration_sec: idxs.iter().map(|&i| pending[i].raw.duration_sec).sum(),
            compilation,
            artist_source,
        });
    }

    for (i, it) in pending.iter().enumerate() {
        if !kept_tracks.contains(&i) {
            continue;
        }
        let aid = album_of.get(&it.group).cloned();
        // 曲目**优先用自己的封面**：同一张专辑里个别曲目嵌了不同版本封面是常见的
        // （实测 unformed 28 首里有两种），显示专辑封面等于抹掉这个差异；
        // 自己没有内嵌封面时才回落到专辑封面。
        let album_cover = albums.iter().find(|a| Some(&a.id) == aid.as_ref()).map(|a| &a.cover);
        let cover = match album_cover {
            Some(c) if it.raw.cover_key.is_some() => {
                Cover { cover_key: it.raw.cover_key.clone(), ..c.clone() }
            }
            Some(c) => c.clone(),
            None => make_cover(aid.as_deref().unwrap_or(""), &it.album, it.raw.cover_key.clone()),
        };
        let album_artist_src = albums
            .iter()
            .find(|a| Some(&a.id) == aid.as_ref())
            .map(|a| a.artist_source)
            .unwrap_or(it.album_artist_src);
        let ov = overrides.get(&it.raw.id);
        tracks.push(Track {
            id: it.raw.id.clone(),
            title: it.title.clone(),
            artist: it.artist.clone(),
            album: it.album.clone(),
            album_id: aid,
            cover,
            duration_sec: it.raw.duration_sec,
            path: it.raw.path.to_string_lossy().to_string(),
            disc_no: ov.and_then(|o| o.disc_no).or(it.raw.tags.disc_no),
            track_no: ov.and_then(|o| o.track_no).or(it.raw.tags.track_no),
            format: it.raw.format.clone(),
            sources: FieldSources {
                title: it.title_src,
                artist: it.artist_src,
                album: it.album_src,
                album_artist: album_artist_src,
            },
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
    albums.sort_by(|a, b| a.artist.cmp(&b.artist).then(a.title.cmp(&b.title)));

    LibrarySnapshot { albums, tracks, failed: 0, duplicates }
}

/// 按「用户覆盖 > 标签 > 兜底推断 > 未知」定值并记录来源。
fn pick(
    user: Option<String>,
    tag: Option<String>,
    guess: Option<(String, FieldSource)>,
    unknown: &str,
) -> (String, FieldSource) {
    let nonempty = |s: String| if s.trim().is_empty() { None } else { Some(s) };
    if let Some(v) = user.and_then(nonempty) {
        return (v, FieldSource::UserEdit);
    }
    if let Some(v) = tag.and_then(nonempty) {
        return (v, FieldSource::Tag);
    }
    if let Some((v, src)) = guess {
        if !v.trim().is_empty() {
            return (v, src);
        }
    }
    (unknown.to_string(), FieldSource::Unknown)
}

/// 同一张专辑内折叠重复曲目。返回保留的下标与折叠掉的数量。
///
/// 曲库里同一首歌存在多份拷贝很常见（Apple Music 导入会留下 `xxx 1.m4a` 这样的副本，
/// 同一张专辑也可能既有整盘版本又有分碟版本）。这些副本的音频内容一样，但元数据块
/// 差几十字节，文件大小因此不同，**曲目 ID 是内容哈希所以并不相同**，光靠 ID 去不掉。
///
/// 判据始终带「时长（0.01 秒）」：先比精确标题，再比去掉末尾译名括注后的标题，
/// 最后比碟号 + 音轨号。同一录音的副本时长一致，而两首不同录音同时长到百分位的
/// 概率极低。只在同一张专辑内比较，跨专辑的同名曲不会被牵连。
///
/// **三条判据都还要求音频规格一致**（容器、编码族、编码、采样率、位深、声道数、
/// 声道掩码、空间音频标记），这一条是后加的：
/// 只看标题与时长的话，同一首歌的不同**格式**版本会被当成副本折叠掉——
/// 实测把一段音乐转成 11 种编码放进同一目录，扫出来只剩 4 首。那不是清理，是丢东西。
///
/// 分界在于**用户知不知情**：导入工具留下的副本是同一份音频的字节级拷贝，
/// 规格必然完全相同，用户既没要它也不想看见它；而同时留着 FLAC 与 MP3、
/// 16 bit 与 24 bit 的人是有意为之（车载用一份、听音用一份），
/// 那是两个不同的东西，折叠等于替用户做了他没同意的删除。
///
/// 已知残留：同一编码的不同码率（MP3 128k 与 320k）规格键相同，仍会被折叠。
/// 没把码率纳入键是因为 lofty 的码率由文件大小估算，副本之间差几十字节就可能
/// 抖动 1 kbps，纳入反而会让真副本漏掉。
fn dedupe_within_album(pending: &[Pending], idxs: &[usize]) -> (Vec<usize>, u32) {
    /// 标题的宽松键：末尾括注常只是同一标题的翻译，如
    /// `Winter Alice` / `Winter Alice（冬日爱丽丝）`。括注前的版本标识仍保留，
    /// `Alive ~rearranged~（活着·重编版）` 不会退化成 `Alive`。
    fn title_key(title: &str) -> String {
        let mut base = title.trim();
        loop {
            let mut stripped = None;
            for (open, close) in [('(', ')'), ('（', '）'), ('[', ']'), ('【', '】')] {
                if let Some(before_close) = base.strip_suffix(close) {
                    if let Some(at) = before_close.rfind(open) {
                        let candidate = before_close[..at].trim_end();
                        if !candidate.is_empty() {
                            stripped = Some(candidate);
                            break;
                        }
                    }
                }
            }
            match stripped {
                Some(next) => base = next,
                None => break,
            }
        }
        base.chars()
            .flat_map(char::to_lowercase)
            .filter(|c| !c.is_whitespace())
            .map(|c| match c {
                '～' | '〜' => '~',
                _ => c,
            })
            .collect()
    }

    /// 多份副本的标签可能互相冲突；取出现次数最多的编号，票数相同时取较小值，
    /// 保证结果稳定。缺失值不投票，但带音轨号、缺碟号的副本仍能参与音轨号多数决。
    fn mode(values: impl Iterator<Item = u16>) -> Option<u16> {
        let mut counts: HashMap<u16, usize> = HashMap::new();
        for value in values {
            *counts.entry(value).or_insert(0) += 1;
        }
        counts
            .into_iter()
            .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
            .map(|(value, _)| value)
    }

    /// 保留信息更全的那一份：有封面 > 有碟号 > 有音轨号 > 有专辑艺人标签。
    fn quality(p: &Pending) -> u8 {
        (p.raw.cover_key.is_some() as u8) * 8
            + (p.raw.tags.disc_no.is_some() as u8) * 4
            + (p.raw.tags.track_no.is_some() as u8) * 2
            + (p.raw.tags.album_artist.is_some() as u8)
    }
    // 并查集：三条判据任一命中就并到一组。
    let mut parent: Vec<usize> = (0..idxs.len()).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let union = |parent: &mut Vec<usize>, a: usize, b: usize| {
        let (ra, rb) = (find(parent, a), find(parent, b));
        if ra != rb {
            let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
            parent[hi] = lo;
        }
    };

    /// 音频规格键。规格不同就不是同一份编码，不该被当成副本。
    ///
    /// 声道数本身不够：6 声道可能是 5.1 也可能是 6.0；空间音频又与声道维度
    /// 正交，普通 E-AC-3 与 E-AC-3/JOC 甚至可能报出完全相同的声道数。漏掉这两项
    /// 会让去重把用户明确保留的环绕/Atmos 版本吞掉。
    #[derive(Clone, PartialEq, Eq, Hash)]
    struct SpecKey {
        container: String,
        codec: String,
        encoding: Encoding,
        sample_rate_hz: u32,
        bit_depth: Option<u8>,
        channels: u8,
        channel_mask: Option<u32>,
        spatial: Option<SpatialFormat>,
    }

    fn spec_key(p: &Pending) -> SpecKey {
        let f = &p.raw.format;
        SpecKey {
            container: f.container.clone(),
            codec: f.codec.clone(),
            encoding: f.encoding,
            sample_rate_hz: f.sample_rate_hz,
            bit_depth: f.bit_depth,
            channels: f.channels,
            channel_mask: f.channel_mask,
            spatial: f.spatial,
        }
    }

    let mut by_title: HashMap<(SpecKey, String, i64), usize> = HashMap::new();
    let mut by_title_key: HashMap<(SpecKey, String, i64), usize> = HashMap::new();
    let mut by_slot: HashMap<(SpecKey, u16, u16, i64), usize> = HashMap::new();
    for (pos, &i) in idxs.iter().enumerate() {
        let dur = (pending[i].raw.duration_sec * 100.0).round() as i64;
        let spec = spec_key(&pending[i]);

        // 判据一：规格 + 标题 + 时长。
        let tkey = (spec.clone(), pending[i].title.trim().to_lowercase(), dur);
        if let Some(&prev) = by_title.get(&tkey) {
            union(&mut parent, pos, prev);
        } else {
            by_title.insert(tkey, pos);
        }

        // 判据二：去掉末尾译名括注的标题 + 时长。真实库里恰好有两组副本因为
        // `Winter Alice` / `Winter Alice（冬日爱丽丝）` 这类写法且轨位标签互相冲突，
        // 精确标题和轨位两条规则都认不出来。版本标识在括注前，所以仍会保留。
        let ckey = (spec.clone(), title_key(&pending[i].title), dur);
        if let Some(&prev) = by_title_key.get(&ckey) {
            union(&mut parent, pos, prev);
        } else {
            by_title_key.insert(ckey, pos);
        }

        // 判据三：碟号 + 音轨号 + 时长。抓的是「同一首歌被标了不同标题」——
        // 实测同一轨位出现过 `ロミオとシンデレラ`、`ロミオとシンデレラ（罗密欧与仙杜瑞拉）`、
        // `ロミオとシンデレラ（罗密欧与灰姑娘）` 三种写法，只比标题永远认不出是一首。
        // 同一张专辑的同一个轨位就是同一首歌；再要求时长一致，防的是标签把两首
        // 不同的歌错标到同一轨位（那种情况下时长几乎不可能同到百分位）。
        if let (Some(d), Some(t)) = (pending[i].raw.tags.disc_no, pending[i].raw.tags.track_no) {
            let skey = (spec.clone(), d, t, dur);
            if let Some(&prev) = by_slot.get(&skey) {
                union(&mut parent, pos, prev);
            } else {
                by_slot.insert(skey, pos);
            }
        }
    }

    // 先收集重复组。每组的轨位可能冲突，不能只按路径随便留一份：实测同一录音既有
    // `1-05` 又有 `1-11`，而完整副本的多数票能给出正确位置。
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for (pos, &i) in idxs.iter().enumerate() {
        let root = find(&mut parent, pos);
        groups.entry(root).or_default().push(i);
    }

    let mut kept = Vec::with_capacity(groups.len());
    for members in groups.into_values() {
        let preferred_track = mode(members.iter().filter_map(|&i| pending[i].raw.tags.track_no));
        let preferred_disc = preferred_track.and_then(|track| {
            mode(members.iter().filter_map(|&i| {
                (pending[i].raw.tags.track_no == Some(track)).then_some(pending[i].raw.tags.disc_no).flatten()
            }))
        });
        let score = |i: usize| {
            (
                preferred_track.is_some_and(|n| pending[i].raw.tags.track_no == Some(n)),
                preferred_disc.is_some_and(|n| pending[i].raw.tags.disc_no == Some(n)),
                quality(&pending[i]),
            )
        };
        let mut best = members[0];
        for &candidate in &members[1..] {
            if score(candidate) > score(best)
                || (score(candidate) == score(best)
                    && pending[candidate].raw.path < pending[best].raw.path)
            {
                best = candidate;
            }
        }
        kept.push(best);
    }
    kept.sort_unstable();
    let dropped = (idxs.len() - kept.len()) as u32;
    (kept, dropped)
}

/// 剥离专辑名末尾的碟号后缀，用于把「同一张专辑的各碟」认作一张。
///
/// `Rebirth Story 5 Disc 1 SUN` → `Rebirth Story 5`。碟名被写进专辑标签是常见做法
/// （实测同一张专辑既有标注 `disc 1/2` 的规范版本，也有把碟名塞进专辑名的版本），
/// 不剥离就会变成三张专辑。
///
/// 只在「独立的词」上匹配，`Discovery`、`CD Player` 这种不会被误伤。
fn strip_disc_suffix(album: &str) -> &str {
    let mut cut = None;
    let mut idx = 0usize;
    let words: Vec<&str> = album.split_whitespace().collect();
    for (w, word) in words.iter().enumerate() {
        // 定位该词在原串中的起点，截断时要保留原有大小写与间隔。
        idx = album[idx..].find(*word).map(|off| idx + off).unwrap_or(idx);
        let bare = word.trim_matches(|c: char| c == '(' || c == '[' || c == ')' || c == ']');
        let lower = bare.to_lowercase();
        let is_kw = matches!(lower.as_str(), "disc" | "disk" | "cd");
        // 「disc 1」这样分开写，或「cd2」连写。
        let joined_num = ["disc", "disk", "cd"].iter().any(|k| {
            lower.strip_prefix(k).map(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())).unwrap_or(false)
        });
        let next_is_num = words
            .get(w + 1)
            .map(|n| n.chars().next().is_some_and(|c| c.is_ascii_digit()))
            .unwrap_or(false);
        if (is_kw && next_is_num) || joined_num {
            cut = Some(idx);
            break;
        }
        idx += word.len();
    }
    match cut {
        // 剥离后不能变成空串（专辑就叫「Disc 1」的话，原样保留更诚实）。
        Some(at) if !album[..at].trim().is_empty() => album[..at].trim_end(),
        _ => album,
    }
}

/// 组内多数值；票数相同时取字典序最小的，保证每次扫描结果一致。
fn majority(values: &[&str]) -> Option<String> {
    let mut count: HashMap<&str, usize> = HashMap::new();
    for v in values {
        *count.entry(*v).or_insert(0) += 1;
    }
    count
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(a.0)))
        .map(|(v, _)| v.to_string())
}

/// 专辑归组键 = 专辑名 + 作用域。**保守**：宁可分得细，合并交给下一步。
///
/// 作用域依次取「用户指定的专辑艺人 > 标签里的专辑艺人 > 所在目录」。
/// 不退化成「只按专辑名」，否则两位歌手各自的《Greatest Hits》会撞成一张。
fn album_group_key(
    album: &str,
    user_album_artist: Option<&str>,
    tag_album_artist: Option<&str>,
    path: &Path,
) -> String {
    fn named(s: Option<&str>) -> Option<&str> {
        s.map(str::trim).filter(|v| !v.is_empty())
    }
    let scope = if let Some(a) = named(user_album_artist) {
        format!("aa:{}", a.to_lowercase())
    } else if let Some(a) = named(tag_album_artist) {
        format!("aa:{}", a.to_lowercase())
    } else {
        format!(
            "dir:{}",
            path.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default()
        )
    };
    format!("{}\x1f{}", album.trim().to_lowercase(), scope)
}

/// 同名专辑之间按封面指纹合并。
///
/// 封面**只作合并证据，不作拆分依据**——这是踩过坑换来的规矩：一张专辑内部
/// 完全可能有几首嵌了不同版本的封面（实测 `unformed` 28 首里有两种），
/// 若把封面当归组键，本来好好的专辑会被劈成两半；反过来，同名专辑之间只要
/// 存在一张共同的封面，就几乎可以断定是同一张（实测 116 首、横跨 6 个艺人目录
/// 的合辑，仅主创目录写了 album_artist，只有封面认得出它们同属一张）。
///
/// 两条护栏：只在专辑名相同时比较（不同名的专辑共用封面也不合并）；
/// 任一侧带用户指定的专辑艺人时，必须两侧一致才合并——用户指定即锁定。
fn merge_groups_by_cover(groups: &[GroupInfo]) -> Vec<usize> {
    // 并查集：parent[i] 指向所属合并组的代表。
    let mut parent: Vec<usize> = (0..groups.len()).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }

    let mut by_album: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, g) in groups.iter().enumerate() {
        if g.mergeable {
            by_album.entry(g.album_norm.as_str()).or_default().push(i);
        }
    }
    for (_, bucket) in by_album {
        for (bi, &i) in bucket.iter().enumerate() {
            for &j in &bucket[bi + 1..] {
                let (a, b) = (&groups[i], &groups[j]);
                // 用户指定即锁定：一侧指定了专辑艺人，就必须两侧相同才合并。
                if (a.user_aa.is_some() || b.user_aa.is_some()) && a.user_aa != b.user_aa {
                    continue;
                }
                // 封面是首选证据。
                let by_cover = !a.covers.is_disjoint(&b.covers);
                // 退路：**恰好一方**有封面、另一方一张都没有，且两者的目录名相同。
                // 这对应「整张专辑好好地待在一处，另有几首零散文件缺封面又缺标签、
                // 被搁在别的艺人目录下」——实测 doriko BEST 就有两首这样的漏网之鱼，
                // 只看封面永远并不回去，只能各自变成一张无封面的假专辑。
                //
                // 必须是 XOR 而不是「任一方为空」：两边都没有封面时，没有任何证据
                // 表明它们是一体的，此时目录名相同也可能只是两位歌手各有一张同名
                // 专辑（`甲/greatest hits` 与 `乙/greatest hits`），保守分开，
                // 想合并交给用户改专辑艺人。
                let one_side_coverless = a.covers.is_empty() != b.covers.is_empty();
                let by_dir_name = one_side_coverless && !a.dir_names.is_disjoint(&b.dir_names);
                if !by_cover && !by_dir_name {
                    continue;
                }
                let (ra, rb) = (find(&mut parent, i), find(&mut parent, j));
                if ra != rb {
                    // 统一并到下标小的一侧，组键有序，结果可重复。
                    let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
                    parent[hi] = lo;
                }
            }
        }
    }
    (0..groups.len()).map(|i| find(&mut parent, i)).collect()
}

/// 合并判定所需的组级信息。
struct GroupInfo {
    key: String,
    album_norm: String,
    covers: std::collections::HashSet<String>,
    /// 组内成员所在目录的**目录名**（不含路径）。没有封面可比时的退路证据。
    dir_names: std::collections::HashSet<String>,
    user_aa: Option<String>,
    /// 专辑名是「未知专辑」兜底时不参与合并：一堆互不相干的散装文件本来就没有
    /// 专辑，按封面拼只会造出假专辑。
    mergeable: bool,
    members: Vec<usize>,
}

/// 由路径推断「歌手 / 专辑」：父目录当专辑名，祖父目录当歌手名。
/// 仅用于标签缺失时兜底，有标签一律以标签为准。
///
/// **扫描根目录本身不参与推断**：用户选的是音乐库根（如 `~/Music`），
/// 它的名字既不是专辑名也不是歌手名——曾经会把散落在根目录的文件聚成
/// 一张以库目录命名的假专辑，歌手则变成上一级目录名。
fn folder_hint(path: &Path, roots: &[PathBuf]) -> (Option<String>, Option<String>) {
    let is_root = |d: &Path| roots.iter().any(|r| r.as_path() == d);
    let name = |d: &Path| d.file_name().map(|s| s.to_string_lossy().to_string());
    let parent = path.parent().filter(|d| !is_root(d));
    let album = parent.and_then(name);
    let artist = parent.and_then(|d| d.parent()).filter(|d| !is_root(d)).and_then(name);
    (artist, album)
}

/// 生成封面描述。
///
/// 占位渐变**始终生成**，哪怕有内嵌封面：缩略图是异步加载的，图到位之前用同色调
/// 的渐变打底比空白好；封面文件缺失或损坏时也能无缝回落，不必额外处理错误态。
fn make_cover(album_id: &str, album_name: &str, cover_key: Option<String>) -> Cover {
    let initial = album_name.chars().next().unwrap_or('?').to_string();
    // 按专辑 ID 稳定取色：同一张专辑每次扫描配色一致。
    let idx = album_id.bytes().map(|b| b as usize).sum::<usize>() % GRADIENTS.len();
    let (from, to) = GRADIENTS[idx];
    Cover { initial, gradient: (from.to_string(), to.to_string()), cover_key }
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
        let files = collect_files(std::slice::from_ref(&d));
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
        assert_eq!(count_candidates(std::slice::from_ref(&d)), 2);
        let _ = fs::remove_dir_all(d);
    }

    /// 无法解析的文件要计入 failed 并如实上报，不能静默丢弃。
    #[test]
    fn unparseable_files_counted_as_failed() {
        let d = tmpdir("shannon_scan_failed");
        fs::write(d.join("broken.flac"), b"definitely not flac").unwrap();
        let mut events = 0;
        let snap = scan_folders(std::slice::from_ref(&d), None, |_| events += 1)
            .library(&Overrides::default());
        assert_eq!(snap.failed, 1);
        assert_eq!(snap.tracks.len(), 0);
        assert!(events > 0, "至少要有一次收尾进度事件");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn empty_folder_yields_empty_snapshot() {
        let d = tmpdir("shannon_scan_empty");
        let snap = scan_folders(std::slice::from_ref(&d), None, |_| {})
            .library(&Overrides::default());
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
            let (a, b) = folder_hint(&dir.join("01.wav"), std::slice::from_ref(&d));
            assert_eq!(a.as_deref(), Some(artist));
            assert_eq!(b.as_deref(), Some(album));
        }
        let _ = fs::remove_dir_all(d);
    }

    /// 扫描根目录本身不是专辑名，也不是歌手名。
    /// 曾经会把散落在库根目录的文件聚成一张以库目录命名的假专辑。
    #[test]
    fn scan_root_is_not_used_as_album_or_artist() {
        let root = PathBuf::from("/Users/x/Music");
        let (artist, album) =
            folder_hint(&root.join("散装.flac"), std::slice::from_ref(&root));
        assert_eq!(album, None, "根目录名不该当专辑名");
        assert_eq!(artist, None, "根目录的上级更不该当歌手名");

        // 根下一层仍可作专辑名，但再上一层是根，就不给歌手名。
        let (artist, album) = folder_hint(
            &root.join("长夜电波/01.flac"),
            std::slice::from_ref(&root),
        );
        assert_eq!(album.as_deref(), Some("长夜电波"));
        assert_eq!(artist, None);
    }

    /// 保守归组键的作用域优先级：用户指定 > 标签专辑艺人 > 所在目录。
    #[test]
    fn group_key_scope_priority() {
        let gk = |album, user, tag, path| album_group_key(album, user, tag, Path::new(path));

        // 有专辑艺人时跨目录合并，且大小写不敏感。
        let a = gk("全集", None, Some("白鲸电台"), "/m/disc1/1.flac");
        let b = gk("全集", None, Some("白鲸电台"), "/m/disc2/2.flac");
        assert_eq!(a, b);

        // 用户指定压过标签：把我们错并的两张拆开。
        let c = gk("Live", Some("甲"), Some("同一个"), "/m/甲/1.flac");
        let d = gk("Live", Some("乙"), Some("同一个"), "/m/乙/1.flac");
        assert_ne!(c, d, "用户改了专辑艺人就该拆开");

        // 也能反过来把两处合并。
        let e = gk("Language", Some("Sakuzyo"), None, "/m/A/1.flac");
        let f = gk("language", Some("sakuzyo"), Some("别的"), "/m/B/2.flac");
        assert_eq!(e, f, "用户改成同一专辑艺人就该合并");

        // 什么线索都没有：退回目录，两位歌手的同名专辑不能撞成一张。
        let g = gk("Greatest Hits", None, None, "/m/甲/1.flac");
        let h = gk("Greatest Hits", None, None, "/m/乙/1.flac");
        assert_ne!(g, h);
    }

    /// 回归：封面只作合并证据，不作拆分依据。
    /// 一张专辑内部完全可能有几首嵌了不同版本的封面（实测 `unformed` 28 首里有两种），
    /// 曾经把封面当归组键，把好好的专辑劈成了两半。
    #[test]
    fn differing_covers_inside_one_album_do_not_split_it() {
        let mut items: Vec<_> = (0..6)
            .map(|i| {
                probed_cover(&format!("/m/doriko/unformed/{i}.m4a"), &format!("曲{i}"), "doriko", "unformed", "封面A")
            })
            .collect();
        items.extend((6..10).map(|i| {
            probed_cover(&format!("/m/doriko/unformed/{i}.m4a"), &format!("曲{i}"), "doriko", "unformed", "封面B")
        }));
        let snap = agg(items, &Overrides::default());
        assert_eq!(snap.albums.len(), 1, "同目录同专辑名不该因为封面不一致被拆开");
        assert_eq!(snap.albums[0].track_count, 10);
    }

    /// 用户指定即锁定：即使同名同封面，也不该把用户明确分开的两张自动并回去。
    #[test]
    fn user_pinned_album_artist_blocks_auto_merge() {
        let items = || {
            vec![
                probed_cover("/m/甲/live/1.flac", "A", "甲", "Live", "同一张图"),
                probed_cover("/m/乙/live/1.flac", "B", "乙", "Live", "同一张图"),
            ]
        };
        let before = agg(items(), &Overrides::default());
        assert_eq!(before.albums.len(), 1, "同名同封面默认合并");

        let mut ov = Overrides::default();
        let first = before.tracks.iter().find(|t| t.artist == "甲").unwrap();
        ov.merge(
            &first.id,
            crate::overrides::TrackMetadataPatch {
                album_artist: Some("甲".into()),
                ..Default::default()
            },
        );
        let after = agg(items(), &ov);
        assert_eq!(after.albums.len(), 2, "用户给一侧指定了专辑艺人，就不能再自动并回去");
    }

    // ---- 聚合：合辑与多数决 ----

    fn fake_format() -> crate::model::AudioFormat {
        crate::model::AudioFormat {
            container: "flac".into(),
            codec: "flac".into(),
            encoding: crate::model::Encoding::Pcm,
            sample_rate_hz: 44100,
            bit_depth: Some(16),
            bitrate_kbps: None,
            lossless: Some(true),
            channels: 2,
            channel_mask: Some(3),
            channel_layout: Some(crate::model::ChannelLayout::Stereo),
            spatial: None,
            probe_notes: vec![],
            probe_version: crate::model::PROBE_VERSION,
        }
    }

    /// 把一条记录改成另一种音频规格，用来构造「同一首歌的不同格式版本」。
    fn with_spec(mut t: RawTrack, codec: &str, rate: u32, depth: Option<u8>) -> RawTrack {
        t.format.codec = codec.into();
        t.format.container = codec.into();
        t.format.sample_rate_hz = rate;
        t.format.bit_depth = depth;
        // ID 是内容哈希，不同编码必然不同；测试里用路径派生已保证唯一。
        t
    }

    /// 构造一条原始记录。`album_artist` 为 None 模拟「文件没写专辑艺人标签」。
    fn probed_at(
        path: &str,
        title: &str,
        artist: &str,
        album: &str,
        album_artist: Option<&str>,
    ) -> RawTrack {
        probed_full(path, title, artist, album, album_artist, None)
    }

    /// 带封面指纹的版本：模拟「同一张专辑的曲目嵌同一张封面图」。
    fn probed_cover(path: &str, title: &str, artist: &str, album: &str, cover: &str) -> RawTrack {
        probed_full(path, title, artist, album, None, Some(cover.into()))
    }

    fn probed_full(
        path: &str,
        title: &str,
        artist: &str,
        album: &str,
        album_artist: Option<&str>,
        cover_key: Option<String>,
    ) -> RawTrack {
        RawTrack {
            // 真实 ID 是文件内容哈希；测试里用路径派生，只需唯一且稳定。
            id: format!("t-{}", path.replace('/', "_")),
            path: PathBuf::from(path),
            tags: RawTags {
                title: Some(title.into()),
                artist: Some(artist.into()),
                album_artist: album_artist.map(|s| s.into()),
                album: Some(album.into()),
                year: Some(2020),
                genre: Some("Electronic".into()),
                track_no: None,
                disc_no: None,
            },
            has_cover: cover_key.is_some(),
            cover_key,
            format: fake_format(),
            duration_sec: 100.0,
        }
    }

    fn agg(items: Vec<RawTrack>, ov: &Overrides) -> LibrarySnapshot {
        aggregate(&items, &[PathBuf::from("/m")], ov)
    }

    /// 回归：拼盘合辑曾被按曲目艺人拆成十几张专辑。
    #[test]
    fn compilation_stays_one_album() {
        let items = (0..8)
            .map(|i| {
                probed_at(
                    &format!("/m/合辑/eclipse/{i}.flac"),
                    &format!("曲{i}"),
                    &format!("歌手{i}"),
                    "eclipse III",
                    None,
                )
            })
            .collect();
        let snap = agg(items, &Overrides::default());
        assert_eq!(snap.albums.len(), 1, "合辑必须聚成一张");
        assert_eq!(snap.albums[0].artist, VARIOUS_ARTISTS);
        assert!(snap.albums[0].compilation);
        assert_eq!(snap.albums[0].track_count, 8);
    }

    /// 回归（真实曲库形态）：Apple Music 的 `艺人/专辑/曲目` 布局下，合辑曲目
    /// 按曲目艺人散在不同目录、且没有专辑艺人标签——只有封面认得出它们是一张专辑。
    #[test]
    fn compilation_across_artist_folders_merges_by_cover() {
        let mut items: Vec<_> = (0..8)
            .map(|i| {
                probed_cover(
                    &format!("/m/FELT/rs4/{i}.m4a"),
                    &format!("曲{i}"),
                    "FELT",
                    "Rebirth Story 4",
                    "同一张封面",
                )
            })
            .collect();
        items.push(probed_cover("/m/舞花/rs4/a.m4a", "客串A", "舞花", "Rebirth Story 4", "同一张封面"));
        items.push(probed_cover("/m/NAGI/rs4/b.m4a", "客串B", "NAGI☆", "Rebirth Story 4", "同一张封面"));
        let snap = agg(items, &Overrides::default());
        assert_eq!(snap.albums.len(), 1, "同名专辑 + 同封面必须聚成一张，哪怕分散在多个艺人目录");
        assert_eq!(snap.albums[0].track_count, 10);
        assert_eq!(snap.albums[0].artist, "FELT", "八成曲目属 FELT，应归到它名下");
    }

    /// 回归：整张专辑在一处、另有几首缺封面缺标签的漏网之鱼搁在别的艺人目录下，
    /// 要能并回去。实测 doriko BEST 2008-2016 就是这样——361 首带封面待在
    /// `doriko/doriko BEST 2008-2016/`，另两首没有内嵌封面、专辑艺人还各写各的，
    /// 躺在 `初音ミク/doriko BEST 2008-2016/`，此前变成了两张各 1 首的无封面假专辑。
    #[test]
    fn coverless_strays_merge_into_the_album_with_same_dir_name() {
        let mut items: Vec<_> = (0..6)
            .map(|i| {
                probed_cover(
                    &format!("/m/doriko/doriko BEST 2008-2016/{i}.m4a"),
                    &format!("曲{i}"),
                    "初音ミク",
                    "doriko BEST 2008-2016",
                    "专辑封面",
                )
            })
            .collect();
        // 漏网之一：专辑艺人写成了曲目艺人，没有封面。
        items.push(probed_at(
            "/m/初音ミク/doriko BEST 2008-2016/a.m4a",
            "last will",
            "初音ミク",
            "doriko BEST 2008-2016",
            Some("初音ミク"),
        ));
        // 漏网之二：连专辑艺人标签都没有，也没有封面。
        items.push(probed_at(
            "/m/初音ミク/doriko BEST 2008-2016/b.m4a",
            "歌に形はないけれど",
            "初音ミク",
            "doriko BEST 2008-2016",
            None,
        ));
        let snap = agg(items, &Overrides::default());
        assert_eq!(snap.albums.len(), 1, "漏网曲目应并回同名同目录名的那张专辑");
        assert_eq!(snap.albums[0].track_count, 8);
        // 并回去之后，这两首也就跟着有封面了（自己没有则用专辑的）。
        assert!(snap.tracks.iter().all(|t| t.cover.cover_key.is_some()));
    }

    /// 碟名被写进专辑标签时，各碟要认作同一张专辑。
    #[test]
    fn disc_suffix_is_stripped_for_grouping() {
        assert_eq!(strip_disc_suffix("Rebirth Story 5 Disc 1 SUN"), "Rebirth Story 5");
        assert_eq!(strip_disc_suffix("全集 CD2"), "全集");
        assert_eq!(strip_disc_suffix("Album (Disc 2)"), "Album");
        assert_eq!(strip_disc_suffix("Rebirth Story 5"), "Rebirth Story 5");
        // 不能误伤：这些词里的 disc/cd 不是碟号。
        assert_eq!(strip_disc_suffix("Discovery"), "Discovery");
        assert_eq!(strip_disc_suffix("CD Player Blues"), "CD Player Blues");
        // 剥完会变空的（专辑就叫这个名）原样保留。
        assert_eq!(strip_disc_suffix("Disc 1"), "Disc 1");
    }

    /// 回归：同一张专辑既有整盘版本、又有把碟名写进专辑名的分碟版本时，
    /// 应聚成一张，且重复曲目要折叠掉。实测 Rebirth Story 5 就是这个形态：
    /// 27 首整盘版 + 13 首 Disc 1 SUN + 14 首 Disc 2 LUNA，是同一批录音的两份拷贝。
    #[test]
    fn disc_split_copies_merge_and_dedupe() {
        let mut items = Vec::new();
        for i in 0..4 {
            items.push(probed_cover(
                &format!("/m/FELT/Rebirth Story 5/{i}.m4a"),
                &format!("曲{i}"),
                "FELT",
                "Rebirth Story 5",
                "同一张封面",
            ));
        }
        // 同一批录音的另一份拷贝，碟名被写进了专辑标签。
        for i in 0..4 {
            items.push(probed_cover(
                &format!("/m/FELT/Rebirth Story 5 Disc 1 SUN/{i}.m4a"),
                &format!("曲{i}"),
                "FELT",
                "Rebirth Story 5 Disc 1 SUN",
                "同一张封面",
            ));
        }
        let snap = agg(items, &Overrides::default());
        assert_eq!(snap.albums.len(), 1, "各碟应认作同一张专辑");
        assert_eq!(snap.albums[0].title, "Rebirth Story 5", "显示名该用主专辑名，不带碟号");
        assert_eq!(snap.albums[0].track_count, 4, "重复曲目要折叠");
        assert_eq!(snap.tracks.len(), 4);
        assert_eq!(snap.duplicates, 4, "折叠数如实上报");
    }

    /// 回归：同一首歌被标了不同标题（带各种译名）时，只比标题永远认不出是一首。
    /// 实测同一轨位出现过三种写法：`ロミオとシンデレラ`、`…（罗密欧与仙杜瑞拉）`、
    /// `…（罗密欧与灰姑娘）`，此前一张 30 首的精选集因此显示成 66 首。
    #[test]
    fn same_slot_different_titles_is_a_duplicate() {
        let mut items = Vec::new();
        for (n, title) in [
            "ロミオとシンデレラ",
            "ロミオとシンデレラ（罗密欧与仙杜瑞拉）",
            "ロミオとシンデレラ（罗密欧与灰姑娘）",
        ]
        .iter()
        .enumerate()
        {
            let mut t = probed_cover(
                &format!("/m/doriko/best/1-10 v{n}.m4a"),
                title,
                "初音ミク",
                "BEST",
                "封面",
            );
            t.tags.disc_no = Some(1);
            t.tags.track_no = Some(10);
            items.push(t);
        }
        let snap = agg(items, &Overrides::default());
        assert_eq!(snap.tracks.len(), 1, "同一轨位同时长就是同一首歌，标题写法不影响");
        assert_eq!(snap.duplicates, 2);
    }

    /// 末尾括注只是译名时，即使两份拷贝的轨位标签也互相冲突，仍应按标题主体 + 时长折叠。
    #[test]
    fn translated_title_suffix_is_a_duplicate_even_when_slots_conflict() {
        let mut plain = probed_cover(
            "/m/doriko/best/11 Winter Alice.m4a",
            "Winter Alice",
            "初音ミク",
            "BEST",
            "封面",
        );
        plain.tags.track_no = Some(11);
        plain.duration_sec = 301.973;
        let mut translated = probed_cover(
            "/m/doriko/best/1-05 Winter Alice.m4a",
            "Winter Alice（冬日爱丽丝）",
            "初音ミク",
            "BEST",
            "封面",
        );
        translated.tags.disc_no = Some(1);
        translated.tags.track_no = Some(5);
        translated.duration_sec = 301.973;

        let snap = agg(vec![plain, translated], &Overrides::default());
        assert_eq!(snap.tracks.len(), 1, "带/不带译名括注的是同一录音");
        assert_eq!(snap.duplicates, 1);
    }

    /// 同一录音的多份拷贝可能写着互相冲突的轨位；应采用组内多数，而不是路径最靠前的错值。
    #[test]
    fn dedupe_prefers_consensus_disc_and_track() {
        let mut items = Vec::new();
        for n in 0..2 {
            let mut t = probed_cover(
                &format!("/m/doriko/best/a-wrong-{n}.m4a"),
                "Winter Alice（冬日爱丽丝）",
                "初音ミク",
                "BEST",
                "封面",
            );
            t.tags.disc_no = Some(1);
            t.tags.track_no = Some(5);
            t.duration_sec = 301.973;
            items.push(t);
        }
        for n in 0..3 {
            let mut t = probed_cover(
                &format!("/m/doriko/best/z-correct-{n}.m4a"),
                "Winter Alice",
                "初音ミク",
                "BEST",
                "封面",
            );
            t.tags.disc_no = Some(1);
            t.tags.track_no = Some(11);
            t.duration_sec = 301.973;
            items.push(t);
        }
        let mut partial = probed_cover(
            "/m/doriko/best/11 Winter Alice.m4a",
            "Winter Alice（冬日爱丽丝）",
            "初音ミク",
            "BEST",
            "封面",
        );
        partial.tags.track_no = Some(11);
        partial.duration_sec = 301.973;
        items.push(partial);

        let snap = agg(items, &Overrides::default());
        assert_eq!(snap.tracks.len(), 1);
        assert_eq!(snap.duplicates, 5);
        assert_eq!(snap.tracks[0].disc_no, Some(1));
        assert_eq!(snap.tracks[0].track_no, Some(11));
        assert!(snap.tracks[0].path.contains("z-correct"), "应保留多数轨位对应的副本");
    }

    /// 但同一轨位时长不同的，是标签把两首不同的歌错标到了一处，两首都要留。
    #[test]
    fn same_slot_different_length_is_kept() {
        let mut a = probed_cover("/m/x/best/1-05 a.m4a", "Winter Alice", "甲", "BEST", "封面");
        let mut b = probed_cover("/m/x/best/1-05 b.m4a", "あなたの願いをうたうもの", "甲", "BEST", "封面");
        for t in [&mut a, &mut b] {
            t.tags.disc_no = Some(1);
            t.tags.track_no = Some(5);
        }
        a.duration_sec = 301.97;
        b.duration_sec = 316.43;
        let snap = agg(vec![a, b], &Overrides::default());
        assert_eq!(snap.tracks.len(), 2);
        assert_eq!(snap.duplicates, 0);
    }

    /// 不同轨位的同名曲目（如正篇与 reprise 同名）不能因为轨位判据被误折叠。
    #[test]
    fn different_slots_are_not_merged_by_slot_rule() {
        let mut a = probed_cover("/m/x/al/1-03.m4a", "Theme", "甲", "专辑", "封面");
        let mut b = probed_cover("/m/x/al/2-03.m4a", "Theme", "甲", "专辑", "封面");
        a.tags.disc_no = Some(1);
        a.tags.track_no = Some(3);
        a.duration_sec = 120.0;
        b.tags.disc_no = Some(2);
        b.tags.track_no = Some(3);
        b.duration_sec = 95.5;
        let snap = agg(vec![a, b], &Overrides::default());
        assert_eq!(snap.tracks.len(), 2, "轨位与时长都不同，是两首歌");
    }

    /// 同名同时长才算重复；同名但时长不同（重录、remix）要各自保留。
    #[test]
    fn same_title_different_length_is_not_a_duplicate() {
        let mut a = probed_cover("/m/x/al/1.m4a", "Intro", "甲", "专辑", "封面");
        let mut b = probed_cover("/m/x/al/2.m4a", "Intro", "甲", "专辑", "封面");
        a.duration_sec = 100.0;
        b.duration_sec = 137.5;
        let snap = agg(vec![a, b], &Overrides::default());
        assert_eq!(snap.tracks.len(), 2);
        assert_eq!(snap.duplicates, 0);
    }

    /// 折叠时保留信息更全的那一份（有封面、有音轨号的优先）。
    #[test]
    fn dedupe_keeps_the_richer_copy() {
        let plain = probed_at("/m/x/al/copy.m4a", "曲", "甲", "专辑", None);
        let mut rich = probed_cover("/m/x/al/original.m4a", "曲", "甲", "专辑", "封面");
        rich.tags.track_no = Some(3);
        let snap = agg(vec![plain, rich], &Overrides::default());
        assert_eq!(snap.tracks.len(), 1);
        assert!(snap.tracks[0].path.ends_with("original.m4a"), "该留下带封面和音轨号的那份");
        assert_eq!(snap.tracks[0].track_no, Some(3));
    }

    /// 跨专辑的同名同长曲目不算重复：原盘与精选集各自收录，都该显示。
    #[test]
    fn same_track_in_two_albums_is_kept_twice() {
        let a = probed_cover("/m/甲/原盘/1.m4a", "招牌曲", "甲", "原盘", "封面A");
        let b = probed_cover("/m/甲/精选集/1.m4a", "招牌曲", "甲", "精选集", "封面B");
        let snap = agg(vec![a, b], &Overrides::default());
        assert_eq!(snap.albums.len(), 2);
        assert_eq!(snap.tracks.len(), 2);
        assert_eq!(snap.duplicates, 0);
    }

    /// 两边都没有封面时不能靠目录名合并：那可能只是两位歌手各有一张同名专辑。
    #[test]
    fn coverless_on_both_sides_stays_apart() {
        let items = vec![
            probed_at("/m/甲/greatest hits/1.flac", "A", "甲", "Greatest Hits", None),
            probed_at("/m/乙/greatest hits/1.flac", "B", "乙", "Greatest Hits", None),
        ];
        assert_eq!(agg(items, &Overrides::default()).albums.len(), 2);
    }

    /// 同名专辑但封面不同：两位歌手各自的精选集，绝不能因为同名就并成一张。
    #[test]
    fn same_name_different_cover_stays_apart() {
        let items = vec![
            probed_cover("/m/甲/gh/1.flac", "A", "甲", "Greatest Hits", "封面甲"),
            probed_cover("/m/乙/gh/1.flac", "B", "乙", "Greatest Hits", "封面乙"),
        ];
        assert_eq!(agg(items, &Overrides::default()).albums.len(), 2);
    }

    /// 社团 / 主创专辑：多数曲目属于同一位艺人时归到他名下，而不是标成合辑。
    #[test]
    fn dominant_artist_owns_the_album() {
        let mut items: Vec<_> = (0..8)
            .map(|i| {
                probed_at(
                    &format!("/m/FELT/rs4/{i}.flac"),
                    &format!("曲{i}"),
                    "FELT",
                    "Rebirth Story 4",
                    None,
                )
            })
            .collect();
        items.push(probed_at("/m/FELT/rs4/g1.flac", "客串1", "Vivienne", "Rebirth Story 4", None));
        items.push(probed_at("/m/FELT/rs4/g2.flac", "客串2", "美歌", "Rebirth Story 4", None));
        let snap = agg(items, &Overrides::default());
        assert_eq!(snap.albums.len(), 1);
        assert_eq!(snap.albums[0].artist, "FELT");
        assert!(!snap.albums[0].compilation);
        assert_eq!(snap.albums[0].artist_source, FieldSource::Majority);
    }

    /// 不同歌手的同名专辑不能合并——这是不按目录归组会踩的坑。
    #[test]
    fn same_album_name_different_folders_stay_apart() {
        let items = vec![
            probed_at("/m/甲/gh/1.flac", "A", "甲", "Greatest Hits", None),
            probed_at("/m/乙/gh/1.flac", "B", "乙", "Greatest Hits", None),
        ];
        let snap = agg(items, &Overrides::default());
        assert_eq!(snap.albums.len(), 2);
    }

    /// 有专辑艺人标签时，同一张专辑散在两个目录也应合并。
    #[test]
    fn album_artist_tag_merges_across_folders() {
        let items = vec![
            probed_at("/m/disc1/1.flac", "A", "甲", "全集", Some("白鲸电台")),
            probed_at("/m/disc2/2.flac", "B", "乙", "全集", Some("白鲸电台")),
        ];
        let snap = agg(items, &Overrides::default());
        assert_eq!(snap.albums.len(), 1);
        assert_eq!(snap.albums[0].artist, "白鲸电台");
        assert_eq!(snap.albums[0].artist_source, FieldSource::Tag);
    }

    /// 用户改专辑艺人后，曲目要**重新归组**（这正是合并两个目录的手段），
    /// 而不是留在原处只换个显示名。
    #[test]
    fn user_override_regroups_tracks() {
        // Probed 不是 Clone（含封面字节），用闭包重建两份。
        let items = || {
            vec![
                probed_at("/m/Sakuzyo/lang/1.flac", "A", "Sakuzyo", "Language", None),
                probed_at("/m/Sakuzyo (削除)/lang/2.flac", "B", "Sakuzyo (削除)", "Language", None),
            ]
        };
        let before = agg(items(), &Overrides::default());
        assert_eq!(before.albums.len(), 2, "两个目录默认分开");

        let mut ov = Overrides::default();
        for t in &before.tracks {
            ov.merge(
                &t.id,
                crate::overrides::TrackMetadataPatch {
                    album_artist: Some("Sakuzyo".into()),
                    ..Default::default()
                },
            );
        }
        let after = agg(items(), &ov);
        assert_eq!(after.albums.len(), 1, "改成同一专辑艺人后应合并为一张");
        assert_eq!(after.albums[0].artist, "Sakuzyo");
        assert_eq!(after.albums[0].artist_source, FieldSource::UserEdit);
    }

    /// 缓存落盘再读回，重新聚合的结果必须一致——重启免重扫依赖这一点。
    #[test]
    fn cache_round_trips_and_reaggregates() {
        let cache = ScanCache {
            roots: vec![PathBuf::from("/m")],
            tracks: vec![
                probed_at("/m/甲/专辑/1.flac", "A", "甲", "专辑", None),
                probed_at("/m/甲/专辑/2.flac", "B", "甲", "专辑", None),
            ],
            failed: 3,
            cover_failed: 0,
        };
        let p = std::env::temp_dir().join("shannon_cache_roundtrip.json");
        cache.save(&p).unwrap();
        let back = ScanCache::load(&p).unwrap();
        assert_eq!(back, cache);

        let lib = back.library(&Overrides::default());
        assert_eq!(lib.albums.len(), 1);
        assert_eq!(lib.tracks.len(), 2);
        assert_eq!(lib.failed, 3, "解析失败数要跟着缓存一起还原");
        let _ = std::fs::remove_file(p);
    }

    /// 改元数据后重新聚合走的是缓存，不必重扫文件。
    #[test]
    fn editing_metadata_reaggregates_from_cache() {
        let cache = ScanCache {
            roots: vec![PathBuf::from("/m")],
            tracks: vec![probed_at("/m/甲/专辑/1.flac", "原标题", "甲", "专辑", None)],
            failed: 0,
            cover_failed: 0,
        };
        let id = cache.tracks[0].id.clone();
        let mut ov = Overrides::default();
        ov.merge(
            &id,
            crate::overrides::TrackMetadataPatch { title: Some("新标题".into()), ..Default::default() },
        );
        assert_eq!(cache.library(&ov).tracks[0].title, "新标题");
        assert_eq!(cache.library(&Overrides::default()).tracks[0].title, "原标题");
    }

    /// 用户改的标题优先于标签，且来源标为 UserEdit（界面据此显示「已修改」）。
    #[test]
    fn user_override_beats_tag_and_is_marked() {
        let items = || vec![probed_at("/m/a/b/1.flac", "标签标题", "甲", "专辑", None)];
        let snap0 = agg(items(), &Overrides::default());
        let id = snap0.tracks[0].id.clone();
        assert_eq!(snap0.tracks[0].sources.title, FieldSource::Tag);

        let mut ov = Overrides::default();
        ov.merge(
            &id,
            crate::overrides::TrackMetadataPatch { title: Some("我改的".into()), ..Default::default() },
        );
        let snap = agg(items(), &ov);
        assert_eq!(snap.tracks[0].title, "我改的");
        assert_eq!(snap.tracks[0].sources.title, FieldSource::UserEdit);
    }

    /// 目录兜底要如实标记来源，界面才能提示「这是猜的」。
    #[test]
    fn folder_fallback_is_marked_as_guess() {
        let mut item = probed_at("/m/白鲸电台/长夜电波/1.flac", "曲", "甲", "专辑", None);
        item.tags.artist = None;
        item.tags.album = None;
        let snap = agg(vec![item], &Overrides::default());
        assert_eq!(snap.tracks[0].artist, "白鲸电台");
        assert_eq!(snap.tracks[0].sources.artist, FieldSource::Folder);
        assert_eq!(snap.tracks[0].album, "长夜电波");
        assert_eq!(snap.tracks[0].sources.album, FieldSource::Folder);
    }

    #[test]
    fn cover_color_is_stable_per_album() {
        let a = make_cover("a-abc", "长夜电波", None);
        let b = make_cover("a-abc", "长夜电波", None);
        assert_eq!(a.gradient, b.gradient);
        assert_eq!(a.initial, "长");
    }
    #[test]
    fn different_formats_of_one_song_are_not_folded() {
        // 同一段音乐的不同编码：标题、时长、轨位全都一样，只有规格不同。
        // 只看标题与时长的判据会把它们全折叠掉——实测 11 个格式扫出来只剩 4 首。
        // 用户同时留着 FLAC 与 MP3 是有意为之，不是导入工具的手滑。
        let mut flac_in_mka = with_spec(
            probed_at("/m/实测/06.mka", "同一首", "歌手", "格式实测", None),
            "flac",
            44100,
            Some(16),
        );
        flac_in_mka.format.container = "mka".into();
        let items = vec![
            with_spec(
                probed_at("/m/实测/01.flac", "同一首", "歌手", "格式实测", None),
                "flac",
                44100,
                Some(16),
            ),
            with_spec(
                probed_at("/m/实测/02.m4a", "同一首", "歌手", "格式实测", None),
                "alac",
                44100,
                Some(16),
            ),
            with_spec(
                probed_at("/m/实测/03.mp3", "同一首", "歌手", "格式实测", None),
                "mp3",
                44100,
                None,
            ),
            with_spec(
                probed_at("/m/实测/04.flac", "同一首", "歌手", "格式实测", None),
                "flac",
                48000,
                Some(16),
            ),
            with_spec(
                probed_at("/m/实测/05.flac", "同一首", "歌手", "格式实测", None),
                "flac",
                44100,
                Some(24),
            ),
            flac_in_mka,
        ];
        let snap = agg(items, &Overrides::default());
        assert_eq!(
            snap.tracks.len(),
            6,
            "不同容器/格式/采样率/位深是不同的东西，不该折叠"
        );
        assert_eq!(snap.duplicates, 0);
    }

    #[test]
    fn different_layouts_and_spatial_versions_are_not_folded() {
        let mut five_one = with_spec(
            probed_at("/m/实测/5.1.flac", "环绕版本", "歌手", "格式实测", None),
            "flac",
            48000,
            Some(24),
        );
        five_one.format.channels = 6;
        five_one.format.channel_mask = Some(0x3f);

        let mut six_zero = five_one.clone();
        six_zero.id = "t-six-zero".into();
        six_zero.path = PathBuf::from("/m/实测/6.0.flac");
        six_zero.format.channel_mask = Some(0x70f);

        let mut bed = with_spec(
            probed_at("/m/实测/bed.m4a", "空间版本", "歌手", "格式实测", None),
            "eac3",
            48000,
            None,
        );
        bed.format.channels = 6;
        bed.format.channel_mask = Some(0x3f);

        let mut atmos = bed.clone();
        atmos.id = "t-atmos".into();
        atmos.path = PathBuf::from("/m/实测/atmos.m4a");
        atmos.format.spatial = Some(SpatialFormat::DolbyAtmos {
            joc: true,
            objects: Some(16),
        });

        let snap = agg(vec![five_one, six_zero, bed, atmos], &Overrides::default());
        assert_eq!(
            snap.tracks.len(),
            4,
            "同声道数的不同布局、普通声床与对象音频都必须分别保留"
        );
        assert_eq!(snap.duplicates, 0);
    }

    #[test]
    fn identical_copies_are_still_folded() {
        // 规格纳入判据之后，导入工具留下的真副本仍要被折叠——
        // 它们是同一份音频的字节级拷贝，规格必然完全相同。
        let items = vec![
            probed_at("/m/album/track.m4a", "一首歌", "歌手", "专辑", None),
            probed_at("/m/album/track 1.m4a", "一首歌", "歌手", "专辑", None),
            probed_at("/m/album/track 2.m4a", "一首歌", "歌手", "专辑", None),
        ];
        let snap = agg(items, &Overrides::default());
        assert_eq!(snap.tracks.len(), 1, "同规格同标题同时长的拷贝仍应折叠");
        assert_eq!(snap.duplicates, 2);
    }
}
