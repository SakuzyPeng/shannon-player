//! 播放链路的无头集成测试。
//!
//! 语料在测试里现生成（16-bit PCM WAV，Symphonia 默认 feature 就能解），
//! 不提交二进制、不依赖外部编码器，因此在无声卡的 CI 上也能跑——
//! 这正是把引擎与 Tauri 解耦的目的。
//!
//! ALAC / AAC 等「不能预先承诺 gapless」的格式要靠真实语料验证，
//! 那属于阶段 1 的语料测试，此处只覆盖阶段 0 的能力边界。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use shannon_audio::decode::Decoder;
use shannon_audio::engine::{Engine, EngineEvent, PlaybackState};
use shannon_audio::layout::ChannelLayout;
use shannon_audio::mix::ChannelAdapt;
use shannon_audio::output::null::NullOutput;
use shannon_audio::{ErrorKind, Stage};

const RATE: u32 = 44_100;
const FREQ: f64 = 440.0;

/// 第 `i` 帧的理论样本值。解码结果要与它逐点比对。
fn sine(i: usize) -> f64 {
    0.3 * (2.0 * std::f64::consts::PI * FREQ * i as f64 / RATE as f64).sin()
}

/// 扫频信号：频率随时间线性上升，因此**没有周期**。
///
/// seek 一类的位置断言必须用它，不能用定频正弦：正弦每 100 帧就重复一次相位，
/// 位置差整数个周期时波形完全重合——差一帧的 bug 会被伪装成「误差为零」。
/// 这不是假设，是本轮实际踩到的：定频语料让 seek 的整数换算 off-by-one 逃过了诊断。
fn chirp(i: usize) -> f64 {
    let t = i as f64 / RATE as f64;
    let (f0, f1, span) = (200.0, 4000.0, 4.0);
    let phase = 2.0 * std::f64::consts::PI * (f0 * t + (f1 - f0) * t * t / (2.0 * span));
    0.3 * phase.sin()
}

