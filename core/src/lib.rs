//! 香农播放器核心逻辑。
//!
//! 刻意**不依赖 Tauri 与任何 GUI 库**：曲库扫描、规格探测、ID 生成都是纯逻辑，
//! 拆出来后既能在无图形环境下 `cargo test`，也让 Tauri 外壳退化为一层薄适配。
//! 进度上报之类的副作用通过回调传入，由外壳决定怎么落地（Tauri event / 日志）。

pub mod cache;
pub mod cover;
pub mod db;
pub mod id;
pub mod model;
pub mod overrides;
pub mod probe;
pub mod scan;
