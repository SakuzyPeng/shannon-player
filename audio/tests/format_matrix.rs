//! 格式矩阵测试：每种启用的编码都要真的解得对、放得完。
//!
//! ## 语料从哪来
//!
//! 由 `cargo run -p shannon-audio --example make_corpus` 用 ffmpeg 生成到
//! `audio/tests/corpus/`（**不入库**，完全可复现）。语料不在时本文件的用例全部跳过并
//! 打印生成命令——跳过是有意的取舍：CI 未必装了 ffmpeg，而纯 Rust 语料能覆盖的
//! PCM 路径已由 `playback.rs` 保证。
//!
//! ## 为什么必须有这一层
//!
//! 引擎启用了一串解码器，但纯 Rust 现造得出的只有 PCM，于是自动化测试一度只覆盖 WAV
//! ——**承诺面比验证面大得多**，ALAC / AAC / FLAC / MP3 全靠手动试听的记忆撑着。
//! 编码器不必自己写，用 ffmpeg 从同一份源转出整个矩阵即可，「支持某格式」这句话
//! 才有回归保护。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use shannon_audio::decode::Decoder;
use shannon_audio::engine::{Engine, EngineEvent};
use shannon_audio::output::null::NullOutput;

const RATE: u32 = 44_100;

/// 定位比对用的位置与窗口。取 0.8 秒是为了让「跳过收敛段 + 比对窗口」整段落在
/// 2 秒语料之内，还留出余量。
const SEEK_AT: f64 = 0.8;
/// 定位后先丢掉这么多帧（约 100 ms，按最低采样率折算）再比对。
const SETTLE_FRAMES: usize = 4_410;
/// 比对窗口长度（约 100 ms）。
const COMPARE_FRAMES: usize = 4_410;

struct Case {
    name: &'static str,
    file: &'static str,
    lossless: bool,
    channels: u16,
    rate: u32,
    /// 定位落点与实际解出音频之间的**已知**偏差（帧）。0 表示报告的位置就是音频的位置。
    ///
    /// 非零项都是上游解复用器的既有偏差，逐条量过、写在各自的注释里。写成期望值而不是
    /// 放宽一个统一容差，是因为容差只能表达「差不多就行」，而这里要表达的是
    /// 「差多少、为什么、什么时候该变」——真出现回归时，容差会默默吃掉它。
    seek_offset: i64,
    /// 在 `seek_offset` 处允许的相对均方根误差上限（有损编码解不出逐样本一致）。
    seek_error: f64,
}