/// 写一个 16-bit PCM WAV。`channels` 路声道内容相同。
fn write_wav(path: &Path, channels: u16, frames: usize, gen: fn(usize) -> f64) {
    let byte_rate = RATE * channels as u32 * 2;
    let data_len = (frames * channels as usize * 2) as u32;
    let mut buf = Vec::with_capacity(44 + data_len as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_len).to_le_bytes());
    buf.extend_from_slice(b"WAVEfmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&channels.to_le_bytes());
    buf.extend_from_slice(&RATE.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&(channels * 2).to_le_bytes());
    buf.extend_from_slice(&16u16.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        let v = (gen(i) * i16::MAX as f64) as i16;
        for _ in 0..channels {
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }
    let mut f = std::fs::File::create(path).expect("写语料失败");
    f.write_all(&buf).expect("写语料失败");
}

/// 每个用例独立的语料文件，避免并行测试互相覆盖。
fn corpus(name: &str, channels: u16, frames: usize) -> PathBuf {
    corpus_with(name, channels, frames, sine)
}

fn corpus_with(name: &str, channels: u16, frames: usize, gen: fn(usize) -> f64) -> PathBuf {
    let dir = std::env::temp_dir().join("shannon-audio-tests");
    std::fs::create_dir_all(&dir).expect("建语料目录失败");
    let path = dir.join(format!("{name}.wav"));
    write_wav(&path, channels, frames, gen);
    path
}

fn decode_all(path: &Path) -> Vec<f32> {
    let mut decoder = Decoder::open(path).expect("打开失败");
    let mut out = Vec::new();
    while decoder.next_frames(&mut out).expect("解码失败") {}
    out
}

#[test]
fn decodes_pcm_to_expected_samples() {
    let path = corpus("decode", 2, 4410);
    let mut decoder = Decoder::open(&path).unwrap();
    let spec = decoder.spec().clone();
    assert_eq!(spec.sample_rate, RATE);
    assert!(spec.layout.is_stereo());
    assert_eq!(spec.container, "wave");

    let mut out = Vec::new();
    while decoder.next_frames(&mut out).unwrap() {}
    assert_eq!(out.len(), 4410 * 2);

    // 16-bit 量化误差上界约 1/32768，放宽一档留给格式转换。
    for (i, frame) in out.chunks(2).enumerate() {
        let expect = sine(i) as f32;
        assert!(
            (frame[0] - expect).abs() < 1e-4,
            "第 {i} 帧偏差过大：得到 {}，期望 {expect}",
            frame[0]
        );
        assert_eq!(frame[0], frame[1], "两路声道内容应当一致");
    }
}

#[test]
fn seek_output_matches_decoding_from_start() {
    // seek 等价性：任意位置 seek 后的输出，等于从头解码的对应后缀。
    // 语料必须无周期，否则差整数个周期的偏移会被波形重合掩盖。
    let path = corpus_with("seek", 2, RATE as usize * 2, chirp);
    let full = decode_all(&path);

    let mut decoder = Decoder::open(&path).unwrap();
    let frames = decoder.seek(1.0).unwrap();
    let mut after = Vec::new();
    while decoder.next_frames(&mut after).unwrap() {}

    let offset = frames as usize * 2;
    assert!(offset + after.len() <= full.len() + 2, "seek 后不应多解出数据");
    let compare = after.len().min(full.len() - offset);
    assert!(compare > RATE as usize, "seek 后应还剩接近一秒的音频");
    for i in 0..compare {
        assert!(
            (after[i] - full[offset + i]).abs() < 1e-6,
            "seek 后第 {i} 个样本与整段解码不一致"
        );
    }
}

#[test]
fn seek_past_end_does_not_panic() {
    let path = corpus("seek_past_end", 2, RATE as usize / 2);
    let mut decoder = Decoder::open(&path).unwrap();
    // 越界定位要么报错要么落到末尾，但绝不能 panic——进度条能拖到任何位置。
    let _ = decoder.seek(9_999.0);
}

#[test]
fn mono_source_is_upmixed_to_stereo() {
    let path = corpus("mono", 1, 441);
    let spec_layout = Decoder::open(&path).unwrap().spec().layout;
    assert!(spec_layout.is_mono());

    let plan = ChannelAdapt::plan(spec_layout, ChannelLayout::STEREO).unwrap();
    assert_eq!(plan, ChannelAdapt::MonoToStereo);

    let mono = decode_all(&path);
    let mut stereo = vec![0.0; plan.out_samples(mono.len(), 2)];
    plan.apply(&mono, &mut stereo);
    for (i, frame) in stereo.chunks(2).enumerate() {
        assert_eq!(frame[0], mono[i]);
        assert_eq!(frame[1], mono[i]);
    }
}

#[test]
fn multichannel_source_is_routed_to_platform_backend() {
    // 多声道不走立体声路径：下混与空间化都交给系统，应用自己混会把本可被空间化的流
    // 提前拍扁。所以这里要的是明确的**路由**错误，不是静默丢声道，
    // 也不是按猜的系数把声场弄乱。
    let path = corpus("surround", 6, 441);
    let layout = Decoder::open(&path).unwrap().spec().layout;
    assert_eq!(layout.count(), 6);

    let err = ChannelAdapt::plan(layout, ChannelLayout::STEREO).unwrap_err();
    assert_eq!(err.kind, ErrorKind::Unsupported);
    assert!(
        err.message.contains("交由系统"),
        "错误要说清多声道该走哪条路，而不是暗示我们欠一个下混算法：{}",
        err.message
    );
}

#[test]
fn missing_file_reports_io_error_not_panic() {
    let Err(err) = Decoder::open(Path::new("/不存在/的/文件.wav")) else {
        panic!("不存在的文件不该打开成功");
    };
    assert_eq!(err.kind, ErrorKind::Io);
    assert_eq!(err.stage, Stage::Open);
}

/// 收集引擎事件，供端到端用例断言。
#[derive(Default)]
struct Recorder {
    states: Mutex<Vec<PlaybackState>>,
    ended: AtomicBool,
    errors: Mutex<Vec<String>>,
    last_position_ms: AtomicU64,
}

#[test]
fn plays_to_completion_through_null_backend() {
    let seconds = 0.6;
    let path = corpus("e2e", 2, (RATE as f64 * seconds) as usize);
    let rec = Arc::new(Recorder::default());

    let engine = {
        let rec = rec.clone();
        Engine::spawn(Box::new(NullOutput::new()), move |event| match event {
            EngineEvent::StateChanged(s) => rec.states.lock().unwrap().push(s),
            EngineEvent::TrackEnded => rec.ended.store(true, Ordering::Relaxed),
            EngineEvent::Error(e) => rec.errors.lock().unwrap().push(e.to_string()),
            EngineEvent::Progress { position_sec, .. } => {
                rec.last_position_ms.store((position_sec * 1000.0) as u64, Ordering::Relaxed);
            }
            EngineEvent::Opened { .. } => {}
        })
    };

    engine.load(&path, true).unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    while !rec.ended.load(Ordering::Relaxed) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }

    assert!(rec.errors.lock().unwrap().is_empty(), "不该有错误：{:?}", rec.errors.lock().unwrap());
    assert!(rec.ended.load(Ordering::Relaxed), "应当播放到自然结束");

    let states = rec.states.lock().unwrap().clone();
    assert!(states.contains(&PlaybackState::Playing), "状态序列应经过 Playing：{states:?}");
    assert_eq!(states.last(), Some(&PlaybackState::Ended));

    let stats = engine.stats();
    let played = stats.frames_consumed as f64 / RATE as f64;
    assert!(
        (played - seconds).abs() < 0.05,
        "消费帧数应对应音频时长：播了 {played:.3} 秒，语料 {seconds} 秒"
    );
    assert_eq!(stats.underruns, 0, "正常播放不该欠载");
}

