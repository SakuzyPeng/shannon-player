//! 播放引擎与前端之间的契约类型。
//!
//! 放在本 crate 而不是外壳里，与 `ScanProgress` 放在 `shannon-core` 是同一个道理：
//! **契约属于产生它的那一层**。放进 `src-tauri` 会让本该薄的外壳承担领域建模，
//! 而外壳一旦开始定义「播放状态有哪几种」，这个定义就再也无法被无头测试覆盖。
//!
//! 与 `engine.rs` 的内部类型刻意分开，不是重复：
//!
//! - `EngineEvent` 携带 `SourceSpec` / `EngineError` 这类**引擎内部结构**，它们会随
//!   实现演进（比如将来加对象音频的元数据），不该每改一次就震动前端；
//! - 契约类型只暴露界面真正要显示的东西，且必须 `camelCase` 序列化并经 ts-rs 导出，
//!   Rust 一漂移前端 `pnpm build` 就报错。
//!
//! ## 为什么有 `resampled` 与 `outputSampleRate`
//!
//! 「bit-perfect」「原样输出」这类说法必须有据可依。链路里插没插重采样是客观事实，
//! 界面要么如实显示，要么闭嘴——悄悄转换一级却仍宣称原样输出，是这类播放器最常见的
//! 失实描述（架构约束验收条件第 7 条：未经证实的状态不得展示）。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::decode::SourceSpec;
use crate::engine::{EngineEvent, PlaybackState, StampedEngineEvent};
use crate::error::{EngineError, ErrorKind, Stage};
use crate::output::OutputConfig;

/// 播放状态。与 [`PlaybackState`] 一一对应。
///
/// `ended` 与 `idle` 分开是给界面用的：一个是「放完了」（进度条停在末尾、可以重播），
/// 一个是「还没放过」（进度条空着）。合并成一个状态，界面就只能靠进度值反推。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/player.ts")]
#[serde(rename_all = "camelCase")]
pub enum PlayerStatus {
    Idle,
    Loading,
    Playing,
    Paused,
    Ended,
    Error,
}

impl From<PlaybackState> for PlayerStatus {
    fn from(state: PlaybackState) -> Self {
        match state {
            PlaybackState::Idle => Self::Idle,
            PlaybackState::Loading => Self::Loading,
            PlaybackState::Playing => Self::Playing,
            PlaybackState::Paused => Self::Paused,
            PlaybackState::Ended => Self::Ended,
            PlaybackState::Error => Self::Error,
        }
    }
}

/// 已打开音源的规格，以及实际协商到的输出配置。
///
/// 声道布局给的是**描述串**而不是枚举：布局判不出来时它就该是空的，
/// 而一个「未知」枚举值会诱使界面把它显示成一种布局（见音频规格建模戒律①）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/player.ts")]
#[serde(rename_all = "camelCase")]
pub struct PlaybackFormat {
    /// 容器与编码的原始名（探测器怎么报的就怎么给，不归一化）。
    pub container: String,
    pub codec: String,
    /// 源采样率。
    pub sample_rate: u32,
    pub channels: u16,
    /// 声道布局描述，判不出来就是 `None`——不用声道数硬猜。
    pub layout: Option<String>,
    pub duration_sec: Option<f64>,
    /// 输出端点名。
    pub device_name: String,
    /// 输出采样率。与 `sampleRate` 不同即说明插了重采样。
    pub output_sample_rate: u32,
    pub sample_format: String,
    /// 链路里是否发生了重采样。界面若要展示「原样输出」，判据是这一项。
    pub resampled: bool,
}

impl PlaybackFormat {
    pub fn new(spec: &SourceSpec, output: &OutputConfig) -> Self {
        let layout = spec.layout.describe();
        Self {
            container: spec.container.clone(),
            codec: spec.codec.clone(),
            sample_rate: spec.sample_rate,
            channels: spec.layout.count(),
            // `describe()` 在判不出布局时给的是占位描述，这里只保留确有结论的。
            layout: (!layout.is_empty()).then_some(layout),
            duration_sec: spec.duration_sec(),
            device_name: output.device_name.clone(),
            output_sample_rate: output.sample_rate,
            sample_format: output.sample_format.clone(),
            resampled: output.sample_rate != spec.sample_rate,
        }
    }
}

/// 播放失败的结构化描述。
///
/// 分级给出而不是拍成一句话：界面要按 `kind` 决定措辞（找不到文件 vs 格式不支持
/// vs 设备被占用，用户要做的事完全不同），而 `message` 只适合放进详情或日志。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/player.ts")]
#[serde(rename_all = "camelCase")]
pub struct PlaybackError {
    /// 出错阶段：`open` / `probe` / `decode` / `output`。
    pub stage: String,
    /// 错误类别：`io` / `unsupported` / `decode` / `noDevice` / `deviceConfig` / `stream`。
    pub kind: String,
    /// 容器与编码（已知时给出），界面据此说清「哪种格式放不了」。
    pub container: Option<String>,
    pub codec: Option<String>,
    /// 面向诊断的原文，不直接当界面文案用（它没有经过 i18n）。
    pub message: String,
}

