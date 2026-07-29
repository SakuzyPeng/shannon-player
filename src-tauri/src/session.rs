//! 播放会话的持久化：队列、当前曲目、播放位置、循环与随机状态。
//!
//! ## 为什么这里只存一个字符串
//!
//! 曲库快照与元数据覆盖是**后端产出**的，所以它们的结构定义在 `shannon-core` 并经
//! ts-rs 下发给前端。播放会话相反——它是**前端拥有**的状态（队列怎么排、随机顺序如何、
//! 进度到哪），后端在其中没有任何领域判断可做。给它定义一份 Rust 结构，只会让前端每加
//! 一个字段就要改两处、重跑一次契约导出，而后端从头到尾不读它一个字节。
//!
//! 所以这里的职责就是「原子地存一段文本、原样读回来」，schema 与版本号由前端自己带
//! （见 `src/lib/session.ts`）。
//!
//! ## 三份落盘数据的重要性不同
//!
//! - `library-cache.json`：可重建，损坏就重扫；
//! - `metadata-overrides.json`：**不可重建**，损坏时保留 `.corrupt` 残骸而非静默覆盖；
//! - 本文件：可重建但用户会在意——丢了不致命（重新点一次歌），可每次重启都丢很烦。
//!   因此读取失败一律**静默当作没有会话**：为一份能随手重建的数据弹错误框，
//!   打扰的成本高于它本身的价值。

use std::path::PathBuf;

use tauri::Manager;

const SESSION_FILE: &str = "playback-session.json";

fn session_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|d| d.join(SESSION_FILE))
        .map_err(|e| e.to_string())
}

/// 保存播放会话。
///
/// 原子写：先写同目录临时文件再 rename，避免退出过程中断电留下半个 JSON——
/// 那会让下次启动读到一份语法正确但内容截断的会话，比没有会话更难排查。
#[tauri::command]
pub fn save_session(app: tauri::AppHandle, json: String) -> Result<(), String> {
    let path = session_path(&app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// 读回播放会话；没有或读不出来都返回 `None`。
///
/// 不区分「文件不存在」与「文件损坏」：对调用方而言两者要做的事完全相同——
/// 按没有会话处理。多给一个错误分支，只会让前端多写一段没有动作的代码。
#[tauri::command]
pub fn load_session(app: tauri::AppHandle) -> Option<String> {
    let path = session_path(&app).ok()?;
    std::fs::read_to_string(path).ok()
}
