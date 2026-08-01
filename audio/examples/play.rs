//! 开发期播放 CLI：`cargo run -p shannon-audio --example play -- <文件或目录>… [选项]`
//!
//! 属于架构约束允许的开发期诊断工具，不进入产品播放路径。
//! 它同时是引擎的手动验收入口——规格、协商结果、位置推进、欠载次数都打在一屏里，
//! 出问题时先看这里能立刻分清是解码、协商还是喂料的毛病。
//!
//! 传目录即按文件名顺序连续播放，配合 `make_playlist` 产出的多格式歌单做实测：
//! 无损之间应当听不出差别，44.1k 与 48k 那两首的对比就是重采样质量。
//! **换曲走无缝接续**（不拆输出流），因此这里也是 gapless 的手动验收入口：
//! 接缝处不该有停顿、爆音或任何可闻的断点。`--each` 要求把每首截断，与无缝互斥，
//! 给了它就退回「一首一装载」的老路（曲目之间会有一声停顿，那是预期的）。
//!
//! 选项：
//!   --each N      每首最多播 N 秒（默认放完；用来快速过一遍歌单）
//!   --seconds N   总共播 N 秒后退出
//!   --seek N      开播前先定位到第 N 秒（只作用于第一首）
//!   --volume V    音量 0.0 ~ 1.0（默认 1.0）
//!   --device 名字 指定输出设备（名字片段，不区分大小写；默认走系统默认设备）
//!   --null        用空输出后端（无声卡环境验证链路）
//!
//! `--device` 是用来排除「送错设备」的：系统默认输出可能是蓝牙耳机、虚拟声卡或
//! 聚合设备，此时引擎一切正常（欠载为零、位置照常推进）而人耳什么也听不到。
//! 设备清单用 `devices` 例子看。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use shannon_audio::engine::{
    Engine, EngineEvent, LoadContext, LoadRequest, NextRequest, PlaybackState,
};
use shannon_audio::output::{cpal_out::CpalOutput, null::NullOutput, OutputBackend};

/// 指定无缝接续的下一首；越过列表末尾就明确告诉引擎「没有下一首了」。
///
/// 清空这一步不能省：不清的话引擎会一直接着上一次指定的那首，
/// 放到列表末尾反而绕回去——这正是「下一首」必须由外面**每次重算**的原因。
fn next_request(tracks: &[PathBuf], index: usize) -> Option<NextRequest> {
    tracks
        .get(index)
        .map(|path| NextRequest::new(path, LoadContext::new(None, format!("play-next-{index}"))))
}

fn queue_next(engine: &Engine, chain_id: &str, tracks: &[PathBuf], index: usize, revision: u32) {
    engine
        .set_next(chain_id, next_request(tracks, index), revision)
        .unwrap();
}

/// 可播放的扩展名。识别与播放能力解耦——这里只是**遍历目录时的筛选**，
/// 真正能不能放由探测器说了算，扫到不认识的容器会给出明确错误而不是被悄悄跳过。
const AUDIO_EXT: &[&str] = &[
    "flac", "m4a", "mp4", "mp3", "wav", "aiff", "aif", "caf", "ogg", "oga", "mka", "webm",
];

