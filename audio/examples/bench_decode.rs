//! 解码吞吐基准：`cargo run --release -p shannon-audio --example bench_decode -- <文件或目录>`
//!
//! 回答一个具体的产品问题：**把整个曲库解码一遍要多久**。
//! 响度分析（ReplayGain）、波形图、指纹这类离线工作都要完整解码一遍音频，
//! 而「要不要一口气全扫」取决于这个数字是 5 分钟还是 5 小时——凭感觉排不了期。
//!
//! 报告的是**倍速**（音频时长 ÷ 墙钟时间）而不是「多少文件每秒」：
//! 文件长短差着几十倍，按文件计数得出的速度换个曲库就不成立。
//!
//! 选项：
//!   --limit N     最多测 N 个文件（默认全部）
//!   --jobs N      并发线程数（默认 1；给出并行加速比用）
//!
//! 属于架构约束允许的开发期诊断工具，不进入产品路径。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use shannon_audio::decode::Decoder;

const AUDIO_EXT: &[&str] = &[
    "flac", "m4a", "mp4", "mp3", "wav", "aiff", "aif", "caf", "ogg", "oga", "mka", "webm",
];

fn main() {
    let mut args = std::env::args().skip(1);
    let mut inputs: Vec<PathBuf> = Vec::new();
    let mut limit = usize::MAX;
    let mut jobs = 1usize;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--limit" => {
                limit = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(usize::MAX)
            }
            "--jobs" => jobs = args.next().and_then(|v| v.parse().ok()).unwrap_or(1).max(1),
            other => inputs.push(PathBuf::from(other)),
        }
    }

    let mut files = Vec::new();
    for input in &inputs {
        if input.is_dir() {
            collect(input, &mut files);
        } else {
            files.push(input.clone());
        }
    }
    files.sort();
    files.truncate(limit);

    if files.is_empty() {
        eprintln!("用法：bench_decode <文件或目录>… [--limit N] [--jobs N]");
        std::process::exit(2);
    }

    println!("解码 {} 个文件，{jobs} 线程\n", files.len());

    let queue = Arc::new(Mutex::new(files.clone().into_iter()));
    let cursor = Arc::new(AtomicUsize::new(0));
    let totals = Arc::new(Mutex::new(Vec::<(f64, f64, String)>::new()));
    let failed = Arc::new(AtomicUsize::new(0));

    let started = Instant::now();
    std::thread::scope(|scope| {
        for _ in 0..jobs {
            let queue = queue.clone();
            let totals = totals.clone();
            let failed = failed.clone();
            let cursor = cursor.clone();
            scope.spawn(move || loop {
                let Some(path) = queue.lock().expect("队列锁").next() else {
                    break;
                };
                let n = cursor.fetch_add(1, Ordering::Relaxed) + 1;
                match decode_whole(&path) {
                    Ok((audio_sec, wall_sec, codec)) => {
                        totals
                            .lock()
                            .expect("结果锁")
                            .push((audio_sec, wall_sec, codec));
                        if jobs == 1 {
                            print!("\r  {n} 个…");
                            use std::io::Write;
                            let _ = std::io::stdout().flush();
                        }
                    }
                    Err(err) => {
                        failed.fetch_add(1, Ordering::Relaxed);
                        eprintln!("\n  跳过 {}：{err}", path.display());
                    }
                }
            });
        }
    });
    let wall = started.elapsed().as_secs_f64();

    let results = totals.lock().expect("结果锁");
    let audio_total: f64 = results.iter().map(|(a, _, _)| a).sum();
    let cpu_total: f64 = results.iter().map(|(_, w, _)| w).sum();

    println!("\r{:40}", "");
    println!("音频总长 {:.1} 分钟", audio_total / 60.0);
    println!("墙钟耗时 {wall:.1} 秒 · CPU 累计 {cpu_total:.1} 秒");
    println!("吞吐 {:.0}x 实时（{jobs} 线程）", audio_total / wall);
    if jobs == 1 {
        println!("单线程 {:.0}x 实时", audio_total / cpu_total);
    }
    if failed.load(Ordering::Relaxed) > 0 {
        println!("失败 {} 个", failed.load(Ordering::Relaxed));
    }

    // 按编码分组：ALAC 与 FLAC 的解码代价差得远，混合曲库的总时间由占比决定。
    let mut by_codec: std::collections::HashMap<String, (f64, f64, usize)> = Default::default();
    for (audio, cpu, codec) in results.iter() {
        let e = by_codec.entry(codec.clone()).or_default();
        e.0 += audio;
        e.1 += cpu;
        e.2 += 1;
    }
    let mut rows: Vec<_> = by_codec.into_iter().collect();
    rows.sort_by_key(|row| std::cmp::Reverse(row.1 .2));
    println!("\n按编码：");
    for (codec, (audio, cpu, count)) in rows {
        println!("  {codec:<8} {count:>4} 首 · {:.0}x 实时", audio / cpu);
    }
}

/// 完整解码一个文件，返回（音频秒数、解码墙钟秒数、编码名）。
fn decode_whole(path: &Path) -> Result<(f64, f64, String), String> {
    let started = Instant::now();
    let mut decoder = Decoder::open(path).map_err(|e| e.to_string())?;
    let codec = decoder.spec().codec.clone();
    let rate = decoder.spec().sample_rate as f64;
    let channels = decoder.spec().layout.count().max(1) as f64;

    let mut buf = Vec::new();
    let mut samples = 0u64;
    loop {
        buf.clear();
        match decoder.next_frames(&mut buf) {
            Ok(true) => samples += buf.len() as u64,
            Ok(false) => break,
            Err(e) => return Err(e.to_string()),
        }
    }
    let audio_sec = samples as f64 / channels / rate;
    Ok((audio_sec, started.elapsed().as_secs_f64(), codec))
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| AUDIO_EXT.contains(&e.to_ascii_lowercase().as_str()))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
}