impl From<&EngineError> for PlaybackError {
    fn from(err: &EngineError) -> Self {
        Self {
            stage: match err.stage {
                Stage::Open => "open",
                Stage::Probe => "probe",
                Stage::Decode => "decode",
                Stage::Output => "output",
            }
            .into(),
            kind: match err.kind {
                ErrorKind::Io => "io",
                ErrorKind::Unsupported => "unsupported",
                ErrorKind::Decode => "decode",
                ErrorKind::NoDevice => "noDevice",
                ErrorKind::DeviceConfig => "deviceConfig",
                ErrorKind::Stream => "stream",
            }
            .into(),
            container: err.container.clone(),
            codec: err.codec.clone(),
            message: err.message.clone(),
        }
    }
}

/// 推给前端的播放事件。
///
/// `trackId` 与 `loadId` 随装载命令进入引擎，再由产生事件的那个装载代际原样回带。
/// 不能由外壳读取一个共享的“最新曲目”来盖章：命令入队是异步的，后一首会先覆盖共享值，
/// 让前一首随后产生的事件冒充后一首。`loadId` 还负责区分同一曲目的连续重载。
// `rename_all` 在标签枚举上**只作用于 variant 名**，字段名要另外用 `rename_all_fields`。
// 少了它，序列化出来的是 `track_id` 而前端按 `trackId` 读——每个字段都是 undefined，
// 且不会有任何编译期或运行期报错。下面那条序列化测试就是为了钉死这一点。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/player.ts")]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum PlayerEvent {
    /// 音源已打开，附规格与输出配置。
    Opened {
        track_id: Option<String>,
        load_id: String,
        format: PlaybackFormat,
    },
    /// 播放状态变化。
    Status {
        track_id: Option<String>,
        load_id: String,
        status: PlayerStatus,
    },
    /// 进度推进（约 5 Hz）。界面在事件之间自行插值，事件只做重锚定。
    Progress {
        track_id: Option<String>,
        load_id: String,
        position_sec: f64,
        duration_sec: Option<f64>,
        buffered_sec: f64,
    },
    /// 无缝接续到了下一首。
    ///
    /// 与 `Opened` 的差别不只是字段：它由**消费端越过边界帧**时判定，也就是说事件到达时
    /// 新曲已经在响了。前端据此对账（按 `trackId` 在当前队列里定位），而不是按队列版本号
    /// 校验相等——已经发声的切歌是既成事实，丢弃它只会让界面停在一首早已放完的歌上。
    TrackChanged {
        track_id: Option<String>,
        load_id: String,
        /// 刚放完的那首。前端用它确认这次交接接在谁后面。
        from_track_id: Option<String>,
        /// 这次交接依据的是哪一版队列。用于诊断与对账，不作为丢弃事件的判据。
        queue_revision: u32,
        format: PlaybackFormat,
    },
    /// 播放到自然结束。**队列已经没有下一首**——有的话走的是 `TrackChanged`。
    /// 循环与随机是前端的领域，引擎只负责放好当前这一首。
    Ended {
        track_id: Option<String>,
        load_id: String,
    },
    /// 播放失败。
    Failed {
        track_id: Option<String>,
        load_id: String,
        error: PlaybackError,
    },
}

impl PlayerEvent {
    /// 把已带装载上下文的引擎事件转成前端契约事件。
    pub fn from_engine(stamped: &StampedEngineEvent) -> Self {
        let track_id = stamped.context.track_id.clone();
        let load_id = stamped.context.load_id.clone();
        match &stamped.event {
            EngineEvent::Opened { spec, output } => Self::Opened {
                track_id,
                load_id,
                format: PlaybackFormat::new(spec, output),
            },
            EngineEvent::StateChanged(state) => Self::Status {
                track_id,
                load_id,
                status: (*state).into(),
            },
            EngineEvent::Progress {
                position_sec,
                duration_sec,
                buffered_sec,
            } => Self::Progress {
                track_id,
                load_id,
                position_sec: *position_sec,
                duration_sec: *duration_sec,
                buffered_sec: *buffered_sec,
            },
            EngineEvent::TrackChanged {
                from,
                spec,
                output,
                queue_revision,
            } => Self::TrackChanged {
                track_id,
                load_id,
                from_track_id: from.as_ref().and_then(|c| c.track_id.clone()),
                queue_revision: *queue_revision,
                format: PlaybackFormat::new(spec, output),
            },
            EngineEvent::TrackEnded => Self::Ended { track_id, load_id },
            EngineEvent::Error(err) => Self::Failed {
                track_id,
                load_id,
                error: err.into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_kinds_map_to_stable_strings() {
        // 这些字符串是前端的分支依据，改动等于改契约。
        let err = EngineError::new(Stage::Probe, ErrorKind::Unsupported, "测试")
            .with_format(Some("mka".into()), Some("opus".into()));
        let dto = PlaybackError::from(&err);
        assert_eq!(dto.stage, "probe");
        assert_eq!(dto.kind, "unsupported");
        assert_eq!(dto.container.as_deref(), Some("mka"));
        assert_eq!(dto.codec.as_deref(), Some("opus"));
    }

    #[test]
    fn events_serialize_with_a_type_tag() {
        // 前端按 `type` 分派；少了它就只能靠字段有无来猜事件种类。
        let event = PlayerEvent::Ended {
            track_id: Some("t-1".into()),
            load_id: "load-7".into(),
        };
        let json = serde_json::to_string(&event).expect("契约事件必须可序列化");
        assert!(json.contains("\"type\":\"ended\""), "实际 JSON：{json}");
        assert!(json.contains("\"trackId\":\"t-1\""), "字段必须是 camelCase");
        assert!(
            json.contains("\"loadId\":\"load-7\""),
            "装载代际必须随事件回带"
        );
    }
}
