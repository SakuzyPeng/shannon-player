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

/** 各专辑的已命名种子曲目（可少于曲目数，缺口由「曲目 N」占位补齐）。 */
const SEED_TRACKS: Record<string, ReadonlyArray<readonly [string, number]>> = {
  长夜电波: NIGHT_WAVE_TRACKS,
  空港日记: [
    ["空港日记", 255],
    ["候机厅的雨", 220],
    ["塔台之歌", 208],
    ["秦皇岛以东", 259],
  ],
  白鲸电台: [
    ["白鲸 FM", 292],
    ["浅海电台", 230],
  ],
  夜航: [["夜航", 284]],
};

/** 曲库种子数据（来自设计稿；「白鲸电台」为设计稿虚构乐队，其专辑详见歌手页稿）。
    n 为曲目数。后期由 Rust 后端扫描本地曲库替换。 */
const RAW = [
  { t: "长夜电波", ar: "白鲸电台", yr: 2023, g: "独立摇滚", n: 10, ch: "鲸", c1: "#4A6070", c2: "#26343E" },
  { t: "范特西", ar: "周杰伦", yr: 2001, g: "流行", n: 9, ch: "范", c1: "#7A4A3A", c2: "#46291F" },
  { t: "In Rainbows", ar: "Radiohead", yr: 2007, g: "另类摇滚", n: 10, ch: "In", c1: "#3E5C50", c2: "#20332B" },
  { t: "我去2000年", ar: "朴树", yr: 1999, g: "民谣摇滚", n: 11, ch: "我", c1: "#B08D57", c2: "#75582F" },
  { t: "Blonde", ar: "Frank Ocean", yr: 2016, g: "R&B", n: 12, ch: "B", c1: "#A8A29A", c2: "#67625B" },
  { t: "八度空间", ar: "周杰伦", yr: 2002, g: "流行", n: 13, ch: "八", c1: "#5B6B7A", c2: "#333E48" },
  { t: "之乎者也", ar: "罗大佑", yr: 1982, g: "民谣", n: 8, ch: "之", c1: "#8A6A4A", c2: "#523B26" },
  { t: "Kind of Blue", ar: "Miles Davis", yr: 1959, g: "爵士", n: 9, ch: "K", c1: "#33506B", c2: "#1B2C3E" },
  { t: "小宇宙", ar: "苏打绿", yr: 2006, g: "流行", n: 10, ch: "小", c1: "#6B8A5A", c2: "#3F5733" },
  { t: "Rumours", ar: "Fleetwood Mac", yr: 1977, g: "摇滚", n: 11, ch: "R", c1: "#B07A50", c2: "#71482A" },
  { t: "万能青年旅店", ar: "万能青年旅店", yr: 2010, g: "摇滚", n: 12, ch: "万", c1: "#706A58", c2: "#403C2C" },
  { t: "Random Access Memories", ar: "Daft Punk", yr: 2013, g: "电子", n: 13, ch: "RA", c1: "#4A4458", c2: "#282332" },
  { t: "华丽的冒险", ar: "陈绮贞", yr: 2005, g: "流行", n: 8, ch: "华", c1: "#B99A76", c2: "#7C6244" },
  { t: "Abbey Road", ar: "The Beatles", yr: 1969, g: "摇滚", n: 9, ch: "A", c1: "#7F7F52", c2: "#4C4C2D" },
  { t: "生活因你而火热", ar: "新裤子", yr: 2016, g: "新浪潮", n: 10, ch: "生", c1: "#A45A48", c2: "#633327" },
  { t: "空港日记", ar: "白鲸电台", yr: 2021, g: "独立摇滚", n: 9, ch: "空", c1: "#6E8A8A", c2: "#3E5252" },
  { t: "白鲸电台", ar: "白鲸电台", yr: 2019, g: "独立摇滚", n: 9, ch: "白", c1: "#55627A", c2: "#2E3648" },
  { t: "深海信号 EP", ar: "白鲸电台", yr: 2022, g: "独立摇滚", n: 5, ch: "深", c1: "#3E5C6B", c2: "#22343E" },
  { t: "环线现场", ar: "白鲸电台", yr: 2020, g: "独立摇滚", n: 12, ch: "现", c1: "#7A6A55", c2: "#463B2C" },
  { t: "夜航", ar: "白鲸电台", yr: 2018, g: "独立摇滚", n: 8, ch: "夜", c1: "#4A4458", c2: "#282332" },
  { t: "白噪音练习", ar: "白鲸电台", yr: 2017, g: "独立摇滚", n: 7, ch: "噪", c1: "#8A7A66", c2: "#524636" },
] as const;

