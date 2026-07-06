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

/** 曲库种子数据（来自 2a 设计稿的 ALB）。后期由 Rust 后端扫描本地曲库替换。 */
const RAW = [
  { t: "范特西", ar: "周杰伦", yr: 2001, ch: "范", c1: "#7A4A3A", c2: "#46291F" },
  { t: "In Rainbows", ar: "Radiohead", yr: 2007, ch: "In", c1: "#3E5C50", c2: "#20332B" },
  { t: "我去2000年", ar: "朴树", yr: 1999, ch: "我", c1: "#B08D57", c2: "#75582F" },
  { t: "Blonde", ar: "Frank Ocean", yr: 2016, ch: "B", c1: "#A8A29A", c2: "#67625B" },
  { t: "八度空间", ar: "周杰伦", yr: 2002, ch: "八", c1: "#5B6B7A", c2: "#333E48" },
  { t: "之乎者也", ar: "罗大佑", yr: 1982, ch: "之", c1: "#8A6A4A", c2: "#523B26" },
  { t: "Kind of Blue", ar: "Miles Davis", yr: 1959, ch: "K", c1: "#33506B", c2: "#1B2C3E" },
  { t: "小宇宙", ar: "苏打绿", yr: 2006, ch: "小", c1: "#6B8A5A", c2: "#3F5733" },
  { t: "Rumours", ar: "Fleetwood Mac", yr: 1977, ch: "R", c1: "#B07A50", c2: "#71482A" },
  { t: "万能青年旅店", ar: "万能青年旅店", yr: 2010, ch: "万", c1: "#706A58", c2: "#403C2C" },
  { t: "Random Access Memories", ar: "Daft Punk", yr: 2013, ch: "RA", c1: "#4A4458", c2: "#282332" },
  { t: "华丽的冒险", ar: "陈绮贞", yr: 2005, ch: "华", c1: "#B99A76", c2: "#7C6244" },
  { t: "Abbey Road", ar: "The Beatles", yr: 1969, ch: "A", c1: "#7F7F52", c2: "#4C4C2D" },
  { t: "生活因你而火热", ar: "新裤子", yr: 2016, ch: "生", c1: "#A45A48", c2: "#633327" },
] as const;

export const ALBUMS: Album[] = RAW.map((a, i) => ({
  id: `alb-${i}`,
  title: a.t,
  artist: a.ar,
  year: a.yr,
  cover: { initial: a.ch, gradient: [a.c1, a.c2] },
  trackCount: 8 + (i % 6),
  durationSec: (32 + ((i * 7) % 28)) * 60,
}));

/** 悬浮播放条演示曲目（万能青年旅店《秦皇岛》）。 */
export const DEMO_TRACK: Track = {
  id: "trk-demo",
  title: "秦皇岛",
  artist: "万能青年旅店",
  album: "万能青年旅店",
  albumId: "alb-9",
  cover: { initial: "万", gradient: ["#706A58", "#403C2C"] },
  durationSec: 371, // 6:11
  favorited: false,
};