const CASES: &[Case] = &[
    Case {
        name: "flac",
        file: "stereo.flac",
        lossless: true,
        channels: 2,
        rate: RATE,
        seek_offset: 0,
        seek_error: 1e-6,
    },
    Case {
        name: "alac",
        file: "stereo_alac.m4a",
        lossless: true,
        channels: 2,
        rate: RATE,
        seek_offset: 0,
        seek_error: 1e-6,
    },
    Case {
        name: "aiff",
        file: "stereo.aiff",
        lossless: true,
        channels: 2,
        rate: RATE,
        seek_offset: 0,
        seek_error: 1e-6,
    },
    Case {
        name: "caf",
        file: "stereo.caf",
        lossless: true,
        channels: 2,
        rate: RATE,
        seek_offset: 0,
        seek_error: 1e-6,
    },
    Case {
        name: "flac-in-mka",
        file: "stereo_flac.mka",
        lossless: true,
        channels: 2,
        rate: RATE,
        // Matroska 的时间戳只有毫秒精度（44.1 kHz 下一毫秒是 44.1 帧），落点因此
        // 落在最近的毫秒上，实测差 3 帧（0.07 ms）。同容器的定位本身是准的。
        seek_offset: -3,
        seek_error: 1e-6,
    },
    Case {
        name: "aac",
        file: "stereo_aac.m4a",
        lossless: false,
        channels: 2,
        rate: RATE,
        seek_offset: 0,
        seek_error: 1e-3,
    },
    Case {
        name: "mp3",
        file: "stereo.mp3",
        lossless: false,
        channels: 2,
        rate: RATE,
        seek_offset: 0,
        seek_error: 1e-3,
    },
    Case {
        name: "vorbis",
        file: "stereo.ogg",
        lossless: false,
        channels: 2,
        rate: RATE,
        // 上游 Ogg/Vorbis 报告的落点比实际音频早**整整半个长窗**（1024 帧，23 ms）：
        // 拖到 1:00 时进度条说 1:00，出声的是 1:00.023。修它要么按块长回补一个
        // 编码相关的常数（我们拿不到），要么等上游；23 ms 听不出来，如实记在这里。
        seek_offset: 1024,
        seek_error: 1e-3,
    },
    Case {
        name: "opus-ogg",
        file: "stereo_opus.opus",
        lossless: false,
        channels: 2,
        // Opus 一律解到 48 kHz，与源的 44.1 kHz 无关（容器头里那个 44100 是
        // 「原始输入采样率」，不是解码输出的采样率）。
        rate: 48_000,
        seek_offset: 0,
        // pre-roll 之后仍有解码器状态的残差，实测 1e-4 量级。
        seek_error: 1e-3,
    },
    Case {
        name: "opus-webm",
        file: "stereo_opus.webm",
        lossless: false,
        channels: 2,
        rate: 48_000,
        // 这一条走的是「重开 + 向前解码」的定位路径（见 `decode.rs` 的
        // `reader_seek_is_trustworthy`），与参考解码是同一条路，因此逐样本一致。
        seek_offset: 0,
        seek_error: 1e-6,
    },
    Case {
        name: "mono-flac",
        file: "mono.flac",
        lossless: true,
        channels: 1,
        rate: RATE,
        seek_offset: 0,
        seek_error: 1e-6,
    },
    Case {
        name: "48k-flac",
        file: "stereo_48k.flac",
        lossless: true,
        channels: 2,
        rate: 48_000,
        seek_offset: 0,
        seek_error: 1e-6,
    },
];

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("corpus")
}

/// 语料齐备时返回目录，否则打印生成命令并返回 `None`（用例跳过）。
fn corpus() -> Option<PathBuf> {
    let dir = corpus_dir();
    if dir.join("source.wav").exists() {
        return Some(dir);
    }
    eprintln!(
        "\n跳过格式矩阵：语料不存在。\n生成命令：cargo run -p shannon-audio --example make_corpus\n"
    );
    None
}

fn decode_all(path: &Path) -> (Vec<f32>, u32, u16) {
    let mut decoder = Decoder::open(path).unwrap_or_else(|e| panic!("打开 {path:?} 失败：{e}"));
    let rate = decoder.spec().sample_rate;
    let channels = decoder.spec().layout.count();
    let out = decode_remaining(&mut decoder, path);
    (out, rate, channels)
}

fn decode_remaining(decoder: &mut Decoder, path: &Path) -> Vec<f32> {
    let mut out = Vec::new();
    while decoder
        .next_frames(&mut out)
        .unwrap_or_else(|e| panic!("解码 {path:?} 失败：{e}"))
    {}
    out
}

/// 均方根。有损编码逐样本比不了，只能看整体能量是否守恒。
fn rms(samples: &[f32]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / samples.len() as f64).sqrt()
}

/// 在 ±`window` 帧内搜索使两段最接近的偏移，返回 (偏移帧, 该偏移下的均方根误差)。
///
/// 有偏移搜索是因为编码器会插入前导延迟；直接按下标比对会把「对齐问题」
/// 误报成「解码错误」，两者的修法完全不同。
fn best_alignment(a: &[f32], b: &[f32], channels: usize, window: i64) -> (i64, f64) {
    let compare_frames = 20_000
        .min(a.len() / channels / 2)
        .min(b.len() / channels / 2);
    let mut best = (0i64, f64::MAX);
    for shift in -window..=window {
        let (ai, bi) = if shift >= 0 {
            (shift as usize * channels, 0usize)
        } else {
            (0usize, (-shift) as usize * channels)
        };
        if ai + compare_frames * channels > a.len() || bi + compare_frames * channels > b.len() {
            continue;
        }
        let err: f64 = (0..compare_frames * channels)
            .map(|i| (a[ai + i] as f64 - b[bi + i] as f64).powi(2))
            .sum::<f64>()
            / (compare_frames * channels) as f64;
        let err = err.sqrt();
        if err < best.1 {
            best = (shift, err);
        }
    }
    best
}

