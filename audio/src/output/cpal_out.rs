//! CPAL 共享模式输出后端。
//!
//! 覆盖范围按架构约束划界：**共享模式的立体声与普通多声道**归这里，
//! 独占、直通、空间路由、设备热插拔归各平台的原生实现。
//!
//! 两条实现纪律：
//!
//! 1. **不假设设备支持 f32**。引擎内部全链是 f32，到设备边界才转成协商出的采样格式，
//!    转换在回调内完成且不分配（scratch 预分配，超长的回调分块处理）。
//! 2. **采样率不匹配就报错，不静默变调**。阶段 0 还没有重采样，
//!    此时唯一正确的行为是报出明确原因——静默按设备采样率播放等于把每首歌都变调。

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, FromSample, SampleFormat, SizedSample, Stream, StreamConfig};

use crate::error::{EngineError, ErrorKind, Result, Stage};
use crate::output::{
    fill_from_ring, ramp_step_for, OutputBackend, OutputConfig, OutputRequest, OutputShared,
};
use crate::ring::RingConsumer;

/// 回调 scratch 的帧数上限。回调请求超过它时分块处理，因此这只影响拷贝次数，不影响正确性。
const SCRATCH_FRAMES: usize = 8192;

pub struct CpalOutput {
    stream: Option<Stream>,
    config: Option<OutputConfig>,
    errors: Option<Receiver<String>>,
}

impl Default for CpalOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl CpalOutput {
    pub fn new() -> Self {
        Self { stream: None, config: None, errors: None }
    }

    /// 取出运行期错误（设备被拔出、被独占等）。回调只投递，处置由控制线程决定。
    pub fn take_error(&self) -> Option<String> {
        self.errors.as_ref().and_then(|rx| rx.try_recv().ok())
    }
}

fn device_err(kind: ErrorKind, msg: impl Into<String>) -> EngineError {
    EngineError::new(Stage::Output, kind, msg)
}

/// 设备名，取不到就退回占位串——诊断信息不该因为拿不到名字而整条失败。
fn device_label(device: &Device) -> String {
    device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "未知设备".into())
}

/// 在设备支持的配置里挑一个：声道数与采样率都必须**精确匹配**。
///
/// 不做「最接近」的回退：声道数不符会静默丢声道，采样率不符会静默变调，
/// 两者都属于用户听得出、却没有任何提示的降级。
fn negotiate(device: &Device, request: &OutputRequest) -> Result<cpal::SupportedStreamConfig> {
    let channels = request.layout.count();
    let supported = device
        .supported_output_configs()
        .map_err(|e| device_err(ErrorKind::DeviceConfig, format!("读取设备能力失败：{e}")))?;

    let mut saw_channels = false;
    let mut candidates: Vec<cpal::SupportedStreamConfigRange> = Vec::new();
    for range in supported {
        if range.channels() == channels {
            saw_channels = true;
            if range.min_sample_rate() <= request.sample_rate
                && request.sample_rate <= range.max_sample_rate()
            {
                candidates.push(range);
            }
        }
    }

    // 优先 f32：引擎内部就是 f32，选中它可以省掉设备边界的格式转换。
    candidates.sort_by_key(|r| match r.sample_format() {
        SampleFormat::F32 => 0,
        SampleFormat::I32 => 1,
        SampleFormat::I16 => 2,
        _ => 3,
    });

    if let Some(range) = candidates.into_iter().next() {
        return Ok(range.with_sample_rate(request.sample_rate));
    }

    let name = device_label(device);
    if saw_channels {
        Err(device_err(
            ErrorKind::DeviceConfig,
            format!(
                "设备「{name}」不支持 {} Hz（重采样尚未实现，不按设备采样率强播以免变调）",
                request.sample_rate
            ),
        ))
    } else {
        Err(device_err(
            ErrorKind::DeviceConfig,
            format!("设备「{name}」不支持 {channels} 声道输出"),
        ))
    }
}

