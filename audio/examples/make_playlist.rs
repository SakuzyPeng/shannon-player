//! 生成多格式实测歌单：`cargo run -p shannon-audio --example make_playlist -- <源音频> [选项]`
//!
//! 与 `make_corpus` 的分工：那个产的是**给断言用的**扫频语料，两秒、听不出所以然；
//! 这个产的是**给耳朵用的**——同一段真实音乐转成各种格式，连续放一遍，
//! 听爆音、听切歌间隙、听重采样有没有劣化。自动化测试证明不了「听着对」。
//!
//! 选项：
//!   --seconds N   每首截取 N 秒（默认 45；给 0 表示整首）
//!   --start N     从源的第 N 秒开始截（默认 30，跳过前奏，直接进有内容的段落）
//!   --album 名称  专辑名（默认「格式实测」，曲目在应用里会聚成这张专辑）
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
    /// 曲目标题，**必须逐首不同**——理由见下方 `main` 里写元数据那段。
    /// 它同时用作文件名，因为 WAV / CAF 的标签支持很弱，扫描器读不到时要靠文件名兜底。
    title: &'static str,
    ext: &'static str,
    args: &'static [&'static str],
    note: &'static str,
}

const ITEMS: &[Item] = &[
    Item {
        order: 1,
        title: "01 FLAC 44.1k 16bit",
        ext: "flac",
        args: &["-c:a", "flac", "-ar", "44100"],
        note: "无损基准，听感的参照点",
    },
    Item {
        order: 2,
        title: "02 ALAC 44.1k 16bit",
        ext: "m4a",
        args: &["-c:a", "alac", "-ar", "44100"],
        note: "曲库主力格式，应与 01 无法区分",
    },
    Item {
        order: 3,
        title: "03 WAV 44.1k 16bit",
        ext: "wav",
        args: &["-c:a", "pcm_s16le", "-ar", "44100"],
        note: "裸 PCM，排除容器解析的影响",
    },
    Item {
        order: 4,
        title: "04 AIFF 44.1k 16bit",
        ext: "aiff",
        args: &["-c:a", "pcm_s16be", "-ar", "44100"],
        note: "大端 PCM，字节序搞反会是明显的噪声",
    },
    Item {
        order: 5,
        title: "05 CAF 44.1k 16bit",
        ext: "caf",
        args: &["-c:a", "pcm_s16le", "-f", "caf", "-ar", "44100"],
        note: "CAF 容器",
    },
    Item {
        order: 6,
        title: "06 AAC 256k",
        ext: "m4a",
        args: &["-c:a", "aac", "-b:a", "256k", "-ar", "44100"],
        note: "有损，注意开头结尾有无咔哒（编码器延迟裁剪）",
    },
    Item {
        order: 7,
        title: "07 MP3 320k",
        ext: "mp3",
        args: &["-c:a", "libmp3lame", "-b:a", "320k", "-ar", "44100"],
        note: "有损，同上",
    },
    Item {
        order: 8,
        title: "08 Vorbis q6",
        ext: "ogg",
        args: &[
            "-c:a", "vorbis", "-strict", "-2", "-q:a", "6", "-ar", "44100",
        ],
        note: "有损",
    },
    Item {
        order: 9,
        title: "09 FLAC 48k 16bit",
        ext: "flac",
        args: &["-c:a", "flac", "-ar", "48000"],
        note: "48k 无损：设备也是 48k 时这首直通，与 01 对比即可听出重采样有无劣化",
    },
    Item {
        order: 10,
        title: "10 FLAC 96k 16bit",
        ext: "flac",
        args: &["-c:a", "flac", "-ar", "96000"],
        note: "96k 无损：设备不支持时走下采样",
    },
    Item {
        order: 11,
        title: "11 FLAC 44.1k 24bit",
        ext: "flac",
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
    let mut album = String::from("格式实测");

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seconds" => seconds = args.next().and_then(|v| v.parse().ok()).unwrap_or(45.0),
            "--start" => start = args.next().and_then(|v| v.parse().ok()).unwrap_or(30.0),
            "--album" => album = args.next().unwrap_or_else(|| "格式实测".into()),
            "--out" => out = args.next().map(PathBuf::from),
            other => source = Some(PathBuf::from(other)),
        }
    }

    let Some(source) = source else {
        eprintln!(
            "用法：make_playlist <源音频> [--seconds N] [--start N] [--album 名称] [--out DIR]"
        );
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

    // 放进以专辑名命名的子目录：WAV 的 RIFF INFO 没有 album_artist 字段、
    // AIFF 与 CAF 的标签 ffmpeg 写不全或扫描器读不了，这些文件只能靠**目录名**兜底
    // 才会跟其余曲目归到同一张专辑；否则界面上会散成三张，实测过。
    let dir = out.unwrap_or_else(default_dir).join(&album);
    std::fs::create_dir_all(&dir).expect("建歌单目录失败");
    let artist = probe_tag(&source, "artist").unwrap_or_else(|| "格式实测".into());

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

    let total = ITEMS.len();
    let mut made = 0usize;
    for item in ITEMS {
        let file = format!("{}.{}", item.title, item.ext);
        let out_path = dir.join(&file);
        let _ = std::fs::remove_file(&out_path);

        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-hide_banner", "-loglevel", "error", "-y"]);
        if seconds > 0.0 {
            cmd.args(["-ss", &start.to_string(), "-t", &seconds.to_string()]);
        }
        cmd.arg("-i").arg(&source);
        // 只取第一条音频轨：曲库文件普遍内嵌封面，那是一条视频流，
        // 不排除的话 ffmpeg 会试图把它一起塞进输出容器，m4a 与 ogg 会直接失败。
        cmd.args(["-map", "0:a:0", "-vn", "-ac", "2"]);
        // 丢掉源标签再写自己的。**每首必须有不同的标题与音轨号**：
        // 这 11 个文件是同一段音乐的不同编码，时长一模一样，沿用源标题的话
        // 会正好命中扫描器的重复判据（相同标题 + 相同时长 / 相同轨位 + 相同时长），
        // 整批被折叠成一两首——实测过，10 个文件扫出来只剩 4 首。
        cmd.args(["-map_metadata", "-1"])
            .args(["-metadata", &format!("title={}", item.title)])
            .args(["-metadata", &format!("album={album}")])
            .args(["-metadata", &format!("artist={artist}")])
            // 刻意不写 album_artist：写了的话有标签的曲目按「标签专辑艺人」归组、
            // 读不到标签的按「所在目录」归组，作用域不同就会裂成两张专辑。
            // 全都缺失反而一致——统一由目录兜底。
            .args(["-metadata", &format!("track={}/{}", item.order, total)])
            .args(["-metadata", &format!("comment={}", item.note)]);
        // WAV / AIFF 的原生标签块字段极少（RIFF INFO 没有 album_artist，
        // AIFF 的 NAME/ANNO 只放得下标题与注释），音轨号会整个丢掉，
        // 界面上就表现为这几首排到末尾、序号还与前面重复。挂一份 ID3v2 才写得全。
        if matches!(item.ext, "wav" | "aiff") {
            cmd.args(["-write_id3v2", "1"]);
        }
        cmd.args(item.args).arg(&out_path);

        match cmd.status() {
            Ok(s) if s.success() && out_path.exists() => {
                let size = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
                println!("  {:<26} {:>6} KB  {}", file, size / 1024, item.note);
                made += 1;
            }
            _ => println!("  {file:<26} 生成失败（编码器可能不可用）"),
        }
    }

    println!("\n{made}/{total} 首 · 目录 {}", dir.display());
    println!("\n在应用里听（曲目会聚成专辑「{album}」）：");
    println!("  把上面这个目录加进设置页的音乐文件夹，重新扫描即可。");
    println!("  注意播放引擎尚未接入前端，界面上看得到、点得动，但还不会出声。");
    println!("\n在命令行听（引擎已经能放）：");
    println!(
        "  cargo run --release -p shannon-audio --example play -- {}",
        dir.display()
    );
    println!("\n听的时候留意：");
    println!("  · 01 / 02 / 03 / 04 / 05 应当完全一致——无损之间听得出差别就是解码有问题");
    println!("  · 01 与 09 的对比是重采样质量（设备为 48k 时 01 走重采样、09 直通）");
    println!("  · 每首开头结尾有无咔哒声（音量斜坡与编码器延迟裁剪）");
    println!("  · 换曲之间的间隙——当前会重建输出流，gapless 是后面阶段的事");
}

/// 读源的某个标签，用来让实测曲目保留「这是哪首歌」的线索。
fn probe_tag(path: &Path, tag: &str) -> Option<String> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            &format!("format_tags={tag}"),
            "-of",
            "default=nw=1:nk=1",
        ])
        .arg(path)
        .output()
        .ok()?;
    let value = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!value.is_empty()).then_some(value)
}

/// 读源的声道数。读不到就按立体声处理，不为一个提示信息中断整个流程。
fn probe_channels(path: &Path) -> u32 {
    Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=channels",
            "-of",
            "csv=p=0",
        ])
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