#[test]
fn every_format_decodes_with_correct_spec() {
    let Some(dir) = corpus() else { return };
    for case in CASES {
        let path = dir.join(case.file);
        if !path.exists() {
            eprintln!("  跳过 {}：语料缺失", case.name);
            continue;
        }
        let (samples, rate, channels) = decode_all(&path);
        assert_eq!(rate, case.rate, "{} 的采样率读错了", case.name);
        assert_eq!(channels, case.channels, "{} 的声道数读错了", case.name);
        assert!(!samples.is_empty(), "{} 没解出任何样本", case.name);

        // 时长应接近 2 秒。差得远说明容器时长解析或 gapless 裁剪出了问题。
        let seconds = samples.len() as f64 / channels as f64 / rate as f64;
        assert!(
            (seconds - 2.0).abs() < 0.1,
            "{} 解出的时长偏离过大：{seconds:.3} 秒",
            case.name
        );
    }
}

#[test]
fn lossless_formats_match_the_source_sample_for_sample() {
    let Some(dir) = corpus() else { return };
    let (source, _, _) = decode_all(&dir.join("source.wav"));

    for case in CASES
        .iter()
        .filter(|c| c.lossless && c.channels == 2 && c.rate == RATE)
    {
        let path = dir.join(case.file);
        if !path.exists() {
            eprintln!("  跳过 {}：语料缺失", case.name);
            continue;
        }
        let (decoded, _, _) = decode_all(&path);
        let (shift, err) = best_alignment(&source, &decoded, 2, 4_096);
        // 无损就是无损：对齐之后必须逐样本一致，误差只剩 16-bit 量化那一档。
        assert!(
            err < 1e-4,
            "{} 号称无损却与源不符：最佳偏移 {shift} 帧时均方根误差仍有 {err:.2e}",
            case.name
        );
        assert_eq!(shift, 0, "{} 的解码结果相对源有 {shift} 帧偏移", case.name);
    }
}

#[test]
fn lossy_formats_preserve_overall_energy() {
    let Some(dir) = corpus() else { return };
    let (source, _, _) = decode_all(&dir.join("source.wav"));
    let source_rms = rms(&source);

    for case in CASES.iter().filter(|c| !c.lossless) {
        let path = dir.join(case.file);
        if !path.exists() {
            eprintln!("  跳过 {}：语料缺失", case.name);
            continue;
        }
        let (decoded, _, _) = decode_all(&path);
        let ratio = rms(&decoded) / source_rms;
        // 有损编码改的是细节不是响度。整体能量差一成以上，
        // 说明的多半不是「压缩损失」而是声道错位或半数样本丢了。
        assert!(
            (0.9..1.1).contains(&ratio),
            "{} 的整体能量偏离源 {:.1}%",
            case.name,
            (ratio - 1.0) * 100.0
        );
    }
}

#[test]
fn every_format_seeks_to_the_right_place() {
    let Some(dir) = corpus() else { return };
    for case in CASES {
        let path = dir.join(case.file);
        if !path.exists() {
            continue;
        }
        let mut decoder = Decoder::open(&path).unwrap();
        let frames = decoder
            .seek(1.0)
            .unwrap_or_else(|e| panic!("{} 定位失败：{e}", case.name));
        let expect = case.rate as i64;
        assert!(
            (frames as i64 - expect).abs() < case.rate as i64 / 10,
            "{} 定位到 1.0 秒却报告第 {frames} 帧（应约 {expect}）",
            case.name
        );

        // 定位后必须还能解出音频——报告的位置对但解不出东西同样是坏的。
        let mut out = Vec::new();
        let got = decoder.next_frames(&mut out).unwrap();
        assert!(got && !out.is_empty(), "{} 定位后解不出数据", case.name);
    }
}