/** 专辑的曲目行：种子在前，缺口由「曲目 N」占位补齐（时长确定性伪随机 2:45–5:14）。 */
function rowsFor(title: string, trackCount: number): Array<readonly [string, number]> {
  const rows: Array<readonly [string, number]> = [...(SEED_TRACKS[title] ?? [])];
  for (let i = rows.length; i < trackCount; i++) {
    rows.push([`曲目 ${i + 1}`, 165 + (((trackCount * 7 + i) * 37) % 150)]);
  }
  return rows.slice(0, trackCount);
}

export const ALBUMS: Album[] = RAW.map((a, i) => ({
  id: `alb-${i}`,
  title: a.t,
  artist: a.ar,
  year: a.yr,
  genre: a.g,
  cover: { initial: a.ch, gradient: [a.c1, a.c2] },
  trackCount: a.n,
  durationSec: rowsFor(a.t, a.n).reduce((s, [, d]) => s + d, 0),
}));

/** 专辑曲目列表。后期由后端扫描替换。 */
export function tracksOf(album: Album): Track[] {
  return rowsFor(album.title, album.trackCount).map(([title, durationSec], i) => ({
    id: `${album.id}-t${i}`,
    title,
    artist: album.artist,
    album: album.title,
    albumId: album.id,
    cover: album.cover,
    durationSec,
  }));
}

/** 歌手的专辑（按年份倒序，对齐歌手页设计稿）。 */
export function albumsOfArtist(artist: string): Album[] {
  return ALBUMS.filter((a) => a.artist === artist).sort((a, b) => b.year - a.year);
}

/** 歌手页热门歌曲的种子排序（[专辑, 曲名]，来自设计稿）。 */
const ARTIST_TOP_SONGS: Record<string, ReadonlyArray<readonly [string, string]>> = {
  白鲸电台: [
    ["长夜电波", "午夜环线"],
    ["长夜电波", "凌晨广播站"],
    ["空港日记", "空港日记"],
    ["空港日记", "候机厅的雨"],
    ["白鲸电台", "白鲸 FM"],
    ["长夜电波", "无人月台"],
    ["空港日记", "塔台之歌"],
    ["夜航", "夜航"],
    ["长夜电波", "末班车挽歌"],
    ["白鲸电台", "浅海电台"],
  ],
};

/** 歌手热门歌曲（有种子按种子排序，否则取其专辑曲目的前 10 首）。 */
export function topTracksOf(artist: string): Track[] {
  const byKey = new Map<string, Track>();
  const all: Track[] = [];
  for (const album of albumsOfArtist(artist)) {
    for (const track of tracksOf(album)) {
      byKey.set(`${album.title}/${track.title}`, track);
      all.push(track);
    }
  }
  const seed = ARTIST_TOP_SONGS[artist];
  if (seed) {
    return seed
      .map(([al, ti]) => byKey.get(`${al}/${ti}`))
      .filter((tk): tk is Track => tk !== undefined);
  }
  return all.slice(0, 10);
}

/** 歌手收听次数（演示统计，后期由后端提供）。 */
const ARTIST_PLAYS: Record<string, number> = { 白鲸电台: 214 };

export function playsOf(artist: string): number {
  const seeded = ARTIST_PLAYS[artist];
  if (seeded !== undefined) return seeded;
  let h = 0;
  for (const ch of artist) h = (h * 31 + ch.charCodeAt(0)) % 997;
  return 40 + (h % 400);
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

const albumIdByTitle = (title: string): string => ALBUMS.find((a) => a.title === title)!.id;

/** 收藏种子（对齐设计稿演示状态）：播放条演示曲目 + 午夜环线 / 出租屋的海 / 候机厅的雨。 */
export const SEED_FAVORITE_TRACKS: Record<string, boolean> = {
  [DEMO_TRACK.id]: true,
  [`${albumIdByTitle("长夜电波")}-t1`]: true, // 午夜环线
  [`${albumIdByTitle("长夜电波")}-t5`]: true, // 出租屋的海
  [`${albumIdByTitle("空港日记")}-t1`]: true, // 候机厅的雨
};

/** 收藏专辑种子（设计稿演示状态：长夜电波、环线现场为已收藏态）。 */
export const SEED_FAVORITE_ALBUMS: Record<string, boolean> = {
  [albumIdByTitle("长夜电波")]: true,
  [albumIdByTitle("环线现场")]: true,
};
