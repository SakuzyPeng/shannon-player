//! 生成格式矩阵测试语料：`cargo run -p shannon-audio --example make_corpus`
//!
//! 开发期工具（架构约束允许创建子进程的场景之一），产物**不入库**——
//! 几 MB 的音频二进制进 git 没有意义，而且它完全可复现。
//!
//! ## 为什么需要它
//!
//! 引擎启用了 ALAC / AAC / FLAC / MP3 / Vorbis 等一串解码器，但纯 Rust 能现造的
//! 只有 PCM 语料，于是自动化测试长期只覆盖 WAV 一种——**承诺面比验证面大得多**。
//! 编码器不必自己实现，用 ffmpeg 从同一份源转出整个矩阵即可，
//! 于是「支持某格式」这句话才有回归保护，而不是靠某次手动试听的记忆。
//!
//! ## 源信号是扫频而非定频
//!
//! 定频正弦每 100 帧重复一次相位，位置差整数个周期时波形完全重合——
//! 这曾把一个 off-by-one 伪装成「误差为零」。凡是要断言位置或对齐的语料一律用扫频。

use std::path::{Path, PathBuf};
use std::process::Command;

const RATE: u32 = 44_100;
const SECONDS: f64 = 2.0;

/// 一个目标格式。`lossless` 决定测试用逐样本比对还是只看统计量。
struct Target {
    name: &'static str,
    file: &'static str,
    args: &'static [&'static str],
    lossless: bool,
}

const TARGETS: &[Target] = &[
    Target {
        name: "flac",
        file: "stereo.flac",
        args: &["-c:a", "flac"],
        lossless: true,
    },
    Target {
        name: "alac",
        file: "stereo_alac.m4a",
        args: &["-c:a", "alac"],
        lossless: true,
    },
    Target {
        name: "aiff",
        file: "stereo.aiff",
        args: &["-c:a", "pcm_s16be"],
        lossless: true,
    },
    Target {
        name: "caf",
        file: "stereo.caf",
        args: &["-c:a", "pcm_s16le", "-f", "caf"],
        lossless: true,
    },
    Target {
        name: "flac-in-mka",
        file: "stereo_flac.mka",
        args: &["-c:a", "flac", "-f", "matroska"],
        lossless: true,
    },
    Target {
        name: "aac",
        file: "stereo_aac.m4a",
        args: &["-c:a", "aac", "-b:a", "192k"],
        lossless: false,
    },
    Target {
        name: "mp3",
        file: "stereo.mp3",
        args: &["-c:a", "libmp3lame", "-b:a", "256k"],
        lossless: false,
    },
    Target {
        name: "vorbis",
        file: "stereo.ogg",
        args: &["-c:a", "vorbis", "-strict", "-2", "-b:a", "192k"],
        lossless: false,
    },
    // Opus 两个容器都要：格式表承诺的是「Opus / WebM / Matroska」，而解复用与解码
    // 是两回事——ogg 那条走 Symphonia 的 OggReader，webm 走 MkvReader，
    // 只测一个容器等于只验了一半。用 libopus 编码器，ffmpeg 自带的 `opus` 是实验性的。
    Target {
        name: "opus-ogg",
        file: "stereo_opus.opus",
        args: &["-c:a", "libopus", "-b:a", "128k"],
        lossless: false,
    },
    Target {
        name: "opus-webm",
        file: "stereo_opus.webm",
        args: &["-c:a", "libopus", "-b:a", "128k", "-f", "webm"],
        lossless: false,
    },
    // 单声道与 48 kHz 各留一份：前者验证上混，后者让「设备采样率恰好匹配」
    // 也有真实编码格式可用，不至于只能拿 PCM 试。
    Target {
        name: "mono-flac",
        file: "mono.flac",
        args: &["-c:a", "flac", "-ac", "1"],
        lossless: true,
    },
    Target {
        name: "48k-flac",
        file: "stereo_48k.flac",
        args: &["-c:a", "flac", "-ar", "48000"],
        lossless: true,
    },
];

fn chirp(i: usize) -> f64 {
    let t = i as f64 / RATE as f64;
    let (f0, f1, span) = (200.0, 4000.0, 4.0);
    let phase = 2.0 * std::f64::consts::PI * (f0 * t + (f1 - f0) * t * t / (2.0 * span));
    0.3 * phase.sin()
}

fn write_source(path: &Path) {
    let frames = (RATE as f64 * SECONDS) as usize;
    let channels = 2u16;
    let data_len = (frames * channels as usize * 2) as u32;
    let mut buf = Vec::with_capacity(44 + data_len as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_len).to_le_bytes());
    buf.extend_from_slice(b"WAVEfmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&channels.to_le_bytes());
    buf.extend_from_slice(&RATE.to_le_bytes());
    buf.extend_from_slice(&(RATE * channels as u32 * 2).to_le_bytes());
    buf.extend_from_slice(&(channels * 2).to_le_bytes());
    buf.extend_from_slice(&16u16.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        let v = (chirp(i) * i16::MAX as f64) as i16;
        for _ in 0..channels {
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }
    std::fs::write(path, buf).expect("写源语料失败");
}

fn main() {
    let dir = corpus_dir();
    std::fs::create_dir_all(&dir).expect("建语料目录失败");

    if Command::new("ffmpeg").arg("-version").output().is_err() {
        eprintln!("找不到 ffmpeg。装一个（macOS：brew install ffmpeg）后重跑。");
        std::process::exit(1);
    }

    let source = dir.join("source.wav");
    write_source(&source);
    println!("源  source.wav（扫频 200→4000 Hz，{SECONDS} 秒，{RATE} Hz 立体声）");

    let mut failed = Vec::new();
    for target in TARGETS {
        let out = dir.join(target.file);
        let _ = std::fs::remove_file(&out);
        let status = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(&source)
            .args(target.args)
            .arg(&out)
            .status();

        match status {
            Ok(s) if s.success() && out.exists() => {
                let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
                println!(
                    "  {:<14} {:<20} {:>7} KB{}",
                    target.name,
                    target.file,
                    size / 1024,
                    if target.lossless {
                        "  无损"
                    } else {
                        "  有损"
                    }
                );
            }
            _ => {
                // 某个编码器缺失不该让整个矩阵失败：其余格式仍然可用，
                // 测试侧按「文件在就测」处理，缺哪个一目了然。
                println!(
                    "  {:<14} 生成失败（编码器可能未编入本机 ffmpeg）",
                    target.name
                );
                failed.push(target.name);
            }
        }
    }

    println!("\n语料目录 {}", dir.display());
    if !failed.is_empty() {
        println!("缺失 {} 项：{}", failed.len(), failed.join(", "));
    }
    println!("现在可跑 cargo test -p shannon-audio --test format_matrix");
}

/// 语料目录：`audio/tests/corpus/`，已在 .gitignore 里。
fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("corpus")
}
