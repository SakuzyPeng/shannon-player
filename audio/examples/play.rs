//! 开发期播放 CLI：`cargo run -p shannon-audio --example play -- <文件> [选项]`
//!
//! 属于架构约束允许的开发期诊断工具，不进入产品播放路径。
//! 它同时是引擎的手动验收入口——规格、协商结果、位置推进、欠载次数都打在一屏里，
//! 出问题时先看这里能立刻分清是解码、协商还是喂料的毛病。
//!
//! 选项：
//!   --seconds N   播放 N 秒后退出（默认放完整首）
//!   --seek N      开播前先定位到第 N 秒
//!   --volume V    音量 0.0 ~ 1.0（默认 1.0）
//!   --null        用空输出后端（无声卡环境验证链路）

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use shannon_audio::engine::{Engine, EngineEvent, PlaybackState};
use shannon_audio::output::{cpal_out::CpalOutput, null::NullOutput, OutputBackend};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut path: Option<PathBuf> = None;
    let mut seconds: Option<f64> = None;
    let mut seek: Option<f64> = None;
    let mut volume = 1.0f32;
    let mut use_null = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seconds" => seconds = args.next().and_then(|v| v.parse().ok()),
            "--seek" => seek = args.next().and_then(|v| v.parse().ok()),
            "--volume" => volume = args.next().and_then(|v| v.parse().ok()).unwrap_or(1.0),
            "--null" => use_null = true,
            other => path = Some(PathBuf::from(other)),
        }
    }

    let Some(path) = path else {
        eprintln!("用法：play <文件> [--seconds N] [--seek N] [--volume V] [--null]");
        std::process::exit(2);
    };

    let backend: Box<dyn OutputBackend> =
        if use_null { Box::new(NullOutput::new()) } else { Box::new(CpalOutput::new()) };

    let finished = Arc::new(AtomicBool::new(false));
    let failed = Arc::new(AtomicBool::new(false));

    let engine = {
        let finished = finished.clone();
        let failed = failed.clone();
        Engine::spawn(backend, move |event| match event {
            EngineEvent::Opened { spec, output } => {
                println!("音源  {} / {}", spec.container, spec.codec);
                println!(
                    "      {} Hz · {} · 布局来源 {:?}{}",
                    spec.sample_rate,
                    spec.layout.describe(),
                    spec.layout.source(),
                    spec.duration_sec().map(|d| format!(" · {d:.1} 秒")).unwrap_or_default()
                );
                println!(
                    "输出  {} · {} Hz · {} · {}",
                    output.device_name,
                    output.sample_rate,
                    output.layout.describe(),
                    output.sample_format
                );
            }
            EngineEvent::Progress { position_sec, duration_sec, buffered_sec } => {
                let total = duration_sec.map(|d| format!("/{d:.1}")).unwrap_or_default();
                print!("\r播放  {position_sec:6.1}{total} 秒 · 缓冲 {buffered_sec:.2} 秒   ");
                use std::io::Write;
                let _ = std::io::stdout().flush();
            }
            EngineEvent::StateChanged(state) => {
                if state == PlaybackState::Error {
                    failed.store(true, Ordering::Relaxed);
                }
            }
            EngineEvent::TrackEnded => {
                println!("\n播完");
                finished.store(true, Ordering::Relaxed);
            }
            EngineEvent::Error(err) => {
                eprintln!("\n错误  {err}");
                failed.store(true, Ordering::Relaxed);
                finished.store(true, Ordering::Relaxed);
            }
        })
    };

    engine.set_volume(volume).unwrap();
    engine.load(&path, true).unwrap();
    if let Some(sec) = seek {
        // 等装载完成再定位；这是诊断工具，不值得为它引入一套同步协议。
        std::thread::sleep(Duration::from_millis(400));
        engine.seek(sec).unwrap();
    }

    let started = Instant::now();
    while !finished.load(Ordering::Relaxed) {
        if let Some(limit) = seconds {
            if started.elapsed().as_secs_f64() >= limit {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let stats = engine.stats();
    println!(
        "\n统计  消费 {} 帧 · 欠载 {} 次 · 设备延迟 {} 帧",
        stats.frames_consumed, stats.underruns, stats.output_delay_frames
    );

    if failed.load(Ordering::Relaxed) {
        std::process::exit(1);
    }
}
