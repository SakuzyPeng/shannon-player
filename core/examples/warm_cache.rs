//! 预热扫描缓存：扫描目录并把原始缓存写进曲库数据库。
//!
//! 用途是端到端验证「重启免重扫」——把缓存写进应用数据目录
//! （macOS: ~/Library/Application Support/com.shannon.player/library.db）
//! 再启动应用，就能在不点任何按钮的情况下检查恢复路径。
//!
//! **只替换扫描缓存，不动 `track_override`**（与应用里重扫的语义一致）：拿它预热一个
//! 真在用的数据库时，用户手改过的元数据不会被这一趟冲掉。
//!
//! **走的是增量路径**（`scan_folders_incremental`），与外壳的 `scan_library` 同一条：
//! 库里已有缓存时先读出来交给扫描器，没变过的文件不再打开。这也让它成为量增量效果的
//! 工具——末尾会打印本轮耗时，`scan` 模块另会在 stderr 上报复用比例。示例若停在全量
//! 扫描上，「预热一次再重扫」这条最该被验证的路径反而永远走不到。
//!
//! 用法：cargo run -p shannon-core --example warm_cache -- <音乐目录> <数据库路径>
use shannon_core::db::LibraryDb;
use shannon_core::overrides::Overrides;

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args
        .next()
        .expect("用法: warm_cache <音乐目录> <数据库路径>");
    let out = args
        .next()
        .expect("用法: warm_cache <音乐目录> <数据库路径>");
    // 封面缩略图写到数据库同级的 covers/，与应用运行时的布局一致。
    let out_path = std::path::PathBuf::from(&out);
    let covers = out_path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("covers");
    // 先开库再扫描：上一轮的缓存要交给扫描器当增量依据。读不出来（新库、schema 太新）
    // 就按没有处理，退化成一次全量扫描。
    let (mut db, report) = LibraryDb::open(&out_path).expect("打开数据库失败");
    if let Some(backup) = report.corrupt_backup {
        eprintln!("原数据库损坏，残骸保留在 {}", backup.display());
    }
    let previous = db.load_cache().unwrap_or_else(|e| {
        eprintln!("读取上一轮缓存失败，本次全量扫描: {e}");
        Default::default()
    });
    let started = std::time::Instant::now();
    let cache = shannon_core::scan::scan_folders_incremental(
        &[dir.into()],
        Some(&covers),
        Some(&previous),
        |_| {},
    );
    let elapsed = started.elapsed();
    db.replace_cache(&cache).expect("写入缓存失败");

    // 顺带导出聚合后的快照：浏览器预览没有后端，把它灌进 store 就能用真实曲库调 UI
    // （用法见 src/main.tsx 的 __shannon）。套用库里已有的覆盖，预览才与应用里看到的一致。
    let overrides = db.load_overrides().unwrap_or_else(|e| {
        eprintln!("读取元数据修改失败，预览快照按未修改导出: {e}");
        Overrides::default()
    });
    let snapshot = cache.library(&overrides);
    let snap_path = out_path.with_file_name("library-snapshot.json");
    std::fs::write(
        &snap_path,
        serde_json::to_string(&snapshot).expect("序列化失败"),
    )
    .expect("写入快照失败");

    println!(
        "已写入 {} 首曲目 / {} 张专辑 → {out}\n扫描耗时 {:.2} 秒（上一轮缓存 {} 条）\n封面 → {}（解码失败 {} 张）\n预览快照 → {}",
        cache.tracks.len(),
        snapshot.albums.len(),
        elapsed.as_secs_f64(),
        previous.tracks.len(),
        covers.display(),
        cache.cover_failed,
        snap_path.display()
    );
}
