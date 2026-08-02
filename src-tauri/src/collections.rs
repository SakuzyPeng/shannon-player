//! 收藏与歌单的命令。
//!
//! 与元数据覆盖同一条规矩：**先落库、成功才算数**。这两份都是用户自己攒出来的东西，
//! 写失败必须让他知道，不能界面上红心亮着、重启就没了。
//!
//! 歌单 ID 在**后端**生成而不是前端：它要进数据库当主键，也要被收藏表引用，
//! 由拥有存储的那一侧发号才不会出现「前端以为叫这个、库里叫那个」。

use std::time::{SystemTime, UNIX_EPOCH};

use shannon_core::collections::{Favorites, Playlist};
use tauri::State;

use crate::LibraryState;

/// 取数据库的可变引用并执行一段操作；数据库不可用时给出与元数据编辑一致的说法。
fn with_db<T>(
    state: &LibraryState,
    what: &str,
    f: impl FnOnce(&mut shannon_core::db::LibraryDb) -> shannon_core::db::Result<T>,
) -> Result<T, String> {
    let mut slot = state.db.lock().map_err(|e| e.to_string())?;
    let db = slot.as_mut().ok_or("曲库数据库不可用，修改无法保存")?;
    f(db).map_err(|e| format!("{what}失败: {e}"))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        // 系统时钟被调到 1970 之前时不该让「收藏一首歌」直接失败：时间戳只用来排序与
        // 显示「多久以前」，退化成 0 只是显示得难看，而报错会让用户完全无法操作。
        .unwrap_or(0)
}

#[tauri::command]
pub fn collections_load(
    state: State<'_, LibraryState>,
) -> Result<(Favorites, Vec<Playlist>), String> {
    let mut slot = state.db.lock().map_err(|e| e.to_string())?;
    let Some(db) = slot.as_mut() else {
        // 数据库打不开时返回空而不是报错：那一趟应用仍然可用，只是没有收藏可恢复。
        // 真正需要惊动用户的是**写**失败，那时他刚做了动作、期待着结果。
        return Ok((Favorites::default(), Vec::new()));
    };
    let favorites = db
        .load_favorites()
        .map_err(|e| format!("读取收藏失败: {e}"))?;
    let playlists = db
        .load_playlists()
        .map_err(|e| format!("读取歌单失败: {e}"))?;
    Ok((favorites, playlists))
}

#[tauri::command]
pub fn favorite_track(
    state: State<'_, LibraryState>,
    track_id: String,
    on: bool,
) -> Result<(), String> {
    with_db(&state, "保存收藏", |db| {
        db.set_favorite_track(&track_id, on)
    })
}

/// 收藏 / 取消收藏一张专辑。传入的是它**当前**的全部曲目 ID——专辑 ID 由含目录的
/// 归组键哈希而来，改标签或挪文件就变，不能作为持久化的键（见 `core/src/id.rs`）。
#[tauri::command]
pub fn favorite_album(
    state: State<'_, LibraryState>,
    track_ids: Vec<String>,
    on: bool,
) -> Result<(), String> {
    with_db(&state, "保存收藏", |db| {
        db.set_favorite_album(&track_ids, on)
    })
}

#[tauri::command]
pub fn favorite_artist(
    state: State<'_, LibraryState>,
    name: String,
    on: bool,
) -> Result<(), String> {
    with_db(&state, "保存收藏", |db| {
        db.set_favorite_artist(&name, on)
    })
}

#[tauri::command]
pub fn favorite_playlist(
    state: State<'_, LibraryState>,
    playlist_id: String,
    on: bool,
) -> Result<(), String> {
    with_db(&state, "保存收藏", |db| {
        db.set_favorite_playlist(&playlist_id, on)
    })
}

/// 新建歌单，返回后端发的 ID 与时间戳。
#[tauri::command]
pub fn playlist_create(
    state: State<'_, LibraryState>,
    title: String,
    track_ids: Vec<String>,
) -> Result<Playlist, String> {
    let playlist = Playlist {
        // 用毫秒时间戳加一段随机后缀：单靠时间戳时，同一毫秒里连建两个歌单会撞 ID。
        id: format!("pl-{}-{:x}", now_ms(), fastrand_suffix()),
        title,
        description: String::new(),
        track_ids,
        updated_at_ms: now_ms(),
    };
    with_db(&state, "新建歌单", |db| db.save_playlist(&playlist))?;
    Ok(playlist)
}

/// 整体保存一个歌单（改名、改简介、重排或增删曲目都走这条）。
///
/// 歌单的编辑单位本来就是整条曲目列表——拖一次就全变了——所以这里整份重写它的曲目，
/// 与覆盖层「只写点到名的那几行」不冲突：那边的编辑单位是单首歌的单个字段。
#[tauri::command]
pub fn playlist_save(
    state: State<'_, LibraryState>,
    mut playlist: Playlist,
) -> Result<Playlist, String> {
    // 时间戳由后端盖，不信前端传来的：那是「最后一次改动发生在什么时候」的事实，
    // 让调用方自报会让一次时钟不准或一处漏传就把排序搞乱。
    playlist.updated_at_ms = now_ms();
    with_db(&state, "保存歌单", |db| db.save_playlist(&playlist))?;
    Ok(playlist)
}

#[tauri::command]
pub fn playlist_delete(state: State<'_, LibraryState>, playlist_id: String) -> Result<(), String> {
    with_db(&state, "删除歌单", |db| {
        db.delete_playlist(&playlist_id)
    })
}

#[tauri::command]
pub fn playlist_reorder(state: State<'_, LibraryState>, ids: Vec<String>) -> Result<(), String> {
    with_db(&state, "重排歌单", |db| db.reorder_playlists(&ids))
}

/// 一小段随机后缀，只为在同一毫秒内区分两次新建。不引入 rand 依赖：这里既不需要
/// 密码学强度，也不需要均匀分布，够区分就行。
fn fastrand_suffix() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    RandomState::new().build_hasher().finish()
}
