import type { MessageKey } from "@/i18n/messages";
import type { Album, NavKey, Track } from "@/types/player";

/** 导航项定义（label 走 i18n：nav.<key>；图标键见 Icon.tsx）。 */
export interface NavItem {
  key: NavKey;
  labelKey: MessageKey;
  icon: "albums" | "songs" | "artists" | "favorites" | "settings";
}

export const NAV_ITEMS: NavItem[] = [
  { key: "albums", labelKey: "nav.albums", icon: "albums" },
  { key: "songs", labelKey: "nav.songs", icon: "songs" },
  { key: "artists", labelKey: "nav.artists", icon: "artists" },
  { key: "favorites", labelKey: "nav.favorites", icon: "favorites" },
  { key: "settings", labelKey: "nav.settings", icon: "settings" },
];

/**
 * 界面暴露的语言列表。当前仅承诺简体中文与 English；
 * 繁體中文 / 日本語 的词条已备在 i18n 目录中，待正式承诺后再加入此处。
 */
export const LANGUAGES = ["跟随系统", "简体中文", "English"] as const;

/** 右键菜单项（label 走 i18n）。 */
export interface AlbumMenuItem {
  kind: "item" | "separator";
  labelKey?: MessageKey;
  kbd?: string;
  danger?: boolean;
}

export const ALBUM_MENU: AlbumMenuItem[] = [
  { kind: "item", labelKey: "menu.play", kbd: "⏎" },
  { kind: "item", labelKey: "menu.playNext" },
  { kind: "separator" },
  { kind: "item", labelKey: "menu.addToPlaylist" },
  { kind: "item", labelKey: "menu.favorite" },
  { kind: "separator" },
  { kind: "item", labelKey: "menu.editTags", kbd: "⌘I" },
  { kind: "item", labelKey: "menu.showInfo" },
  { kind: "item", labelKey: "menu.revealInFinder", kbd: "⇧⌘R" },
  { kind: "separator" },
  { kind: "item", labelKey: "menu.removeFromLibrary", kbd: "⌫", danger: true },
];

/** 曲目右键菜单（专辑页）：与专辑菜单的差异是「查看歌词」替代「显示专辑简介」。 */
export const TRACK_MENU: AlbumMenuItem[] = [
  { kind: "item", labelKey: "menu.play", kbd: "⏎" },
  { kind: "item", labelKey: "menu.playNext" },
  { kind: "separator" },
  { kind: "item", labelKey: "menu.addToPlaylist" },
  { kind: "item", labelKey: "menu.favorite" },
  { kind: "separator" },
  { kind: "item", labelKey: "menu.editTags", kbd: "⌘I" },
  { kind: "item", labelKey: "menu.showLyrics" },
  { kind: "item", labelKey: "menu.revealInFinder", kbd: "⇧⌘R" },
  { kind: "separator" },
  { kind: "item", labelKey: "menu.removeFromLibrary", kbd: "⌫", danger: true },
];

/** 「长夜电波」曲目种子（来自专辑页设计稿）：[标题, 时长（秒）]。 */
const NIGHT_WAVE_TRACKS: ReadonlyArray<readonly [string, number]> = [
  ["凌晨广播站", 192],
  ["午夜环线", 236],
  ["无人月台", 248],
  ["城市白噪音", 167],
  ["高架桥以南", 213],
  ["出租屋的海", 261],
  ["信号消失前", 185],
  ["便利店灯光", 228],
  ["末班车挽歌", 302],
  ["黎明四站远", 280],
];

/** 有完整曲目种子的专辑（按标题索引）；其余专辑的曲目由 tracksOf 生成占位。 */
const SEED_TRACKS: Record<string, ReadonlyArray<readonly [string, number]>> = {
  长夜电波: NIGHT_WAVE_TRACKS,
};

/** 曲库种子数据（来自设计稿；「长夜电波 — 白鲸电台」为设计稿虚构乐队，最近添加排最前）。
    后期由 Rust 后端扫描本地曲库替换。 */
