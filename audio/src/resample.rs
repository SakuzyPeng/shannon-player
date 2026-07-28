//! 采样率转换。
//!
//! **只在源采样率与设备采样率不一致时启用**，并在 stats 里如实标记——
//! 「bit-perfect」一类的措辞必须有据可依，链路里悄悄插了一级转换却仍宣称原样输出，
//! 是这类播放器最常见的失实描述。
//!
//! 用 rubato 的 `Fft`（同步 FFT 重采样）而不是异步 sinc：播放期的比率是**固定**的
//! （44 100 → 48 000 就一直是这个比），同步重采样器为固定比率而设计，同等质量下开销更低；
//! 异步那套的可变比率能力在这里用不上，只是白付插值代价。变速播放要是哪天要做，
//! 才需要换成可调比率的实现。
//!
//! ## 为什么放在声道适配之前
//!
//! 管线顺序是「解码 → 重采样 → 声道适配」：重采样按**源**声道数做。
//! 单声道上混后再重采样等于把同一条声道算两遍。将来做多声道下混时顺序要反过来
//! （先把 12 路下混成 2 路再重采样，比重采样 12 路便宜得多）——通则是
//! **在声道数少的那一侧做重采样**，而不是死记某个固定顺序。

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler};

use crate::error::{EngineError, ErrorKind, Result, Stage};

/// 每次送进重采样器的帧数。1024 帧在 48 kHz 下约 21 ms，
/// 足够摊薄 FFT 的固定开销，又不至于让 seek 后的首帧等太久。
const CHUNK_FRAMES: usize = 1024;

/// 采样率转换器。比率为 1:1 时是直通，不构造任何重采样器。
pub enum Resampling {
    /// 源与设备采样率一致，原样搬运。
    Passthrough,
    Active(Box<Active>),
}

pub struct Active {
    resampler: Fft<f32>,
    channels: usize,
    src_rate: u32,
    dst_rate: u32,
    /// 攒够一整块才能送进重采样器，不足的留到下次。
    pending: Vec<f32>,
    /// 复用的输出缓冲，避免每块都分配。
    scratch: Vec<f32>,
}

impl Resampling {
    /// 按源与目标采样率建转换器。
    pub fn new(src_rate: u32, dst_rate: u32, channels: usize) -> Result<Self> {
        if src_rate == dst_rate {
            return Ok(Resampling::Passthrough);
        }
        let resampler =
            Fft::<f32>::new(src_rate as usize, dst_rate as usize, CHUNK_FRAMES, channels, FixedSync::Input)
                .map_err(|e| {
                    EngineError::new(
                        Stage::Output,
                        ErrorKind::DeviceConfig,
                        format!("建不出 {src_rate} → {dst_rate} Hz 的重采样器：{e}"),
                    )
                })?;
        let scratch = vec![0.0; resampler.output_frames_max() * channels];
        Ok(Resampling::Active(Box::new(Active {
            resampler,
            channels,
            src_rate,
            dst_rate,
            pending: Vec::with_capacity(CHUNK_FRAMES * 2 * channels),
            scratch,
        })))
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Resampling::Active(_))
    }

    /// 目标采样率下的等效帧位置。seek 后重设播放位置要用它——
    /// 位置计数器记的是**输出**帧，拿源帧去填会让进度条按比率走偏
    /// （44.1 → 48 kHz 时快 8.8%）。
    pub fn src_frames_to_out(&self, frames: u64) -> u64 {
        match self {
            Resampling::Passthrough => frames,
            Resampling::Active(a) => {
                (frames as u128 * a.dst_rate as u128 / a.src_rate as u128) as u64
            }
        }
    }

    /// 处理一批交错样本，结果追加到 `out`。
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        match self {
            Resampling::Passthrough => out.extend_from_slice(input),
            Resampling::Active(a) => a.process(input, out),
        }
    }

    /// 流末尾：把不足一块的残余与重采样器内部延迟一并冲刷出来。
    ///
    /// 不冲刷会丢掉结尾几十毫秒。单曲听不太出来，但 gapless 拼接时
    /// 正好丢在两首歌的接缝上，那是最容易被听见的位置。
    pub fn flush(&mut self, out: &mut Vec<f32>) {
        if let Resampling::Active(a) = self {
            a.flush(out);
        }
    }

    /// seek 与切歌后复位：重采样器内部持有跨块的历史样本，
    /// 不复位会把定位前的尾巴混进定位后的开头。
    pub fn reset(&mut self) {
        if let Resampling::Active(a) = self {
            a.resampler.reset();
            a.pending.clear();
        }
    }
}

