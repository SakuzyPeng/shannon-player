//! 离线分析吞吐基准：`cargo run --release -p shannon-audio --example bench_decode -- <文件或目录>`
//!
//! 回答两个不同的问题，不能混在一起：
//!
//! - 默认只测完整解码，供波形图、指纹等只消费 PCM 的任务估算成本；
//! - `--loudness` 测阶段 1 的实际负载：解码 + EBU R128 integrated loudness + true peak。
//!
//! 报告的是**倍速**（音频时长 ÷ 墙钟时间）而不是「多少文件每秒」：
//! 文件长短差着几十倍，按文件计数得出的速度换个曲库就不成立。
//!
//! 选项：
//!   --limit N       最多测 N 个文件（默认全部）
//!   --jobs N        并发线程数（默认 1；给出并行加速比用）
//!   --loudness      跑完整响度分析，而非只解码
//!
//! 属于架构约束允许的开发期诊断工具，不进入产品路径。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use shannon_audio::decode::Decoder;
use shannon_audio::loudness::{self, LoudnessAnalyzer, LoudnessOutcome};
use walkdir::WalkDir;

const AUDIO_EXT: &[&str] = &[
    "flac", "m4a", "mp4", "mp3", "wav", "aiff", "aif", "caf", "ogg", "oga", "mka", "webm",
];

// 目标响度、峰值上限与增益公式都取自 `shannon_audio::loudness`，
// **不在这里另抄一份**：基准工具一旦与产品用不同的常量，它测出的就不是产品的负载。

struct Options {
    inputs: Vec<PathBuf>,
    limit: Option<usize>,
    jobs: usize,
    loudness: bool,
}

struct FileResult {
    /// 保留路径是为了让「不可测」能指名道姓。一个「4% 不可测」的统计若说不出是哪些
    /// 文件，就没法判断那是真静音、是隐藏轨，还是分析器用错了——统计会掩盖问题。
    path: PathBuf,
    audio_sec: f64,
    wall_sec: f64,
    codec: String,
    loudness: Option<LoudnessOutcome>,
}