#[test]
fn pause_stops_position_from_advancing() {
    // 暂停期间输出流仍在跑（继续写零帧），但位置不得推进——
    // 位置的事实来源是「消费了多少帧」，不是墙上时钟。
    let path = corpus("pause", 2, RATE as usize * 3);
    let position = Arc::new(AtomicU64::new(0));

    let engine = {
        let position = position.clone();
        Engine::spawn(Box::new(NullOutput::new()), move |event| {
            if let EngineEvent::Progress { position_sec, .. } = event {
                position.store((position_sec * 1000.0) as u64, Ordering::Relaxed);
            }
        })
    };

    engine.load(&path, true).unwrap();
    std::thread::sleep(Duration::from_millis(600));
    engine.pause().unwrap();
    // 等音量斜坡走完并让位置事件刷新一轮。
    std::thread::sleep(Duration::from_millis(400));

    let before = position.load(Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(500));
    let after = position.load(Ordering::Relaxed);

    assert!(before > 0, "暂停前位置应已推进");
    // 斜坡期间还会消费少量帧，留出一档余量。
    assert!(
        after.saturating_sub(before) < 100,
        "暂停后位置不应继续推进：{before} ms → {after} ms"
    );
}

#[test]
fn seek_does_not_count_as_underrun() {
    // seek 后缓冲被清空，此时的「取不到数据」是预期的重缓冲，
    // 计进欠载会让这项指标失去诊断价值。
    let path = corpus("seek_stats", 2, RATE as usize * 3);
    let engine = Engine::spawn(Box::new(NullOutput::new()), |_| {});

    engine.load(&path, true).unwrap();
    std::thread::sleep(Duration::from_millis(400));
    for target in [2.0, 0.5, 1.5] {
        engine.seek(target).unwrap();
        std::thread::sleep(Duration::from_millis(150));
    }

    assert_eq!(engine.stats().underruns, 0, "seek 引起的重缓冲不算欠载");
}


