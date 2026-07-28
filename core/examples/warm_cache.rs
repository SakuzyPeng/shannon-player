//! 预热扫描缓存：扫描目录并把原始缓存写到指定路径。
//!
//! 用途是端到端验证「重启免重扫」——把缓存写进应用数据目录
//! （macOS: ~/Library/Application Support/com.shannon.player/library-cache.json）
//! 再启动应用，就能在不点任何按钮的情况下检查恢复路径。
//!
//! 用法：cargo run -p shannon-core --example warm_cache -- <音乐目录> <缓存文件路径>
use shannon_core::overrides::Overrides;

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args
        .next()
        .expect("用法: warm_cache <音乐目录> <缓存文件路径>");
    let out = args
        .next()
        .expect("用法: warm_cache <音乐目录> <缓存文件路径>");
    // 封面缩略图写到缓存文件同级的 covers/，与应用运行时的布局一致。
    let out_path = std::path::PathBuf::from(&out);
    let covers = out_path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("covers");
    let cache = shannon_core::scan::scan_folders(&[dir.into()], Some(&covers), |_| {});
    cache.save(&out_path).expect("写入缓存失败");

    // 顺带导出聚合后的快照：浏览器预览没有后端，把它灌进 store 就能用真实曲库调 UI
    // （用法见 src/main.tsx 的 __shannon）。
    let snapshot = cache.library(&Overrides::default());
    let snap_path = out_path.with_file_name("library-snapshot.json");
    std::fs::write(
        &snap_path,
        serde_json::to_string(&snapshot).expect("序列化失败"),
    )
    .expect("写入快照失败");

    println!(
        "已写入 {} 首曲目 / {} 张专辑 → {out}\n封面 → {}（解码失败 {} 张）\n预览快照 → {}",
        cache.tracks.len(),
        snapshot.albums.len(),
        covers.display(),
        cache.cover_failed,
        snap_path.display()
    );
}
