//! Tauri 外壳：窗口 + 命令注册 + 状态持有。
//!
//! 业务逻辑全在 `shannon-core` 与 `shannon-audio`（都不依赖 Tauri，可在无图形环境下
//! 测试），这里只做四件事：把命令暴露给前端、把进度与播放事件转成 Tauri event、
//! 持有曲库与播放器状态、把状态落到应用数据目录。
//!
//! 播放那半边在 `player.rs`，播放会话与界面设置的落盘在 `frontend_state.rs`。

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use shannon_core::cache::ScanCache;
use shannon_core::db::LibraryDb;
use shannon_core::model::{LibrarySnapshot, ScanProgress};
use shannon_core::overrides::{Overrides, TrackMetadataPatch, TrackOverride};
use shannon_core::scan;
use tauri::{Emitter, Manager, State};

mod collections;
mod frontend_state;
mod loudness;
mod player;
use loudness::LoudnessState;
use player::PlayerState;

/// 扫描进度事件名。前端 `listen()` 用同一字符串。
pub const EVENT_SCAN_PROGRESS: &str = "library://scan-progress";

/// 曲库数据库。扫描缓存与元数据覆盖层都在里面（schema 见 `shannon_core::db`）。
const LIBRARY_DB: &str = "library.db";
/// 0.1 时期的两份 JSON。只在首次打开数据库时读一次做迁移，随后改名为 `.migrated`。
const LEGACY_CACHE_FILE: &str = "library-cache.json";
const LEGACY_OVERRIDES_FILE: &str = "metadata-overrides.json";
/// 封面缩略图目录（应用数据目录下）。里面按封面内容指纹命名，多档共存。
const COVER_DIR: &str = "covers";
/// 前端正常会在首屏提交后立即显示窗口；若启动脚本异常，外壳最多等这么久便兜底显示。
const WINDOW_REVEAL_FALLBACK: Duration = Duration::from_secs(3);

/// 曲库状态。
///
/// 分成两份是刻意的：`cache` 是扫描的原始产出（可重建），`overrides` 是用户手改的
/// 元数据（**不可重建**，丢了就找不回来）。两者都落盘，但重要性完全不同——
/// 缓存损坏大不了重扫，覆盖损坏就是用户的劳动白费。
///
/// 内存里这两份仍是权威的**读**路径：聚合要全部曲目在手（专辑艺人是组级结论），
/// 每次取快照都回数据库查一遍只会把毫秒级的纯内存计算变成一次全表扫描。数据库负责
/// 的是**写**与重启后的恢复。
///
/// `db` 是 `Option` 而不是必然存在：磁盘满、卷只读、权限不对都会让它打不开。那时应用
/// 照常启动（曲库为空，需要重扫），而用户一旦去改元数据就会当场收到写入失败——这与
/// 过去 JSON 写失败的表现一致，比「静默接受修改、重启后消失」诚实得多。
pub struct LibraryState {
    cache: Mutex<ScanCache>,
    overrides: Mutex<Overrides>,
    /// 连接长期持有：WAL 与同步级别都是连接级状态，每条命令现开一次会反复配置，
    /// 还会在并发命令之间重复抢文件锁。
    pub(crate) db: Mutex<Option<LibraryDb>>,
}

/// 应用数据目录下的文件路径。
pub(crate) fn data_path(app: &tauri::AppHandle, name: &str) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|d| d.join(name))
        .map_err(|e| e.to_string())
}

/// 套用当前覆盖，聚合出前端要的快照。纯内存计算，改一次元数据不必重扫。
fn snapshot(state: &LibraryState) -> Result<LibrarySnapshot, String> {
    let cache = state.cache.lock().map_err(|e| e.to_string())?;
    let overrides = state.overrides.lock().map_err(|e| e.to_string())?;
    Ok(cache.library(&overrides))
}

