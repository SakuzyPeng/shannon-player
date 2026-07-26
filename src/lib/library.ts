/* ============================================================
   生效曲库的访问器。
   组件一律经这里读曲库，不直接碰种子数据——这样「种子 / 真实扫描」
   的切换只发生在 store 里，调用点无感。
   ============================================================ */

import { useLibraryStore } from "@/store/library";
import type { Album, Track } from "@/types/player";

/** 当前生效的专辑列表。 */
export function albums(): Album[] {
  return useLibraryStore.getState().albums;
}

/** 当前生效的全部曲目（顺序即「最近添加」序）。 */
export function allTracks(): Track[] {
  return useLibraryStore.getState().tracks;
}

/** 某张专辑的曲目。真实曲库按 albumId 归属，种子曲库同样带 albumId。 */
export function tracksOf(album: Album): Track[] {
  return allTracks().filter((t) => t.albumId === album.id);
}

/** 歌手的专辑（按年份倒序，对齐歌手页设计稿）。 */
export function albumsOfArtist(artist: string): Album[] {
  return albums()
    .filter((a) => a.artist === artist)
    // 无年份的排在最后，而不是被当成 0 年
    .sort((a, b) => (b.year ?? -Infinity) - (a.year ?? -Infinity));
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

/**
 * 歌手热门歌曲。
 *
 * 种子曲库有设计稿给定的排序；真实曲库没有播放统计（需要后端持久化播放次数），
 * 此时退化为「其专辑曲目的前 10 首」——不假装有热度数据。
 */
export function topTracksOf(artist: string): Track[] {
  const byKey = new Map<string, Track>();
  const all: Track[] = [];
  for (const album of albumsOfArtist(artist)) {
    for (const track of tracksOf(album)) {
      byKey.set(`${album.title}/${track.title}`, track);
      all.push(track);
    }
  }
  const seed = useLibraryStore.getState().source === "seed" ? ARTIST_TOP_SONGS[artist] : undefined;
  if (seed) {
    return seed
      .map(([al, ti]) => byKey.get(`${al}/${ti}`))
      .filter((tk): tk is Track => tk !== undefined);
  }
  return all.slice(0, 10);
}

/** 歌手收听次数（演示统计；真实播放次数要等后端持久化）。 */
const ARTIST_PLAYS: Record<string, number> = { 白鲸电台: 214 };

export function playsOf(artist: string): number {
  const seeded = ARTIST_PLAYS[artist];
  if (seeded !== undefined) return seeded;
  let h = 0;
  for (const ch of artist) h = (h * 31 + ch.charCodeAt(0)) % 997;
  return 40 + (h % 400);
}
