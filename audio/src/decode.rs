//! Symphonia 解码管线：容器探测、解复用、解码、seek。
//!
//! 一条 Symphonia 管线打底，将来 Opus 等它未覆盖的编码以自定义 Decoder 注册进
//! `CodecRegistry`，复用其解复用器，不另起容器解析栈。
//!
//! 解码结果统一转成 **f32 交错 PCM**（不变量第 1 条），源规格自这里起就携带
//! [`ChannelLayout`] 与布局来源——事后补录代价大，PCM 一旦进了环形缓冲就再没有源头信息。

use std::fs::File;
use std::path::Path;

use symphonia::core::audio::GenericAudioBufferRef;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::codecs::CodecParameters;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase};

use crate::error::{EngineError, ErrorKind, Result, Stage};
use crate::layout::ChannelLayout;

/// 源的音频规格。
#[derive(Debug, Clone)]
pub struct SourceSpec {
    pub sample_rate: u32,
    pub layout: ChannelLayout,
    /// 总帧数；容器没给就为 `None`（不拿 0 当哨兵）。
    pub total_frames: Option<u64>,
    /// 容器与编码的**原始名**，不做归一化——归一化会丢信息，与曲库侧同一条戒律。
    pub container: String,
    pub codec: String,
}

impl SourceSpec {
    pub fn duration_sec(&self) -> Option<f64> {
        self.total_frames.map(|f| f as f64 / self.sample_rate as f64)
    }
}

/// 单个音源的解码器。
pub struct Decoder {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    spec: SourceSpec,
    /// 下一个待输出帧的位置，用于进度与 seek 后的重锚定。
    position_frames: u64,
}

fn open_err(kind: ErrorKind, stage: Stage, msg: impl Into<String>) -> EngineError {
    EngineError::new(stage, kind, msg)
}

