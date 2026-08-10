//! Symphonia 解码管线：容器探测、解复用、解码、seek。
//!
//! 一条 Symphonia 管线打底，它未覆盖的编码（当前是 Opus）以自定义 Decoder 注册进
//! `CodecRegistry`，复用其解复用器，不另起容器解析栈。
//!
//! 解码结果统一转成 **f32 交错 PCM**（不变量第 1 条），源规格自这里起就携带
//! [`ChannelLayout`] 与布局来源——事后补录代价大，PCM 一旦进了环形缓冲就再没有源头信息。

use std::fs::File;
use std::path::Path;
use std::sync::OnceLock;

use symphonia::core::audio::GenericAudioBufferRef;
use symphonia::core::codecs::audio::{AudioCodecParameters, AudioDecoder, AudioDecoderOptions};
use symphonia::core::codecs::registry::CodecRegistry;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase};

use crate::error::{EngineError, ErrorKind, Result, Stage};
use crate::layout::ChannelLayout;

/// 本引擎的解码器注册表：Symphonia 内建的那一套，外加它没覆盖的编码。
///
/// 不用 `symphonia::default::get_codecs()`，因为那份表**只含 Symphonia 自己的解码器**，
/// 拿它查 Opus 会得到「没有可用的解码器」。自建一份是架构约束点名的集成方式：
/// 容器探测、解复用、元数据、seek 仍统一走 Symphonia，缺的只补编码这一层。
fn codecs() -> &'static CodecRegistry {
    static REGISTRY: OnceLock<CodecRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut registry);
        // libopus 绑定。它只接受 1~2 声道，多声道 Opus 会在这里得到能力错误——
        // 与「多声道整体划归平台原生后端」是同一条边界，不是缺陷。
        registry.register_audio_decoder::<symphonia_adapter_libopus::OpusDecoder>();
        registry
    })
}

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
        self.total_frames
            .map(|f| f as f64 / self.sample_rate as f64)
    }
}

/// 单个音源的解码器。
pub struct Decoder {
    /// 源文件路径。仅 [`Decoder::seek`] 的重开路径需要，见 [`reader_seek_is_trustworthy`]。
    path: std::path::PathBuf,
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    spec: SourceSpec,
    /// 下一个待输出帧的位置，用于进度与 seek 后的重锚定。
    position_frames: u64,
    /// 本次打开后是否至少产出过一帧。与坏包标志配对，区分「合法空流/定位到末尾」
    /// 和「每个数据包都坏了却被循环跳过」。
    decoded_any_audio: bool,
    saw_decode_error: bool,
    /// seek 后需要预热的帧数；0 表示这个编码不需要（见 [`preroll_frames_for`]）。
    preroll_frames: u64,
    /// 能不能信任 `FormatReader::seek`，见 [`reader_seek_is_trustworthy`]。
    trust_reader_seek: bool,
    /// OpusHead 声明的前导裁剪，换算到当前输出采样率后的帧数。
    ///
    /// `symphonia-adapter-libopus` 把它藏成一次性内部状态，而 `reset()` 既不会恢复也不会
    /// 清掉它：解过一包后 seek 回 0 会漏裁，首次解码前 seek 到中间又会误裁。因此打开时
    /// 把所有权收回来，由本层按「实际是否落在流开头」明确重新装填。
    initial_skip_frames: u64,
    pending_skip_frames: u64,
}

/// 从 OpusHead 中取走 pre-skip，让播放器而不是适配器持有这枚 seek 相关状态。
///
/// OpusHead 的第 10~11 字节是以 48 kHz 计的 little-endian pre-skip。适配器也是按
/// `pre_skip * sample_rate / 48_000` 换算；这里复用同一口径，同时把原字段清零，避免
/// 它在 `AudioDecoder::reset()` 前后表现成两套语义。
fn detach_opus_pre_skip(params: &mut AudioCodecParameters) -> u64 {
    use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;

    if params.codec != CODEC_ID_OPUS {
        return 0;
    }
    let Some(extra) = params.extra_data.as_deref_mut() else {
        return 0;
    };
    if extra.len() < 12 {
        return 0;
    }
    let pre_skip = u16::from_le_bytes([extra[10], extra[11]]) as u64;
    extra[10..12].fill(0);
    pre_skip * u64::from(params.sample_rate.unwrap_or(48_000)) / 48_000
}

