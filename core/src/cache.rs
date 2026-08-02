//! 扫描缓存：一次扫描的**原始**产出，未经聚合、未套用用户覆盖。
//!
//! 为什么要把它单独存下来，而不是只留聚合好的 `LibrarySnapshot`：
//!
//! - **改一次元数据不该重扫整库**。用户改了专辑艺人后曲目要重新归组，归组依据
//!   （原始标签、封面指纹、路径）在聚合后的快照里已经丢失了，只能回头重读文件。
//!   留着原始数据，重新聚合是纯内存计算，毫秒级。
//! - **重启不该重扫**。缓存落盘后，下次启动读缓存 + 套覆盖即可还原曲库。
//! - **探测器升级后可增量重扫**：`probe_version` 落后的条目才需要重读文件。
//!
//! 缓存里**不存封面字节**，只存指纹：几百张图会让缓存文件膨胀到几十兆，
//! 而归组只需要判断「是不是同一张」。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::AudioFormat;

/// 从文件读到的原始标签，不含任何兜底推断。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RawTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub track_no: Option<u16>,
    pub disc_no: Option<u16>,
}

/// 单个文件的原始探测结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawTrack {
    /// 稳定曲目 ID（内容哈希），也是覆盖层的键。
    pub id: String,
    pub path: PathBuf,
    pub tags: RawTags,
    /// 内嵌封面的内容指纹（不存图本身）。
    pub cover_key: Option<String>,
    pub has_cover: bool,
    pub format: AudioFormat,
    pub duration_sec: f64,
}

/// 一次扫描的完整原始产出。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScanCache {
    /// 扫描过的根目录，重启后据此重扫。
    pub roots: Vec<PathBuf>,
    pub tracks: Vec<RawTrack>,
    /// 遍历到但无法解析的文件数。
    pub failed: u32,
    /// 内嵌封面解码失败的张数（按封面指纹计，不是曲目数）。这些曲目仍会入库，
    /// 只是回落到占位渐变——但不静默，如实计数。
    #[serde(default)]
    pub cover_failed: u32,
}

impl ScanCache {
    /// 套用覆盖并聚合成前端要的曲库快照。纯内存计算。
    pub fn library(
        &self,
        overrides: &crate::overrides::Overrides,
    ) -> crate::model::LibrarySnapshot {
        let mut snap = crate::scan::aggregate(&self.tracks, &self.roots, overrides);
        snap.failed = self.failed;
        snap
    }

    /// 读旧的 JSON 缓存。**只剩迁移一个用途**——落盘现在走
    /// [`crate::db::LibraryDb`]，这里保留是为了把 0.1 时期的 `library-cache.json`
    /// 搬进数据库（见 [`crate::db::LibraryDb::import_legacy_json`]）。
    ///
    /// 文件不存在返回空（首次运行），内容损坏也返回空——缓存是可重建的派生数据，
    /// 丢了大不了重扫一次，不必像覆盖层那样留残骸。
    pub fn load(path: &Path) -> std::io::Result<Self> {
        Ok(Self::load_legacy(path)?.unwrap_or_default())
    }

    /// 迁移时还要区分「没有旧文件 / 内容损坏」与「旧缓存合法但恰好没有曲目」：
    /// 后者仍可能带着扫描根与失败统计，不能因为 `tracks` 为空就整份丢掉。
    pub(crate) fn load_legacy(path: &Path) -> std::io::Result<Option<Self>> {
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        Ok(serde_json::from_str(&raw).ok())
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }
}