/// 计算并落盘这些曲目的新覆盖值；数据库成功后才提交到内存。
///
/// 只写点到名的那几行，不再整份重写：改一个字段就把用户的全部修改重新序列化一遍，
/// 等于为一次小改动把不可重建的数据整体置于风险之下。空覆盖由 `put_override`
/// 处理成删除，所以「改」与「还原」走同一条路径。
///
/// `overrides` 锁一直持有到数据库事务结束：这样其它命令与 `snapshot` 看不到暂存值，
/// 写失败时内存仍是原样。只复制受影响的几行，不为改一首歌克隆整份覆盖表。
fn update_overrides(
    state: &LibraryState,
    ids: &[String],
    update: impl FnOnce(&mut Overrides),
) -> Result<(), String> {
    let mut current = state.overrides.lock().map_err(|e| e.to_string())?;
    let mut staged = Overrides::default();
    for id in ids {
        if let Some(value) = current.get(id).cloned() {
            staged.set(id, value);
        }
    }
    update(&mut staged);

    let values: Vec<(String, TrackOverride)> = ids
        .iter()
        .map(|id| (id.clone(), staged.get(id).cloned().unwrap_or_default()))
        .collect();
    {
        let mut db_slot = state.db.lock().map_err(|e| e.to_string())?;
        let db = db_slot.as_mut().ok_or("曲库数据库不可用，修改无法保存")?;
        db.put_overrides(values.iter().map(|(id, ov)| (id.as_str(), ov)))
            .map_err(|e| format!("保存元数据修改失败: {e}"))?;
    }

    for (id, value) in values {
        current.set(&id, value);
    }
    Ok(())
}

/// 扫描指定文件夹并返回曲库快照，同时把进度以事件推给前端。
///
/// 阻塞式命令：Tauri 会在独立线程上执行，不会卡住 UI。
#[tauri::command]
fn scan_library(
    app: tauri::AppHandle,
    state: State<'_, LibraryState>,
    folders: Vec<String>,
) -> Result<LibrarySnapshot, String> {
    let roots: Vec<PathBuf> = folders.into_iter().map(PathBuf::from).collect();
    if roots.is_empty() {
        return Err("未指定音乐文件夹".into());
    }
    let covers = data_path(&app, COVER_DIR)?;
    // 把上一次的结果交给扫描器：文件没变过的条目直接复用，不再打开。
    // 这里克隆一份而不是持锁扫描——扫一次真实曲库要几十秒，全程占着锁会让取曲库、
    // 改元数据这些命令统统卡住。
    let previous = state.cache.lock().map_err(|e| e.to_string())?.clone();
    let cache = scan::scan_folders_incremental(
        &roots,
        Some(&covers),
        Some(&previous),
        |p: ScanProgress| {
            // 事件发送失败不该中断扫描（例如窗口已关闭）。
            let _ = app.emit(EVENT_SCAN_PROGRESS, &p);
        },
    );
    if cache.cover_failed > 0 {
        log::warn!(
            "{} 张内嵌封面解码失败，这些专辑回落占位渐变",
            cache.cover_failed
        );
    }
    // 缓存写失败只记日志：曲库这次仍然可用，只是下次启动要重扫。
    // 这一步**不碰覆盖层**，重扫不该动用户手改的东西（`replace_cache` 的注释讲了为什么
    // 两张表之间没有外键）。
    match state.db.lock() {
        Ok(mut db) => match db.as_mut() {
            Some(db) => {
                if let Err(e) = db.replace_cache(&cache) {
                    log::warn!("曲库缓存写入失败，下次启动需重扫: {e}");
                }
            }
            None => log::warn!("曲库数据库不可用，本次扫描结果不会保留到下次启动"),
        },
        Err(e) => log::warn!("曲库数据库锁不可用，本次扫描结果不会保留: {e}"),
    }
    *state.cache.lock().map_err(|e| e.to_string())? = cache;
    snapshot(&state)
}

/// 取当前曲库快照（前端启动时问一次；尚未扫描过则曲目为空）。
#[tauri::command]
fn get_library(state: State<'_, LibraryState>) -> Result<Option<LibrarySnapshot>, String> {
    let snap = snapshot(&state)?;
    Ok((!snap.tracks.is_empty()).then_some(snap))
}

/// 封面缩略图目录。前端据此拼 asset URL（按显示尺寸挑档位），
/// 路径拼接放前端是因为档位选择本来就是前端的显示决策。
#[tauri::command]
fn get_cover_dir(app: tauri::AppHandle) -> Result<String, String> {
    Ok(data_path(&app, COVER_DIR)?.to_string_lossy().to_string())
}

/// 上次扫描用的音乐文件夹。设置页显示的是这个，不是写死的示例路径。
#[tauri::command]
fn get_music_folders(state: State<'_, LibraryState>) -> Result<Vec<String>, String> {
    let cache = state.cache.lock().map_err(|e| e.to_string())?;
    Ok(cache
        .roots
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect())
}

/// 只遍历不解析，快速估算规模（首启页在开扫前显示总数）。
#[tauri::command]
fn count_audio_files(folders: Vec<String>) -> u32 {
    let roots: Vec<PathBuf> = folders.into_iter().map(PathBuf::from).collect();
    scan::count_candidates(&roots)
}