fn main() {
    let options = parse_args().unwrap_or_else(|err| {
        eprintln!("{err}\n");
        usage();
        std::process::exit(2);
    });

    let mut files = Vec::new();
    for input in &options.inputs {
        if input.is_dir() {
            collect(input, &mut files).unwrap_or_else(|err| {
                eprintln!("{err}");
                std::process::exit(2);
            });
        } else {
            // 显式传文件时不按扩展名过滤；内容探测器才有最终决定权。
            files.push(input.clone());
        }
    }
    files.sort();
    files.dedup();
    if let Some(limit) = options.limit {
        files.truncate(limit);
    }

    if files.is_empty() {
        usage();
        std::process::exit(2);
    }

    let mode = if options.loudness {
        format!(
            "完整响度分析（目标 {:.0} LUFS · 真峰值上限 {:.0} dBTP）",
            loudness::TARGET_LUFS,
            loudness::TRUE_PEAK_CEILING_DBTP
        )
    } else {
        "仅解码".into()
    };
    let file_count = files.len();
    let jobs = options.jobs;
    let measure_loudness = options.loudness;
    println!("{mode} · {file_count} 个文件 · {jobs} 线程\n");

    let queue = Arc::new(Mutex::new(files.into_iter()));
    let cursor = Arc::new(AtomicUsize::new(0));
    let totals = Arc::new(Mutex::new(Vec::<FileResult>::new()));
    let failed = Arc::new(AtomicUsize::new(0));

    let started = Instant::now();
    std::thread::scope(|scope| {
        for _ in 0..jobs {
            let queue = queue.clone();
            let totals = totals.clone();
            let failed = failed.clone();
            let cursor = cursor.clone();
            let loudness = measure_loudness;
            scope.spawn(move || loop {
                let Some(path) = queue.lock().expect("队列锁").next() else {
                    break;
                };
                let n = cursor.fetch_add(1, Ordering::Relaxed) + 1;
                match analyze_file(&path, loudness) {
                    Ok(result) => {
                        totals.lock().expect("结果锁").push(result);
                        if jobs == 1 {
                            print!("\r  {n}/{file_count}…");
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
    let failed = failed.load(Ordering::Relaxed);
    if results.is_empty() {
        eprintln!("\n没有任何文件成功处理（失败 {failed} 个），不输出无意义的吞吐统计。");
        std::process::exit(1);
    }

    let audio_total: f64 = results.iter().map(|r| r.audio_sec).sum();
    let cpu_total: f64 = results.iter().map(|r| r.wall_sec).sum();

    println!("\r{:40}", "");
    println!("音频总长 {:.1} 分钟", audio_total / 60.0);
    println!("墙钟耗时 {wall:.1} 秒 · CPU 累计 {cpu_total:.1} 秒");
    println!("吞吐 {:.0}x 实时（{jobs} 线程）", audio_total / wall);
    if jobs == 1 {
        println!("单线程 {:.0}x 实时", audio_total / cpu_total);
    }
    if failed > 0 {
        println!("失败 {failed} 个");
    }

    if measure_loudness {
        print_loudness_summary(&results);
    }

    // 按编码分组：ALAC 与 FLAC 的解码代价差得远，混合曲库的总时间由占比决定。
    let mut by_codec: std::collections::HashMap<String, (f64, f64, usize)> = Default::default();
    for result in results.iter() {
        let entry = by_codec.entry(result.codec.clone()).or_default();
        entry.0 += result.audio_sec;
        entry.1 += result.wall_sec;
        entry.2 += 1;
    }
    let mut rows: Vec<_> = by_codec.into_iter().collect();
    rows.sort_by_key(|row| std::cmp::Reverse(row.1 .2));
    println!("\n按编码：");
    for (codec, (audio, cpu, count)) in rows {
        println!("  {codec:<8} {count:>4} 首 · {:.0}x 实时", audio / cpu);
    }
}

fn parse_args() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let mut inputs = Vec::new();
    let mut limit = None;
    let mut jobs = 1usize;
    let mut loudness = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--limit" => limit = Some(parse_positive(args.next(), "--limit")?),
            "--jobs" => jobs = parse_positive(args.next(), "--jobs")?,
            "--loudness" => loudness = true,
            option if option.starts_with("--") => return Err(format!("未知选项：{option}")),
            other => inputs.push(PathBuf::from(other)),
        }
    }

    if inputs.is_empty() {
        return Err("没有给出文件或目录".into());
    }
    Ok(Options {
        inputs,
        limit,
        jobs,
        loudness,
    })
}

fn parse_positive(value: Option<String>, option: &str) -> Result<usize, String> {
    let raw = value.ok_or_else(|| format!("{option} 缺少数值"))?;
    let value = raw
        .parse::<usize>()
        .map_err(|_| format!("{option} 需要正整数，收到：{raw}"))?;
    if value == 0 {
        return Err(format!("{option} 必须大于 0"));
    }
    Ok(value)
}

fn usage() {
    eprintln!("用法：bench_decode <文件或目录>… [--limit N] [--jobs N] [--loudness]");
}

/// 完整处理一个文件。计时包括打开、解码，以及可选的响度和真峰值计算。
fn analyze_file(path: &Path, measure_loudness: bool) -> Result<FileResult, String> {
    let started = Instant::now();
    let mut decoder = Decoder::open(path).map_err(|e| e.to_string())?;
    let codec = decoder.spec().codec.clone();
    let rate = decoder.spec().sample_rate;
    let layout = decoder.spec().layout;
    let channels = layout.count();
    // 布局不支持时 `new` 给 None；收尾时必须把它还原成明确的 UnsupportedLayout，
    // 不能与「没有开启响度模式」共用同一个最终状态。
    let mut loudness_analyzer = match measure_loudness {
        true => LoudnessAnalyzer::new(layout, rate).map_err(|e| e.to_string())?,
        false => None,
    };

    let mut buf = Vec::new();
    let mut samples = 0u64;
    loop {
        buf.clear();
        match decoder.next_frames(&mut buf) {
            Ok(true) => {
                samples += buf.len() as u64;
                if let Some(analyzer) = loudness_analyzer.as_mut() {
                    analyzer.feed(&buf).map_err(|e| e.to_string())?;
                }
            }
            Ok(false) => break,
            Err(e) => return Err(e.to_string()),
        }
    }

    let loudness = finish_loudness(measure_loudness, loudness_analyzer.as_ref())?;
    let audio_sec = samples as f64 / channels.max(1) as f64 / rate as f64;
    Ok(FileResult {
        path: path.to_path_buf(),
        audio_sec,
        wall_sec: started.elapsed().as_secs_f64(),
        codec,
        loudness,
    })
}

/// 把测量器状态收敛成结果。
///
/// `None` 有两种来源：未开启响度模式，或布局不受支持。必须由 `measure_loudness`
/// 区分，否则后一种会在汇总阶段被误当成「响度模式没有结果」并触发 panic。
fn finish_loudness(
    measure_loudness: bool,
    analyzer: Option<&LoudnessAnalyzer>,
) -> Result<Option<LoudnessOutcome>, String> {
    match (measure_loudness, analyzer) {
        (false, _) => Ok(None),
        (true, Some(analyzer)) => analyzer.finish().map(Some).map_err(|e| e.to_string()),
        (true, None) => Ok(Some(LoudnessOutcome::UnsupportedLayout)),
    }
}

fn print_loudness_summary(results: &[FileResult]) {
    let mut measured = 0usize;
    let mut unmeasurable = 0usize;
    let mut unmeasurable_paths: Vec<&Path> = Vec::new();
    let mut unsupported = 0usize;
    let mut unsupported_paths: Vec<&Path> = Vec::new();
    let mut loudness_min = f64::INFINITY;
    let mut loudness_max = f64::NEG_INFINITY;
    let mut peak_max = f64::NEG_INFINITY;
    let mut gain_min = f64::INFINITY;
    let mut gain_max = f64::NEG_INFINITY;

    for result in results {
        match result.loudness.as_ref().expect("响度模式必有分析结果") {
            outcome @ LoudnessOutcome::Measured {
                integrated_lufs,
                true_peak_dbtp,
            } => {
                // 增益由模块按同一公式现算，不再随结果一起存——目标响度属播放策略，
                // 改策略应能立即重算而不必重新解码（见实现计划「结果的存储与失效」）。
                let applied_gain_db = &outcome.gain_db();
                measured += 1;
                loudness_min = loudness_min.min(*integrated_lufs);
                loudness_max = loudness_max.max(*integrated_lufs);
                peak_max = peak_max.max(*true_peak_dbtp);
                gain_min = gain_min.min(*applied_gain_db);
                gain_max = gain_max.max(*applied_gain_db);
            }
            LoudnessOutcome::Unmeasurable => {
                unmeasurable += 1;
                unmeasurable_paths.push(result.path.as_path());
            }
            // 布局不支持是**确定状态**，不是失败：多声道要等平台原生后端，
            // 混进「不可测」会让那个数字说不清到底是曲目太短还是我们暂时做不了。
            LoudnessOutcome::UnsupportedLayout => {
                unsupported += 1;
                unsupported_paths.push(result.path.as_path());
            }
        }
    }

    println!("响度可测 {measured} 首 · 不可测 {unmeasurable} 首 · 布局不支持 {unsupported} 首");
    // 逐个列出：这类文件要么真静音、要么触到了门限的边界条件，值得逐个看过
    // 再决定 `unmeasurable` 该不该缓存成永久结论。
    for path in &unmeasurable_paths {
        println!("  不可测  {}", path.display());
    }
    for path in &unsupported_paths {
        println!("  布局不支持  {}", path.display());
    }
    if measured > 0 {
        println!(
            "响度 {loudness_min:.1}…{loudness_max:.1} LUFS · 最高真峰值 {peak_max:.1} dBTP · 应用增益 {gain_min:.1}…{gain_max:.1} dB"
        );
    }
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in WalkDir::new(dir).follow_links(false) {
        let entry = entry.map_err(|e| format!("遍历 {} 失败：{e}", dir.display()))?;
        if entry.file_type().is_file() && has_audio_ext(entry.path()) {
            out.push(entry.into_path());
        }
    }
    Ok(())
}

fn has_audio_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXT.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shannon_audio::ChannelLayout;

    #[test]
    fn gain_reaches_target_when_peak_has_headroom() {
        assert_eq!(loudness::applied_gain_db(-20.0, -6.0), 2.0);
    }

    #[test]
    fn gain_is_reduced_instead_of_limiting_peaks() {
        assert_eq!(loudness::applied_gain_db(-24.0, -0.5), -0.5);
    }

    #[test]
    fn unsupported_layout_is_preserved_as_an_outcome() {
        let analyzer = LoudnessAnalyzer::new(ChannelLayout::discrete(6), 48_000).unwrap();
        assert!(analyzer.is_none(), "多声道不应创建测量器");
        assert_eq!(
            finish_loudness(true, analyzer.as_ref()).unwrap(),
            Some(LoudnessOutcome::UnsupportedLayout),
            "布局不支持是确定结论，不能丢成 None"
        );
    }

    #[test]
    fn silence_is_unmeasurable_instead_of_infinite_gain() {
        let mut analyzer = LoudnessAnalyzer::new(ChannelLayout::STEREO, 44_100)
            .unwrap()
            .unwrap();
        analyzer.feed(&vec![0.0; 44_100 * 2]).unwrap();
        assert!(matches!(
            finish_loudness(true, Some(&analyzer)).unwrap(),
            Some(LoudnessOutcome::Unmeasurable)
        ));
    }
}
