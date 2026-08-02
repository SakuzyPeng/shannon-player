//! 曲库数据库（SQLite）。
//!
//! ## 为什么从 JSON 换过来
//!
//! 原先扫描缓存与覆盖层各是一份 JSON，读写都是**整份重写**。三件事因此做不成：
//!
//! - **改一个字段要重写整份覆盖层**。用户在专辑详情页连改十首，就是十次全量序列化 +
//!   十次 rename。文件小的时候看不出来，但这份数据只增不减，且它**不可重建**——
//!   把「改一个字段」放大成「重写全部用户劳动」，风险窗口没有必要开这么大。
//! - **增量重扫无从下手**。探测器升级后只需重读 `probe_version` 落后的条目
//!   （见 [`crate::model::PROBE_VERSION`]），而 JSON 里要判断这个就得先把整库读进内存。
//! - **收藏与歌单没有地方放**。它们与覆盖层同以曲目 ID 为键，本就该和曲库同处一处、
//!   同一个事务里；再开两份 JSON 只会让「键的一致性」变成三份文件之间的口头约定。
//!
//! ## 边界：数据库属于曲库，不是通用存储层
//!
//! 只有**后端拥有的曲库数据**进这里。播放会话与界面设置仍走
//! `src-tauri/src/frontend_state.rs` 的文本槽位——那两份是前端拥有的状态，后端在其中
//! 没有任何领域判断可做（理由见该模块头），给它们建表等于把 schema 的所有权搬错地方。
//! 响度分析结果也不进：它在 `shannon-audio` 里，把它搬过来要么让解码引擎依赖 SQLite，
//! 要么让本 crate 反向依赖音频引擎，两者都比「多一份 JSON」糟。
//!
//! ## 一个文件带来的新代价，要说清楚
//!
//! 缓存（可重建）与覆盖层（**不可重建**）现在同处一个文件，这是 JSON 时代没有的耦合：
//! 数据库整体损坏会一起带走。换来的是 SQLite 的事务与 WAL——「写到一半断电留下半个
//! 文件」这个 JSON 时代真实存在的失败模式基本消失，而那正是过去最可能弄丢覆盖层的路径。
//! 权衡后仍分两个文件的话，跨库事务与外键都没了，收藏和歌单接进来时更难保证一致。
//!
//! 因此两处补偿：① `synchronous = FULL`，覆盖层写入多付一次 fsync 换掉电安全，反正
//! 用户改元数据的频率以「次/分钟」计；② 确认真损坏时保留 `.corrupt` 残骸并
//! **如实上报**，不像扫描缓存那样静默重建——里面有用户手改的东西。只读、锁竞争、
//! 磁盘满等运行时错误只报错，不移动完好的文件。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};

use crate::cache::{RawTags, RawTrack, ScanCache};
use crate::collections::{Favorites, Playlist};
use crate::model::{AudioFormat, ChannelLayout, Encoding, SpatialFormat};
use crate::overrides::{Overrides, TrackOverride};

/// 当前 schema 版本，落在 SQLite 的 `user_version` 上。
///
/// 改表结构时 +1 并在 [`migrate`] 里补一段升级。用 `user_version` 而不是自建元数据表，
/// 是因为它由 SQLite 自己维护、读取不需要先假设任何表存在——首次打开一个空文件时，
/// 「还没有表」与「表读不出来」不该走两条不同的代码路径。
const SCHEMA_VERSION: i64 = 5;

/// 统计量在 `meta` 表里的键。这些是**一次扫描的结论**而不是曲目属性，
/// 单独建表只会得到一张永远两行的表。
const META_FAILED: &str = "scan.failed";
const META_COVER_FAILED: &str = "scan.cover_failed";
const META_LEGACY_IMPORTED: &str = "legacy.imported";

#[derive(Debug)]
pub enum DbError {
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    Io(std::io::Error),
    /// 路径不是合法 UTF-8。原先存 JSON 时 serde 也会在这里失败，因此不是新增的限制；
    /// 但要报得明确，不能 `to_string_lossy` 一了百了——那会把一个存不下的路径悄悄
    /// 改成另一个路径，等到播放时才表现为「文件找不到」。
    NonUtf8Path(PathBuf),
    /// 文件是新版本应用写的。
    ///
    /// **必须与「损坏」分开**：损坏走的是「留残骸 + 新建空库」，而这份文件完好无损，
    /// 只是这个版本读不懂。按损坏处理等于用户装了一次旧版本，整个曲库连同手改的元数据
    /// 就被改名搬走、界面上一片空白——他做错的只是打开了旧版本。
    SchemaTooNew {
        found: i64,
        supported: i64,
    },
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite(e) => write!(f, "数据库错误: {e}"),
            Self::Json(e) => write!(f, "字段解析失败: {e}"),
            Self::Io(e) => write!(f, "文件操作失败: {e}"),
            Self::NonUtf8Path(p) => write!(f, "路径不是合法 UTF-8，无法入库: {}", p.display()),
            Self::SchemaTooNew { found, supported } => write!(
                f,
                "曲库数据库版本 {found} 高于本程序支持的 {supported}，请升级应用（文件未改动）"
            ),
        }
    }
}

impl std::error::Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

impl From<serde_json::Error> for DbError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

impl From<std::io::Error> for DbError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, DbError>;

/// 打开数据库时发生过什么。调用方据此决定要不要惊动用户。
#[derive(Debug, Default, PartialEq)]
pub struct OpenReport {
    /// 原文件被 SQLite 判为损坏或未通过完整性检查，已改名保留在这里；当前是新空库。
    /// **必须让用户知道**：里面有他手改的元数据。
    pub corrupt_backup: Option<PathBuf>,
}

/// 从旧 JSON 迁移的结果。
#[derive(Debug, Default, PartialEq)]
pub struct LegacyImport {
    pub tracks: usize,
    pub overrides: usize,
}

impl LegacyImport {
    pub fn is_empty(&self) -> bool {
        self.tracks == 0 && self.overrides == 0
    }
}

pub struct LibraryDb {
    conn: Connection,
}

