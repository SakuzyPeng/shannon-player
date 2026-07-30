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
//! 单声道上混后再重采样等于把同一条声道算两遍。多声道整体交给平台原生后端，
//! 不进入本转换器；若平台路径需要重采样，由该后端按系统要求决定管线位置。

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
    /// 本轮（构造或 reset 之后）实际收到的源帧数。flush 用它算出**精确**输出长度，
    /// 不能把 FFT 固定块为对齐而补的零也当成曲目内容。
    input_frames: u64,
    /// 已经交给下游的输出帧数。与 `input_frames` 配对，保证最终长度严格等于
    /// `ceil(input * dst / src)`。
    output_frames: u64,
    /// FFT 重采样器开头的群延迟。rubato 的分块接口不会替调用方裁掉，必须在第一批
    /// 输出里显式跳过，否则每次切歌都会多一段静音。
    trim_remaining: usize,
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
        let resampler = Fft::<f32>::new(
            src_rate as usize,
            dst_rate as usize,
            CHUNK_FRAMES,
            channels,
            FixedSync::Input,
        )
        .map_err(|e| {
            EngineError::new(
                Stage::Output,
                ErrorKind::DeviceConfig,
                format!("建不出 {src_rate} → {dst_rate} Hz 的重采样器：{e}"),
            )
        })?;
        let scratch = vec![0.0; resampler.output_frames_max() * channels];
        let trim_remaining = resampler.output_delay();
        Ok(Resampling::Active(Box::new(Active {
            resampler,
            channels,
            src_rate,
            dst_rate,
            input_frames: 0,
            output_frames: 0,
            trim_remaining,
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
            a.input_frames = 0;
            a.output_frames = 0;
            a.trim_remaining = a.resampler.output_delay();
        }
    }
}

impl Active {
    fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        debug_assert_eq!(input.len() % self.channels, 0, "输入必须按声道数对齐");
        self.input_frames = self
            .input_frames
            .saturating_add((input.len() / self.channels) as u64);
        self.pending.extend_from_slice(input);
        self.drain(out, false, None);
    }

    /// 把 `pending` 里够一整块的部分送进重采样器。
    ///
    /// `finish` 为真时，最后不足一块的残余也用 `partial_len` 补零送出。
    fn drain(&mut self, out: &mut Vec<f32>, finish: bool, target_frames: Option<u64>) {
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
            let input =
                InterleavedSlice::new(&self.pending[..need * self.channels], self.channels, need)
                    .expect("输入缓冲已按块大小对齐");
            let mut output = InterleavedSlice::new_mut(&mut self.scratch, self.channels, out_cap)
                .expect("输出缓冲按 output_frames_max 预留");

            let indexing = Indexing {
                partial_len: partial,
                ..Indexing::new()
            };
            let (_read, written) = self
                .resampler
                .process_into_buffer(&input, &mut output, Some(&indexing))
                .expect("块大小与缓冲容量均由 resampler 自己报出，不应失配");

            self.append_scratch(out, written, target_frames);
            self.pending.drain(..need * self.channels);

            if partial.is_some() {
                return;
            }
        }
    }

    fn flush(&mut self, out: &mut Vec<f32>) {
        let target_frames = self.expected_output_frames();
        self.drain(out, true, Some(target_frames));

        // partial 块之后仍可能没把滤波器延迟全部推出。继续喂「有效长度为 0」的块，
        // 但只接到目标帧数为止；rubato 自带的 process_all_into_buffer 也是这套语义。
        while self.output_frames < target_frames {
            let need = self.resampler.input_frames_next();
            self.pending.clear();
            self.pending.resize(need * self.channels, 0.0);
            let out_cap = self.scratch.len() / self.channels;
            let input =
                InterleavedSlice::new(&self.pending, self.channels, need).expect("静音块尺寸正确");
            let mut output = InterleavedSlice::new_mut(&mut self.scratch, self.channels, out_cap)
                .expect("输出缓冲按 output_frames_max 预留");
            let indexing = Indexing {
                partial_len: Some(0),
                ..Indexing::new()
            };
            let Ok((_read, written)) =
                self.resampler
                    .process_into_buffer(&input, &mut output, Some(&indexing))
            else {
                break;
            };
            if written == 0 {
                break;
            }
            self.append_scratch(out, written, Some(target_frames));
        }
        self.pending.clear();
    }

    /// 追加一次 rubato 输出：先裁开头的群延迟，流末尾再按精确目标长度截断块填充。
    fn append_scratch(&mut self, out: &mut Vec<f32>, written: usize, target_frames: Option<u64>) {
        let trim = self.trim_remaining.min(written);
        self.trim_remaining -= trim;
        let available = written - trim;
        let remaining = target_frames
            .map(|target| target.saturating_sub(self.output_frames))
            .unwrap_or(u64::MAX);
        let take = available.min(usize::try_from(remaining).unwrap_or(usize::MAX));
        let start = trim * self.channels;
        let end = start + take * self.channels;
        out.extend_from_slice(&self.scratch[start..end]);
        self.output_frames = self.output_frames.saturating_add(take as u64);
    }

    fn expected_output_frames(&self) -> u64 {
        let numerator = u128::from(self.input_frames) * u128::from(self.dst_rate);
        let denominator = u128::from(self.src_rate);
        let frames = numerator.div_ceil(denominator);
        u64::try_from(frames).unwrap_or(u64::MAX)
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
        // 固定 FFT 块会为不足一块的输入补零，滤波器本身还有群延迟；两者都不能
        // 泄漏到曲目时长里。短到 1 帧与刚好跨块的长度最容易暴露这个问题。
        for frames in [1usize, 68, 1_024, 44_100, 44_101] {
            let mut r = Resampling::new(44_100, 48_000, 2).unwrap();
            assert!(r.is_active());
            let input = vec![0.0; frames * 2];
            let mut out = Vec::new();
            r.process(&input, &mut out);
            r.flush(&mut out);
            let got = out.len() / 2;
            let expected = (frames as u128 * 48_000).div_ceil(44_100) as usize;
            assert_eq!(
                got, expected,
                "{frames} 帧的 44.1k 输入必须精确映射到目标域，不能带块填充或群延迟"
            );
        }
    }

    #[test]
    fn startup_delay_is_trimmed_from_the_signal() {
        let mut r = Resampling::new(44_100, 48_000, 1).unwrap();
        let mut input = vec![0.0; 4_096];
        input[0] = 1.0;
        let mut out = Vec::new();
        r.process(&input, &mut out);
        r.flush(&mut out);

        let peak = out
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))
            .map(|(index, _)| index)
            .unwrap();
        assert!(
            peak <= 1,
            "输入首帧的脉冲不应被 FFT 群延迟推到曲目中段，实际峰值在第 {peak} 帧"
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

        // 群延迟已由转换器裁掉；仍取中间一整段，避开有限长度信号两端的滤波过渡。
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
        assert_eq!(
            pick_output_rate(44_100, &[44_100, 48_000, 96_000]),
            Some(44_100)
        );
        // 没有精确匹配时，整数倍优于任意更高值：44.1 → 88.2 的滤波器比 → 48 简单得多。
        assert_eq!(
            pick_output_rate(44_100, &[48_000, 88_200, 96_000]),
            Some(88_200)
        );
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