fn build_stream<T>(
    device: &Device,
    config: &StreamConfig,
    mut consumer: RingConsumer,
    shared: Arc<OutputShared>,
    err_tx: Sender<String>,
) -> Result<Stream>
where
    T: SizedSample + FromSample<f32> + 'static,
{
    let channels = config.channels as usize;
    let sample_rate = config.sample_rate;
    let ramp_step = ramp_step_for(sample_rate);
    let mut scratch = vec![0.0f32; SCRATCH_FRAMES * channels];
    let mut gain = 0.0f32;

    device
        .build_output_stream(
            *config,
            move |out: &mut [T], info: &cpal::OutputCallbackInfo| {
                // 设备延迟：播放时刻与回调时刻之差。播放位置要扣掉它，
                // 否则歌词逐字高亮会系统性偏早（共享模式延迟普遍数十毫秒）。
                let ts = info.timestamp();
                let delay = ts.playback.duration_since(ts.callback);
                let frames = (delay.as_secs_f64() * sample_rate as f64) as u64;
                shared.output_delay_frames.store(frames, std::sync::atomic::Ordering::Relaxed);

                // 回调请求可能超过 scratch，分块处理——分配是绝对禁止的。
                let chunk_samples = scratch.len();
                let mut written = 0;
                while written < out.len() {
                    let n = (out.len() - written).min(chunk_samples);
                    let block = &mut scratch[..n];
                    fill_from_ring(block, channels, &mut consumer, &shared, &mut gain, ramp_step);
                    for (dst, src) in out[written..written + n].iter_mut().zip(block.iter()) {
                        *dst = T::from_sample(*src);
                    }
                    written += n;
                }
            },
            move |err| {
                // 错误回调不是音频回调，可以投递到通道；处置由控制线程决定。
                let _ = err_tx.send(err.to_string());
            },
            None,
        )
        .map_err(|e| device_err(ErrorKind::Stream, format!("创建输出流失败：{e}")))
}

impl OutputBackend for CpalOutput {
    fn name(&self) -> &'static str {
        "cpal"
    }

    fn open(
        &mut self,
        request: &OutputRequest,
        consumer: RingConsumer,
        shared: Arc<OutputShared>,
    ) -> Result<OutputConfig> {
        self.close();

        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| device_err(ErrorKind::NoDevice, "没有可用的音频输出设备"))?;
        let device_name = device_label(&device);

        let supported = negotiate(&device, request)?;
        let sample_format = supported.sample_format();
        let config = supported.config();

        let (err_tx, err_rx) = mpsc::channel();
        let stream = match sample_format {
            SampleFormat::F32 => build_stream::<f32>(&device, &config, consumer, shared, err_tx),
            SampleFormat::I32 => build_stream::<i32>(&device, &config, consumer, shared, err_tx),
            SampleFormat::I16 => build_stream::<i16>(&device, &config, consumer, shared, err_tx),
            SampleFormat::U16 => build_stream::<u16>(&device, &config, consumer, shared, err_tx),
            SampleFormat::U8 => build_stream::<u8>(&device, &config, consumer, shared, err_tx),
            other => Err(device_err(
                ErrorKind::DeviceConfig,
                format!("设备采样格式 {other} 暂不支持"),
            )),
        }?;

        stream
            .play()
            .map_err(|e| device_err(ErrorKind::Stream, format!("启动输出流失败：{e}")))?;

        let out_config = OutputConfig {
            sample_rate: config.sample_rate,
            layout: request.layout,
            sample_format: format!("{sample_format}"),
            device_name,
        };
        self.stream = Some(stream);
        self.errors = Some(err_rx);
        self.config = Some(out_config.clone());
        Ok(out_config)
    }

    fn close(&mut self) {
        self.stream = None;
        self.errors = None;
        self.config = None;
    }

    fn config(&self) -> Option<&OutputConfig> {
        self.config.as_ref()
    }
}
