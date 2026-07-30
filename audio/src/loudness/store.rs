//! 响度分析结果的存储：曲目 ID → 一次分析的结论。
//!
//! ## 键为什么是曲目 ID
//!
//! 曲目 ID 是「文件大小 + 跳过元数据区的三段内容 blake3」（见 `core/src/id.rs`），
//! 改标签、改文件名、移动文件都不变。响度是**音频内容**的属性，与该 ID 的语义正好
//! 对齐——分析一次长期复用，用户改元数据不会触发重算。
//!
//! ## 只存原始事实，不存最终增益
//!
//! 每条记录只有 `integrated_lufs`、`true_peak_dbtp` 与状态，**没有 `applied_gain_db`**。
//! 目标响度（-18 LUFS）与峰值上限（-1 dBTP）属于播放策略：改策略应当能立即重算增益，
//! 而不是把全库重新解码一遍。
//!
//! ## 两个版本号，管的不是同一件事
//!
//! - `schemaVersion` 在文件上，描述**这份文件长什么样**；不认识就整份丢弃。
//! - `analysisVersion` 在每条记录上，描述**这条结论是怎么测出来的**（`ebur128` 的版本
//!   与 feature、`Mode` 组合、真峰值算法、声道映射规则，任一变动就 +1，见
//!   [`super::ANALYSIS_VERSION`]）。版本不符视为没有结果，重测后原地覆盖。
//!
//! 分不开的话，只要测量器升级就得连带丢掉整份文件；而实际上大多数情况是逐条重测。
//!
//! ## 这份数据的性质介于缓存与覆盖层之间
//!
//! 像 `library-cache.json` 一样可重建，但重建代价是**全库解码一遍**。所以读失败时
//! 静默当空（后台会自己补回来，用户看不见），而不像元数据覆盖层那样留 `.corrupt`
//! 残骸——那份是用户手改的、丢了就真没了，这份不是。

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{LoudnessOutcome, ANALYSIS_VERSION};

/// 文件格式版本。**结构**不兼容地变化时 +1（换测量方式请改 [`super::ANALYSIS_VERSION`]）。
const SCHEMA_VERSION: u32 = 1;

/// 一条分析记录。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoudnessRecord {
    /// 产出这条结论时的分析版本。与当前版本不符即视为没有结果。
    pub analysis_version: u32,
    pub outcome: LoudnessOutcome,
}

/// 全库的分析结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoudnessStore {
    schema_version: u32,
    records: HashMap<String, LoudnessRecord>,
    /// 有没有未落盘的改动。不进 JSON——它描述的是内存与磁盘的关系，不是数据本身。
    #[serde(skip)]
    dirty: bool,
}

impl Default for LoudnessStore {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            records: HashMap::new(),
            dirty: false,
        }
    }
}