/// 这个「容器 + 编码」组合的 `FormatReader::seek` 能不能信。
///
/// 绝大多数组合能信。**Opus 装在 Matroska / WebM 里不能**：实测在 2 秒与 10 秒语料上、
/// 四个不同定位点、`.webm` 与 `.mka` 两种扩展名下结果一致——报告的落点像模像样，
/// 解出来的音频却与整曲任何一段都对不上（全局搜索下最小相对误差 1.6~3.9，而不相关
/// 信号约为 1.41），且回退定位是原地不动（无 cue 时上游只能向前扫）。同一容器里的
/// FLAC 定位只差 4 帧，所以这不是 Matroska 整体不行，是这一组合的上游缺陷。
///
/// 这类文件正是 yt-dlp 下载 YouTube 音频的默认产物，会真的出现在用户曲库里。
/// **线性播放是好的**（与 Ogg 版逐样本一致），所以不能因此拒播；坏的只有定位，
/// 于是定位改走「重开文件 + 向前解码」这条已验证正确的路。
fn reader_seek_is_trustworthy(container: &str, codec: &str) -> bool {
    !(codec == "opus" && matches!(container, "matroska" | "webm"))
}

/// seek 之后需要丢弃多少帧来把解码器状态喂热。
///
/// Opus 的解码器状态跨包延续，定位落点处那份状态是空的，直接从落点起解会有一段明显
/// 偏差——实测相对误差自信号 RMS 的 78% 起逐块衰减，约 80 ms 才降到 2% 以下。
/// RFC 7845 §4.2 因此要求至少 3840 个 48 kHz 采样（80 ms）的 pre-roll。
///
/// 其余编码返回 0：MP3 实测 20~30 ms 内即收敛（Symphonia 自己处理了比特池与交叠），
/// 无损编码逐包独立，都不需要这道手续。**不要「顺手给所有编码都加上」**——
/// 那会让每次 seek 都多解一段没人要的音频，换不来任何听得出的东西。
fn preroll_frames_for(
    codec: symphonia::core::codecs::audio::AudioCodecId,
    sample_rate: u32,
) -> u64 {
    if codec == symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS {
        (sample_rate as u64 * 80).div_ceil(1000)
    } else {
        0
    }
}

fn open_err(kind: ErrorKind, stage: Stage, msg: impl Into<String>) -> EngineError {
    EngineError::new(stage, kind, msg)
}