impl Active {
    fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        self.pending.extend_from_slice(input);
        self.drain(out, false);
    }

    /// 把 `pending` 里够一整块的部分送进重采样器。
    ///
    /// `finish` 为真时，最后不足一块的残余也用 `partial_len` 补零送出。
    fn drain(&mut self, out: &mut Vec<f32>, finish: bool) {
        loop {
            let need = self.resampler.input_frames_next();
            let have = self.pending.len() / self.channels;
            let partial = if have >= need {
                None
            } else if finish && have > 0 {
                Some(have)
            } else {
                return;
            };

            // 不足一块时补零填满：`partial_len` 告诉重采样器只有前 n 帧是真数据。
            if partial.is_some() {
                self.pending.resize(need * self.channels, 0.0);
            }

            let out_cap = self.scratch.len() / self.channels;
            let input = InterleavedSlice::new(&self.pending[..need * self.channels], self.channels, need)
                .expect("输入缓冲已按块大小对齐");
            let mut output = InterleavedSlice::new_mut(&mut self.scratch, self.channels, out_cap)
                .expect("输出缓冲按 output_frames_max 预留");

            let indexing = Indexing { partial_len: partial, ..Indexing::new() };
            let (_read, written) = self
                .resampler
                .process_into_buffer(&input, &mut output, Some(&indexing))
                .expect("块大小与缓冲容量均由 resampler 自己报出，不应失配");

            out.extend_from_slice(&self.scratch[..written * self.channels]);
            self.pending.drain(..need * self.channels);

            if partial.is_some() {
                return;
            }
        }
    }

    fn flush(&mut self, out: &mut Vec<f32>) {
        self.drain(out, true);

        // 再喂一块静音，把滤波器里还压着的延迟推出来。
        let delay = self.resampler.output_delay();
        if delay == 0 {
            return;
        }
        let need = self.resampler.input_frames_next();
        self.pending.clear();
        self.pending.resize(need * self.channels, 0.0);
        let out_cap = self.scratch.len() / self.channels;
        let input = InterleavedSlice::new(&self.pending, self.channels, need).expect("静音块尺寸正确");
        let mut output = InterleavedSlice::new_mut(&mut self.scratch, self.channels, out_cap)
            .expect("输出缓冲按 output_frames_max 预留");
        let indexing = Indexing { partial_len: Some(0), ..Indexing::new() };
        if let Ok((_r, written)) = self.resampler.process_into_buffer(&input, &mut output, Some(&indexing)) {
            // 只取延迟那么多帧，多出来的是补零本身产生的静音尾巴。
            let take = written.min(delay);
            out.extend_from_slice(&self.scratch[..take * self.channels]);
        }
        self.pending.clear();
    }
}

