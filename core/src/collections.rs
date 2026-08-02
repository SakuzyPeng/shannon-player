//! 收藏与歌单——用户**自己攒出来的**那部分数据。
//!
//! 它与扫描缓存的性质完全不同：缓存丢了重扫一遍就有，这里丢了就是真没了。所以它和
//! 元数据覆盖层归为一类，落在同一个数据库里（见 [`crate::db`]）。
//!
//! ## 键怎么选：全部落到曲目 ID 上
//!
//! `crate::id::album_id` 的注释早就写死了这条——**专辑不是持久化实体**。专辑 ID 由
//! 归组键哈希而来，而归组键里含**所在目录**（否则两位歌手各自的《Greatest Hits》会
//! 撞成一张），于是改标签、挪文件、重扫都会让它变。拿它当收藏的键，用户整理一次
//! 音乐文件夹就会发现收藏全没了，而他并没有取消收藏过任何东西。
//!
//! 所以：
//!
//! - **曲目收藏**：曲目 ID。内容哈希，扛得住移动、重命名、改标签。
//! - **专辑收藏**：收藏时把该专辑**全部曲目的 ID 存成一组**，之后「当前专辑里有任意
//!   一首命中任一组」即视为已收藏。分组边界不能丢：若收藏时是 A/B/C、重扫时 C 暂时
//!   缺失，用户用当前可见的 A/B 取消收藏时要删掉整组，不能留下 C 等它回来后复活红心。
//!   这样改专辑名、换目录、重扫都不影响；代价是专辑被拆成两张时两半都算收藏，而这比
//!   「整理一次文件就掉收藏」轻得多。
//! - **歌手收藏**：只能用名字——歌手是纯粹从字符串聚合出来的，系统里根本没有比名字
//!   更稳的标识。因此改写歌手名会让收藏落空，这一条在实现上无解，只能如实记下。
//! - **歌单收藏**：歌单 ID 由我们自己生成，天然稳定。
//!
//! ## 歌单只存曲目 ID
//!
//! 与播放会话同一条理由：曲目信息的权威在曲库，存 `Track` 副本会让用户改完元数据后
//! 歌单里还是旧标题——他刚改过，这种不一致最让人怀疑软件坏了。代价是前端必须等曲库
//! 就绪后再按 ID 水合。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 全部收藏。四类分开存，因为它们的键根本不是一回事（见模块头）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/collections.ts")]
#[serde(rename_all = "camelCase", default)]
pub struct Favorites {
    /// 被收藏的曲目 ID。
    pub tracks: Vec<String>,
    /// 被收藏专辑的成员快照。外层每项是一笔专辑收藏，内层是收藏当时的全部曲目 ID。
    pub album_groups: Vec<Vec<String>>,
    /// 被收藏的歌手名。
    pub artists: Vec<String>,
    /// 被收藏的歌单 ID。
    pub playlists: Vec<String>,
}

/// 一个歌单。曲目只留 ID，显示所需的信息由前端按曲库水合。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/collections.ts")]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: String,
    pub title: String,
    /// 简介。是内容不是界面文案，不进 i18n。
    pub description: String,
    /// 曲目 ID，按用户拖拽出来的顺序。**允许重复**——同一首歌可以在一个歌单里出现
    /// 两次，这是用户的自由，不是需要去重的脏数据。
    pub track_ids: Vec<String>,
    /// 最后修改时间（Unix 毫秒）。
    ///
    /// 存时间戳而不是「上周更新」那样的现成标签：那句话属于显示层，且要随界面语言变，
    /// 存进数据库等于把一份中文文案钉死在用户数据里。
    ///
    /// TS 侧显式标成 `number`：ts-rs 默认把 `i64` 映射成 `bigint`，而 serde 序列化出来
    /// 的是 JSON 数字、前端接到的也是 `number`——照默认走会得到一个与线上格式不符的
    /// 类型，等到做减法算「多久以前」时才炸。毫秒时间戳在 f64 里精确到公元 275760 年，
    /// 精度不是问题。
    #[ts(type = "number")]
    pub updated_at_ms: i64,
}

impl Playlist {
    pub fn new(id: impl Into<String>, title: impl Into<String>, updated_at_ms: i64) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: String::new(),
            track_ids: Vec::new(),
            updated_at_ms,
        }
    }
}

impl Favorites {
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
            && self.album_groups.is_empty()
            && self.artists.is_empty()
            && self.playlists.is_empty()
    }

    /// 这张专辑算不算已收藏：当前曲目里**任意一首**在集合内即算。
    ///
    /// 用「任意」而不是「全部」，是因为专辑的曲目集合会变——补进一首漏扫的、删掉一首
    /// 重复的，都不该让收藏消失。反过来的代价（专辑被拆成两张时两半都显示已收藏）
    /// 只是偶尔多一颗红心，量级完全不同。
    pub fn has_album(&self, track_ids: &[String]) -> bool {
        self.album_groups
            .iter()
            .any(|group| track_ids.iter().any(|id| group.contains(id)))
    }
}
