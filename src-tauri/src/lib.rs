//! Tauri 外壳：窗口 + 命令注册 + 状态持有。
//!
//! 业务逻辑全在 `shannon-core`（不依赖 Tauri，可在无图形环境下测试），
//! 这里只做三件事：把命令暴露给前端、把扫描进度转成 Tauri event、持有曲库快照。

use std::path::PathBuf;
use std::sync::Mutex;

use shannon_core::model::{LibrarySnapshot, ScanProgress};
use shannon_core::scan;
use tauri::{Emitter, Manager, State};

/// 扫描进度事件名。前端 `listen()` 用同一字符串。
pub const EVENT_SCAN_PROGRESS: &str = "library://scan-progress";

/// 曲库状态。当前为内存快照——SQLite 持久化是下一步。
#[derive(Default)]
pub struct LibraryState {
    snapshot: Mutex<Option<LibrarySnapshot>>,
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
    let snapshot = scan::scan_folders(&roots, |p: ScanProgress| {
        // 事件发送失败不该中断扫描（例如窗口已关闭）。
        let _ = app.emit(EVENT_SCAN_PROGRESS, &p);
    });
    *state.snapshot.lock().map_err(|e| e.to_string())? = Some(snapshot.clone());
    Ok(snapshot)
}

/// 取当前曲库快照（前端启动时问一次；尚未扫描则为 null）。
#[tauri::command]
fn get_library(state: State<'_, LibraryState>) -> Result<Option<LibrarySnapshot>, String> {
    Ok(state.snapshot.lock().map_err(|e| e.to_string())?.clone())
}

/// 只遍历不解析，快速估算规模（首启页在开扫前显示总数）。
#[tauri::command]
fn count_audio_files(folders: Vec<String>) -> u32 {
    let roots: Vec<PathBuf> = folders.into_iter().map(PathBuf::from).collect();
    scan::count_candidates(&roots)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(LibraryState::default());
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_library,
            get_library,
            count_audio_files
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
