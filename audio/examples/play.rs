//! 开发期播放 CLI：`cargo run -p shannon-audio --example play -- <文件或目录>… [选项]`
//!
//! 属于架构约束允许的开发期诊断工具，不进入产品播放路径。
//! 它同时是引擎的手动验收入口——规格、协商结果、位置推进、欠载次数都打在一屏里，
//! 出问题时先看这里能立刻分清是解码、协商还是喂料的毛病。
//!
//! 传目录即按文件名顺序连续播放，配合 `make_playlist` 产出的多格式歌单做实测：
//! 无损之间应当听不出差别，44.1k 与 48k 那两首的对比就是重采样质量。
//! **换曲时会重建输出流，间隙是当前的真实行为**（gapless 在后面的阶段）。
//!
//! 选项：
//!   --each N      每首最多播 N 秒（默认放完；用来快速过一遍歌单）
//!   --seconds N   总共播 N 秒后退出
//!   --seek N      开播前先定位到第 N 秒（只作用于第一首）
//!   --volume V    音量 0.0 ~ 1.0（默认 1.0）
//!   --null        用空输出后端（无声卡环境验证链路）

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use shannon_audio::engine::{Engine, EngineEvent, PlaybackState};
use shannon_audio::output::{cpal_out::CpalOutput, null::NullOutput, OutputBackend};

/// 可播放的扩展名。识别与播放能力解耦——这里只是**遍历目录时的筛选**，
/// 真正能不能放由探测器说了算，扫到不认识的容器会给出明确错误而不是被悄悄跳过。
const AUDIO_EXT: &[&str] =
    &["flac", "m4a", "mp4", "mp3", "wav", "aiff", "aif", "caf", "ogg", "oga", "mka", "webm"];

fn main() {
    let mut args = std::env::args().skip(1);
    let mut inputs: Vec<PathBuf> = Vec::new();
    let mut total_limit: Option<f64> = None;
    let mut each_limit: Option<f64> = None;
    let mut seek: Option<f64> = None;
    let mut volume = 1.0f32;
    let mut use_null = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seconds" => total_limit = args.next().and_then(|v| v.parse().ok()),
            "--each" => each_limit = args.next().and_then(|v| v.parse().ok()),
            "--seek" => seek = args.next().and_then(|v| v.parse().ok()),
            "--volume" => volume = args.next().and_then(|v| v.parse().ok()).unwrap_or(1.0),
            "--null" => use_null = true,
            other => inputs.push(PathBuf::from(other)),
        }
    }

    let tracks = expand(&inputs);
    if tracks.is_empty() {
        eprintln!("用法：play <文件或目录>… [--each N] [--seconds N] [--seek N] [--volume V] [--null]");
        std::process::exit(2);
    }

    let backend: Box<dyn OutputBackend> =
        if use_null { Box::new(NullOutput::new()) } else { Box::new(CpalOutput::new()) };

    let ended = Arc::new(AtomicBool::new(false));
    let failed = Arc::new(AtomicBool::new(false));
    let position_ms = Arc::new(AtomicU64::new(0));

    let engine = {
        let (ended, failed, position_ms) = (ended.clone(), failed.clone(), position_ms.clone());
        Engine::spawn(backend, move |event| match event {
            EngineEvent::Opened { spec, output } => {
                let resampled = if output.sample_rate != spec.sample_rate {
                    format!("  ← 重采样 {} → {} Hz", spec.sample_rate, output.sample_rate)
                } else {
                    String::new()
                };
                println!(
                    "     {} / {} · {} Hz · {} · 布局来源 {:?}{}",
                    spec.container,
                    spec.codec,
                    spec.sample_rate,
                    spec.layout.describe(),
                    spec.layout.source(),
                    spec.duration_sec().map(|d| format!(" · {d:.1} 秒")).unwrap_or_default()
                );
                println!(
                    "     输出 {} · {} Hz · {}{resampled}",
                    output.device_name, output.sample_rate, output.sample_format
                );
            }
            EngineEvent::Progress { position_sec, duration_sec, buffered_sec } => {
                position_ms.store((position_sec * 1000.0) as u64, Ordering::Relaxed);
                let total = duration_sec.map(|d| format!("/{d:.1}")).unwrap_or_default();
                print!("\r     {position_sec:6.1}{total} 秒 · 缓冲 {buffered_sec:.2} 秒   ");
                use std::io::Write;
                let _ = std::io::stdout().flush();
            }
            EngineEvent::StateChanged(state) => {
                if state == PlaybackState::Error {
                    failed.store(true, Ordering::Relaxed);
                }
            }
            EngineEvent::TrackEnded => ended.store(true, Ordering::Relaxed),
            EngineEvent::Error(err) => {
                eprintln!("\n     错误 {err}");
                failed.store(true, Ordering::Relaxed);
                ended.store(true, Ordering::Relaxed);
            }
        })
    };

    engine.set_volume(volume).unwrap();

    let started = Instant::now();
    let count = tracks.len();
    let mut any_failed = false;
    let mut last_frames = 0u64;
    let mut last_underruns = 0u64;

    for (i, path) in tracks.iter().enumerate() {
        if let Some(limit) = total_limit {
            if started.elapsed().as_secs_f64() >= limit {
                break;
            }
        }

        println!(
            "\n[{:>2}/{}] {}",
            i + 1,
            count,
            path.file_name().and_then(|n| n.to_str()).unwrap_or("?")
        );
        ended.store(false, Ordering::Relaxed);
        failed.store(false, Ordering::Relaxed);
        engine.load(path, true).unwrap();

        if i == 0 {
            if let Some(sec) = seek {
                // 等装载完成再定位；这是诊断工具，不值得为它引入一套同步协议。
                std::thread::sleep(Duration::from_millis(400));
                engine.seek(sec).unwrap();
            }
        }

        let track_started = Instant::now();
        while !ended.load(Ordering::Relaxed) {
            if let Some(limit) = each_limit {
                if track_started.elapsed().as_secs_f64() >= limit {
                    break;
                }
            }
            if let Some(limit) = total_limit {
                if started.elapsed().as_secs_f64() >= limit {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        let stats = engine.stats();
        let frames = stats.frames_consumed.saturating_sub(last_frames);
        let underruns = stats.underruns.saturating_sub(last_underruns);
        last_frames = stats.frames_consumed;
        last_underruns = stats.underruns;
        println!(
            "\r     消费 {frames} 帧 · 欠载 {underruns} 次 · 重采样 {}          ",
            if stats.resampled { "是" } else { "否" }
        );
        any_failed |= failed.load(Ordering::Relaxed);
    }

    let stats = engine.stats();
    println!(
        "\n合计  {} 首 · 欠载 {} 次 · 设备延迟 {} 帧",
        count, stats.underruns, stats.output_delay_frames
    );

    if any_failed {
        std::process::exit(1);
    }
}

/// 展开输入：目录取其中的音频文件（按文件名排序），文件原样保留。
fn expand(inputs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for input in inputs {
        if input.is_dir() {
            let mut found: Vec<PathBuf> = std::fs::read_dir(input)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && has_audio_ext(p))
                .collect();
            found.sort();
            out.extend(found);
        } else {
            out.push(input.clone());
        }
    }
    out
}

fn has_audio_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXT.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}
