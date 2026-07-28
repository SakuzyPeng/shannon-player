//! 用户对元数据的覆盖层。
//!
//! 为什么必须有：扫描在标签缺失时会用目录名、文件名**猜**歌手与专辑，猜错无可避免
//! （散装文件的父目录可能只是「下载」）；专辑艺人还要靠组内多数决推断，同样会错。
//! 只要存在兜底推断，就必须给用户改正的手段，否则猜错即死结。
//!
//! 三条设计决定：
//!
//! 1. **键用曲目 ID**（`crate::id`，内容哈希）。曲目 ID 扛得住移动、重命名、改标签，
//!    所以用户的修改不会因为整理文件而失联；专辑 ID 是聚合派生的（改个专辑艺人就变），
//!    绝不能当键。专辑级编辑在写入时展开成逐曲记录。
//! 2. **只存被改过的字段**。`None` 表示「不覆盖」，下次重扫读到更完善的标签时，
//!    用户没碰过的字段仍会自动更新——全量快照会把陈旧值永远钉死。
//! 3. **不改用户的音频文件**。覆盖只存在应用自己的数据目录里，写标签是破坏性操作，
//!    要做也应是显式的「写回文件」功能，而不是编辑元数据的副作用。

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

/// 单曲的元数据覆盖。字段为 `None` 表示沿用扫描结果。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/overrides.ts")]
#[serde(rename_all = "camelCase", default)]
pub struct TrackOverride {
    #[ts(optional)]
    pub title: Option<String>,
    #[ts(optional)]
    pub artist: Option<String>,
    #[ts(optional)]
    pub album: Option<String>,
    /// 改这一项会让曲目重新归组：同一张专辑散在两个目录时，把专辑艺人改成一致即可合并。
    #[ts(optional)]
    pub album_artist: Option<String>,
    #[ts(optional)]
    pub disc_no: Option<u16>,
    #[ts(optional)]
    pub track_no: Option<u16>,
}

/// 一次元数据修改请求。
///
/// 它与落盘的 [`TrackOverride`] 分开建模：落盘值只需要「有覆盖 / 无覆盖」两态，
/// 而补丁必须表达「没动 / 撤销 / 改值」三态。文本沿用空字符串表示撤销；数字字段
/// 使用嵌套 `Option`，经 serde 映射为：字段缺席 = 没动，`null` = 撤销，数字 = 改值。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/overrides.ts")]
#[serde(rename_all = "camelCase", default)]
pub struct TrackMetadataPatch {
    #[ts(optional)]
    pub title: Option<String>,
    #[ts(optional)]
    pub artist: Option<String>,
    #[ts(optional)]
    pub album: Option<String>,
    #[ts(optional)]
    pub album_artist: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_double_option"
    )]
    #[ts(optional)]
    pub disc_no: Option<Option<u16>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_double_option"
    )]
    #[ts(optional)]
    pub track_no: Option<Option<u16>>,
}

/// serde 默认会把「字段缺席」与显式 `null` 都解成外层 `None`；补丁需要区分二者。
fn deserialize_double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

impl TrackOverride {
    /// 是否什么都没覆盖（用于把空记录从表里删掉，避免文件里堆积无用条目）。
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// 空白字符串等同于「不覆盖」——界面上清空输入框应当是撤销修改，
    /// 而不是把标题设成空字符串。
    fn normalized(mut self) -> Self {
        let clean = |v: &mut Option<String>| {
            if v.as_deref().map(|s| s.trim().is_empty()).unwrap_or(false) {
                *v = None;
            } else if let Some(s) = v.as_mut() {
                *s = s.trim().to_string();
            }
        };
        clean(&mut self.title);
        clean(&mut self.artist);
        clean(&mut self.album);
        clean(&mut self.album_artist);
        self
    }
}

/// 全部覆盖记录，按曲目 ID 索引。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/overrides.ts")]
#[serde(rename_all = "camelCase", default)]
pub struct Overrides {
    pub tracks: HashMap<String, TrackOverride>,
}

