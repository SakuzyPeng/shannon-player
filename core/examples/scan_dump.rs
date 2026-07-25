//! 手动验证工具：扫描一个目录并打印完整规格。
//! 用法：cargo run -p shannon-core --example scan_dump -- <目录>
fn main() {
    let dir = std::env::args().nth(1).expect("用法: scan_dump <目录>");
    let snap = shannon_core::scan::scan_folders(&[dir.into()], |p| {
        if p.current.is_empty() {
            println!("[进度] 完成 {}/{}  曲目={} 专辑={}", p.done, p.total, p.tracks, p.albums);
        }
    });
    println!("\n专辑 {} 张 / 曲目 {} 首 / 解析失败 {}", snap.albums.len(), snap.tracks.len(), snap.failed);
    for a in &snap.albums {
        println!("\n■ {} — {} ({} 首, {:.1}s)  id={}", a.title, a.artist, a.track_count, a.duration_sec, a.id);
    }
    for t in &snap.tracks {
        let f = &t.format;
        println!(
            "\n· {}\n    id={}\n    容器={} 编码={} {:?} {}Hz {:?}bit {}ch mask={:?}\n    布局={:?}\n    空间={:?} 无损={:?}\n    notes={:?}",
            t.title, t.id, f.container, f.codec, f.encoding, f.sample_rate_hz, f.bit_depth,
            f.channels, f.channel_mask, f.channel_layout, f.spatial, f.lossless, f.probe_notes
        );
    }
}
