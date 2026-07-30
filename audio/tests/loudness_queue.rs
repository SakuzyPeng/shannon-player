//! 后台响度分析的端到端用例：喂队列 → 后台跑完 → 结果落盘 → 重启后复用。
//!
//! 语料在测试里现生成（16-bit PCM WAV），不提交二进制、不依赖外部编码器，
//! 因此在无声卡的 CI 上也能跑。队列本身的顺序与替换语义有确定性的单元测试
//! （`audio/src/loudness/service.rs`），这里只看跨线程、跨进程重启后的行为。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use shannon_audio::loudness::{analyze_file_interruptible, LoudnessOutcome};
use shannon_audio::{AnalysisItem, LoudnessService};

const RATE: u32 = 44_100;

fn write_wav(path: &Path, seconds: f64, freq: f64, amplitude: f64) {
    let frames = (RATE as f64 * seconds) as usize;
    let data_len = (frames * 2 * 2) as u32;
    let mut buf = Vec::with_capacity(44 + data_len as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_len).to_le_bytes());
    buf.extend_from_slice(b"WAVEfmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&2u16.to_le_bytes()); // 立体声
    buf.extend_from_slice(&RATE.to_le_bytes());
    buf.extend_from_slice(&(RATE * 4).to_le_bytes());
    buf.extend_from_slice(&4u16.to_le_bytes());
    buf.extend_from_slice(&16u16.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        let t = i as f64 / RATE as f64;
        let v = ((amplitude * (2.0 * std::f64::consts::PI * freq * t).sin()) * i16::MAX as f64)
            as i16;
        buf.extend_from_slice(&v.to_le_bytes());
        buf.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::File::create(path)
        .and_then(|mut f| f.write_all(&buf))
        .expect("写语料失败");
}

/// 每个用例独立的目录，避免并行测试互相覆盖。
fn case_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("shannon-loudness-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建用例目录失败");
    dir
}

/// 等后台把队列跑空。分析是「快于实时」的（实测真实曲库 383× 实时），
/// 这点语料远用不到上限；超时说明真的卡住了，不是机器慢。
fn wait_until_drained(service: &LoudnessService) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while service.pending() > 0 {
        assert!(Instant::now() < deadline, "后台分析超时未跑完");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn queue_is_analyzed_in_the_background_and_survives_a_restart() {
    let dir = case_dir("restart");
    let store_path = dir.join("loudness.json");
    let loud = dir.join("loud.wav");
    let quiet = dir.join("quiet.wav");
    write_wav(&loud, 1.5, 997.0, 0.8);
    write_wav(&quiet, 1.5, 997.0, 0.05);

    let items = vec![
        AnalysisItem {
            track_id: "t-loud".into(),
            path: loud.clone(),
        },
        AnalysisItem {
            track_id: "t-quiet".into(),
            path: quiet.clone(),
        },
    ];

    let service = LoudnessService::spawn(store_path.clone());
    assert_eq!(service.set_queue(items.clone()), 2);
    wait_until_drained(&service);

    // 响的要衰减、轻的要提升——这也顺带验证了增益不是单向的。
    assert!(service.linear_gain("t-loud") < 1.0, "响的应当被压低");
    assert!(service.linear_gain("t-quiet") > 1.0, "轻的应当被提升");
    let quiet_outcome = service.outcome("t-quiet").expect("应当已有结论");
    drop(service); // Drop 里停 worker 并落盘

    assert!(store_path.exists(), "退出时必须把结果写下来");

    // 重启：同一份结果直接复用，不该再排队重算——重算一遍要把全库解码一次。
    let restarted = LoudnessService::spawn(store_path);
    assert_eq!(restarted.set_queue(items), 0, "已分析过的不该再入队");
    assert_eq!(restarted.outcome("t-quiet"), Some(quiet_outcome));
}

#[test]
fn transient_failure_is_retried_instead_of_being_recorded() {
    // 文件读不到是**瞬态**的（网络盘掉线、外置硬盘没插）。把它写成永久结论，
    // 等于让那首歌再也不会被分析。
    let dir = case_dir("transient");
    let store_path = dir.join("loudness.json");
    let missing = AnalysisItem {
        track_id: "t-missing".into(),
        path: dir.join("not-here.wav"),
    };

    let service = LoudnessService::spawn(store_path);
    assert_eq!(service.set_queue([missing.clone()]), 1);
    wait_until_drained(&service);

    assert_eq!(service.outcome("t-missing"), None, "失败不留结论");
    assert_eq!(service.linear_gain("t-missing"), 1.0, "未命中不改音量");
    assert_eq!(
        service.set_queue([missing]),
        1,
        "下次重排队列时应当重试，而不是被当成已经有答案"
    );
}

#[test]
fn silence_is_a_conclusion_and_is_not_retried() {
    // 与上一条相对：静音测不出积分响度，但那是**确定**的结论，
    // 缓存它才不会每次播放都白扫一遍。
    let dir = case_dir("silence");
    let store_path = dir.join("loudness.json");
    let silent = dir.join("silent.wav");
    write_wav(&silent, 1.0, 997.0, 0.0);
    let item = AnalysisItem {
        track_id: "t-silent".into(),
        path: silent,
    };

    let service = LoudnessService::spawn(store_path);
    service.set_queue([item.clone()]);
    wait_until_drained(&service);

    assert_eq!(service.outcome("t-silent"), Some(LoudnessOutcome::Unmeasurable));
    assert_eq!(service.linear_gain("t-silent"), 1.0, "测不出就不处理");
    assert_eq!(service.set_queue([item]), 0, "确定结论不该重测");
}

#[test]
fn analysis_can_be_interrupted_without_leaving_a_half_answer() {
    // 关窗时的退出延迟就靠这条：不必等一整首分析完（一首 4 分钟的约 0.6 秒），
    // 每解一块 PCM 问一次即可。半途没有中间产物——积分响度必须看完整首。
    let dir = case_dir("interrupt");
    let path = dir.join("long.wav");
    write_wav(&path, 5.0, 997.0, 0.5);

    let asked = std::cell::Cell::new(0usize);
    let outcome = analyze_file_interruptible(&path, || {
        asked.set(asked.get() + 1);
        asked.get() <= 2
    })
    .expect("打断不是错误");
    assert_eq!(outcome, None, "被打断时不给结论");
    assert!(asked.get() <= 4, "应当很快停下，实际问了 {} 次", asked.get());

    // 不打断则照常得出结论，证明上面停下的是分析而不是文件本身有问题。
    assert!(matches!(
        analyze_file_interruptible(&path, || true).unwrap(),
        Some(LoudnessOutcome::Measured { .. })
    ));
}