impl Overrides {
    pub fn get(&self, track_id: &str) -> Option<&TrackOverride> {
        self.tracks.get(track_id)
    }

    /// 设置覆盖。传入空记录等于清除，不留空壳。
    pub fn set(&mut self, track_id: &str, ov: TrackOverride) {
        let ov = ov.normalized();
        if ov.is_empty() {
            self.tracks.remove(track_id);
        } else {
            self.tracks.insert(track_id.to_string(), ov);
        }
    }

    /// 把补丁合并进已有覆盖，逐字段三态语义：
    ///
    /// - 字段缺席 —— 这个字段没动，保持原样；
    /// - 文本空串 / 数字 `null` —— **撤销**该字段的修改，回到文件里的原值；
    /// - 非空文本 / 数字 —— 改成这个值。
    ///
    /// 三态是必要的：界面上「没碰这一栏」和「把这一栏清空」是两种不同的意图，
    /// 只有两态的话用户就只能整条还原，没法单独撤销某一个字段。
    pub fn merge(&mut self, track_id: &str, patch: TrackMetadataPatch) {
        let mut cur = self.tracks.get(track_id).cloned().unwrap_or_default();
        // Some("") 表示撤销 → 落到 None；Some(值) 去掉首尾空白后写入。
        let apply = |slot: &mut Option<String>, patch: Option<String>| {
            if let Some(v) = patch {
                *slot = Some(v.trim().to_string()).filter(|s| !s.is_empty());
            }
        };
        apply(&mut cur.title, patch.title);
        apply(&mut cur.artist, patch.artist);
        apply(&mut cur.album, patch.album);
        apply(&mut cur.album_artist, patch.album_artist);
        if let Some(value) = patch.disc_no {
            cur.disc_no = value;
        }
        if let Some(value) = patch.track_no {
            cur.track_no = value;
        }
        self.set(track_id, cur);
    }