#[test]
fn every_format_seeks_to_audio_that_actually_matches() {
    // 上一条只验「报告的位置数字对不对」，而这两件事是可以分开坏的：Opus 装在
    // Matroska / WebM 里实测正是数字漂亮、音频对不上——报告 1.0 秒，解出来的那段
    // 与整曲任何一段都不匹配（全局搜索下最小相对误差 1.6~3.9，而两段不相关信号
    // 约为 1.41）。只验位置的话，一个把音频解成噪声的定位实现也能满分通过。
    let Some(dir) = corpus() else { return };
    for case in CASES {
        let path = dir.join(case.file);
        if !path.exists() {
            continue;
        }
        let (full, _, _) = decode_all(&path);
        let ch = case.channels as usize;

        let mut decoder = Decoder::open(&path).unwrap();
        let landed = decoder.seek(SEEK_AT).unwrap() as usize;
        let mut after = Vec::new();
        while after.len() / ch < SETTLE_FRAMES + COMPARE_FRAMES {
            if !decoder.next_frames(&mut after).unwrap() {
                break;
            }
        }
        assert!(
            after.len() / ch >= SETTLE_FRAMES + COMPARE_FRAMES,
            "{} 定位后解出的音频不够比对：只有 {} 帧",
            case.name,
            after.len() / ch
        );

        // 跳过定位后的头 100 ms 再比：那一段是解码器状态的收敛期（Opus 无 pre-roll
        // 时相对误差从 78% 起步），把它算进来会让「热身不够」与「拿错了音频」
        // 混成同一个数字，而这两者的修法完全不同。
        let got = &after[SETTLE_FRAMES * ch..(SETTLE_FRAMES + COMPARE_FRAMES) * ch];
        let start = (landed + SETTLE_FRAMES) as i64 + case.seek_offset;
        assert!(start >= 0, "{} 的期望比对起点为负", case.name);
        let start = start as usize * ch;
        assert!(
            start + got.len() <= full.len(),
            "{} 的期望比对区间超出整曲长度",
            case.name
        );

        let err: f64 = (0..got.len())
            .map(|i| (full[start + i] as f64 - got[i] as f64).powi(2))
            .sum::<f64>()
            / got.len() as f64;
        let relative = err.sqrt() / rms(got);
        assert!(
            relative < case.seek_error,
            "{} 定位后解出的不是它报告的那段音频：相对误差 {relative:.2e}（上限 {:.0e}）。\
             若这是有意的行为变化，连同 seek_offset 一起改，并写清测得的新值",
            case.name,
            case.seek_error
        );
    }
}

#[test]
fn opus_seek_does_not_depend_on_decoder_history() {
    // libopus 适配器的 pre-skip 原本是一枚只消费一次、reset 不会恢复的内部状态：
    // 首次解码前 seek 与播过一包后 seek 会走成两套语义，seek(0) 还会多放 312 帧。
    // 这里同时钉住「回到开头」与「中途 pre-roll」两条分支。
    let Some(dir) = corpus() else { return };
    let path = dir.join("stereo_opus.opus");
    if !path.exists() {
        eprintln!("  跳过 Opus seek 状态用例：语料缺失");
        return;
    }

    for target in [0.0, SEEK_AT] {
        let mut fresh = Decoder::open(&path).unwrap();
        let fresh_landed = fresh.seek(target).unwrap();

        let mut used = Decoder::open(&path).unwrap();
        let mut first_packet = Vec::new();
        assert!(used.next_frames(&mut first_packet).unwrap());
        let used_landed = used.seek(target).unwrap();

        assert_eq!(
            used_landed, fresh_landed,
            "Opus 定位到 {target} 秒的落点不该取决于此前是否解过音频"
        );

        let fresh_after = decode_remaining(&mut fresh, &path);
        let used_after = decode_remaining(&mut used, &path);
        assert_eq!(
            used_after.len(),
            fresh_after.len(),
            "Opus 定位到 {target} 秒后输出长度随解码历史变化"
        );
        let err = fresh_after
            .iter()
            .zip(&used_after)
            .map(|(a, b)| (*a as f64 - *b as f64).powi(2))
            .sum::<f64>()
            / fresh_after.len() as f64;
        assert!(
            err.sqrt() < 1e-6,
            "Opus 定位到 {target} 秒后内容随解码历史变化：均方根误差 {:.2e}",
            err.sqrt()
        );
    }
}