/// 改写单曲元数据。传入的字段为 null 表示不动，空字符串表示撤销该字段的修改。
#[tauri::command]
fn set_track_metadata(
    state: State<'_, LibraryState>,
    track_id: String,
    patch: TrackMetadataPatch,
) -> Result<LibrarySnapshot, String> {
    update_overrides(&state, std::slice::from_ref(&track_id), |overrides| {
        overrides.merge(&track_id, patch);
    })?;
    snapshot(&state)
}

/// 改写整张专辑：展开成该专辑当前每一首的覆盖记录。
///
/// 之所以逐曲展开而不是记「专辑级覆盖」：专辑 ID 是聚合派生的，用户一改专辑艺人
/// 它就变了，拿它当持久化的键会立刻失效。曲目 ID 是内容哈希，不会。
#[tauri::command]
fn set_album_metadata(
    state: State<'_, LibraryState>,
    album_id: String,
    patch: TrackMetadataPatch,
) -> Result<LibrarySnapshot, String> {
    let ids: Vec<String> = snapshot(&state)?
        .tracks
        .into_iter()
        .filter(|t| t.album_id.as_deref() == Some(album_id.as_str()))
        .map(|t| t.id)
        .collect();
    if ids.is_empty() {
        return Err("找不到该专辑的曲目".into());
    }
    update_overrides(&state, &ids, |overrides| {
        for id in &ids {
            overrides.merge(id, patch.clone());
        }
    })?;
    snapshot(&state)
}

/// 还原为文件里的原始信息（清除该曲的全部修改）。
#[tauri::command]
fn reset_track_metadata(
    state: State<'_, LibraryState>,
    track_id: String,
) -> Result<LibrarySnapshot, String> {
    update_overrides(&state, std::slice::from_ref(&track_id), |overrides| {
        overrides.clear(&track_id);
    })?;
    snapshot(&state)
}

/// 还原整张专辑的修改。
#[tauri::command]
fn reset_album_metadata(
    state: State<'_, LibraryState>,
    album_id: String,
) -> Result<LibrarySnapshot, String> {
    let ids: Vec<String> = snapshot(&state)?
        .tracks
        .into_iter()
        .filter(|t| t.album_id.as_deref() == Some(album_id.as_str()))
        .map(|t| t.id)
        .collect();
    update_overrides(&state, &ids, |overrides| {
        for id in &ids {
            overrides.clear(id);
        }
    })?;
    snapshot(&state)
}