const RAW = [
  { t: "长夜电波", ar: "白鲸电台", yr: 2023, g: "独立摇滚", ch: "鲸", c1: "#4A6070", c2: "#26343E" },
  { t: "范特西", ar: "周杰伦", yr: 2001, g: "流行", ch: "范", c1: "#7A4A3A", c2: "#46291F" },
  { t: "In Rainbows", ar: "Radiohead", yr: 2007, g: "另类摇滚", ch: "In", c1: "#3E5C50", c2: "#20332B" },
  { t: "我去2000年", ar: "朴树", yr: 1999, g: "民谣摇滚", ch: "我", c1: "#B08D57", c2: "#75582F" },
  { t: "Blonde", ar: "Frank Ocean", yr: 2016, g: "R&B", ch: "B", c1: "#A8A29A", c2: "#67625B" },
  { t: "八度空间", ar: "周杰伦", yr: 2002, g: "流行", ch: "八", c1: "#5B6B7A", c2: "#333E48" },
  { t: "之乎者也", ar: "罗大佑", yr: 1982, g: "民谣", ch: "之", c1: "#8A6A4A", c2: "#523B26" },
  { t: "Kind of Blue", ar: "Miles Davis", yr: 1959, g: "爵士", ch: "K", c1: "#33506B", c2: "#1B2C3E" },
  { t: "小宇宙", ar: "苏打绿", yr: 2006, g: "流行", ch: "小", c1: "#6B8A5A", c2: "#3F5733" },
  { t: "Rumours", ar: "Fleetwood Mac", yr: 1977, g: "摇滚", ch: "R", c1: "#B07A50", c2: "#71482A" },
  { t: "万能青年旅店", ar: "万能青年旅店", yr: 2010, g: "摇滚", ch: "万", c1: "#706A58", c2: "#403C2C" },
  { t: "Random Access Memories", ar: "Daft Punk", yr: 2013, g: "电子", ch: "RA", c1: "#4A4458", c2: "#282332" },
  { t: "华丽的冒险", ar: "陈绮贞", yr: 2005, g: "流行", ch: "华", c1: "#B99A76", c2: "#7C6244" },
  { t: "Abbey Road", ar: "The Beatles", yr: 1969, g: "摇滚", ch: "A", c1: "#7F7F52", c2: "#4C4C2D" },
  { t: "生活因你而火热", ar: "新裤子", yr: 2016, g: "新浪潮", ch: "生", c1: "#A45A48", c2: "#633327" },
] as const;

export const ALBUMS: Album[] = RAW.map((a, i) => {
  const seed = SEED_TRACKS[a.t];
  return {
    id: `alb-${i}`,
    title: a.t,
    artist: a.ar,
    year: a.yr,
    genre: a.g,
    cover: { initial: a.ch, gradient: [a.c1, a.c2] },
    trackCount: seed ? seed.length : 8 + (i % 6),
    durationSec: seed ? seed.reduce((s, [, d]) => s + d, 0) : (32 + ((i * 7) % 28)) * 60,
  };
});

/**
 * 专辑曲目列表。有种子的专辑用真实曲目；其余生成「曲目 N」占位
 * （时长确定性分摊，总和与专辑 durationSec 一致），后期由后端扫描替换。
 */
export function tracksOf(album: Album): Track[] {
  const seed = SEED_TRACKS[album.title];
  const rows: ReadonlyArray<readonly [string, number]> = seed ?? genPlaceholderRows(album);
  return rows.map(([title, durationSec], i) => ({
    id: `${album.id}-t${i}`,
    title,
    artist: album.artist,
    album: album.title,
    albumId: album.id,
    cover: album.cover,
    durationSec,
  }));
}

function genPlaceholderRows(album: Album): Array<[string, number]> {
  const n = album.trackCount;
  const base = album.durationSec / n;
  const rows: Array<[string, number]> = [];
  let used = 0;
  for (let i = 0; i < n - 1; i++) {
    const d = Math.max(90, Math.round(base * (0.78 + ((i * 37) % 45) / 100)));
    rows.push([`曲目 ${i + 1}`, d]);
    used += d;
  }
  rows.push([`曲目 ${n}`, Math.max(90, album.durationSec - used)]);
  return rows;
}

/** 悬浮播放条演示曲目（万能青年旅店《秦皇岛》）。 */
export const DEMO_TRACK: Track = {
  id: "trk-demo",
  title: "秦皇岛",
  artist: "万能青年旅店",
  album: "万能青年旅店",
  albumId: ALBUMS.find((a) => a.title === "万能青年旅店")?.id,
  cover: { initial: "万", gradient: ["#706A58", "#403C2C"] },
  durationSec: 371, // 6:11
};

/** 收藏种子（对齐设计稿演示状态）：播放条演示曲目 + 长夜电波第 2/6 首。 */
export const SEED_FAVORITE_TRACKS: Record<string, boolean> = {
  [DEMO_TRACK.id]: true,
  [`${ALBUMS[0].id}-t1`]: true, // 午夜环线
  [`${ALBUMS[0].id}-t5`]: true, // 出租屋的海
};

/** 收藏专辑种子（专辑页设计稿：长夜电波为已收藏态）。 */
export const SEED_FAVORITE_ALBUMS: Record<string, boolean> = {
  [ALBUMS[0].id]: true,
};