#[test]
fn every_format_plays_to_completion() {
    let Some(dir) = corpus() else { return };
    for case in CASES {
        let path = dir.join(case.file);
        if !path.exists() {
            continue;
        }

        let ended = Arc::new(AtomicBool::new(false));
        let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let engine = {
            let (ended, errors) = (ended.clone(), errors.clone());
            // 固定 48 kHz：既覆盖 44.1 → 48 的重采样，也让 48k 语料走直通，
            // 一个后端同时验两条路径。
            Engine::spawn(
                Box::new(NullOutput::with_fixed_rate(48_000)),
                move |event| match event {
                    EngineEvent::TrackEnded => ended.store(true, Ordering::Relaxed),
                    EngineEvent::Error(e) => errors.lock().unwrap().push(e.to_string()),
                    _ => {}
                },
            )
        };

        engine.load(&path, true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !ended.load(Ordering::Relaxed) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }

        let errs = errors.lock().unwrap().clone();
        assert!(errs.is_empty(), "{} 播放报错：{errs:?}", case.name);
        assert!(
            ended.load(Ordering::Relaxed),
            "{} 没能播放到自然结束",
            case.name
        );
        assert_eq!(
            engine.stats().underruns,
            0,
            "{} 播放期间发生欠载",
            case.name
        );
    }
}

#[test]
fn one_engine_plays_a_mixed_format_playlist() {
    // 同一个引擎依次放不同格式，模拟真实歌单。这条覆盖的是**换曲时重新协商**：
    // 采样率与声道数都会变（44.1k 立体声 → 48k 立体声 → 44.1k 单声道），
    // 输出流、环形缓冲与重采样器必须整套重建。沿用旧配置的话，
    // 轻则音高不对，重则按错误的声道数读缓冲。
    let Some(dir) = corpus() else { return };
    let playlist = [
        "stereo.flac",
        "stereo_48k.flac",
        "mono.flac",
        "stereo_alac.m4a",
    ];
    if playlist.iter().any(|f| !dir.join(f).exists()) {
        eprintln!("  跳过歌单用例：语料缺失");
        return;
    }

    let ended = Arc::new(AtomicBool::new(false));
    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let rates: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));

    let engine = {
        let (ended, errors, rates) = (ended.clone(), errors.clone(), rates.clone());
        Engine::spawn(Box::new(NullOutput::new()), move |event| match event {
            EngineEvent::TrackEnded => ended.store(true, Ordering::Relaxed),
            EngineEvent::Error(e) => errors.lock().unwrap().push(e.to_string()),
            EngineEvent::Opened { output, .. } => rates.lock().unwrap().push(output.sample_rate),
            _ => {}
        })
    };

    for file in playlist {
        ended.store(false, Ordering::Relaxed);
        engine.load(dir.join(file), true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !ended.load(Ordering::Relaxed) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(ended.load(Ordering::Relaxed), "歌单里的 {file} 没能播完");
    }

    let errs = errors.lock().unwrap().clone();
    assert!(errs.is_empty(), "歌单播放报错：{errs:?}");
    // 空后端全盘接受请求，所以协商结果应逐首跟着源走——这正好证明换曲重新协商了，
    // 而不是把第一首的配置一路沿用下去。
    assert_eq!(*rates.lock().unwrap(), vec![RATE, 48_000, RATE, RATE]);
}