#[test]
fn resamples_when_device_rate_differs() {
    // 复现实测场景：设备只给得出 48 kHz，而曲库主力是 44.1 kHz。
    // 早先这里是直接报错「不支持 44100 Hz」——一首歌都放不了。
    let seconds = 0.6;
    let path = corpus("resample_e2e", 2, (RATE as f64 * seconds) as usize);
    let ended = Arc::new(AtomicBool::new(false));
    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let out_rate = Arc::new(AtomicU64::new(0));

    let engine = {
        let (ended, errors, out_rate) = (ended.clone(), errors.clone(), out_rate.clone());
        Engine::spawn(Box::new(NullOutput::with_fixed_rate(48_000)), move |event| match event {
            EngineEvent::TrackEnded => ended.store(true, Ordering::Relaxed),
            EngineEvent::Error(e) => errors.lock().unwrap().push(e.to_string()),
            EngineEvent::Opened { output, .. } => {
                out_rate.store(output.sample_rate as u64, Ordering::Relaxed)
            }
            _ => {}
        })
    };

    engine.load(&path, true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ended.load(Ordering::Relaxed) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }

    assert!(errors.lock().unwrap().is_empty(), "不该有错误：{:?}", errors.lock().unwrap());
    assert!(ended.load(Ordering::Relaxed), "重采样后仍应播放到自然结束");
    assert_eq!(out_rate.load(Ordering::Relaxed), 48_000);

    let stats = engine.stats();
    assert!(stats.resampled, "插了重采样就必须如实标记");

    // 位置计数记的是输出帧，所以按 48 kHz 换算才对得上音频时长。
    // 若误用源采样率，0.6 秒会被算成 0.653 秒（快 8.8%）。
    let played = stats.frames_consumed as f64 / 48_000.0;
    assert!(
        (played - seconds).abs() < 0.05,
        "重采样后的时长应保持不变：算得 {played:.3} 秒，语料 {seconds} 秒"
    );
    assert_eq!(stats.underruns, 0, "重采样不该造成欠载");
}

#[test]
fn no_resampling_when_rates_match() {
    let path = corpus("no_resample", 2, RATE as usize / 4);
    let engine = Engine::spawn(Box::new(NullOutput::with_fixed_rate(RATE)), |_| {});
    engine.load(&path, true).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(!engine.stats().resampled, "采样率一致时不该插入重采样");
}

#[test]
fn seek_position_is_correct_under_resampling() {
    // seek 返回的是源域帧位置，位置计数器要的是输出域。不换算的话，
    // 44.1 → 48 kHz 时进度会偏 8.8%——拖到 1:00 显示成 1:05。
    let path = corpus("resample_seek", 2, RATE as usize * 3);
    let position = Arc::new(AtomicU64::new(0));
    let engine = {
        let position = position.clone();
        Engine::spawn(Box::new(NullOutput::with_fixed_rate(48_000)), move |event| {
            if let EngineEvent::Progress { position_sec, .. } = event {
                position.store((position_sec * 1000.0) as u64, Ordering::Relaxed);
            }
        })
    };

    engine.load(&path, true).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    engine.seek(2.0).unwrap();
    std::thread::sleep(Duration::from_millis(400));

    let ms = position.load(Ordering::Relaxed);
    assert!(
        (2_000..2_400).contains(&ms),
        "定位到 2.0 秒后位置应在 2.0～2.4 秒之间，实际 {ms} ms"
    );
}

#[test]
fn per_track_frame_count_survives_track_changes() {
    // 位置与累计消费是两个量：位置每首归零，累计跨曲目单调递增。
    // 合成一个字段的话「这首播了多少帧」只能靠差值算，而差值会被归零抹平——
    // 实测表现为歌单里第二首起每首都显示消费 0 帧。
    let path = corpus("per_track_frames", 2, (RATE as f64 * 0.4) as usize);
    let ended = Arc::new(AtomicBool::new(false));
    let engine = {
        let ended = ended.clone();
        Engine::spawn(Box::new(NullOutput::new()), move |event| {
            if matches!(event, EngineEvent::TrackEnded) {
                ended.store(true, Ordering::Relaxed);
            }
        })
    };

    let mut last_total = 0u64;
    for round in 1..=3 {
        ended.store(false, Ordering::Relaxed);
        engine.load(&path, true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ended.load(Ordering::Relaxed) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(ended.load(Ordering::Relaxed), "第 {round} 遍没播完");

        let stats = engine.stats();
        let this_track = stats.frames_consumed - last_total;
        last_total = stats.frames_consumed;
        assert!(
            this_track > RATE as u64 / 4,
            "第 {round} 遍的单曲消费量算成了 {this_track} 帧——累计计数被换曲清零了"
        );
        // 位置则相反：每首从头开始，不会累加到三倍。
        assert!(
            stats.position_frames < RATE as u64,
            "第 {round} 遍的位置累加到了 {} 帧，换曲应当归零",
            stats.position_frames
        );
    }
}
