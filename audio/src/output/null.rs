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

use crate::error::Result;
use crate::output::{
    fill_from_ring, ramp_step_for, OutputBackend, OutputConfig, OutputRequest, OutputShared,
};
use crate::ring::RingConsumer;

/// 模拟回调的周期。取值贴近常见设备的缓冲区（约 10 ms）。
const TICK: Duration = Duration::from_millis(10);

fn config_for(request: &OutputRequest, fixed_rate: Option<u32>) -> OutputConfig {
    OutputConfig {
        sample_rate: fixed_rate.unwrap_or(request.sample_rate),
        layout: request.layout,
        sample_format: "f32".into(),
        device_name: match fixed_rate {
            Some(r) => format!("空输出（无声，固定 {r} Hz）"),
            None => "空输出（无声）".into(),
        },
    }
}

pub struct NullOutput {
    config: Option<OutputConfig>,
    /// 强制采样率：不为 `None` 时，无论请求什么都只给这个值。
    ///
    /// 用来模拟**只支持有限采样率的真实设备**——这不是臆想的边界，
    /// 实测本机默认输出设备就只提供 24 与 48 kHz。重采样路径必须能在无声卡的
    /// CI 上验证，否则它的正确性就只能靠人在特定机器上手动听。
    fixed_rate: Option<u32>,
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
            fixed_rate: None,
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
        }
    }

    /// 模拟一台只支持单一采样率的设备，用于验证重采样路径。
    pub fn with_fixed_rate(rate: u32) -> Self {
        let mut out = Self::new();
        out.fixed_rate = Some(rate);
        out
    }
}

impl OutputBackend for NullOutput {
    fn name(&self) -> &'static str {
        "null"
    }

    fn negotiate(&self, request: &OutputRequest) -> Result<OutputConfig> {
        Ok(config_for(request, self.fixed_rate))
    }

    fn open(
        &mut self,
        request: &OutputRequest,
        mut consumer: RingConsumer,
        shared: Arc<OutputShared>,
    ) -> Result<OutputConfig> {
        self.close();

        let config = config_for(request, self.fixed_rate);

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
                fill_from_ring(
                    &mut buf,
                    channels,
                    &mut consumer,
                    &shared,
                    &mut gain,
                    ramp_step,
                );
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