fn main() {
    let mut args = std::env::args().skip(1);
    let mut inputs: Vec<PathBuf> = Vec::new();
    let mut total_limit: Option<f64> = None;
    let mut each_limit: Option<f64> = None;
    let mut seek: Option<f64> = None;
    let mut volume = 1.0f32;
    let mut device: Option<String> = None;
    let mut use_null = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seconds" => total_limit = args.next().and_then(|v| v.parse().ok()),
            "--each" => each_limit = args.next().and_then(|v| v.parse().ok()),
            "--seek" => seek = args.next().and_then(|v| v.parse().ok()),
            "--volume" => volume = args.next().and_then(|v| v.parse().ok()).unwrap_or(1.0),
            "--device" => device = args.next(),
            "--null" => use_null = true,
            other => inputs.push(PathBuf::from(other)),
        }
    }

    let tracks = expand(&inputs);
    if tracks.is_empty() {
        eprintln!(
            "用法：play <文件或目录>… [--each N] [--seconds N] [--seek N] [--volume V] [--device 名字] [--null]"
        );
        std::process::exit(2);
    }

    let backend: Box<dyn OutputBackend> = if use_null {
        Box::new(NullOutput::new())
    } else if let Some(name) = device {
        Box::new(CpalOutput::with_device(name))
    } else {
        Box::new(CpalOutput::new())
    };

    let ended = Arc::new(AtomicBool::new(false));
    let failed = Arc::new(AtomicBool::new(false));
    let position_ms = Arc::new(AtomicU64::new(0));
    // 已完成的无缝交接次数。曲序靠它推进：交接由引擎在消费端越过边界时判定，
    // 外面只能等它报告，不能自己数拍子。
    let changed = Arc::new(AtomicU64::new(0));

    let engine = {
        let (ended, failed, position_ms) = (ended.clone(), failed.clone(), position_ms.clone());
        let changed = changed.clone();
        Engine::spawn(backend, move |event| match event {
            EngineEvent::Opened { spec, output } => {
                let resampled = if output.sample_rate != spec.sample_rate {
                    format!(
                        "  ← 重采样 {} → {} Hz",
                        spec.sample_rate, output.sample_rate
                    )
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
                    spec.duration_sec()
                        .map(|d| format!(" · {d:.1} 秒"))
                        .unwrap_or_default()
                );
                println!(
                    "     输出 {} · {} Hz · {}{resampled}",
                    output.device_name, output.sample_rate, output.sample_format
                );
            }
            EngineEvent::Progress {
                position_sec,
                duration_sec,
                buffered_sec,
            } => {
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
            EngineEvent::TrackChanged { spec, .. } => {
                // 无缝交接：这一行出现时新曲已经在响了（判定发生在消费端越过边界时）。
                println!(
                    "\n     ── 无缝接上 {} / {} Hz",
                    spec.codec, spec.sample_rate
                );
                changed.fetch_add(1, Ordering::Relaxed);
            }
            EngineEvent::OutputChanged { spec, output, .. } => {
                println!(
                    "\n     ── 换到 {} · {} Hz · {}{}",
                    output.device_name,
                    output.sample_rate,
                    output.sample_format,
                    if output.sample_rate != spec.sample_rate {
                        format!(
                            "  ← 重采样 {} → {} Hz",
                            spec.sample_rate, output.sample_rate
                        )
                    } else {
                        String::new()
                    }
                );
            }
            EngineEvent::DeviceRejected { error, .. } => {
                // 换不成不是播放失败：这一行出现时上一台设备仍在正常出声。
                eprintln!("\n     换设备被拒：{error}");
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
    let mut used_gapless = false;

    // 每首播完的一行小结。放进闭包是因为无缝链与截断模式都要打，而两边的
    // 「一首结束了」是完全不同的时刻：前者是引擎报的交接，后者是外面掐的表。
    let mut report = |engine: &Engine| {
        let stats = engine.stats();
        let frames = stats.frames_consumed.saturating_sub(last_frames);
        let underruns = stats.underruns.saturating_sub(last_underruns);
        last_frames = stats.frames_consumed;
        last_underruns = stats.underruns;
        println!(
            "\r     消费 {frames} 帧 · 欠载 {underruns} 次 · 重采样 {}          ",
            if stats.resampled { "是" } else { "否" }
        );
    };

    let header = |i: usize| {
        println!(
            "\n[{:>2}/{}] {}",
            i + 1,
            count,
            tracks[i]
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
        );
    };

    let over_total = || total_limit.is_some_and(|limit| started.elapsed().as_secs_f64() >= limit);

    if each_limit.is_none() {
        // ── 无缝链 ──
        used_gapless = true;
        let chain_id = "play-gapless-chain";
        header(0);
        engine
            .load_request(
                LoadRequest::new(&tracks[0], true, LoadContext::new(None, chain_id))
                    .with_next(next_request(&tracks, 1), 1),
            )
            .unwrap();
        if let Some(sec) = seek {
            // 等装载完成再定位；这是诊断工具，不值得为它引入一套同步协议。
            std::thread::sleep(Duration::from_millis(400));
            engine.seek(sec).unwrap();
        }
        let mut index = 0usize;
        loop {
            let crossed = changed.load(Ordering::Relaxed) as usize;
            while index < crossed {
                report(&engine);
                index += 1;
                header(index);
                // 交接完成才指定再下一首：早指定也无妨，但这样与前端的做法一致
                // ——「下一首是谁」在每次切歌后按当时的队列重算。
                queue_next(&engine, chain_id, &tracks, index + 1, index as u32 + 1);
            }
            if ended.load(Ordering::Relaxed) || over_total() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        report(&engine);
        any_failed |= failed.load(Ordering::Relaxed);
    } else {
        // ── 截断模式：一首一装载，曲目之间有停顿 ──
        for (i, path) in tracks.iter().enumerate() {
            if over_total() {
                break;
            }
            header(i);
            ended.store(false, Ordering::Relaxed);
            failed.store(false, Ordering::Relaxed);
            engine.load(path, true).unwrap();

            if i == 0 {
                if let Some(sec) = seek {
                    std::thread::sleep(Duration::from_millis(400));
                    engine.seek(sec).unwrap();
                }
            }

            let track_started = Instant::now();
            while !ended.load(Ordering::Relaxed) {
                if each_limit.is_some_and(|limit| track_started.elapsed().as_secs_f64() >= limit)
                    || over_total()
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            report(&engine);
            any_failed |= failed.load(Ordering::Relaxed);
        }
    }

    let stats = engine.stats();
    println!(
        "\n合计  {} 首 · 无缝交接 {} 次 · 欠载 {} 次 · 设备延迟 {} 帧{}",
        count,
        changed.load(Ordering::Relaxed),
        stats.underruns,
        stats.output_delay_frames,
        if used_gapless {
            ""
        } else {
            "（--each 模式，换曲拆流）"
        }
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
            // 递归：make_playlist 会把曲目放进以专辑名命名的子目录，
            // 只看一层的话传歌单根目录会什么都找不到。
            let mut found = Vec::new();
            collect(input, &mut found);
            found.sort();
            out.extend(found);
        } else {
            out.push(input.clone());
        }
    }
    out
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if has_audio_ext(&path) {
            out.push(path);
        }
    }
}

fn has_audio_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXT.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}
