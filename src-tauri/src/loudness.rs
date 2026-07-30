//! 响度归一化的外壳接线：持有分析服务、暴露队列命令。
//!
//! 逻辑全在 `shannon-audio::loudness`（零 Tauri 依赖、可无头测试），这里只做三件事：
//! 把服务放进应用数据目录、把前端给的播放顺序转成分析队列、在装载时把增益查出来。
//!
//! ## 分析顺序为什么由前端给
//!
//! 优先级就是「距当前播放位置的远近」，而**队列的权威在前端**（见
//! `docs/AUDIO_BACKEND_IMPLEMENTATION_PLAN.md` 的「队列归属」）。后端自己排会需要
//! 复制一份队列、循环与随机状态，那正是两处状态迟早分叉的经典写法。
//!
//! ## 增益为什么不由前端传
//!
//! 反过来，具体增益是后端知识：它取决于分析结果、目标响度与峰值上限，改策略应当
//! 立即生效而不必让前端跟着改。前端只说「这次装载要不要归一化」（那是用户的设置），
//! 后端答「那就是这个倍率」。

use std::path::PathBuf;

use serde::Deserialize;
use shannon_audio::{AnalysisItem, LoudnessService};
use tauri::State;

/// 分析结果的落盘文件名（应用数据目录下）。
pub const LOUDNESS_FILE: &str = "loudness-analysis.json";

pub struct LoudnessState {
    service: LoudnessService,
}

impl LoudnessState {
    pub fn spawn(store_path: PathBuf) -> Self {
        Self {
            service: LoudnessService::spawn(store_path),
        }
    }

    /// 该曲目的线性增益。没分析过就是 1.0（不处理）。
    pub fn linear_gain(&self, track_id: &str) -> f32 {
        self.service.linear_gain(track_id)
    }
}

/// 前端给的一件待分析曲目。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    pub track_id: String,
    pub path: String,
}

/// 按播放顺序重排分析队列，返回还有多少首要分析。
///
/// 传空表示停下（用户关掉了响度归一化）：为一个已经关掉的功能继续解码全库，
/// 用户看到的就是一个凭空吃 CPU 的播放器。
#[tauri::command]
pub fn loudness_set_queue(state: State<'_, LoudnessState>, items: Vec<QueueItem>) -> usize {
    log::info!("响度分析队列：收到 {} 首", items.len());
    state.service.set_queue(items.into_iter().map(|item| AnalysisItem {
        track_id: item.track_id,
        path: PathBuf::from(item.path),
    }))
}

/// 还有多少首没分析完。0 表示当前队列范围内已全部完成。
#[tauri::command]
pub fn loudness_pending(state: State<'_, LoudnessState>) -> usize {
    state.service.pending()
}
