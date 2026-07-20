/* ============================================================
   香农播放器 · 歌单种子数据（来自歌单页设计稿「深夜驾驶」）
   跨专辑曲目集合；封面为前若干首曲目封面的 2×2 拼贴。
   后期由 Rust 后端 / 用户数据替换。
   ============================================================ */

import { ALBUMS, DEMO_TRACK, allTracks } from "@/data/library";
import type { Cover, Id, Playlist, Track } from "@/types/player";

/** 专辑封面素材（标题 → [c1, c2, 首字]），来自设计稿 ALB_ART。 */
const ALB_ART: Record<string, [string, string, string]> = {
  长夜电波: ["#4A6070", "#26343E", "鲸"],
  万能青年旅店: ["#706A58", "#403C2C", "万"],
  依然范特西: ["#5B6B7A", "#333E48", "范"],
  "In Rainbows": ["#3E5C50", "#20332B", "In"],
  Blonde: ["#A8A29A", "#67625B", "B"],
  "Can't Buy a Thrill": ["#8A6A4A", "#523B26", "C"],
  白鲸电台: ["#55627A", "#2E3648", "白"],
  "Hurry Up, We're Dreaming": ["#4A4458", "#282332", "M"],
  生活因你而火热: ["#A45A48", "#633327", "生"],
};

function coverOf(album: string): Cover {
  const art = ALB_ART[album] ?? ["#706A58", "#403C2C", album.slice(0, 1)];
  return { initial: art[2], gradient: [art[0], art[1]] };
}

/** 「深夜驾驶」曲目：[标题, 歌手, 专辑, 时长(秒)]。 */
const NIGHT_DRIVE: ReadonlyArray<readonly [string, string, string, number]> = [
  ["午夜环线", "白鲸电台", "长夜电波", 236],
  ["秦皇岛", "万能青年旅店", "万能青年旅店", 371],
  ["夜的第七章", "周杰伦", "依然范特西", 226],
  ["Nude", "Radiohead", "In Rainbows", 255],
  ["末班车挽歌", "白鲸电台", "长夜电波", 302],
  ["杀死那个石家庄人", "万能青年旅店", "万能青年旅店", 383],
  ["Nights", "Frank Ocean", "Blonde", 307],
  ["无人月台", "白鲸电台", "长夜电波", 248],
  ["Do It Again", "Steely Dan", "Can't Buy a Thrill", 356],
  ["白鲸 FM", "白鲸电台", "白鲸电台", 292],
  ["Midnight City", "M83", "Hurry Up, We're Dreaming", 243],
  ["公路之光", "新裤子", "生活因你而火热", 259],
];

const LIBRARY_TRACKS = [DEMO_TRACK, ...allTracks()];

/** 优先复用曲库规范 ID，避免同一曲目因歌单种子 ID 不同而绕过去重。 */
function nightDriveTrack(
  [title, artist, album, durationSec]: (typeof NIGHT_DRIVE)[number],
  index: number,
): Track {
  const libraryTrack = LIBRARY_TRACKS.find(
    (track) =>
      track.title === title &&
      track.artist === artist &&
      track.album === album &&
      track.durationSec === durationSec,
  );
  if (libraryTrack) return libraryTrack;

  const albumId = ALBUMS.find((item) => item.title === album && item.artist === artist)?.id;
  return {
    id: `pl-nightdrive-t${index}`,
    title,
    artist,
    album,
    albumId,
    cover: coverOf(album),
    durationSec,
  };
}

export const PLAYLISTS: Playlist[] = [
  {
    id: "pl-nightdrive",
    title: "深夜驾驶",
    description: "环线空了，音量开大一点。适合凌晨一点以后的城市快速路。",
    updatedLabel: "上周更新",
    tracks: NIGHT_DRIVE.map(nightDriveTrack),
  },
];

export function playlistOf(id: Id): Playlist | undefined {
  return PLAYLISTS.find((p) => p.id === id);
}

/** 歌单封面拼贴：取前 4 首曲目的封面（不足 4 则重复填充）。 */
export function collageOf(playlist: Playlist): Cover[] {
  const covers = playlist.tracks.slice(0, 4).map((t) => t.cover);
  while (covers.length < 4 && covers.length > 0) covers.push(covers[covers.length % covers.length]);
  return covers.slice(0, 4);
}
