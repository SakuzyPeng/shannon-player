//! 引擎错误。
//!
//! 结构对齐架构约束验收条件第 2 条：**容器、编码、失败阶段、可展示信息**四者齐备。
//! 理由是「播放失败」对用户毫无意义——他需要知道是文件坏了、格式不支持，还是声卡被占用，
//! 而这三者的下一步动作完全不同。所以错误不是一个字符串，是一条结构化记录。

use std::fmt;

/// 失败发生在链路的哪一段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// 打开文件。
    Open,
    /// 容器探测与解复用。
    Probe,
    /// 解码器创建或解码。
    Decode,
    /// 输出设备协商或播放。
    Output,
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Stage::Open => "打开",
            Stage::Probe => "探测",
            Stage::Decode => "解码",
            Stage::Output => "输出",
        };
        f.write_str(s)
    }
}

/// 错误类别。决定前端的处置方式：能力不足只能提示，解码失败可以跳下一首。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// 文件读不到。
    Io,
    /// 容器 / 编码不受支持。**明确报出，不静默降级、不外挂播放器兜底。**
    Unsupported,
    /// 码流损坏或解码失败。
    Decode,
    /// 没有可用输出设备。
    NoDevice,
    /// 设备不支持所需配置（采样率 / 声道数 / 采样格式）。
    DeviceConfig,
    /// 输出流运行期错误（设备被拔出、被独占等）。
    Stream,
}

/// 结构化引擎错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineError {
    pub stage: Stage,
    pub kind: ErrorKind,
    /// 容器名，探测成功后才有。
    pub container: Option<String>,
    /// 编码名，探测成功后才有。
    pub codec: Option<String>,
    /// 可展示信息（中文，可直接进界面）。
    pub message: String,
}

impl EngineError {
    pub fn new(stage: Stage, kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { stage, kind, container: None, codec: None, message: message.into() }
    }

    /// 补上探测出的容器与编码。探测之前的失败没有这两项，因此是后置补充而非构造参数。
    pub fn with_format(mut self, container: Option<String>, codec: Option<String>) -> Self {
        self.container = container;
        self.codec = codec;
        self
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.stage, self.message)?;
        match (&self.container, &self.codec) {
            (Some(c), Some(k)) => write!(f, "（{c} / {k}）"),
            (Some(c), None) => write!(f, "（{c}）"),
            (None, Some(k)) => write!(f, "（{k}）"),
            (None, None) => Ok(()),
        }
    }
}

impl std::error::Error for EngineError {}

pub type Result<T> = std::result::Result<T, EngineError>;
