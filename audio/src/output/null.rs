//! 无声输出后端。
//!
//! 两个用途：无声卡的 CI 与集成测试（`shannon-audio` 的验收测试要能无头跑），
//! 以及前端 `MockEngine` 的行为参照——两边表现不一致的话，浏览器预览就会骗人。
//!
//! 它按真实时间节奏消费环形缓冲，而不是尽快抽干：位置推进、欠载计数、暂停语义
//! 都要与真实设备一致，否则拿它验证不出任何时序相关的问题。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::error::{ErrorKind, Result, Stage};
use crate::output::{
    fill_from_ring, ramp_step_for, DeviceEnumerator, DeviceInfo, OutputBackend, OutputConfig,
    OutputRequest, OutputShared,
};
use crate::ring::RingConsumer;

/// 模拟回调的周期。取值贴近常见设备的缓冲区（约 10 ms）。
const TICK: Duration = Duration::from_millis(10);

/// 一台假设备。
///
/// 存在的理由与 `with_fixed_rate` 相同：设备切换会静默出错（切完仍在旧设备上出声、
/// 位置按旧时基走、暂停态被切成播放），而这些都不需要声卡就能验证。真机上验证要靠
/// 插拔硬件，那是没法进 CI 的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NullDevice {
    pub id: String,
    pub label: String,
    /// 这台设备唯一支持的采样率；`None` 表示什么都支持。
    pub fixed_rate: Option<u32>,
    /// 支持的声道数。给不出请求的声道数就报能力错误，模拟「只有多声道口的接口卡」。
    pub channels: u16,
}

impl NullDevice {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            fixed_rate: None,
            channels: 2,
        }
    }

    pub fn with_fixed_rate(mut self, rate: u32) -> Self {
        self.fixed_rate = Some(rate);
        self
    }

    pub fn with_channels(mut self, channels: u16) -> Self {
        self.channels = channels;
        self
    }
}

fn device_err(kind: ErrorKind, msg: impl Into<String>) -> crate::error::EngineError {
    crate::error::EngineError::new(Stage::Output, kind, msg)
}

fn config_for(request: &OutputRequest, device: Option<&NullDevice>) -> Result<OutputConfig> {
    let Some(device) = device else {
        return Ok(OutputConfig {
            sample_rate: request.sample_rate,
            layout: request.layout,
            sample_format: "f32".into(),
            device_name: "空输出（无声）".into(),
            device_id: None,
        });
    };
    if device.channels != request.layout.count() {
        return Err(device_err(
            ErrorKind::DeviceConfig,
            format!(
                "设备「{}」不支持 {} 声道输出",
                device.label,
                request.layout.count()
            ),
        ));
    }
    Ok(OutputConfig {
        sample_rate: device.fixed_rate.unwrap_or(request.sample_rate),
        layout: request.layout,
        sample_format: "f32".into(),
        device_name: device.label.clone(),
        device_id: Some(device.id.clone()),
    })
}

pub struct NullOutput {
    config: Option<OutputConfig>,
    /// 可选的假设备表。为空时退化成「一台什么都支持的设备」，与引入设备切换前一致。
    /// 表非空时**第一台是默认**。
    devices: Vec<NullDevice>,
    /// 当前偏好的端点标识；`None` = 跟随默认。
    prefer: Option<String>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Default for NullOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl NullOutput {
    pub fn new() -> Self {
        Self {
            config: None,
            devices: Vec::new(),
            prefer: None,
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
        }
    }

    /// 模拟一台只支持单一采样率的设备，用于验证重采样路径。
    ///
    /// 这不是臆想的边界：实测本机默认输出设备就只提供 24 与 48 kHz。重采样路径必须
    /// 能在无声卡的 CI 上验证，否则它的正确性就只能靠人在特定机器上手动听。
    pub fn with_fixed_rate(rate: u32) -> Self {
        Self::with_devices([NullDevice::new(
            "null:fixed",
            format!("空输出（无声，固定 {rate} Hz）"),
        )
        .with_fixed_rate(rate)])
    }

    /// 模拟一组可切换的端点。第一台视为系统默认。
    pub fn with_devices(devices: impl IntoIterator<Item = NullDevice>) -> Self {
        let mut out = Self::new();
        out.devices = devices.into_iter().collect();
        out
    }

    /// 与该后端共享同一份设备表的枚举器。
    pub fn enumerator(&self) -> NullDevices {
        NullDevices {
            devices: self.devices.clone(),
        }
    }

    /// 解析当前该用哪台设备。挑不到指定的那台就**报错**，不回落默认——理由与 CPAL
    /// 后端同一条：静默换设备会让界面显示的端点与实际出声的端点对不上。
    fn resolve(&self) -> Result<Option<&NullDevice>> {
        if self.devices.is_empty() {
            return Ok(None);
        }
        let Some(want) = self.prefer.as_deref() else {
            return Ok(self.devices.first());
        };
        self.devices
            .iter()
            .find(|d| d.id == want)
            .map(Some)
            .ok_or_else(|| {
                device_err(
                    ErrorKind::NoDevice,
                    format!("标识为「{want}」的输出设备已不可用"),
                )
            })
    }
}

/// 与 [`NullOutput`] 共享设备表的枚举器。
pub struct NullDevices {
    devices: Vec<NullDevice>,
}

impl DeviceEnumerator for NullDevices {
    fn devices(&self) -> Result<Vec<DeviceInfo>> {
        Ok(self
            .devices
            .iter()
            .enumerate()
            .map(|(i, d)| DeviceInfo {
                id: d.id.clone(),
                label: d.label.clone(),
                is_default: i == 0,
            })
            .collect())
    }
}

impl OutputBackend for NullOutput {
    fn name(&self) -> &'static str {
        "null"
    }

    fn negotiate(&self, request: &OutputRequest) -> Result<OutputConfig> {
        config_for(request, self.resolve()?)
    }

    fn set_preferred_device(&mut self, id: Option<String>) {
        self.prefer = id;
    }

    fn open(
        &mut self,
        request: &OutputRequest,
        mut consumer: RingConsumer,
        shared: Arc<OutputShared>,
    ) -> Result<OutputConfig> {
        self.close();

        let config = config_for(request, self.resolve()?)?;

        let channels = config.layout.count() as usize;
        let sample_rate = config.sample_rate;
        let frames_per_tick = (sample_rate as u64 * TICK.as_millis() as u64 / 1000) as usize;
        let ramp_step = ramp_step_for(sample_rate);
        let stop = Arc::new(AtomicBool::new(false));
        self.stop = stop.clone();

        self.worker = Some(std::thread::spawn(move || {
            let mut buf = vec![0.0f32; frames_per_tick * channels];
            let mut gain = 0.0f32;
            let mut next = Instant::now();
            while !stop.load(Ordering::Relaxed) {
                shared.begin_callback();
                let audio_frames = fill_from_ring(
                    &mut buf,
                    channels,
                    &mut consumer,
                    &shared,
                    &mut gain,
                    ramp_step,
                );
                shared.finish_callback(audio_frames);
                next += TICK;
                let now = Instant::now();
                if next > now {
                    std::thread::sleep(next - now);
                } else {
                    // 落后了就重新对齐，不追赶——追赶会让消费速率短时超过实时。
                    next = now;
                }
            }
        }));

        self.config = Some(config.clone());
        Ok(config)
    }

    fn close(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
        self.config = None;
    }

    fn config(&self) -> Option<&OutputConfig> {
        self.config.as_ref()
    }
}

impl Drop for NullOutput {
    fn drop(&mut self) {
        self.close();
    }
}