impl Decoder {
    /// 打开文件并准备解码。
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)
            .map_err(|e| open_err(ErrorKind::Io, Stage::Open, format!("打不开文件：{e}")))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        // 扩展名只是提示，探测仍以内容为准——曲库里改错扩展名的文件并不少见。
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let reader = symphonia::default::get_probe()
            .probe(
                &hint,
                mss,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| {
                open_err(
                    ErrorKind::Unsupported,
                    Stage::Probe,
                    format!("识别不出容器格式：{e}"),
                )
            })?;

        let container = reader.format_info().short_name.to_string();

        let track = reader.default_track(TrackType::Audio).ok_or_else(|| {
            open_err(ErrorKind::Unsupported, Stage::Probe, "文件里没有音频轨")
                .with_format(Some(container.clone()), None)
        })?;
        let track_id = track.id;
        let time_base = track.time_base;
        let total_frames = track.num_frames;

        let mut params = match &track.codec_params {
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

        // 适配器自己的 pre-skip 在 reset 后不能给出稳定语义，取出来由本层管理。
        let initial_skip_frames = detach_opus_pre_skip(&mut params);

        // gapless 在这里只做「裁掉编码器的前导延迟与尾部填充」，
        // 让单曲开头不出现莫名的静音。真正的**曲目边界帧级续播**是阶段 1 的事。
        let opts = AudioDecoderOptions::default().gapless(true);
        let decoder = codecs().make_audio_decoder(&params, &opts).map_err(|e| {
            open_err(
                ErrorKind::Unsupported,
                Stage::Decode,
                format!("没有可用的解码器：{e}"),
            )
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

        Ok(Self {
            path: path.to_path_buf(),
            reader,
            decoder,
            track_id,
            time_base,
            preroll_frames: preroll_frames_for(params.codec, spec.sample_rate),
            trust_reader_seek: reader_seek_is_trustworthy(&spec.container, &spec.codec),
            spec,
            position_frames: 0,
            decoded_any_audio: false,
            saw_decode_error: false,
            initial_skip_frames,
            pending_skip_frames: initial_skip_frames,
        })
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
                Ok(None) => return self.end_of_stream(),
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    // 有些容器不给出干净的结束标记，读到 EOF 即为正常播完。
                    return self.end_of_stream();
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
                    let skipped = self.pending_skip_frames.min(frames as u64) as usize;
                    self.pending_skip_frames -= skipped as u64;
                    if skipped == frames {
                        continue;
                    }
                    append_interleaved(&buf, out, skipped);
                    self.position_frames += (frames - skipped) as u64;
                    self.decoded_any_audio = true;
                    return Ok(true);
                }
                // 可恢复错误：丢掉这个包继续往下解。
                Err(SymphoniaError::DecodeError(_)) => {
                    self.saw_decode_error = true;
                    continue;
                }
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
    /// 需要 pre-roll 的编码（当前是 Opus）多走一趟：先定位一次拿到落点，再退回落点之前
    /// 一段重新起解、把中间的帧解出来丢掉，只为把解码器状态喂热。**落点仍以第一次定位
    /// 为准**，所以位置语义与其它编码完全一致，变的只是那段音频从错的变成对的。
    pub fn seek(&mut self, seconds: f64) -> Result<u64> {
        let seconds = seconds.max(0.0);
        if !self.trust_reader_seek {
            return self.seek_by_decoding(seconds);
        }

        let anchor = self.seek_reader(seconds)?;
        if self.preroll_frames == 0 || anchor == 0 {
            return Ok(anchor);
        }

        // 这一次定位只求「够早」，精确落点由随后的丢弃循环走到 anchor，
        // 所以这里用浮点秒不违反「位置换算一律走整数」——它算的不是位置，是起跑线。
        let warm = anchor.saturating_sub(self.preroll_frames);
        self.seek_reader(warm as f64 / self.spec.sample_rate as f64)?;
        self.discard_until(anchor)?;
        Ok(self.position_frames)
    }

    /// 重开文件、从头向前解码到目标位置，供 `FormatReader::seek` 不可信的组合使用。
    ///
    /// 代价是与目标位置成正比的一次解码，换来的是**落点与音频一定对得上**。
    /// 不复用已打开的 reader 从当前位置往前赶：一来目标可能在身后，二来那个 reader
    /// 的定位正是不可信的那一个，绕开它才是这条路径的意义。
    fn seek_by_decoding(&mut self, seconds: f64) -> Result<u64> {
        let target = (seconds * self.spec.sample_rate as f64).max(0.0) as u64;
        let path = std::mem::take(&mut self.path);
        *self = Self::open(&path)?;
        if target > 0 {
            self.discard_until(target)?;
        }
        Ok(self.position_frames)
    }

    /// 一直解码并丢弃，直到到达 `target` 帧或流结束。
    fn discard_until(&mut self, target: u64) -> Result<()> {
        let mut discard = Vec::new();
        while self.position_frames < target {
            discard.clear();
            // 途中撞上流末尾就停：定位越过末尾是合法请求（拖到进度条最右端），
            // 报错反而会把它变成一次播放失败。
            if !self.next_frames(&mut discard)? {
                break;
            }
        }
        Ok(())
    }

    /// 定位读取器并复位解码器，返回落点帧位置。
    ///
    /// 用 `SeekMode::Accurate`：粗略 seek 可能落到请求位置之后，
    /// 表现为「拖到 1:00 却从 1:03 开始」，是用户能直接察觉的偏差。
    fn seek_reader(&mut self, seconds: f64) -> Result<u64> {
        let time = Time::try_from_secs_f64(seconds)
            .ok_or_else(|| self.decode_err(format!("定位时间 {seconds} 秒超出可表示范围")))?;
        let to = SeekTo::Time {
            time,
            track_id: Some(self.track_id),
        };
        let seeked = self
            .reader
            .seek(SeekMode::Accurate, to)
            .map_err(|e| self.decode_err(format!("定位失败：{e}")))?;

        // 定位后解码器状态与新位置不连续，必须复位，否则会解出杂音。
        self.decoder.reset();

        self.position_frames = if let Some(raw_frames) = self.ts_to_frames(seeked.actual_ts) {
            // Ogg Opus 的容器时间戳仍含 OpusHead 的 pre-skip，而本层位置从裁剪后的第一帧
            // 算 0。两条时间轴必须在这里对齐；否则报告第 N 帧，实际出声的是 N-312。
            raw_frames.saturating_sub(self.initial_skip_frames)
        } else {
            // 时基缺失时只能以请求位置为准；它已经是展示域秒数，不能再减 pre-skip。
            (seconds * self.spec.sample_rate as f64).max(0.0) as u64
        };
        // 只有真的回到流开头才重新裁 OpusHead 的前导。中途的 pre-roll 不能套这份值，
        // 否则同一个 seek 会因「此前是否解过一包」而相差一整个 Opus 包。
        self.pending_skip_frames = if self.position_frames == 0 {
            self.initial_skip_frames
        } else {
            0
        };
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
        EngineError::new(Stage::Decode, ErrorKind::Decode, msg).with_format(
            Some(self.spec.container.clone()),
            Some(self.spec.codec.clone()),
        )
    }

    fn end_of_stream(&self) -> Result<bool> {
        if eof_is_decode_failure(self.decoded_any_audio, self.saw_decode_error) {
            return Err(self.decode_err("整条音频流没有解出任何有效帧".into()));
        }
        Ok(false)
    }
}

fn eof_is_decode_failure(decoded_any_audio: bool, saw_decode_error: bool) -> bool {
    saw_decode_error && !decoded_any_audio
}

/// 把任意采样格式的解码缓冲转成 f32 交错样本追加到 `out`，可裁掉开头若干帧。
fn append_interleaved(buf: &GenericAudioBufferRef<'_>, out: &mut Vec<f32>, skip_frames: usize) {
    // Symphonia 提供了到 Vec 的交错拷贝，但它会覆盖而非追加，
    // 因此先用临时游标记录长度，再把新数据接在后面。
    let start = out.len();
    let needed = buf.samples_interleaved();
    out.resize(start + needed, 0.0);
    buf.copy_to_slice_interleaved(&mut out[start..]);

    let skip_samples = skip_frames * buf.num_planes();
    if skip_samples > 0 {
        let end = start + needed;
        out.copy_within(start + skip_samples..end, start);
        out.truncate(end - skip_samples);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        detach_opus_pre_skip, eof_is_decode_failure, preroll_frames_for, reader_seek_is_trustworthy,
    };
    use symphonia::core::codecs::audio::well_known::{CODEC_ID_OPUS, CODEC_ID_VORBIS};
    use symphonia::core::codecs::audio::AudioCodecParameters;

    #[test]
    fn only_opus_in_matroska_needs_the_rewind_seek() {
        assert!(!reader_seek_is_trustworthy("matroska", "opus"));
        assert!(!reader_seek_is_trustworthy("webm", "opus"));
        // 同一个容器里的其它编码定位是好的，不能一并拖下水——那会让 mka 里的 FLAC
        // 白白付出一次全曲解码。
        assert!(reader_seek_is_trustworthy("matroska", "flac"));
        // 同一个编码在 Ogg 里也是好的：坏的是组合，不是 Opus 本身。
        assert!(reader_seek_is_trustworthy("ogg", "opus"));
    }

    #[test]
    fn only_opus_asks_for_a_preroll() {
        assert_eq!(preroll_frames_for(CODEC_ID_OPUS, 48_000), 3_840);
        assert_eq!(preroll_frames_for(CODEC_ID_VORBIS, 48_000), 0);
    }

    #[test]
    fn opus_pre_skip_is_detached_from_the_resettable_decoder() {
        let mut extra = vec![0; 19];
        extra[..8].copy_from_slice(b"OpusHead");
        extra[10..12].copy_from_slice(&312u16.to_le_bytes());
        let mut params = AudioCodecParameters::new();
        params
            .for_codec(CODEC_ID_OPUS)
            .with_sample_rate(48_000)
            .with_extra_data(extra.into_boxed_slice());

        assert_eq!(detach_opus_pre_skip(&mut params), 312);
        assert_eq!(&params.extra_data.unwrap()[10..12], &[0, 0]);
    }

    #[test]
    fn all_bad_packets_are_not_a_natural_end() {
        assert!(eof_is_decode_failure(false, true));
        assert!(!eof_is_decode_failure(true, true), "中途孤立坏包仍允许容错");
        assert!(
            !eof_is_decode_failure(false, false),
            "没有坏包的空流仍按自然结束处理"
        );
    }
}