impl Decoder {
    /// 打开文件并准备解码。
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|e| {
            open_err(ErrorKind::Io, Stage::Open, format!("打不开文件：{e}"))
        })?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        // 扩展名只是提示，探测仍以内容为准——曲库里改错扩展名的文件并不少见。
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let reader = symphonia::default::get_probe()
            .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
            .map_err(|e| {
                open_err(ErrorKind::Unsupported, Stage::Probe, format!("识别不出容器格式：{e}"))
            })?;

        let container = reader.format_info().short_name.to_string();

        let track = reader.default_track(TrackType::Audio).ok_or_else(|| {
            open_err(ErrorKind::Unsupported, Stage::Probe, "文件里没有音频轨")
                .with_format(Some(container.clone()), None)
        })?;
        let track_id = track.id;
        let time_base = track.time_base;
        let total_frames = track.num_frames;

        let params = match &track.codec_params {
            Some(CodecParameters::Audio(p)) => p.clone(),
            _ => {
                return Err(open_err(
                    ErrorKind::Unsupported,
                    Stage::Probe,
                    "音频轨缺少解码参数，无法播放",
                )
                .with_format(Some(container), None))
            }
        };

        // gapless 在这里只做「裁掉编码器的前导延迟与尾部填充」，
        // 让单曲开头不出现莫名的静音。真正的**曲目边界帧级续播**是阶段 1 的事。
        let opts = AudioDecoderOptions::default().gapless(true);
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(&params, &opts)
            .map_err(|e| {
                open_err(ErrorKind::Unsupported, Stage::Decode, format!("没有可用的解码器：{e}"))
                    .with_format(Some(container.clone()), None)
            })?;

        let codec = decoder.codec_info().short_name.to_string();

        let sample_rate = params.sample_rate.ok_or_else(|| {
            open_err(ErrorKind::Unsupported, Stage::Decode, "码流未给出采样率")
                .with_format(Some(container.clone()), Some(codec.clone()))
        })?;
        let layout = params
            .channels
            .as_ref()
            .map(ChannelLayout::from_symphonia)
            .ok_or_else(|| {
                open_err(ErrorKind::Unsupported, Stage::Decode, "码流未给出声道信息")
                    .with_format(Some(container.clone()), Some(codec.clone()))
            })?;

        let spec = SourceSpec {
            sample_rate,
            layout,
            total_frames,
            container,
            codec,
        };

        Ok(Self { reader, decoder, track_id, time_base, spec, position_frames: 0 })
    }

    pub fn spec(&self) -> &SourceSpec {
        &self.spec
    }

    pub fn position_frames(&self) -> u64 {
        self.position_frames
    }

    /// 解码下一批帧，追加式写入 `out`（交错 f32）。
    ///
    /// 返回 `false` 表示到达流末尾。单个包解码失败**不终止播放**：丢弃该包继续，
    /// 因为一段损坏不该让整首歌播不了；无法继续的错误才向上报。
    pub fn next_frames(&mut self, out: &mut Vec<f32>) -> Result<bool> {
        loop {
            let packet = match self.reader.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => return Ok(false),
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    // 有些容器不给出干净的结束标记，读到 EOF 即为正常播完。
                    return Ok(false);
                }
                Err(e) => return Err(self.decode_err(format!("读取数据包失败：{e}"))),
            };

            if packet.track_id != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(buf) => {
                    let frames = buf.frames();
                    if frames == 0 {
                        continue;
                    }
                    append_interleaved(&buf, out);
                    self.position_frames += frames as u64;
                    return Ok(true);
                }
                // 可恢复错误：丢掉这个包继续往下解。
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(SymphoniaError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(e) => return Err(self.decode_err(format!("解码失败：{e}"))),
            }
        }
    }

    /// seek 到指定秒位，返回实际到达的帧位置。
    ///
    /// 用 `SeekMode::Accurate`：粗略 seek 可能落到请求位置之后，
    /// 表现为「拖到 1:00 却从 1:03 开始」，是用户能直接察觉的偏差。
    pub fn seek(&mut self, seconds: f64) -> Result<u64> {
        let seconds = seconds.max(0.0);
        let time = Time::try_from_secs_f64(seconds)
            .ok_or_else(|| self.decode_err(format!("定位时间 {seconds} 秒超出可表示范围")))?;
        let to = SeekTo::Time { time, track_id: Some(self.track_id) };
        let seeked = self
            .reader
            .seek(SeekMode::Accurate, to)
            .map_err(|e| self.decode_err(format!("定位失败：{e}")))?;

        // 定位后解码器状态与新位置不连续，必须复位，否则会解出杂音。
        self.decoder.reset();

        self.position_frames = self.ts_to_frames(seeked.actual_ts).unwrap_or_else(|| {
            // 时基缺失时只能以请求位置为准；比谎报一个精确值诚实。
            (seconds * self.spec.sample_rate as f64).max(0.0) as u64
        });
        Ok(self.position_frames)
    }

    /// 把容器时间戳换算成帧位置。
    ///
    /// **必须走整数**：先转成秒再乘采样率会因浮点截断差一帧——实测 WAV 定位到
    /// 第 43776 帧时，`as_secs_f64()` 得到 0.9926530612...，乘回 44100 是
    /// 43775.999999，截断即 43775。差一帧在进度显示上看不出来，却会让
    /// 「seek 后的解码输出等于从头解码的对应后缀」这条不变量失守，
    /// 也就掐断了后续 gapless 与 A/B 比对的立足点。
    fn ts_to_frames(&self, ts: symphonia::core::units::Timestamp) -> Option<u64> {
        let tb = self.time_base?;
        let ticks = ts.get();
        if ticks < 0 {
            return Some(0);
        }
        let frames = ticks as i128 * i128::from(tb.numer.get()) * i128::from(self.spec.sample_rate)
            / i128::from(tb.denom.get());
        u64::try_from(frames).ok()
    }

    fn decode_err(&self, msg: String) -> EngineError {
        EngineError::new(Stage::Decode, ErrorKind::Decode, msg)
            .with_format(Some(self.spec.container.clone()), Some(self.spec.codec.clone()))
    }
}

/// 把任意采样格式的解码缓冲转成 f32 交错样本追加到 `out`。
fn append_interleaved(buf: &GenericAudioBufferRef<'_>, out: &mut Vec<f32>) {
    // Symphonia 提供了到 Vec 的交错拷贝，但它会覆盖而非追加，
    // 因此先用临时游标记录长度，再把新数据接在后面。
    let start = out.len();
    let needed = buf.samples_interleaved();
    out.resize(start + needed, 0.0);
    buf.copy_to_slice_interleaved(&mut out[start..]);
}