/// 打开数据库并读回上次的缓存与覆盖：曲库不该因为重启就消失。
///
/// 数据库打不开时仍然返回一个可用的 `LibraryState`（曲库为空、`db` 为 `None`）：
/// 一个装不了数据库的应用仍然应该能启动、能扫描、能放歌，只是这一趟的结果留不下来。
/// 直接 panic 会让「磁盘满了」表现成「应用打不开」，用户既看不出原因也无从补救。
fn restore(app: &tauri::AppHandle) -> LibraryState {
    let state = LibraryState {
        cache: Mutex::new(ScanCache::default()),
        overrides: Mutex::new(Overrides::default()),
        db: Mutex::new(None),
    };
    let Ok(path) = data_path(app, LIBRARY_DB) else {
        log::error!("取不到应用数据目录，曲库无法持久化");
        return state;
    };
    let (mut db, report) = match LibraryDb::open(&path) {
        Ok(v) => v,
        Err(e) => {
            log::error!("曲库数据库打不开，本次运行的扫描与元数据修改都不会保留: {e}");
            return state;
        }
    };
    if let Some(backup) = &report.corrupt_backup {
        // 缓存可以重扫，覆盖层不能——残骸留着，至少还有人工挽救的余地。
        log::error!(
            "曲库数据库损坏，已保留残骸 {}；曲库需要重扫，元数据修改可能丢失",
            backup.display()
        );
    }

    // 0.1 的两份 JSON 搬进来。只做一次，做完源文件改名为 `.migrated`。
    if let (Ok(cache_json), Ok(overrides_json)) = (
        data_path(app, LEGACY_CACHE_FILE),
        data_path(app, LEGACY_OVERRIDES_FILE),
    ) {
        match db.import_legacy_json(&cache_json, &overrides_json) {
            Ok(Some(imported)) => log::info!(
                "已从旧 JSON 迁移 {} 首曲目、{} 条元数据修改",
                imported.tracks,
                imported.overrides
            ),
            Ok(None) => {}
            // 迁移失败不能把应用卡住：源文件还在（没改名），下次启动会再试一次。
            Err(e) => log::error!("从旧 JSON 迁移失败，原文件保留待下次重试: {e}"),
        }
    }

    match db.load_cache() {
        Ok(c) if !c.is_empty() => {
            log::info!("已从数据库恢复 {} 首曲目", c.tracks.len());
            if let Ok(mut slot) = state.cache.lock() {
                *slot = c;
            }
        }
        Ok(_) => {}
        Err(e) => log::warn!("曲库读取失败，将需要重扫: {e}"),
    }
    match db.load_overrides() {
        Ok(o) => {
            if let Ok(mut slot) = state.overrides.lock() {
                *slot = o;
            }
        }
        Err(e) => log::warn!("元数据修改读取失败: {e}"),
    }
    if let Ok(mut slot) = state.db.lock() {
        *slot = Some(db);
    }
    state
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            app.manage(restore(app.handle()));
            // 播放器状态是空壳，引擎推迟到第一次真要放东西时才起——
            // 否则应用一启动就占住声卡（理由见 `player.rs` 模块头）。
            app.manage(PlayerState::default());
            // 响度分析服务随启动就位（只起一条空闲的后台线程，不碰设备也不解码），
            // 真正开工要等前端按播放顺序喂队列进来。
            let store_path = data_path(app.handle(), loudness::LOUDNESS_FILE)
                .unwrap_or_else(|_| PathBuf::from(loudness::LOUDNESS_FILE));
            app.manage(LoudnessState::spawn(store_path));
            // 主窗口初始隐藏，等前端把落盘主题同步应用并提交首屏后再显示，避免深色用户
            // 启动时先看到默认浅色背景。若 JS 加载或 show IPC 异常，三秒后仍要把窗口交给
            // 用户，不能让一个视觉优化把应用变成「点了没反应」。show() 是幂等的。
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(WINDOW_REVEAL_FALLBACK);
                let Some(window) = app_handle.get_webview_window("main") else {
                    return;
                };
                let should_reveal = match window.is_visible() {
                    Ok(visible) => !visible,
                    Err(error) => {
                        log::warn!("无法确认主窗口是否可见，仍执行兜底显示: {error}");
                        true
                    }
                };
                if should_reveal {
                    log::warn!("前端未在三秒内显示主窗口，外壳执行兜底显示");
                    if let Err(error) = window.show() {
                        log::error!("兜底显示主窗口失败: {error}");
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_library,
            get_library,
            get_music_folders,
            get_cover_dir,
            count_audio_files,
            set_track_metadata,
            set_album_metadata,
            reset_track_metadata,
            reset_album_metadata,
            collections::collections_load,
            collections::favorite_track,
            collections::favorite_album,
            collections::favorite_artist,
            collections::favorite_playlist,
            collections::playlist_create,
            collections::playlist_save,
            collections::playlist_delete,
            collections::playlist_reorder,
            player::player_load,
            player::player_set_next,
            player::player_list_devices,
            player::player_set_device,
            player::player_play,
            player::player_pause,
            player::player_seek,
            player::player_set_volume,
            player::player_stop,
            frontend_state::save_session,
            frontend_state::load_session,
            frontend_state::save_settings,
            frontend_state::load_settings,
            loudness::loudness_set_queue,
            loudness::loudness_pending
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with(db: Option<LibraryDb>, overrides: Overrides) -> LibraryState {
        LibraryState {
            cache: Mutex::new(ScanCache::default()),
            overrides: Mutex::new(overrides),
            db: Mutex::new(db),
        }
    }

    #[test]
    fn failed_override_write_does_not_change_memory() {
        let mut initial = Overrides::default();
        initial.set(
            "t-1",
            TrackOverride {
                title: Some("原值".into()),
                ..Default::default()
            },
        );
        let state = state_with(None, initial.clone());
        let ids = vec!["t-1".to_string()];

        let result = update_overrides(&state, &ids, |staged| {
            staged.merge(
                "t-1",
                TrackMetadataPatch {
                    title: Some("未落盘的新值".into()),
                    ..Default::default()
                },
            );
        });

        assert!(result.is_err());
        assert_eq!(*state.overrides.lock().unwrap(), initial);
    }

    #[test]
    fn successful_override_write_commits_database_and_memory_together() {
        let state = state_with(
            Some(LibraryDb::open_in_memory().unwrap()),
            Overrides::default(),
        );
        let ids = vec!["t-1".to_string()];
        let expected = TrackOverride {
            title: Some("已落盘".into()),
            ..Default::default()
        };

        update_overrides(&state, &ids, |staged| {
            staged.set("t-1", expected.clone());
        })
        .unwrap();

        assert_eq!(state.overrides.lock().unwrap().get("t-1"), Some(&expected));
        let db = state.db.lock().unwrap();
        assert_eq!(
            db.as_ref().unwrap().load_overrides().unwrap().get("t-1"),
            Some(&expected)
        );
    }
}
