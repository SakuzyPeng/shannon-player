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
use crate::output::{fill_from_ring, ramp_step_for, OutputBackend, OutputConfig, OutputRequest, OutputShared};
use crate::ring::RingConsumer;

/// 模拟回调的周期。取值贴近常见设备的缓冲区（约 10 ms）。
const TICK: Duration = Duration::from_millis(10);

pub struct NullOutput {
    config: Option<OutputConfig>,
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
        Self { config: None, stop: Arc::new(AtomicBool::new(false)), worker: None }
    }
}

impl OutputBackend for NullOutput {
    fn name(&self) -> &'static str {
        "null"
    }

    fn open(
        &mut self,
        request: &OutputRequest,
        mut consumer: RingConsumer,
        shared: Arc<OutputShared>,
    ) -> Result<OutputConfig> {
        self.close();

        // 空后端全盘接受请求：它不代表任何真实设备，没有可协商的能力边界。
        let config = OutputConfig {
            sample_rate: request.sample_rate,
            layout: request.layout,
            sample_format: "f32".into(),
            device_name: "空输出（无声）".into(),
        };

        let channels = request.layout.count() as usize;
        let sample_rate = request.sample_rate;
        let frames_per_tick = (sample_rate as u64 * TICK.as_millis() as u64 / 1000) as usize;
        let ramp_step = ramp_step_for(sample_rate);
        let stop = Arc::new(AtomicBool::new(false));
        self.stop = stop.clone();

        self.worker = Some(std::thread::spawn(move || {
            let mut buf = vec![0.0f32; frames_per_tick * channels];
            let mut gain = 0.0f32;
            let mut next = Instant::now();
            while !stop.load(Ordering::Relaxed) {
                fill_from_ring(&mut buf, channels, &mut consumer, &shared, &mut gain, ramp_step);
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