/// 在设备支持的采样率里挑一个。
///
/// 顺序即质量偏好：
/// 1. **与源一致**——不转换永远是最好的；
/// 2. **源的整数倍**（44.1 → 88.2 优于 44.1 → 48）——整数比的重采样滤波器更简单，
///    相位与混叠表现都更好；
/// 3. 高于源采样率的最小者——升采样不丢高频；
/// 4. 都低于源采样率时取最大者——只能下采样，那就尽量少丢。
pub fn pick_output_rate(src_rate: u32, supported: &[u32]) -> Option<u32> {
    if supported.is_empty() {
        return None;
    }
    if supported.contains(&src_rate) {
        return Some(src_rate);
    }
    let multiple = supported
        .iter()
        .copied()
        .filter(|r| *r > src_rate && r % src_rate == 0)
        .min();
    if multiple.is_some() {
        return multiple;
    }
    supported
        .iter()
        .copied()
        .filter(|r| *r > src_rate)
        .min()
        .or_else(|| supported.iter().copied().max())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_rate_is_passthrough() {
        let mut r = Resampling::new(44_100, 44_100, 2).unwrap();
        assert!(!r.is_active());
        let mut out = Vec::new();
        r.process(&[0.1, 0.2, 0.3, 0.4], &mut out);
        assert_eq!(out, vec![0.1, 0.2, 0.3, 0.4], "直通不该改动任何样本");
    }

    #[test]
    fn output_length_follows_ratio() {
        let mut r = Resampling::new(44_100, 48_000, 2).unwrap();
        assert!(r.is_active());
        let frames = 44_100; // 一秒
        let input = vec![0.0; frames * 2];
        let mut out = Vec::new();
        r.process(&input, &mut out);
        r.flush(&mut out);
        let got = out.len() / 2;
        // 一秒的输入应当产出约一秒的输出（容差留给块对齐与滤波器延迟）。
        assert!(
            (got as i64 - 48_000).abs() < 2_000,
            "44.1k 的一秒重采样到 48k 应约得 48000 帧，实际 {got}"
        );
    }

    #[test]
    fn resampling_preserves_a_sine_wave() {
        // 重采样后波形应仍是同一个频率的正弦：这条能挡住声道错位、
        // 块边界丢样本、比率算反之类会立刻毁掉音质的错误。
        let (src, dst, freq) = (44_100u32, 48_000u32, 1_000.0f64);
        let mut r = Resampling::new(src, dst, 1).unwrap();
        let input: Vec<f32> = (0..src as usize)
            .map(|i| (2.0 * std::f64::consts::PI * freq * i as f64 / src as f64).sin() as f32)
            .collect();
        let mut out = Vec::new();
        r.process(&input, &mut out);

        // 跳过起始的滤波器延迟段，取中间一整段比对。
        let start = 4_000;
        let len = 8_000;
        assert!(out.len() > start + len, "输出长度不足以比对");
        let err = sine_rms_error(&out[start..start + len], dst, freq);
        assert!(err < 0.02, "重采样后的波形与理论正弦偏差过大：{err}");
    }

    /// 与理论正弦的最小均方根偏差。相位要搜索——重采样滤波器有群延迟，
    /// 直接按下标对齐一定对不上。
    fn sine_rms_error(samples: &[f32], rate: u32, freq: f64) -> f64 {
        let mut best = f64::MAX;
        for shift in 0..200 {
            let phase = shift as f64 / 200.0 * 2.0 * std::f64::consts::PI;
            let err: f64 = samples
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let want =
                        (2.0 * std::f64::consts::PI * freq * i as f64 / rate as f64 + phase).sin();
                    (*s as f64 - want).powi(2)
                })
                .sum::<f64>()
                / samples.len() as f64;
            best = best.min(err);
        }
        best.sqrt()
    }

    #[test]
    fn prefers_exact_rate_then_integer_multiple() {
        // 精确匹配优先——不转换永远最好。
        assert_eq!(pick_output_rate(44_100, &[44_100, 48_000, 96_000]), Some(44_100));
        // 没有精确匹配时，整数倍优于任意更高值：44.1 → 88.2 的滤波器比 → 48 简单得多。
        assert_eq!(pick_output_rate(44_100, &[48_000, 88_200, 96_000]), Some(88_200));
        // 没有整数倍就取高于源的最小者，不丢高频。
        assert_eq!(pick_output_rate(44_100, &[48_000, 96_000]), Some(48_000));
        // 全都低于源采样率时只能下采样，取最大的少丢一点。
        assert_eq!(pick_output_rate(96_000, &[24_000, 48_000]), Some(48_000));
        assert_eq!(pick_output_rate(44_100, &[]), None);
    }

    #[test]
    fn reset_clears_carried_over_samples() {
        let mut r = Resampling::new(44_100, 48_000, 2).unwrap();
        let mut out = Vec::new();
        r.process(&vec![0.5; 4_096], &mut out);
        r.reset();
        // 复位后再送静音，不该混进复位前那段非零内容。
        let mut after = Vec::new();
        r.process(&vec![0.0; 44_100 * 2], &mut after);
        let peak = after.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak < 0.01, "复位后仍带出定位前的残留：峰值 {peak}");
    }
}