impl LoudnessStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 查一首曲目的结论。没测过、或测它的分析版本已过时，都返回 `None`。
    pub fn get(&self, track_id: &str) -> Option<LoudnessOutcome> {
        self.records
            .get(track_id)
            .filter(|record| record.analysis_version == ANALYSIS_VERSION)
            .map(|record| record.outcome)
    }

    /// 该乘到 PCM 上的线性增益。
    ///
    /// **未命中一律 1.0（不处理）**，与「测不出就 0 dB」是同一条：宁可不归一化，
    /// 也不拿猜的值去改音量。放在这里是为了让「未命中怎么办」只有一个答案，
    /// 而不是散落在每个调用点。
    pub fn linear_gain(&self, track_id: &str) -> f32 {
        self.get(track_id)
            .map(|outcome| outcome.linear_gain())
            .unwrap_or(1.0)
    }

    /// 记下一条结论（同一曲目重测则覆盖），并盖上当前分析版本。
    pub fn set(&mut self, track_id: impl Into<String>, outcome: LoudnessOutcome) {
        self.records.insert(
            track_id.into(),
            LoudnessRecord {
                analysis_version: ANALYSIS_VERSION,
                outcome,
            },
        );
        self.dirty = true;
    }

    /// 有未落盘的改动吗。分析是逐首完成的，据此可以攒一批再写，
    /// 而不是每测完一首就重写整份文件。
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// 记录条数（含分析版本已过时的）。
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// 读取。文件不存在、读不懂、或 schema 版本不认识，一律当作「还没分析过」。
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::new()),
            Err(e) => return Err(e),
        };
        let parsed: Self = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => return Ok(Self::new()),
        };
        if parsed.schema_version != SCHEMA_VERSION {
            return Ok(Self::new());
        }
        Ok(parsed)
    }

    /// 原子写入，避免写到一半断电留下半个 JSON。写成功后 `dirty` 清零。
    pub fn save(&mut self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_string(self)?)?;
        std::fs::rename(&tmp, path)?;
        self.dirty = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("shannon_loudness_{name}_{}", std::process::id()))
    }

    fn measured(lufs: f64, peak: f64) -> LoudnessOutcome {
        LoudnessOutcome::Measured {
            integrated_lufs: lufs,
            true_peak_dbtp: peak,
        }
    }

    #[test]
    fn records_survive_a_round_trip_through_disk() {
        let path = temp_path("roundtrip.json");
        let _ = std::fs::remove_file(&path);
        let mut store = LoudnessStore::new();
        store.set("t-measured", measured(-6.7, 3.3));
        store.set("t-unmeasurable", LoudnessOutcome::Unmeasurable);
        store.set("t-unsupported", LoudnessOutcome::UnsupportedLayout);
        store.save(&path).unwrap();

        let loaded = LoudnessStore::load(&path).unwrap();
        assert_eq!(loaded.get("t-measured"), Some(measured(-6.7, 3.3)));
        assert_eq!(loaded.get("t-unmeasurable"), Some(LoudnessOutcome::Unmeasurable));
        assert_eq!(
            loaded.get("t-unsupported"),
            Some(LoudnessOutcome::UnsupportedLayout)
        );
        assert_eq!(loaded.get("t-never-analyzed"), None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn infinite_peak_round_trips_as_null() {
        // serde_json 会把 -inf 静默写成 null，读回来却是解析错误——一条记录足以
        // 让整份结果作废，而重建它要把全库解码一遍。两个方向都得明确。
        let path = temp_path("infinite.json");
        let _ = std::fs::remove_file(&path);
        let mut store = LoudnessStore::new();
        store.set("t-1", measured(-20.0, f64::NEG_INFINITY));
        store.save(&path).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"truePeakDbtp\":null"), "实际写出 {text}");
        assert_eq!(
            LoudnessStore::load(&path).unwrap().get("t-1"),
            Some(measured(-20.0, f64::NEG_INFINITY)),
            "读回来必须还是 -inf，不能变成 0 或让整份文件报废"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn applied_gain_is_not_persisted() {
        // 目标响度与峰值上限是播放策略，改了应当立即重算；一旦把增益写进文件，
        // 改策略就得判断哪些记录是旧策略算的。
        let mut store = LoudnessStore::new();
        store.set("t-1", measured(-6.7, 3.3));
        let text = serde_json::to_string(&store).unwrap();
        assert!(!text.contains("ain"), "不该出现任何 gain 字段：{text}");
    }

    #[test]
    fn outdated_analysis_version_counts_as_a_miss() {
        // 文件一个字节没变，换个测量器版本真峰值也可能变——不能只靠曲目 ID 复用。
        let mut store = LoudnessStore::new();
        store.set("t-1", measured(-10.0, -2.0));
        store
            .records
            .get_mut("t-1")
            .unwrap()
            .analysis_version = ANALYSIS_VERSION + 1;

        assert_eq!(store.get("t-1"), None, "版本不符视为没有结果");
        assert_eq!(store.linear_gain("t-1"), 1.0, "未命中不处理");
        assert_eq!(store.len(), 1, "记录仍在，等着被重测覆盖");
    }

    #[test]
    fn unknown_schema_version_discards_the_whole_file() {
        let path = temp_path("schema.json");
        std::fs::write(
            &path,
            r#"{"schemaVersion":999,"records":{"t-1":{"analysisVersion":1,
               "outcome":{"state":"measured","integratedLufs":-10.0,"truePeakDbtp":-2.0}}}}"#,
        )
        .unwrap();
        let loaded = LoudnessStore::load(&path).unwrap();
        assert!(loaded.is_empty(), "不认识的结构不能半懂不懂地用");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupt_and_missing_files_are_both_just_empty() {
        // 这份数据可重建（代价是全库解码），所以读不懂就当空，让后台自己补回来；
        // 不像元数据覆盖层那样留 .corrupt 残骸——那份丢了就真没了。
        let missing = temp_path("nope.json");
        let _ = std::fs::remove_file(&missing);
        assert!(LoudnessStore::load(&missing).unwrap().is_empty());

        let corrupt = temp_path("corrupt.json");
        std::fs::write(&corrupt, "{ 这不是 JSON").unwrap();
        assert!(LoudnessStore::load(&corrupt).unwrap().is_empty());
        assert!(!corrupt.with_extension("corrupt").exists());
        let _ = std::fs::remove_file(&corrupt);
    }

    #[test]
    fn gain_comes_from_the_stored_facts_and_defaults_to_no_change() {
        let mut store = LoudnessStore::new();
        store.set("t-1", measured(-28.0, -20.0));
        // -28 LUFS 要 +10 dB，峰值 -20 dBTP 有余量：线性倍率 10^(10/20)。
        assert!((store.linear_gain("t-1") - 3.1622777).abs() < 1e-5);
        assert_eq!(store.linear_gain("t-unknown"), 1.0, "没分析过就别动音量");
        assert_eq!(
            store.linear_gain("t-1"),
            super::super::db_to_linear(10.0),
            "增益由存下来的事实现算，与公式同源"
        );
    }

    #[test]
    fn dirty_marks_pending_writes_only() {
        let path = temp_path("dirty.json");
        let _ = std::fs::remove_file(&path);
        let mut store = LoudnessStore::new();
        assert!(!store.is_dirty(), "新建时无待写内容");
        store.set("t-1", LoudnessOutcome::Unmeasurable);
        assert!(store.is_dirty());
        store.save(&path).unwrap();
        assert!(!store.is_dirty(), "落盘后不该再重写整份文件");
        let _ = std::fs::remove_file(&path);
    }
}
