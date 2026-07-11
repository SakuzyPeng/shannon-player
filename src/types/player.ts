/* ============================================================
   香农播放器 · 播放器领域类型
   播放队列 / 歌词时间轴 / 音频设备 / 播放进度 均以强类型建模，
   为后续接入 Rust 后端与真实音频引擎留出稳定契约。
   ============================================================ */

/** 唯一 ID（曲目 / 专辑 / 歌手 / 队列项）。 */
export type Id = string;

/** 循环模式。 */
export type RepeatMode = "off" | "all" | "one";

/** 封面：占位渐变（首字母）或真实图片 URL。 */
export interface Cover {
  /** 占位封面首字母（无图时显示）。 */
  initial: string;
  /** 渐变起止色（占位封面）。 */
  gradient: [from: string, to: string];
  /** 真实封面图 URL（有则优先）。 */
  url?: string;
}

/** 专辑。 */
export interface Album {
  id: Id;
  title: string;
  artist: string;
  year: number;
  /** 流派（来自文件标签的内容，不进 i18n）。 */
  genre: string;
  cover: Cover;
  /** 曲目数。 */
  trackCount: number;
  /** 总时长（秒）。 */
  durationSec: number;
}

/** 曲目。 */
export interface Track {
  id: Id;
  title: string;
  artist: string;
  album: string;
  albumId?: Id;
  cover: Cover;
  /** 时长（秒）。 */
  durationSec: number;
  /** 本地文件路径（后期由 Rust 后端提供）。 */
  path?: string;
}

/** 歌单（用户创建的跨专辑曲目集合）。 */
export interface Playlist {
  id: Id;
  title: string;
  /** 简介（内容，不进 i18n）。 */
  description: string;
  /** 更新时间标签（如「上周更新」；后期由后端提供真实时间戳）。 */
  updatedLabel: string;
  tracks: Track[];
}

/** 播放队列中的一项（区分同一曲目的多次入队）。 */
export interface QueueItem {
  /** 队列项自身 ID，与 track.id 不同。 */
  uid: Id;
  track: Track;
  /** 来源：用户手动「下一首播放」优先级更高。 */
  source: "user" | "auto";
}

/** 逐字歌词的一个词（时间轴以毫秒为单位，供 AMLL 逐字填充）。 */
export interface LyricWord {
  text: string;
  startMs: number;
  endMs: number;
}

/** 一行歌词（时间轴以毫秒为单位）。 */
export interface LyricLine {
  /** 起始时间（ms）。 */
  timeMs: number;
  /** 结束时间（ms），用于逐行高亮区间；缺省则到下一行。 */
  endMs?: number;
  text: string;
  /** 逐字时间轴（有则为逐字歌词，无则整行渐显）。 */
  words?: LyricWord[];
  /** 译文 / 音译（AMLL 多行）。 */
  translation?: string;
  romaji?: string;
}

/** 歌词文档。 */
export interface Lyrics {
  trackId: Id;
  lines: LyricLine[];
  /** 是否逐字（word-by-word）。 */
  synced: boolean;
}

/** 音频输出设备。 */
export interface AudioDevice {
  id: Id;
  label: string;
  isDefault: boolean;
}

/** 播放进度快照（供 UI 订阅，避免每帧重渲染整个 store）。 */
export interface PlaybackProgress {
  /** 当前播放位置（秒）。 */
  positionSec: number;
  /** 当前曲目总时长（秒）。 */
  durationSec: number;
  /** 已缓冲位置（秒）。 */
  bufferedSec: number;
}

/** 界面语言。 */
export type Language = "跟随系统" | "简体中文" | "繁體中文" | "English" | "日本語";

/** 外观主题。 */
export type ThemeMode = "light" | "dark" | "system";

/** 主导航目标。 */
export type NavKey = "albums" | "songs" | "artists" | "search" | "favorites" | "settings";

/** 曲库视图模式。 */
export type LibraryView = "grid" | "list";
