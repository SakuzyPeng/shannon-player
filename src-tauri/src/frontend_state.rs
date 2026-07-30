//! 前端拥有的状态的落盘：播放会话与界面设置。
//!
//! ## 为什么这里只存一段字符串
//!
//! 曲库快照与元数据覆盖是**后端产出**的，所以它们的结构定义在 `shannon-core` 并经
//! ts-rs 下发给前端。这两份相反——队列怎么排、进度到哪、主题是深是浅、界面用哪种语言，
//! 都是**前端拥有**的状态，后端在其中没有任何领域判断可做。给它们定义 Rust 结构，
//! 只会让前端每加一个字段就要改两处、重跑一次契约导出，而后端从头到尾不读它一个字节。
//!
//! 所以这里的职责就是「原子地存一段文本、原样读回来」，schema 与版本号由前端自己带
//! （见 `src/lib/session.ts` 与 `src/lib/settings.ts`）。
//!
//! ## 槽位是固定常量，不接受前端给的文件名
//!
//! 一个 `save(name, json)` 式的通用接口看着更省事，但那等于把应用数据目录里的任意路径
//! 交给渲染进程去写。槽位数量有限且已知，写死两个常量既够用，也没有这个口子。
//!
//! ## 四份落盘数据的重要性不同
//!
//! - `library-cache.json`：可重建，损坏就重扫；
//! - `metadata-overrides.json`：**不可重建**，损坏时保留 `.corrupt` 残骸而非静默覆盖；
//! - `loudness-analysis.json`：可重建，但代价是把整个曲库解码一遍；
//! - 本模块这两份：可重建但用户会在意——丢了不致命（重新点一次歌、重新挑一次主题），
//!   可每次重启都丢很烦。因此读取失败一律**静默当作没有**：为一份能随手重建的数据
//!   弹错误框，打扰的成本高于它本身的价值。

use crate::data_path;

const SESSION_FILE: &str = "playback-session.json";
const SETTINGS_FILE: &str = "ui-settings.json";

/// 原子写：先写同目录临时文件再 rename，避免退出过程中断电留下半个 JSON——
/// 那会让下次启动读到一份语法正确但内容截断的状态，比什么都没有更难排查。
fn write_atomic(app: &tauri::AppHandle, name: &str, json: String) -> Result<(), String> {
    let path = data_path(app, name)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// 读回一段状态；没有或读不出来都返回 `None`。
///
/// 不区分「文件不存在」与「文件损坏」：对调用方而言两者要做的事完全相同——按没有处理。
/// 多给一个错误分支，只会让前端多写一段没有动作的代码。
fn read_text(app: &tauri::AppHandle, name: &str) -> Option<String> {
    std::fs::read_to_string(data_path(app, name).ok()?).ok()
}

#[tauri::command]
pub fn save_session(app: tauri::AppHandle, json: String) -> Result<(), String> {
    write_atomic(&app, SESSION_FILE, json)
}

#[tauri::command]
pub fn load_session(app: tauri::AppHandle) -> Option<String> {
    read_text(&app, SESSION_FILE)
}

#[tauri::command]
pub fn save_settings(app: tauri::AppHandle, json: String) -> Result<(), String> {
    write_atomic(&app, SETTINGS_FILE, json)
}

/// 读回界面设置。
///
/// 这一份要在**首帧之前**取到：主题晚一步应用就是一次白闪，而那恰好发生在用户每次
/// 启动应用的那一刻（见 `src/main.tsx`）。所以保持成一次同步文件读取，不做额外的事。
#[tauri::command]
pub fn load_settings(app: tauri::AppHandle) -> Option<String> {
    read_text(&app, SETTINGS_FILE)
}
