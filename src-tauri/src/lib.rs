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
use shannon_core::model::{LibrarySnapshot, ScanProgress};
use shannon_core::overrides::{Overrides, TrackMetadataPatch};
use shannon_core::scan;
use tauri::{Emitter, Manager, State};

mod frontend_state;
mod loudness;
mod player;
use loudness::LoudnessState;
use player::PlayerState;

/// 扫描进度事件名。前端 `listen()` 用同一字符串。
pub const EVENT_SCAN_PROGRESS: &str = "library://scan-progress";

const CACHE_FILE: &str = "library-cache.json";
const OVERRIDES_FILE: &str = "metadata-overrides.json";
/// 封面缩略图目录（应用数据目录下）。里面按封面内容指纹命名，多档共存。
const COVER_DIR: &str = "covers";
/// 前端正常会在首屏提交后立即显示窗口；若启动脚本异常，外壳最多等这么久便兜底显示。
const WINDOW_REVEAL_FALLBACK: Duration = Duration::from_secs(3);

/// 曲库状态。
///
/// 分成两份是刻意的：`cache` 是扫描的原始产出（可重建），`overrides` 是用户手改的
/// 元数据（**不可重建**，丢了就找不回来）。两者都落盘，但重要性完全不同——
/// 缓存损坏大不了重扫，覆盖损坏就是用户的劳动白费。
#[derive(Default)]
pub struct LibraryState {
    cache: Mutex<ScanCache>,
    overrides: Mutex<Overrides>,
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

/// 保存覆盖层。写失败必须让前端知道——用户以为改好了，其实重启就没了。
fn persist_overrides(app: &tauri::AppHandle, state: &LibraryState) -> Result<(), String> {
    let path = data_path(app, OVERRIDES_FILE)?;
    let overrides = state.overrides.lock().map_err(|e| e.to_string())?;
    overrides
        .save(&path)
        .map_err(|e| format!("保存元数据修改失败: {e}"))
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
    let cache = scan::scan_folders(&roots, Some(&covers), |p: ScanProgress| {
        // 事件发送失败不该中断扫描（例如窗口已关闭）。
        let _ = app.emit(EVENT_SCAN_PROGRESS, &p);
    });
    if cache.cover_failed > 0 {
        log::warn!(
            "{} 张内嵌封面解码失败，这些专辑回落占位渐变",
            cache.cover_failed
        );
    }
    // 缓存写失败只记日志：曲库这次仍然可用，只是下次启动要重扫。
    if let Ok(path) = data_path(&app, CACHE_FILE) {
        if let Err(e) = cache.save(&path) {
            log::warn!("曲库缓存写入失败，下次启动需重扫: {e}");
        }
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
    app: tauri::AppHandle,
    state: State<'_, LibraryState>,
    track_id: String,
    patch: TrackMetadataPatch,
) -> Result<LibrarySnapshot, String> {
    {
        let mut overrides = state.overrides.lock().map_err(|e| e.to_string())?;
        overrides.merge(&track_id, patch);
    }
    persist_overrides(&app, &state)?;
    snapshot(&state)
}

/// 改写整张专辑：展开成该专辑当前每一首的覆盖记录。
///
/// 之所以逐曲展开而不是记「专辑级覆盖」：专辑 ID 是聚合派生的，用户一改专辑艺人
/// 它就变了，拿它当持久化的键会立刻失效。曲目 ID 是内容哈希，不会。
#[tauri::command]
fn set_album_metadata(
    app: tauri::AppHandle,
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
    {
        let mut overrides = state.overrides.lock().map_err(|e| e.to_string())?;
        for id in ids {
            overrides.merge(&id, patch.clone());
        }
    }
    persist_overrides(&app, &state)?;
    snapshot(&state)
}

/// 还原为文件里的原始信息（清除该曲的全部修改）。
#[tauri::command]
fn reset_track_metadata(
    app: tauri::AppHandle,
    state: State<'_, LibraryState>,
    track_id: String,
) -> Result<LibrarySnapshot, String> {
    {
        let mut overrides = state.overrides.lock().map_err(|e| e.to_string())?;
        overrides.clear(&track_id);
    }
    persist_overrides(&app, &state)?;
    snapshot(&state)
}

/// 还原整张专辑的修改。
#[tauri::command]
fn reset_album_metadata(
    app: tauri::AppHandle,
    state: State<'_, LibraryState>,
    album_id: String,
) -> Result<LibrarySnapshot, String> {
    let ids: Vec<String> = snapshot(&state)?
        .tracks
        .into_iter()
        .filter(|t| t.album_id.as_deref() == Some(album_id.as_str()))
        .map(|t| t.id)
        .collect();
    {
        let mut overrides = state.overrides.lock().map_err(|e| e.to_string())?;
        for id in ids {
            overrides.clear(&id);
        }
    }
    persist_overrides(&app, &state)?;
    snapshot(&state)
}

/// 启动时读回上次的缓存与覆盖：曲库不该因为重启就消失。
fn restore(app: &tauri::AppHandle, state: &LibraryState) {
    if let Ok(path) = data_path(app, CACHE_FILE) {
        match ScanCache::load(&path) {
            Ok(c) if !c.is_empty() => {
                log::info!("已从缓存恢复 {} 首曲目", c.tracks.len());
                if let Ok(mut slot) = state.cache.lock() {
                    *slot = c;
                }
            }
            Ok(_) => {}
            Err(e) => log::warn!("曲库缓存读取失败，将需要重扫: {e}"),
        }
    }
    if let Ok(path) = data_path(app, OVERRIDES_FILE) {
        match Overrides::load(&path) {
            Ok(o) => {
                if let Ok(mut slot) = state.overrides.lock() {
                    *slot = o;
                }
            }
            Err(e) => log::warn!("元数据修改读取失败: {e}"),
        }
    }
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
            let state = LibraryState::default();
            restore(app.handle(), &state);
            app.manage(state);
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
            player::player_load,
            player::player_set_next,
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
