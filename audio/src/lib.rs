//! 香农播放器音频引擎。
//!
//! 刻意不依赖 Tauri 与任何 GUI 库——gapless、seek、欠载压测这些验收项因此能以
//! `cargo test` 无头执行。副作用（事件上报）经回调注入，由外壳决定落地方式，
//! 与 `shannon-core` 同一套路。
//!
//! 边界见 `docs/AUDIO_BACKEND_ARCHITECTURE.md`：实时播放链全部在应用进程内，
//! 不用外部播放器兜底，遇到做不了的格式返回明确的能力错误。

pub mod contract;
pub mod decode;
pub mod engine;
pub mod error;
pub mod layout;
pub mod loudness;
pub mod mix;
pub mod output;
pub mod resample;
pub mod ring;

pub use contract::{PlaybackError, PlaybackFormat, PlayerEvent, PlayerStatus};
pub use engine::{
    Engine, EngineEvent, EngineStats, LoadContext, PlaybackState, PlayerCmd, StampedEngineEvent,
};
pub use error::{EngineError, ErrorKind, Result, Stage};
pub use layout::{ChannelLayout, LayoutSource};
pub use loudness::{LoudnessAnalyzer, LoudnessOutcome};
