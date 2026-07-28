//! 生成多格式实测歌单：`cargo run -p shannon-audio --example make_playlist -- <源音频> [选项]`
//!
//! 与 `make_corpus` 的分工：那个产的是**给断言用的**扫频语料，两秒、听不出所以然；
//! 这个产的是**给耳朵用的**——同一段真实音乐转成各种格式，连续放一遍，
//! 听爆音、听切歌间隙、听重采样有没有劣化。自动化测试证明不了「听着对」。
//!
//! 选项：
//!   --seconds N   每首截取 N 秒（默认 45；给 0 表示整首）
//!   --start N     从源的第 N 秒开始截（默认 30，跳过前奏，直接进有内容的段落）
//!   --out DIR     输出目录（默认 audio/playlist/，已在 .gitignore 里）
//!
//! ## 采样率那三档是特意留的
//!
//! 44.1 / 48 / 96 kHz 各一份无损。设备工作在 48 kHz 时，44.1 那份要经重采样、
//! 48 那份直通——**两者 AB 对比就能听出重采样有没有劣化**，这是最该用耳朵而不是
//! 断言去验的一项。

use std::path::{Path, PathBuf};
use std::process::Command;

/// 一个输出档位。`note` 会印在清单里，说明这一首是用来听什么的。
struct Item {
    order: u8,
    file: &'static str,
    args: &'static [&'static str],
    note: &'static str,
}

const ITEMS: &[Item] = &[
    Item {
        order: 1,
        file: "01-flac-44k.flac",
        args: &["-c:a", "flac", "-ar", "44100"],
        note: "无损基准，听感的参照点",
    },
    Item {
        order: 2,
        file: "02-alac-44k.m4a",
        args: &["-c:a", "alac", "-ar", "44100"],
        note: "曲库主力格式，应与 01 无法区分",
    },
    Item {
        order: 3,
        file: "03-wav-44k.wav",
        args: &["-c:a", "pcm_s16le", "-ar", "44100"],
        note: "裸 PCM，排除容器解析的影响",
    },
    Item {
        order: 4,
        file: "04-aiff-44k.aiff",
        args: &["-c:a", "pcm_s16be", "-ar", "44100"],
        note: "大端 PCM，字节序搞反会是明显的噪声",
    },
    Item {
        order: 5,
        file: "05-caf-44k.caf",
        args: &["-c:a", "pcm_s16le", "-f", "caf", "-ar", "44100"],
        note: "CAF 容器",
    },
    Item {
        order: 6,
        file: "06-aac-256k.m4a",
        args: &["-c:a", "aac", "-b:a", "256k", "-ar", "44100"],
        note: "有损，注意开头结尾有无咔哒（编码器延迟裁剪）",
    },
    Item {
        order: 7,
        file: "07-mp3-320k.mp3",
        args: &["-c:a", "libmp3lame", "-b:a", "320k", "-ar", "44100"],
        note: "有损，同上",
    },
    Item {
        order: 8,
        file: "08-vorbis-q6.ogg",
        args: &["-c:a", "vorbis", "-strict", "-2", "-q:a", "6", "-ar", "44100"],
        note: "有损",
    },
    Item {
        order: 9,
        file: "09-flac-48k.flac",
        args: &["-c:a", "flac", "-ar", "48000"],
        note: "48k 无损：设备也是 48k 时这首直通，与 01 对比即可听出重采样有无劣化",
    },
    Item {
        order: 10,
        file: "10-flac-96k.flac",
        args: &["-c:a", "flac", "-ar", "96000"],
        note: "96k 无损：设备不支持时走下采样",
    },
    Item {
        order: 11,
        file: "11-flac-24bit-44k.flac",
        args: &["-c:a", "flac", "-sample_fmt", "s32", "-ar", "44100"],
        note: "24 bit：位深转换错误会表现为底噪或削顶",
    },
];

fn main() {
    let mut args = std::env::args().skip(1);
    let mut source: Option<PathBuf> = None;
    let mut seconds = 45.0f64;
    let mut start = 30.0f64;
    let mut out: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seconds" => seconds = args.next().and_then(|v| v.parse().ok()).unwrap_or(45.0),
            "--start" => start = args.next().and_then(|v| v.parse().ok()).unwrap_or(30.0),
            "--out" => out = args.next().map(PathBuf::from),
            other => source = Some(PathBuf::from(other)),
        }
    }

    let Some(source) = source else {
        eprintln!("用法：make_playlist <源音频> [--seconds N] [--start N] [--out DIR]");
        eprintln!("源随便挑一首自己熟的立体声曲目——听不熟的曲子听不出差别。");
        std::process::exit(2);
    };
    if !source.exists() {
        eprintln!("源文件不存在：{}", source.display());
        std::process::exit(1);
    }
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        eprintln!("找不到 ffmpeg（macOS：brew install ffmpeg）。");
        std::process::exit(1);
    }

    let dir = out.unwrap_or_else(default_dir);
    std::fs::create_dir_all(&dir).expect("建歌单目录失败");

    // 源若不是立体声就先降到 2 声道：这份歌单是给立体声路径用的，
    // 多声道要走平台原生后端，不在这条链路上。
    let channels = probe_channels(&source);
    if channels != 2 {
        println!("提示：源是 {channels} 声道，将按立体声转出（多声道走的是另一条后端）");
    }

    println!("源  {}", source.display());
    if seconds > 0.0 {
        println!("截取 第 {start} 秒起 {seconds} 秒\n");
    } else {
        println!("整首\n");
    }

    let mut made = Vec::new();
    for item in ITEMS {
        let out_path = dir.join(item.file);
        let _ = std::fs::remove_file(&out_path);

        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-hide_banner", "-loglevel", "error", "-y"]);
        if seconds > 0.0 {
            cmd.args(["-ss", &start.to_string(), "-t", &seconds.to_string()]);
        }
        // 只取第一条音频轨：曲库文件普遍内嵌封面，那是一条视频流，
        // 不排除的话 ffmpeg 会试图把它一起塞进输出容器，m4a 与 ogg 会直接失败。
        cmd.arg("-i")
            .arg(&source)
            .args(["-map", "0:a:0", "-vn", "-ac", "2"])
            .args(item.args)
            .arg(&out_path);

        match cmd.status() {
            Ok(s) if s.success() && out_path.exists() => {
                let size = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
                println!("  {:>2}. {:<24} {:>6} KB  {}", item.order, item.file, size / 1024, item.note);
                made.push(item.file);
            }
            _ => println!("  {:>2}. {:<24} 生成失败（编码器可能不可用）", item.order, item.file),
        }
    }

    println!("\n歌单目录 {}", dir.display());
    println!("连续播放：cargo run --release -p shannon-audio --example play -- {}", dir.display());
    println!("\n听的时候留意：");
    println!("  · 01 / 02 / 03 / 04 / 05 应当完全一致——无损之间听得出差别就是解码有问题");
    println!("  · 01 与 09 的对比是重采样质量（设备为 48k 时 01 走重采样、09 直通）");
    println!("  · 每首开头结尾有无咔哒声（音量斜坡与编码器延迟裁剪）");
    println!("  · 换曲之间的间隙——当前会重建输出流，gapless 是后面阶段的事");
}

/// 读源的声道数。读不到就按立体声处理，不为一个提示信息中断整个流程。
fn probe_channels(path: &Path) -> u32 {
    Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=channels", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(2)
}

fn default_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("playlist")
}
