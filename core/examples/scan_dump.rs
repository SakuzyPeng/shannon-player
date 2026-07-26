//! 手动验证工具：扫描一个目录并打印完整规格。
//! 用法：cargo run -p shannon-core --example scan_dump -- <目录>
use shannon_core::overrides::Overrides;

fn main() {
    let dir = std::env::args().nth(1).expect("用法: scan_dump <目录>");
    let cache = shannon_core::scan::scan_folders(&[dir.into()], None, |p| {
        if p.current.is_empty() {
            println!("[进度] 完成 {}/{}  曲目={} 专辑={}", p.done, p.total, p.tracks, p.albums);
        }
    });
    // 聚合与套用覆盖是独立的纯内存步骤；这里用空覆盖看「未经用户修改」的判断结果。
    let snap = cache.library(&Overrides::default());
    println!(
        "\n专辑 {} 张 / 曲目 {} 首 / 解析失败 {} / 折叠重复 {} 首",
        snap.albums.len(),
        snap.tracks.len(),
        snap.failed,
        snap.duplicates
    );
    for a in &snap.albums {
        println!(
            "\n■ {} — {}{} ({} 首, {:.1}s)  艺人来源={:?}  id={}",
            a.title,
            a.artist,
            if a.compilation { " [合辑]" } else { "" },
            a.track_count,
            a.duration_sec,
            a.artist_source,
            a.id
        );
    }
    for t in &snap.tracks {
        let f = &t.format;
        println!(
            "\n· {}\n    id={}\n    容器={} 编码={} {:?} {}Hz {:?}bit {}ch mask={:?}\n    布局={:?}\n    空间={:?} 无损={:?}\n    来源={:?}\n    notes={:?}",
            t.title, t.id, f.container, f.codec, f.encoding, f.sample_rate_hz, f.bit_depth,
            f.channels, f.channel_mask, f.channel_layout, f.spatial, f.lossless, t.sources,
            f.probe_notes
        );
    }
}