    /// 清除某曲的全部覆盖（界面「还原为文件信息」）。
    pub fn clear(&mut self, track_id: &str) {
        self.tracks.remove(track_id);
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// 读取覆盖文件。文件不存在返回空表（首次运行的正常情况，不是错误）。
    ///
    /// 内容损坏时把原文件改名保留为 `*.corrupt` 再返回空表：直接照常写回会
    /// 抹掉用户的全部修改，留个残骸至少还有人工挽救的余地。
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e),
        };
        match serde_json::from_str(&raw) {
            Ok(v) => Ok(v),
            Err(_) => {
                let _ = std::fs::rename(path, path.with_extension("corrupt"));
                Ok(Self::default())
            }
        }
    }

    /// 原子写入：先写同目录临时文件再 rename，避免写到一半断电留下半个 JSON。
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpfile(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(name);
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn empty_override_is_removed_not_stored() {
        let mut o = Overrides::default();
        o.set(
            "t-1",
            TrackOverride {
                title: Some("改过".into()),
                ..Default::default()
            },
        );
        assert!(o.get("t-1").is_some());
        o.set("t-1", TrackOverride::default());
        assert!(o.get("t-1").is_none(), "空覆盖应当清除记录，而不是留下空壳");
    }

    /// 清空输入框 = 撤销修改，不是把标题改成空字符串。
    #[test]
    fn blank_string_counts_as_no_override() {
        let mut o = Overrides::default();
        o.set(
            "t-1",
            TrackOverride {
                title: Some("   ".into()),
                ..Default::default()
            },
        );
        assert!(o.get("t-1").is_none());
    }

    #[test]
    fn merge_keeps_untouched_fields() {
        let mut o = Overrides::default();
        o.set(
            "t-1",
            TrackOverride {
                artist: Some("白鲸电台".into()),
                ..Default::default()
            },
        );
        o.merge(
            "t-1",
            TrackMetadataPatch {
                album: Some("长夜电波".into()),
                ..Default::default()
            },
        );
        let got = o.get("t-1").unwrap();
        assert_eq!(got.artist.as_deref(), Some("白鲸电台"));
        assert_eq!(got.album.as_deref(), Some("长夜电波"));
    }

    /// 「没碰这一栏」与「把这一栏清空」是两种意图，不能混为一谈：
    /// 前者保持原样，后者撤销该字段的修改。
    #[test]
    fn merge_distinguishes_untouched_from_cleared() {
        let mut o = Overrides::default();
        o.set(
            "t-1",
            TrackOverride {
                artist: Some("我改的歌手".into()),
                album: Some("我改的专辑".into()),
                ..Default::default()
            },
        );

        // 只提交 album，artist 不动。
        o.merge(
            "t-1",
            TrackMetadataPatch {
                album: Some("再改一次".into()),
                ..Default::default()
            },
        );
        assert_eq!(o.get("t-1").unwrap().artist.as_deref(), Some("我改的歌手"));

        // 提交空串 = 撤销 artist 的修改，其余保留。
        o.merge(
            "t-1",
            TrackMetadataPatch {
                artist: Some("".into()),
                ..Default::default()
            },
        );
        let got = o.get("t-1").unwrap();
        assert_eq!(got.artist, None, "清空该字段应撤销修改");
        assert_eq!(got.album.as_deref(), Some("再改一次"), "其他字段不受影响");

        // 所有字段都撤销后，整条记录消失。
        o.merge(
            "t-1",
            TrackMetadataPatch {
                album: Some("  ".into()),
                ..Default::default()
            },
        );
        assert!(o.get("t-1").is_none());
    }

    #[test]
    fn numeric_patch_distinguishes_untouched_cleared_and_set() {
        let untouched: TrackMetadataPatch = serde_json::from_str("{}").unwrap();
        let cleared: TrackMetadataPatch =
            serde_json::from_str(r#"{"discNo":null,"trackNo":null}"#).unwrap();
        let changed: TrackMetadataPatch =
            serde_json::from_str(r#"{"discNo":2,"trackNo":7}"#).unwrap();

        assert_eq!(untouched.disc_no, None);
        assert_eq!(untouched.track_no, None);
        assert_eq!(cleared.disc_no, Some(None));
        assert_eq!(cleared.track_no, Some(None));
        assert_eq!(changed.disc_no, Some(Some(2)));
        assert_eq!(changed.track_no, Some(Some(7)));
    }

    #[test]
    fn merge_can_clear_one_numeric_override_without_touching_the_other() {
        let mut o = Overrides::default();
        o.set(
            "t-1",
            TrackOverride {
                disc_no: Some(2),
                track_no: Some(7),
                ..Default::default()
            },
        );

        o.merge(
            "t-1",
            TrackMetadataPatch {
                disc_no: Some(None),
                ..Default::default()
            },
        );

        let got = o.get("t-1").unwrap();
        assert_eq!(got.disc_no, None);
        assert_eq!(got.track_no, Some(7));
    }

    #[test]
    fn round_trips_through_disk() {
        let p = tmpfile("shannon_overrides_roundtrip.json");
        let mut o = Overrides::default();
        o.set(
            "t-1",
            TrackOverride {
                album_artist: Some("Various Artists".into()),
                ..Default::default()
            },
        );
        o.save(&p).unwrap();
        let back = Overrides::load(&p).unwrap();
        assert_eq!(back, o);
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn missing_file_is_not_an_error() {
        let p = tmpfile("shannon_overrides_absent.json");
        assert!(Overrides::load(&p).unwrap().is_empty());
    }

    /// 损坏文件不能让用户的修改被静默覆盖，要留下残骸。
    #[test]
    fn corrupt_file_is_preserved() {
        let p = tmpfile("shannon_overrides_corrupt.json");
        std::fs::write(&p, b"{ this is not json").unwrap();
        assert!(Overrides::load(&p).unwrap().is_empty());
        assert!(p.with_extension("corrupt").exists(), "损坏文件应改名保留");
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_file(p.with_extension("corrupt"));
    }
}