impl LibraryDb {
    /// 打开（或新建）数据库，跑完迁移。
    ///
    /// SQLite 明确认定为损坏、或 `integrity_check` 不过时，把原文件改名为 `.corrupt`
    /// 再新建一个空库：直接照常写回会把残骸也覆盖掉，而里面有**不可重建**的覆盖层，
    /// 留着至少还有人工挽救的余地。只读、锁竞争、磁盘满等运行时错误只上报，不动文件。
    pub fn open(path: &Path) -> Result<(Self, OpenReport)> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        match Self::try_open(path) {
            Ok(db) => Ok((db, OpenReport::default())),
            Err(err) => {
                // 只有 SQLite 明确认定为损坏 / 不是数据库才隔离。`try_open` 还会跑
                // WAL 设置与 schema 迁移，它们可能报 Busy / ReadOnly / Full / IOERR；
                // 把这些运行时错误也改名成 `.corrupt`，会把一份完好的用户数据库撤下。
                // SchemaTooNew 同理只是「读不懂」，不代表文件坏了。
                if !path.exists() || !is_corruption_error(&err) {
                    return Err(err);
                }
                let backup = unique_backup_path(path);
                std::fs::rename(path, &backup)?;
                // WAL 的两个附属文件跟着走，否则新库会读到上一个库的预写日志。
                for suffix in ["-wal", "-shm"] {
                    let from = sidecar(path, suffix);
                    if from.exists() {
                        std::fs::rename(&from, sidecar(&backup, suffix))?;
                    }
                }
                let db = Self::try_open(path)?;
                Ok((
                    db,
                    OpenReport {
                        corrupt_backup: Some(backup),
                    },
                ))
            }
        }
    }

    /// 内存库，测试用。
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let mut db = Self { conn };
        db.prepare()?;
        Ok(db)
    }

    fn try_open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let mut db = Self { conn };
        db.check_integrity()?;
        db.prepare()?;
        Ok(db)
    }

    /// 只对已有内容的文件做完整性检查：空文件（首次运行）不需要，也没有东西可检。
    fn check_integrity(&self) -> Result<()> {
        let ok: String = self
            .conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if ok == "ok" {
            Ok(())
        } else {
            Err(DbError::Sqlite(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CORRUPT),
                Some(ok),
            )))
        }
    }

    fn prepare(&mut self) -> Result<()> {
        // 版本检查必须先于任何会写数据库头或创建 sidecar 的 PRAGMA：旧程序面对未来版本
        // 的文件应当只读后拒绝，不能先把它的 journal mode 改掉再说「文件未改动」。
        ensure_schema_supported(&self.conn)?;
        // WAL：写不再独占整个文件，且断电最多丢掉最后一个未提交事务而不是弄坏文件。
        self.conn.pragma_update(None, "journal_mode", "WAL")?;
        // 覆盖层不可重建，为它多付一次 fsync 划算——用户改元数据的频率以「次/分钟」计，
        // 而扫描那种大批量写入本来就合在一个事务里，只在提交时同步一次。
        self.conn.pragma_update(None, "synchronous", "FULL")?;
        // SQLite 默认**不检查**外键，声明了也等于注释。歌单曲目靠它级联删除，
        // 不开的话删掉歌单会在 `playlist_track` 里留下一堆永远不会被读到的孤儿行。
        self.conn.pragma_update(None, "foreign_keys", true)?;
        migrate(&mut self.conn)?;
        // v3 的专辑收藏只有一张扁平曲目集合，无法表达「取消时连暂时缺失的成员一起删」。
        // v4 先把旧行留在 staging 表，这里再借当前扫描缓存尽量重建收藏时的专辑分组。
        // 单独做成可重入收尾，是为了进程若在 schema 事务提交后崩溃，下次仍能接着完成。
        self.finish_album_favorite_migration()?;
        Ok(())
    }

    // ---- 扫描缓存 ----

    /// 读回整份扫描缓存。聚合需要全部曲目在手（专辑艺人是组级结论），所以这里不做分页。
    pub fn load_cache(&self) -> Result<ScanCache> {
        let mut roots_stmt = self
            .conn
            .prepare("SELECT path FROM scan_root ORDER BY ord")?;
        let roots = roots_stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .map(PathBuf::from)
            .collect();

        let mut tracks_stmt = self.conn.prepare(
            "SELECT id, path, title, artist, album_artist, album, year, genre, track_no, disc_no,
                    cover_key, has_cover, duration_sec,
                    container, codec, encoding, sample_rate_hz, bit_depth, bitrate_kbps, lossless,
                    channels, channel_mask, channel_layout, spatial, probe_notes, probe_version,
                    file_size, mtime_ms
             FROM track
             ORDER BY path",
        )?;
        let tracks = tracks_stmt
            .query_map([], row_to_track)?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .collect::<Result<Vec<_>>>()?;

        Ok(ScanCache {
            roots,
            tracks,
            failed: self.meta_u32(META_FAILED)?.unwrap_or(0),
            cover_failed: self.meta_u32(META_COVER_FAILED)?.unwrap_or(0),
        })
    }

    /// 整体替换扫描缓存（一次扫描的产出就是一份完整结论）。
    ///
    /// **不碰 `track_override`**：那是用户的东西，重扫不该动它。表之间没有外键正是为此——
    /// 加了外键，重扫时曲目行一删，用户改过的元数据就跟着级联没了；而曲目 ID 是内容哈希，
    /// 文件挪走再挪回来还是同一个 ID，覆盖记录等在那里正好接上。
    pub fn replace_cache(&mut self, cache: &ScanCache) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM track", [])?;
        tx.execute("DELETE FROM scan_root", [])?;
        {
            let mut stmt = tx.prepare("INSERT INTO scan_root (ord, path) VALUES (?1, ?2)")?;
            for (i, root) in cache.roots.iter().enumerate() {
                stmt.execute(params![i as i64, path_str(root)?])?;
            }
            let mut stmt = tx.prepare(
                "INSERT INTO track (id, path, title, artist, album_artist, album, year, genre,
                                    track_no, disc_no, cover_key, has_cover, duration_sec,
                                    container, codec, encoding, sample_rate_hz, bit_depth,
                                    bitrate_kbps, lossless, channels, channel_mask,
                                    channel_layout, spatial, probe_notes, probe_version,
                                    file_size, mtime_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                         ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28)",
            )?;
            for t in &cache.tracks {
                let f = &t.format;
                stmt.execute(params![
                    t.id,
                    path_str(&t.path)?,
                    t.tags.title,
                    t.tags.artist,
                    t.tags.album_artist,
                    t.tags.album,
                    t.tags.year,
                    t.tags.genre,
                    t.tags.track_no,
                    t.tags.disc_no,
                    t.cover_key,
                    t.has_cover,
                    t.duration_sec,
                    f.container,
                    f.codec,
                    serde_json::to_string(&f.encoding)?,
                    f.sample_rate_hz,
                    f.bit_depth,
                    f.bitrate_kbps,
                    f.lossless,
                    f.channels,
                    f.channel_mask,
                    to_json_opt(&f.channel_layout)?,
                    to_json_opt(&f.spatial)?,
                    serde_json::to_string(&f.probe_notes)?,
                    f.probe_version,
                    t.file_size as i64,
                    t.mtime_ms,
                ])?;
            }
        }
        set_meta(&tx, META_FAILED, &cache.failed.to_string())?;
        set_meta(&tx, META_COVER_FAILED, &cache.cover_failed.to_string())?;
        tx.commit()?;
        Ok(())
    }

    // ---- 元数据覆盖层 ----

    pub fn load_overrides(&self) -> Result<Overrides> {
        let mut stmt = self.conn.prepare(
            "SELECT track_id, title, artist, album, album_artist, disc_no, track_no
             FROM track_override",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                TrackOverride {
                    title: r.get(1)?,
                    artist: r.get(2)?,
                    album: r.get(3)?,
                    album_artist: r.get(4)?,
                    disc_no: r.get(5)?,
                    track_no: r.get(6)?,
                },
            ))
        })?;
        let mut tracks = HashMap::new();
        for row in rows {
            let (id, ov) = row?;
            tracks.insert(id, ov);
        }
        Ok(Overrides { tracks })
    }

    /// 写一首的覆盖。空覆盖等同于删除——表里留一行全 NULL 的记录既占地方，
    /// 又会让「这首用户动过吗」这个问题多出一个说不清的中间态。
    pub fn put_override(&self, track_id: &str, ov: &TrackOverride) -> Result<()> {
        put_override_on(&self.conn, track_id, ov)
    }

    /// 批量写（专辑级编辑展开成逐曲记录）。一个事务：改一半就断电的话，
    /// 用户看到的是「这张专辑改了一半」，比整体没生效更难理解。
    pub fn put_overrides<'a>(
        &mut self,
        items: impl IntoIterator<Item = (&'a str, &'a TrackOverride)>,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        for (id, ov) in items {
            put_override_on(&tx, id, ov)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_override(&self, track_id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM track_override WHERE track_id = ?1", [track_id])?;
        Ok(())
    }

    // ---- 收藏 ----

    pub fn load_favorites(&self) -> Result<Favorites> {
        Ok(Favorites {
            tracks: self.column("SELECT track_id FROM favorite_track")?,
            album_groups: self.load_favorite_album_groups()?,
            artists: self.column("SELECT name FROM favorite_artist")?,
            playlists: self.column("SELECT playlist_id FROM favorite_playlist")?,
        })
    }

    pub fn set_favorite_track(&self, track_id: &str, on: bool) -> Result<()> {
        toggle_row(&self.conn, "favorite_track", "track_id", track_id, on)
    }

    pub fn set_favorite_artist(&self, name: &str, on: bool) -> Result<()> {
        toggle_row(&self.conn, "favorite_artist", "name", name, on)
    }

    pub fn set_favorite_playlist(&self, playlist_id: &str, on: bool) -> Result<()> {
        toggle_row(
            &self.conn,
            "favorite_playlist",
            "playlist_id",
            playlist_id,
            on,
        )
    }

    /// 收藏 / 取消收藏一张专辑：传入它**当前**的全部曲目 ID。
    ///
    /// 收藏时把成员存成一个完整分组；取消时删除与当前任一曲目相交的**整组**。后者是
    /// 必需的：若收藏时是 A/B/C、重扫时 C 暂时缺失，只删当前传来的 A/B 会让 C 回来后
    /// 把红心复活。一个事务保证分组不会只写或只删一半。
    pub fn set_favorite_album(&mut self, track_ids: &[String], on: bool) -> Result<()> {
        let track_ids = normalized_track_ids(track_ids);
        if track_ids.is_empty() {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        if on {
            insert_album_favorite_on(&tx, &track_ids)?;
        } else {
            let mut favorite_ids = HashSet::new();
            {
                let mut stmt =
                    tx.prepare("SELECT favorite_id FROM favorite_album_track WHERE track_id = ?1")?;
                for track_id in &track_ids {
                    let rows = stmt.query_map([track_id], |r| r.get::<_, String>(0))?;
                    for row in rows {
                        favorite_ids.insert(row?);
                    }
                }
            }
            let mut stmt = tx.prepare("DELETE FROM favorite_album WHERE id = ?1")?;
            for favorite_id in favorite_ids {
                stmt.execute([favorite_id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    // ---- 歌单 ----

    /// 读回全部歌单，按用户拖出来的顺序。
    pub fn load_playlists(&self) -> Result<Vec<Playlist>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, title, description, updated_at FROM playlist ORDER BY ord")?;
        let mut playlists: Vec<Playlist> = stmt
            .query_map([], |r| {
                Ok(Playlist {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    description: r.get(2)?,
                    track_ids: Vec::new(),
                    updated_at_ms: r.get(3)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;

        // 一次取全部曲目再分发，不为每个歌单各查一次：歌单数量不大，但「每个实体一次
        // 查询」这种写法一旦成型，后面加个字段就会顺手再来一轮。
        let mut stmt = self.conn.prepare(
            "SELECT playlist_id, track_id FROM playlist_track ORDER BY playlist_id, ord",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut by_id: HashMap<String, Vec<String>> = HashMap::new();
        for row in rows {
            let (pid, tid) = row?;
            by_id.entry(pid).or_default().push(tid);
        }
        for p in &mut playlists {
            if let Some(ids) = by_id.remove(&p.id) {
                p.track_ids = ids;
            }
        }
        Ok(playlists)
    }

    /// 新建或整体更新一个歌单（标题、简介、曲目顺序）。
    ///
    /// 这里**整份重写该歌单的曲目**，与覆盖层「只写点到名的那几行」不冲突：歌单的
    /// 编辑单位本来就是整条曲目列表（拖拽重排一次就全变了），而覆盖层的编辑单位是
    /// 单首歌的单个字段。范围对齐编辑单位，不是越小越好。
    pub fn save_playlist(&mut self, playlist: &Playlist) -> Result<()> {
        let tx = self.conn.transaction()?;
        // 新建时排到末尾；已存在则保持原有顺序不动——保存一次内容不该让它在列表里跳位。
        let next_ord: i64 =
            tx.query_row("SELECT COALESCE(MAX(ord) + 1, 0) FROM playlist", [], |r| {
                r.get(0)
            })?;
        tx.execute(
            "INSERT INTO playlist (id, title, description, updated_at, ord)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                 title = excluded.title,
                 description = excluded.description,
                 updated_at = excluded.updated_at",
            params![
                playlist.id,
                playlist.title,
                playlist.description,
                playlist.updated_at_ms,
                next_ord
            ],
        )?;
        tx.execute(
            "DELETE FROM playlist_track WHERE playlist_id = ?1",
            [&playlist.id],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO playlist_track (playlist_id, ord, track_id) VALUES (?1, ?2, ?3)",
            )?;
            for (i, track_id) in playlist.track_ids.iter().enumerate() {
                stmt.execute(params![playlist.id, i as i64, track_id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// 删除歌单。曲目行由外键级联带走，收藏标记要自己清——它在另一张表上，
    /// 留着的话下次新建歌单万一撞上同一个 ID（用户手改数据库、导入备份），
    /// 会莫名其妙地一出生就是已收藏。
    pub fn delete_playlist(&mut self, playlist_id: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM playlist WHERE id = ?1", [playlist_id])?;
        tx.execute(
            "DELETE FROM favorite_playlist WHERE playlist_id = ?1",
            [playlist_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 按给定顺序重排歌单列表。没点到名的歌单排在后面，保持原相对顺序。
    pub fn reorder_playlists(&mut self, ids: &[String]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare("UPDATE playlist SET ord = ?1 WHERE id = ?2")?;
            for (i, id) in ids.iter().enumerate() {
                stmt.execute(params![i as i64, id])?;
            }
        }
        // 漏掉的那些统一挪到后面，且**保持它们之间的原有次序**：直接给同一个大数字
        // 会让它们的排列变成不确定的，用户每次打开看到的顺序都可能不同。
        let base = ids.len() as i64;
        tx.execute(
            "UPDATE playlist SET ord = ?1 + ord WHERE id NOT IN (SELECT value FROM json_each(?2))",
            params![base, serde_json::to_string(ids)?],
        )?;
        tx.commit()?;
        Ok(())
    }

    // ---- 从旧 JSON 迁移 ----

    /// 把旧的两份 JSON 导入数据库，只做一次。
    ///
    /// 幂等靠 `meta` 里的标记，而不是「表是不是空的」：用户完全可能扫完一次又清空曲库，
    /// 那时表也是空的，再导一次就把他删掉的东西请了回来。
    ///
    /// 导完**不删源文件**，改名为 `.migrated` 保留。覆盖层不可重建，而这是一次性、
    /// 不可回退的搬迁——留个原件的成本是几百 KB，删掉的成本是用户的全部手工修改。
    pub fn import_legacy_json(
        &mut self,
        cache_json: &Path,
        overrides_json: &Path,
    ) -> Result<Option<LegacyImport>> {
        if self.meta(META_LEGACY_IMPORTED)?.is_some() {
            return Ok(None);
        }
        let cache = ScanCache::load_legacy(cache_json)?;
        let overrides = Overrides::load(overrides_json)?;
        let report = LegacyImport {
            tracks: cache.as_ref().map_or(0, |cache| cache.tracks.len()),
            overrides: overrides.tracks.len(),
        };

        // 合法的零曲目缓存也要写：空目录与「候选文件全解析失败」仍带着扫描根和统计。
        // `load_legacy` 用 Option 区分它与「文件不存在 / JSON 损坏」。
        if let Some(cache) = &cache {
            self.replace_cache(cache)?;
        }
        if !overrides.is_empty() {
            self.put_overrides(overrides.tracks.iter().map(|(id, ov)| (id.as_str(), ov)))?;
        }
        set_meta(&self.conn, META_LEGACY_IMPORTED, "1")?;

        for src in [cache_json, overrides_json] {
            if src.exists() {
                let _ = std::fs::rename(src, src.with_extension("migrated"));
            }
        }
        Ok((!report.is_empty()).then_some(report))
    }

    // ---- 内部 ----

    fn load_favorite_album_groups(&self) -> Result<Vec<Vec<String>>> {
        let mut stmt = self.conn.prepare(
            "SELECT favorite_id, track_id
             FROM favorite_album_track
             ORDER BY favorite_id, track_id",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut groups: Vec<Vec<String>> = Vec::new();
        let mut current_id: Option<String> = None;
        for row in rows {
            let (favorite_id, track_id) = row?;
            if current_id.as_deref() != Some(favorite_id.as_str()) {
                groups.push(Vec::new());
                current_id = Some(favorite_id);
            }
            groups
                .last_mut()
                .expect("新收藏分组已在上面创建")
                .push(track_id);
        }
        Ok(groups)
    }

    /// 收尾 v3 → v4 的专辑收藏迁移。
    ///
    /// v3 只留下「哪些曲目曾属于某张收藏专辑」，分组边界已经丢了。若当前缓存仍在，
    /// 就按此刻的聚合结果把同一张专辑重建成一组；暂时不在曲库里的旧 ID 保守地各留
    /// 一组，至少不丢收藏。staging 表与新分组在同一事务里交接，可安全重试。
    fn finish_album_favorite_migration(&mut self) -> Result<()> {
        if !self.table_exists("favorite_album_track_v3")? {
            return Ok(());
        }

        let legacy_ids = self.column("SELECT track_id FROM favorite_album_track_v3")?;
        let legacy_set: HashSet<&str> = legacy_ids.iter().map(String::as_str).collect();
        let mut consumed: HashSet<String> = HashSet::new();
        let mut groups: Vec<Vec<String>> = Vec::new();

        // 缓存只是重建分组的辅助证据；它若读不懂，不能因此挡住整个数据库升级。
        // 那时退化成单曲分组，收藏本身仍一条不少地保留下来。
        if !legacy_ids.is_empty() {
            if let (Ok(cache), Ok(overrides)) = (self.load_cache(), self.load_overrides()) {
                let snapshot = cache.library(&overrides);
                let mut tracks_by_album: HashMap<String, Vec<String>> = HashMap::new();
                for track in &snapshot.tracks {
                    if let Some(album_id) = &track.album_id {
                        tracks_by_album
                            .entry(album_id.clone())
                            .or_default()
                            .push(track.id.clone());
                    }
                }
                for album in &snapshot.albums {
                    let Some(track_ids) = tracks_by_album.remove(&album.id) else {
                        continue;
                    };
                    if !track_ids.iter().any(|id| legacy_set.contains(id.as_str())) {
                        continue;
                    }
                    for id in &track_ids {
                        if legacy_set.contains(id.as_str()) {
                            consumed.insert(id.clone());
                        }
                    }
                    groups.push(normalized_track_ids(&track_ids));
                }
            }
        }
        for id in legacy_ids {
            if !consumed.contains(&id) {
                groups.push(vec![id]);
            }
        }

        let tx = self.conn.transaction()?;
        for group in groups {
            insert_album_favorite_on(&tx, &group)?;
        }
        tx.execute("DROP TABLE favorite_album_track_v3", [])?;
        tx.commit()?;
        Ok(())
    }

    fn table_exists(&self, name: &str) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [name],
            |r| r.get(0),
        )?)
    }

    /// 取一列文本。收藏那四张表都是「单列的集合」，各写一遍 query_map 只是抄四份。
    fn column(&self, sql: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    fn meta_u32(&self, key: &str) -> Result<Option<u32>> {
        Ok(self.meta(key)?.and_then(|v| v.parse().ok()))
    }
}

fn put_override_on(conn: &Connection, track_id: &str, ov: &TrackOverride) -> Result<()> {
    if ov.is_empty() {
        conn.execute("DELETE FROM track_override WHERE track_id = ?1", [track_id])?;
        return Ok(());
    }
    conn.execute(
        "INSERT INTO track_override (track_id, title, artist, album, album_artist, disc_no, track_no)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(track_id) DO UPDATE SET
             title = excluded.title, artist = excluded.artist, album = excluded.album,
             album_artist = excluded.album_artist, disc_no = excluded.disc_no,
             track_no = excluded.track_no",
        params![
            track_id,
            ov.title,
            ov.artist,
            ov.album,
            ov.album_artist,
            ov.disc_no,
            ov.track_no
        ],
    )?;
    Ok(())
}

fn normalized_track_ids(track_ids: &[String]) -> Vec<String> {
    let mut ids: Vec<String> = track_ids
        .iter()
        .filter(|id| !id.is_empty())
        .cloned()
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// 同一份成员快照得到同一个内部 ID，使重试与重复点击天然幂等。
fn album_favorite_id(track_ids: &[String]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"shannon:favorite-album:v1\0");
    for track_id in track_ids {
        hasher.update(&(track_id.len() as u64).to_le_bytes());
        hasher.update(track_id.as_bytes());
    }
    format!("fa-{}", hasher.finalize().to_hex())
}

fn insert_album_favorite_on(conn: &Connection, track_ids: &[String]) -> Result<()> {
    let track_ids = normalized_track_ids(track_ids);
    if track_ids.is_empty() {
        return Ok(());
    }
    let favorite_id = album_favorite_id(&track_ids);
    conn.execute(
        "INSERT OR IGNORE INTO favorite_album (id) VALUES (?1)",
        [&favorite_id],
    )?;
    let mut stmt = conn.prepare(
        "INSERT OR IGNORE INTO favorite_album_track (favorite_id, track_id) VALUES (?1, ?2)",
    )?;
    for track_id in &track_ids {
        stmt.execute(params![&favorite_id, track_id])?;
    }
    Ok(())
}

/// 曲目 / 歌手 / 歌单收藏都是「在 / 不在」的单列集合，插入用 `OR IGNORE`：重复收藏
/// 同一项不该报错，那只是用户点快了或两处界面同时点了。专辑有成员分组，单独处理。
fn toggle_row(conn: &Connection, table: &str, column: &str, value: &str, on: bool) -> Result<()> {
    let sql = if on {
        format!("INSERT OR IGNORE INTO {table} ({column}) VALUES (?1)")
    } else {
        format!("DELETE FROM {table} WHERE {column} = ?1")
    };
    conn.execute(&sql, [value])?;
    Ok(())
}

fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// 只有这两类错误能证明文件本身有问题；锁竞争、只读、磁盘满、I/O 故障与迁移失败
/// 都只是当前打不开，不能因此移动用户数据。
fn is_corruption_error(err: &DbError) -> bool {
    matches!(
        err,
        DbError::Sqlite(rusqlite::Error::SqliteFailure(e, _))
            if matches!(
                e.code,
                rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase
            )
    )
}

fn ensure_schema_supported(conn: &Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(DbError::SchemaTooNew {
            found: version,
            supported: SCHEMA_VERSION,
        });
    }
    Ok(())
}

/// 逐版本升级。每一版只管从 `n` 到 `n+1`，不写「从任意版本直达最新」的分支——
/// 那种分支的组合数随版本平方增长，而且没有哪一条会被经常走到，也就没人会发现它坏了。
fn migrate(conn: &mut Connection) -> Result<()> {
    let mut version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > SCHEMA_VERSION {
        // 未来版本的文件：不认识就不要动它。降级运行时把表按旧结构改一遍，
        // 等于让新版本再也读不回自己的数据。
        return Err(DbError::SchemaTooNew {
            found: version,
            supported: SCHEMA_VERSION,
        });
    }
    while version < SCHEMA_VERSION {
        let tx = conn.transaction()?;
        match version {
            0 => tx.execute_batch(SCHEMA_V1)?,
            1 => tx.execute_batch(SCHEMA_V2)?,
            2 => tx.execute_batch(SCHEMA_V3)?,
            3 => tx.execute_batch(SCHEMA_V4)?,
            4 => tx.execute_batch(SCHEMA_V5)?,
            _ => unreachable!("缺少 v{version} 到 v{} 的升级", version + 1),
        }
        version += 1;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }
    Ok(())
}

/// v1：曲库的最小完整形态。
///
/// **音频规格：标量拆列，树存 JSON。** 拆列的目的是能查、能按 `probe_version` 做增量
/// 重扫；不拆的那几项（具名布局、空间格式、探测备注）是带 tag 的枚举与数组，摊平成列
/// 会让每支持一种新布局就要迁移一次表，而戒律恰恰要求「判不出一律留空、不归一化」——
/// 原样存下来的 JSON 正好满足这一点。
///
/// **`track_override` 与 `track` 之间没有外键。** 键是曲目 ID（内容哈希），覆盖记录
/// 应当比曲目行活得更久：文件挪走后重扫、曲目行消失，用户的修改仍须等在那里，等文件
/// 回来再接上。级联删除会把这件事悄悄办成「整理一次文件就丢一次修改」。
const SCHEMA_V1: &str = r#"
CREATE TABLE meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE scan_root (
  ord  INTEGER PRIMARY KEY,
  path TEXT NOT NULL UNIQUE
);

CREATE TABLE track (
  id             TEXT PRIMARY KEY,
  path           TEXT NOT NULL,
  title          TEXT,
  artist         TEXT,
  album_artist   TEXT,
  album          TEXT,
  year           INTEGER,
  genre          TEXT,
  track_no       INTEGER,
  disc_no        INTEGER,
  cover_key      TEXT,
  has_cover      INTEGER NOT NULL,
  duration_sec   REAL NOT NULL,
  container      TEXT NOT NULL,
  codec          TEXT NOT NULL,
  encoding       TEXT NOT NULL,
  sample_rate_hz INTEGER NOT NULL,
  bit_depth      INTEGER,
  bitrate_kbps   INTEGER,
  lossless       INTEGER,
  channels       INTEGER NOT NULL,
  channel_mask   INTEGER,
  channel_layout TEXT,
  spatial        TEXT,
  probe_notes    TEXT NOT NULL,
  probe_version  INTEGER NOT NULL
);

CREATE INDEX track_path_idx ON track(path);
CREATE INDEX track_probe_version_idx ON track(probe_version);

CREATE TABLE track_override (
  track_id     TEXT PRIMARY KEY,
  title        TEXT,
  artist       TEXT,
  album        TEXT,
  album_artist TEXT,
  disc_no      INTEGER,
  track_no     INTEGER
);
"#;

/// v2：稳定曲目 ID 可以对应多个路径，数据库行改由路径唯一标识。
///
/// v1 把 `id` 当主键，但 ID 的设计目标恰恰是扛住移动 / 重命名：字节级相同的两份副本
/// 必然同 ID。扫描缓存在聚合前必须保留这些副本，才能做标签多数决、质量择优和重复计数，
/// 因此不能在落盘层先吞掉其中一条。
const SCHEMA_V2: &str = r#"
DROP INDEX track_path_idx;
DROP INDEX track_probe_version_idx;
ALTER TABLE track RENAME TO track_v1;

CREATE TABLE track (
  id             TEXT NOT NULL,
  path           TEXT NOT NULL PRIMARY KEY,
  title          TEXT,
  artist         TEXT,
  album_artist   TEXT,
  album          TEXT,
  year           INTEGER,
  genre          TEXT,
  track_no       INTEGER,
  disc_no        INTEGER,
  cover_key      TEXT,
  has_cover      INTEGER NOT NULL,
  duration_sec   REAL NOT NULL,
  container      TEXT NOT NULL,
  codec          TEXT NOT NULL,
  encoding       TEXT NOT NULL,
  sample_rate_hz INTEGER NOT NULL,
  bit_depth      INTEGER,
  bitrate_kbps   INTEGER,
  lossless       INTEGER,
  channels       INTEGER NOT NULL,
  channel_mask   INTEGER,
  channel_layout TEXT,
  spatial        TEXT,
  probe_notes    TEXT NOT NULL,
  probe_version  INTEGER NOT NULL
);

INSERT INTO track (
  id, path, title, artist, album_artist, album, year, genre, track_no, disc_no,
  cover_key, has_cover, duration_sec, container, codec, encoding, sample_rate_hz,
  bit_depth, bitrate_kbps, lossless, channels, channel_mask, channel_layout,
  spatial, probe_notes, probe_version
)
SELECT
  id, path, title, artist, album_artist, album, year, genre, track_no, disc_no,
  cover_key, has_cover, duration_sec, container, codec, encoding, sample_rate_hz,
  bit_depth, bitrate_kbps, lossless, channels, channel_mask, channel_layout,
  spatial, probe_notes, probe_version
FROM track_v1;

DROP TABLE track_v1;
CREATE INDEX track_id_idx ON track(id);
CREATE INDEX track_probe_version_idx ON track(probe_version);
"#;

/// v3：收藏与歌单。
///
/// **键全部落到曲目 ID 上**（理由见 `crate::collections` 模块头）：专辑收藏存的是
/// 「收藏时该专辑的曲目 ID」而不是专辑 ID——后者由含目录的归组键哈希而来，改标签、
/// 挪文件、重扫都会变，拿它当键等于「整理一次音乐文件夹就掉收藏」。歌手是唯一的例外，
/// 因为系统里根本没有比名字更稳的歌手标识。
///
/// **`playlist_track` 到 `playlist` 的外键是对的**，与 `track_override` 那条刻意不加
/// 外键**不矛盾**：覆盖记录要比曲目行活得更久（文件挪走再回来还能接上），而歌单曲目
/// 脱离歌单没有任何意义，删歌单就该连它们一起删。区别在于「这份数据能不能独立存在」。
///
/// 主键是 `(playlist_id, ord)` 而不是 `(playlist_id, track_id)`：**同一首歌可以在一个
/// 歌单里出现两次**，那是用户的自由，不是需要去重的脏数据。
const SCHEMA_V3: &str = r#"
CREATE TABLE favorite_track (
  track_id TEXT PRIMARY KEY
);

CREATE TABLE favorite_album_track (
  track_id TEXT PRIMARY KEY
);

CREATE TABLE favorite_artist (
  name TEXT PRIMARY KEY
);

CREATE TABLE favorite_playlist (
  playlist_id TEXT PRIMARY KEY
);

CREATE TABLE playlist (
  id          TEXT PRIMARY KEY,
  title       TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  updated_at  INTEGER NOT NULL,
  ord         INTEGER NOT NULL
);

CREATE TABLE playlist_track (
  playlist_id TEXT NOT NULL REFERENCES playlist(id) ON DELETE CASCADE,
  ord         INTEGER NOT NULL,
  track_id    TEXT NOT NULL,
  PRIMARY KEY (playlist_id, ord)
);
"#;

/// v4：专辑收藏保留成员分组边界。
///
/// v3 的 `favorite_album_track` 是一张全局扁平集合。它能回答「当前专辑有没有命中」，
/// 却回答不了「命中的旧曲目原本属于哪一次专辑收藏」：收藏 A/B/C 后 C 暂时缺失，用户
/// 用当前 A/B 取消时只能删掉 A/B，C 回来便会把红心复活。v4 因此增加内部收藏实体，
/// 每笔收藏对应一份成员快照；取消任一可见成员时删掉整笔收藏。
///
/// 旧表先改名留作 staging。SQL 不掌握两遍聚合与覆盖层规则，不能在这里硬猜分组；
/// [`LibraryDb::finish_album_favorite_migration`] 会借当前缓存重建后再原子删除 staging。
const SCHEMA_V4: &str = r#"
ALTER TABLE favorite_album_track RENAME TO favorite_album_track_v3;

CREATE TABLE favorite_album (
  id TEXT PRIMARY KEY
);

CREATE TABLE favorite_album_track (
  favorite_id TEXT NOT NULL REFERENCES favorite_album(id) ON DELETE CASCADE,
  track_id    TEXT NOT NULL,
  PRIMARY KEY (favorite_id, track_id)
);

CREATE INDEX favorite_album_track_track_idx ON favorite_album_track(track_id);
"#;

/// v5：增量重扫的判据（文件大小与修改时间）。
///
/// 加列而不是重建表：这两项是纯附加信息，既不改主键也不改既有列的含义。已有记录默认
/// 补 0，而 0 与任何真实 stat 都不相等，于是它们会在下一次扫描时各被重扫一遍——
/// 一次性代价，之后自愈。给个假的初值去骗过判据才是危险的：那等于宣称一批从没核对过
/// 的记录是新鲜的。
const SCHEMA_V5: &str = r#"
ALTER TABLE track ADD COLUMN file_size INTEGER NOT NULL DEFAULT 0;
ALTER TABLE track ADD COLUMN mtime_ms INTEGER NOT NULL DEFAULT 0;
"#;

/// 行 → `RawTrack`。JSON 列的解析可能失败，所以返回值再套一层 `Result`：
/// rusqlite 的闭包只能报它自己的错误类型，把 serde 错误硬塞进去会丢掉原因。
fn row_to_track(r: &rusqlite::Row<'_>) -> rusqlite::Result<Result<RawTrack>> {
    let path: String = r.get(1)?;
    let encoding: String = r.get(15)?;
    let channel_layout: Option<String> = r.get(22)?;
    let spatial: Option<String> = r.get(23)?;
    let probe_notes: String = r.get(24)?;

    let tags = RawTags {
        title: r.get(2)?,
        artist: r.get(3)?,
        album_artist: r.get(4)?,
        album: r.get(5)?,
        year: r.get(6)?,
        genre: r.get(7)?,
        track_no: r.get(8)?,
        disc_no: r.get(9)?,
    };
    let scalar = (
        r.get::<_, String>(0)?,
        r.get::<_, Option<String>>(10)?,
        r.get::<_, bool>(11)?,
        r.get::<_, f64>(12)?,
        r.get::<_, String>(13)?,
        r.get::<_, String>(14)?,
        r.get::<_, u32>(16)?,
        r.get::<_, Option<u8>>(17)?,
        r.get::<_, Option<u32>>(18)?,
        r.get::<_, Option<bool>>(19)?,
        r.get::<_, u8>(20)?,
        r.get::<_, Option<u32>>(21)?,
        r.get::<_, u32>(25)?,
        r.get::<_, i64>(26)? as u64,
        r.get::<_, i64>(27)?,
    );

    Ok((|| {
        Ok(RawTrack {
            id: scalar.0,
            path: PathBuf::from(path),
            tags,
            cover_key: scalar.1,
            has_cover: scalar.2,
            duration_sec: scalar.3,
            file_size: scalar.13,
            mtime_ms: scalar.14,
            format: AudioFormat {
                container: scalar.4,
                codec: scalar.5,
                encoding: serde_json::from_str::<Encoding>(&encoding)?,
                sample_rate_hz: scalar.6,
                bit_depth: scalar.7,
                bitrate_kbps: scalar.8,
                lossless: scalar.9,
                channels: scalar.10,
                channel_mask: scalar.11,
                channel_layout: from_json_opt::<ChannelLayout>(channel_layout.as_deref())?,
                spatial: from_json_opt::<SpatialFormat>(spatial.as_deref())?,
                probe_notes: serde_json::from_str(&probe_notes)?,
                probe_version: scalar.12,
            },
        })
    })())
}

fn to_json_opt<T: serde::Serialize>(value: &Option<T>) -> Result<Option<String>> {
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(Into::into)
}

fn from_json_opt<T: serde::de::DeserializeOwned>(raw: Option<&str>) -> Result<Option<T>> {
    raw.map(serde_json::from_str)
        .transpose()
        .map_err(Into::into)
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| DbError::NonUtf8Path(path.to_path_buf()))
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// 损坏残骸的落点。带序号是因为它可能损坏不止一次，而每一份都可能是唯一一份
/// 还留着某次修改的副本——第二次损坏时覆盖掉第一份残骸，等于把挽救余地又收窄一次。
fn unique_backup_path(path: &Path) -> PathBuf {
    let first = path.with_extension("corrupt");
    if !first.exists() {
        return first;
    }
    for n in 2..1000 {
        let candidate = path.with_extension(format!("corrupt.{n}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    first
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PROBE_VERSION;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// 每个用例一个独立目录：这些用例要真落盘（重开、损坏、迁移都只有在真文件上
    /// 才成立），共用一个路径会让并行执行的用例互相踩。
    fn scratch(tag: &str) -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "shannon_db_{tag}_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn track(id: &str, path: &str) -> RawTrack {
        RawTrack {
            id: id.into(),
            path: PathBuf::from(path),
            tags: RawTags {
                title: Some(format!("标题 {id}")),
                artist: Some("歌手".into()),
                album: Some("专辑".into()),
                ..Default::default()
            },
            cover_key: Some("cover-1".into()),
            has_cover: true,
            duration_sec: 123.456,
            file_size: 1_024,
            mtime_ms: 1_700_000_000_000,
            format: AudioFormat {
                container: "m4a".into(),
                codec: "alac".into(),
                encoding: Encoding::Pcm,
                sample_rate_hz: 44_100,
                bit_depth: Some(16),
                bitrate_kbps: Some(900),
                lossless: Some(true),
                channels: 2,
                channel_mask: Some(0x3),
                channel_layout: Some(ChannelLayout::Stereo),
                spatial: None,
                probe_notes: vec![],
                probe_version: PROBE_VERSION,
            },
        }
    }

    fn cache_of(tracks: Vec<RawTrack>) -> ScanCache {
        ScanCache {
            roots: vec![PathBuf::from("/音乐")],
            tracks,
            failed: 2,
            cover_failed: 1,
        }
    }

    fn edit(title: &str) -> TrackOverride {
        TrackOverride {
            title: Some(title.into()),
            ..Default::default()
        }
    }

    #[test]
    fn favorites_survive_a_reopen() {
        let dir = scratch("fav-reopen");
        let path = dir.join("library.db");
        {
            let (mut db, _) = LibraryDb::open(&path).unwrap();
            db.set_favorite_track("t-1", true).unwrap();
            db.set_favorite_track("t-2", true).unwrap();
            db.set_favorite_track("t-2", false).unwrap();
            db.set_favorite_artist("白鲸电台", true).unwrap();
            db.set_favorite_playlist("pl-1", true).unwrap();
            db.set_favorite_album(&["t-9".into(), "t-8".into()], true)
                .unwrap();
        }
        let (db, _) = LibraryDb::open(&path).unwrap();
        let fav = db.load_favorites().unwrap();
        assert_eq!(fav.tracks, vec!["t-1"]);
        assert_eq!(fav.artists, vec!["白鲸电台"]);
        assert_eq!(fav.playlists, vec!["pl-1"]);
        assert!(fav.has_album(&["t-8".into()]));
    }

    /// 重复收藏同一项不该报错——用户点快了、或两处界面同时点了。
    #[test]
    fn favoriting_twice_is_not_an_error() {
        let db = LibraryDb::open_in_memory().unwrap();
        db.set_favorite_track("t-1", true).unwrap();
        db.set_favorite_track("t-1", true).unwrap();
        assert_eq!(db.load_favorites().unwrap().tracks, vec!["t-1"]);
    }

    /// 专辑收藏的立身之本：改专辑名 / 挪目录后专辑 ID 会变，而收藏必须还在。
    #[test]
    fn an_album_favorite_survives_a_rename_and_a_move() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let ids: Vec<String> = vec!["t-1".into(), "t-2".into()];
        db.set_favorite_album(&ids, true).unwrap();
        db.set_favorite_album(&["t-2".into(), "t-1".into()], true)
            .unwrap();
        let fav = db.load_favorites().unwrap();
        assert_eq!(fav.album_groups.len(), 1, "同一成员的重试应当幂等");

        // 改了名、换了目录，专辑 ID 全新——但曲目 ID 是内容哈希，一个没变。
        assert!(fav.has_album(&ids), "改名换目录后仍应认得出这张专辑");
        // 补进一首漏扫的曲目也不该让收藏消失（判据是「任意命中」而不是「全部」）。
        assert!(fav.has_album(&["t-1".into(), "t-新".into()]));
        // 完全无关的专辑不受影响。
        assert!(!fav.has_album(&["t-别的".into()]));

        db.set_favorite_album(&ids, false).unwrap();
        assert!(!db.load_favorites().unwrap().has_album(&ids));
    }

    /// 取消收藏时要删掉整份成员快照，而不只是这一刻仍在曲库里的子集。
    #[test]
    fn unfavoriting_a_visible_subset_removes_temporarily_missing_members() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        db.set_favorite_album(&["a".into(), "b".into(), "c".into()], true)
            .unwrap();

        // c 此刻没扫到，界面只能把当前可见的 a / b 传回来。
        db.set_favorite_album(&["a".into(), "b".into()], false)
            .unwrap();

        let favorites = db.load_favorites().unwrap();
        assert!(favorites.album_groups.is_empty());
        assert!(
            !favorites.has_album(&["c".into()]),
            "暂时缺失的 c 回来后不能把用户已取消的收藏复活"
        );
    }

    #[test]
    fn a_playlist_round_trips_with_its_order_and_repeats() {
        let dir = scratch("playlist");
        let path = dir.join("library.db");
        let mut playlist = Playlist::new("pl-1", "深夜驾驶", 1_700_000_000_000);
        playlist.description = "跨专辑合集".into();
        // 同一首歌出现两次是用户的自由，不是要去重的脏数据。
        playlist.track_ids = vec!["t-3".into(), "t-1".into(), "t-3".into()];
        {
            let (mut db, _) = LibraryDb::open(&path).unwrap();
            db.save_playlist(&playlist).unwrap();
        }
        let (db, _) = LibraryDb::open(&path).unwrap();
        assert_eq!(db.load_playlists().unwrap(), vec![playlist]);
    }

    #[test]
    fn deleting_a_playlist_takes_its_tracks_and_its_favorite_mark() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let mut p = Playlist::new("pl-1", "临时", 1);
        p.track_ids = vec!["t-1".into()];
        db.save_playlist(&p).unwrap();
        db.set_favorite_playlist("pl-1", true).unwrap();

        db.delete_playlist("pl-1").unwrap();
        assert!(db.load_playlists().unwrap().is_empty());
        assert!(
            db.load_favorites().unwrap().playlists.is_empty(),
            "收藏标记在另一张表上，不会被外键带走，要自己清"
        );
        let orphans: i64 = db
            .conn
            .query_row("SELECT count(*) FROM playlist_track", [], |r| r.get(0))
            .unwrap();
        assert_eq!(orphans, 0, "曲目行应由外键级联删除");
    }

    /// 保存内容不该让歌单在列表里跳位，重排也不该把没点到名的那些打乱。
    #[test]
    fn reordering_keeps_unlisted_playlists_in_their_relative_order() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        for (i, id) in ["a", "b", "c", "d"].iter().enumerate() {
            db.save_playlist(&Playlist::new(*id, *id, i as i64))
                .unwrap();
        }
        let titles = |db: &LibraryDb| -> Vec<String> {
            db.load_playlists()
                .unwrap()
                .into_iter()
                .map(|p| p.id)
                .collect()
        };
        assert_eq!(titles(&db), vec!["a", "b", "c", "d"]);

        // 再存一次 b 的内容：它应当还在原位，不该被挪到末尾。
        let mut b = Playlist::new("b", "b 改名", 99);
        b.track_ids = vec!["t-1".into()];
        db.save_playlist(&b).unwrap();
        assert_eq!(titles(&db), vec!["a", "b", "c", "d"]);

        db.reorder_playlists(&["d".into(), "a".into()]).unwrap();
        assert_eq!(
            titles(&db),
            vec!["d", "a", "b", "c"],
            "没点到名的 b、c 排在后面且保持原有先后"
        );
    }

    #[test]
    fn cache_survives_a_reopen() {
        let dir = scratch("reopen");
        let path = dir.join("library.db");
        let cache = cache_of(vec![
            track("t-1", "/音乐/一.m4a"),
            track("t-2", "/音乐/二.m4a"),
        ]);
        {
            let (mut db, report) = LibraryDb::open(&path).unwrap();
            assert_eq!(report, OpenReport::default());
            db.replace_cache(&cache).unwrap();
        }
        let (db, _) = LibraryDb::open(&path).unwrap();
        assert_eq!(db.load_cache().unwrap(), cache, "重启不该丢曲库");
    }

    /// 稳定 ID 刻意不含路径：字节级副本、移动与重命名后的文件都可能同 ID。
    /// 原始缓存必须把每条路径保留下来，聚合阶段才能质量择优并如实统计重复数。
    #[test]
    fn duplicate_stable_ids_are_stored_as_separate_paths() {
        let first = track("same-id", "/音乐/专辑/a.m4a");
        let second = track("same-id", "/音乐/专辑/b.m4a");
        let cache = cache_of(vec![second.clone(), first.clone()]);
        let expected = cache_of(vec![first, second]);

        let mut db = LibraryDb::open_in_memory().unwrap();
        db.replace_cache(&cache).unwrap();
        assert_eq!(db.load_cache().unwrap(), expected);
    }

    /// 按**当年**的列写一行 track。迁移用例不能借用 `replace_cache`：那是当前实现，
    /// 它写的是最新的列，于是每加一列就会把「旧库能不能升上来」的用例弄成编译期同谋，
    /// 测的东西也从「迁移对不对」滑成「当前实现自洽」。
    fn insert_v1_track(conn: &Connection, t: &RawTrack) {
        conn.execute(
            "INSERT INTO track (id, path, title, artist, album_artist, album, year, genre,
                                track_no, disc_no, cover_key, has_cover, duration_sec,
                                container, codec, encoding, sample_rate_hz, bit_depth,
                                bitrate_kbps, lossless, channels, channel_mask,
                                channel_layout, spatial, probe_notes, probe_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                     ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26)",
            params![
                t.id,
                t.path.to_str().unwrap(),
                t.tags.title,
                t.tags.artist,
                t.tags.album_artist,
                t.tags.album,
                t.tags.year,
                t.tags.genre,
                t.tags.track_no,
                t.tags.disc_no,
                t.cover_key,
                t.has_cover,
                t.duration_sec,
                t.format.container,
                t.format.codec,
                serde_json::to_string(&t.format.encoding).unwrap(),
                t.format.sample_rate_hz,
                t.format.bit_depth,
                t.format.bitrate_kbps,
                t.format.lossless,
                t.format.channels,
                t.format.channel_mask,
                to_json_opt(&t.format.channel_layout).unwrap(),
                to_json_opt(&t.format.spatial).unwrap(),
                serde_json::to_string(&t.format.probe_notes).unwrap(),
                t.format.probe_version,
            ],
        )
        .unwrap();
    }

    /// 已经试跑过旧实现的开发库是 v1；修正主键后也要原地升级，不能要求手删数据库。
    #[test]
    fn v1_database_migrates_to_path_key() {
        let dir = scratch("v1_migration");
        let path = dir.join("library.db");
        let cache = cache_of(vec![track("t-1", "/音乐/一.m4a")]);
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
            for tk in &cache.tracks {
                insert_v1_track(&conn, tk);
            }
            for (i, root) in cache.roots.iter().enumerate() {
                conn.execute(
                    "INSERT INTO scan_root (ord, path) VALUES (?1, ?2)",
                    params![i as i64, root.to_str().unwrap()],
                )
                .unwrap();
            }
            set_meta(&conn, META_FAILED, &cache.failed.to_string()).unwrap();
            set_meta(&conn, META_COVER_FAILED, &cache.cover_failed.to_string()).unwrap();
        }

        let (mut db, report) = LibraryDb::open(&path).unwrap();
        assert_eq!(report, OpenReport::default());
        // v1 的行没有 stat 判据，升上来必然是 0 —— 这正是「这条从没核对过」的正确表达，
        // 它与任何真实 stat 都不相等，于是下次扫描会各重扫一遍后自愈。给个假初值反而
        // 等于宣称一批没核对过的记录是新鲜的。
        let mut expected = cache.clone();
        for tk in &mut expected.tracks {
            tk.file_size = 0;
            tk.mtime_ms = 0;
        }
        assert_eq!(db.load_cache().unwrap(), expected);

        // 升级后同 ID 的另一条路径能写进去，证明约束已经换掉。
        let duplicate = cache_of(vec![
            track("same-id", "/音乐/一.m4a"),
            track("same-id", "/音乐/一 1.m4a"),
        ]);
        db.replace_cache(&duplicate).unwrap();
        assert_eq!(db.load_cache().unwrap().tracks.len(), 2);
    }

    /// v3 的扁平集合要借当前曲库重建成成员分组；否则升级后仍修不好「缺一首时取消」。
    #[test]
    fn v3_album_favorites_migrate_to_grouped_snapshots() {
        let dir = scratch("v3_album_favorites");
        let path = dir.join("library.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.execute_batch(SCHEMA_V2).unwrap();
            conn.execute_batch(SCHEMA_V3).unwrap();
            conn.pragma_update(None, "user_version", 3).unwrap();
            for tk in [
                track("t-1", "/音乐/专辑/一.m4a"),
                track("t-2", "/音乐/专辑/二.m4a"),
            ] {
                insert_v1_track(&conn, &tk);
            }
            conn.execute(
                "INSERT INTO favorite_album_track (track_id) VALUES (?1), (?2)",
                ["t-1", "t-2"],
            )
            .unwrap();
        }

        let (mut db, report) = LibraryDb::open(&path).unwrap();
        assert_eq!(report, OpenReport::default());
        let favorites = db.load_favorites().unwrap();
        assert_eq!(favorites.album_groups.len(), 1);
        assert_eq!(favorites.album_groups[0], vec!["t-1", "t-2"]);

        // 只拿仍可见的一首取消，也要带走迁移后同组的另一首。
        db.set_favorite_album(&["t-1".into()], false).unwrap();
        assert!(db.load_favorites().unwrap().album_groups.is_empty());
    }

    /// 规格里带 tag 的那几项存的是 JSON 列，最容易在往返中悄悄丢形状。
    #[test]
    fn tagged_format_fields_round_trip() {
        let mut t = track("t-1", "/音乐/环绕.flac");
        t.format.channels = 12;
        t.format.channel_mask = None; // 判不出就留空，不用声道数硬猜
        t.format.channel_layout = Some(ChannelLayout::Surround {
            main: 7,
            lfe: 1,
            height: 4,
        });
        t.format.spatial = Some(SpatialFormat::DolbyAtmos {
            joc: true,
            objects: Some(11),
        });
        t.format.probe_notes = vec![
            "codec:fallback-to-container".into(),
            "tags:unreadable".into(),
        ];

        let mut db = LibraryDb::open_in_memory().unwrap();
        db.replace_cache(&cache_of(vec![t.clone()])).unwrap();
        assert_eq!(db.load_cache().unwrap().tracks, vec![t]);
    }

    /// 重扫**不能**动用户手改的元数据。两张表之间刻意没有外键就是为了这一条。
    #[test]
    fn a_rescan_keeps_user_edits() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        db.replace_cache(&cache_of(vec![track("t-1", "/音乐/一.m4a")]))
            .unwrap();
        db.put_override("t-1", &edit("用户改的")).unwrap();

        // 文件挪走了，重扫后这一首暂时不在库里。
        db.replace_cache(&cache_of(vec![track("t-9", "/音乐/九.m4a")]))
            .unwrap();
        assert_eq!(
            db.load_overrides().unwrap().get("t-1"),
            Some(&edit("用户改的")),
            "曲目行没了，用户的修改也要等在原地"
        );

        // 文件回来了，同一个内容哈希应当直接接上。
        db.replace_cache(&cache_of(vec![track("t-1", "/别处/一.m4a")]))
            .unwrap();
        assert_eq!(
            db.load_overrides().unwrap().get("t-1"),
            Some(&edit("用户改的"))
        );
    }

    #[test]
    fn editing_one_track_leaves_the_others_alone() {
        let db = LibraryDb::open_in_memory().unwrap();
        db.put_override("t-1", &edit("甲")).unwrap();
        db.put_override("t-2", &edit("乙")).unwrap();
        db.put_override("t-1", &edit("甲改")).unwrap();

        let all = db.load_overrides().unwrap();
        assert_eq!(all.get("t-1"), Some(&edit("甲改")));
        assert_eq!(all.get("t-2"), Some(&edit("乙")), "改一首不该碰到另一首");
    }

    /// 「还原为文件信息」把覆盖清空，表里就不该再留一行全 NULL 的记录。
    #[test]
    fn an_emptied_override_is_deleted_not_kept() {
        let db = LibraryDb::open_in_memory().unwrap();
        db.put_override("t-1", &edit("改过")).unwrap();
        db.put_override("t-1", &TrackOverride::default()).unwrap();
        assert!(db.load_overrides().unwrap().is_empty());
    }

    #[test]
    fn legacy_json_is_imported_once_and_the_source_is_kept() {
        let dir = scratch("legacy");
        let cache_json = dir.join("library-cache.json");
        let overrides_json = dir.join("metadata-overrides.json");
        let cache = cache_of(vec![track("t-1", "/音乐/一.m4a")]);
        let mut overrides = Overrides::default();
        overrides.set("t-1", edit("旧版改过的"));
        std::fs::write(&cache_json, serde_json::to_string(&cache).unwrap()).unwrap();
        std::fs::write(&overrides_json, serde_json::to_string(&overrides).unwrap()).unwrap();

        let mut db = LibraryDb::open_in_memory().unwrap();
        let report = db
            .import_legacy_json(&cache_json, &overrides_json)
            .unwrap()
            .expect("应当报告导入了什么");
        assert_eq!(report.tracks, 1);
        assert_eq!(report.overrides, 1);
        assert_eq!(db.load_cache().unwrap(), cache);
        assert_eq!(db.load_overrides().unwrap(), overrides);

        assert!(!cache_json.exists(), "源文件应改名让位");
        assert!(
            cache_json.with_extension("migrated").exists(),
            "但不能删掉：覆盖层不可重建，迁移又不可回退"
        );

        // 用户随后清空了曲库；再启动一次不该把删掉的东西请回来。
        db.replace_cache(&ScanCache::default()).unwrap();
        db.delete_override("t-1").unwrap();
        assert!(db
            .import_legacy_json(&cache_json, &overrides_json)
            .unwrap()
            .is_none());
        assert!(db.load_cache().unwrap().is_empty());
        assert!(db.load_overrides().unwrap().is_empty());
    }

    /// 零曲目不等于没有缓存：空目录与全解析失败的扫描仍要记住根目录和统计。
    #[test]
    fn an_empty_legacy_cache_keeps_roots_and_failure_counts() {
        let dir = scratch("legacy_empty");
        let cache_json = dir.join("library-cache.json");
        let overrides_json = dir.join("metadata-overrides.json");
        let cache = ScanCache {
            roots: vec![PathBuf::from("/音乐/暂时为空")],
            tracks: vec![],
            failed: 7,
            cover_failed: 2,
        };
        std::fs::write(&cache_json, serde_json::to_string(&cache).unwrap()).unwrap();

        let mut db = LibraryDb::open_in_memory().unwrap();
        let report = db.import_legacy_json(&cache_json, &overrides_json).unwrap();
        assert!(report.is_none(), "没有曲目或覆盖时无需打印数量日志");
        assert_eq!(db.load_cache().unwrap(), cache);
        assert!(cache_json.with_extension("migrated").exists());
    }

    /// 损坏的库要留残骸，不能静默重建——里面有用户手改的元数据。
    #[test]
    fn a_corrupt_database_is_preserved() {
        let dir = scratch("corrupt");
        let path = dir.join("library.db");
        std::fs::write(&path, "这不是一个 SQLite 文件，只是一堆字节").unwrap();

        let (db, report) = LibraryDb::open(&path).unwrap();
        let backup = report.corrupt_backup.expect("必须报告残骸位置");
        assert!(backup.exists(), "残骸要真的留在盘上");
        assert!(db.load_cache().unwrap().is_empty(), "新库是空的，需要重扫");

        // 再坏一次，第一份残骸不能被覆盖：那可能是唯一还留着某次修改的副本。
        drop(db);
        std::fs::write(&path, "又坏了").unwrap();
        let (_, second) = LibraryDb::open(&path).unwrap();
        let second = second.corrupt_backup.unwrap();
        assert_ne!(second, backup);
        assert!(backup.exists() && second.exists());
    }

    /// 打不开不等于损坏。路径被同名目录占用会报 CannotOpen，但绝不能把整个目录搬走
    /// 再在原地新建一个数据库；只读、锁竞争与磁盘满走的也是同一条保守分支。
    #[test]
    fn an_operational_open_error_does_not_quarantine_the_path() {
        let dir = scratch("open_error");
        let path = dir.join("library.db");
        std::fs::create_dir(&path).unwrap();

        assert!(LibraryDb::open(&path).is_err());
        assert!(path.is_dir(), "普通打开失败不能移动原路径");
        assert!(!path.with_extension("corrupt").exists());
    }

    #[test]
    fn only_corruption_error_codes_are_quarantined() {
        let sqlite_error = |code| {
            DbError::Sqlite(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(code),
                None,
            ))
        };
        assert!(is_corruption_error(&sqlite_error(
            rusqlite::ffi::SQLITE_CORRUPT
        )));
        assert!(is_corruption_error(&sqlite_error(
            rusqlite::ffi::SQLITE_NOTADB
        )));
        assert!(!is_corruption_error(&sqlite_error(
            rusqlite::ffi::SQLITE_READONLY
        )));
        assert!(!is_corruption_error(&sqlite_error(
            rusqlite::ffi::SQLITE_BUSY
        )));
        assert!(!is_corruption_error(&sqlite_error(
            rusqlite::ffi::SQLITE_FULL
        )));
    }

    /// 降级运行时不许把新版本的库按旧结构改一遍——那会让新版本再也读不回自己的数据。
    #[test]
    fn a_newer_schema_is_refused_instead_of_downgraded() {
        let dir = scratch("newer");
        let path = dir.join("library.db");
        let cache = cache_of(vec![track("t-1", "/音乐/一.m4a")]);
        {
            let (mut db, _) = LibraryDb::open(&path).unwrap();
            db.replace_cache(&cache).unwrap();
            db.put_override("t-1", &edit("用户改的")).unwrap();
            db.conn
                .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
                .unwrap();
            let mode: String = db
                .conn
                .query_row("PRAGMA journal_mode = DELETE", [], |r| r.get(0))
                .unwrap();
            assert_eq!(mode, "delete");
        }
        assert!(
            matches!(
                LibraryDb::open(&path),
                Err(DbError::SchemaTooNew { found, supported })
                    if found == SCHEMA_VERSION + 1 && supported == SCHEMA_VERSION
            ),
            "版本高于本程序时应当报错，而不是就地改表"
        );
        assert!(
            !path.with_extension("corrupt").exists(),
            "这不是损坏，不该留残骸把用户吓一跳"
        );
        {
            let conn = Connection::open(&path).unwrap();
            let mode: String = conn
                .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                .unwrap();
            assert_eq!(mode, "delete", "拒绝未来 schema 前不该先改 journal mode");
        }

        // 换回本版本能读的号，数据必须原封不动——证明刚才那次拒绝没有动过文件。
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)
                .unwrap();
        }
        let (db, _) = LibraryDb::open(&path).unwrap();
        assert_eq!(db.load_cache().unwrap(), cache);
        assert_eq!(
            db.load_overrides().unwrap().get("t-1"),
            Some(&edit("用户改的"))
        );
    }
}
