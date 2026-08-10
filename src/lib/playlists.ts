/* ============================================================
   歌单：落盘形态 ↔ 界面形态。
   落盘只有曲目 ID（曲目信息的权威在曲库，存副本会让用户改完元数据后歌单里
   还是旧标题），界面要的是曲目本体，两者之间的水合与写回集中在这里。
   ============================================================ */

import type { Locale, TFunction } from "@/i18n";
import { allTracks } from "@/lib/library";
import type { Playlist as StoredPlaylist } from "@/types/generated/collections";
import type { Id, Playlist, Track } from "@/types/player";

export type { StoredPlaylist };

/** 当前曲库的 ID → 曲目索引。歌单水合是按 ID 逐个查，先建一张表免得退化成 O(n²)。 */
function trackIndex(): Map<Id, Track> {
  return new Map(allTracks().map((track) => [track.id, track]));
}

/** 把一串曲目 ID 换成曲目本体，查不回的**跳过而不是留空位**。 */
function hydrateIds(trackIds: Id[], byId: Map<Id, Track>): Track[] {
  return trackIds
    .map((id) => byId.get(id))
    .filter((track): track is Track => track !== undefined);
}

/** 按当前曲库把落盘歌单水合成界面歌单。 */
export function hydratePlaylists(stored: StoredPlaylist[]): Playlist[] {
  const byId = trackIndex();
  return stored.map((playlist) => ({
    id: playlist.id,
    title: playlist.title,
    description: playlist.description,
    updatedAtMs: playlist.updatedAtMs,
    trackIds: [...playlist.trackIds],
    tracks: hydrateIds(playlist.trackIds, byId),
  }));
}

/**
 * 曲库换了一份之后重新水合：ID 不变，曲目本体要换成新曲库里的那些。
 *
 * 只重算 `tracks`，`trackIds` 原样带过——上一份曲库里查不回的 ID 不代表用户删过它，
 * 在这里丢掉就等于「重扫一次掉几首歌」。
 */
export function rehydratePlaylists(playlists: Playlist[]): Playlist[] {
  const byId = trackIndex();
  return playlists.map((playlist) => ({
    ...playlist,
    tracks: hydrateIds(playlist.trackIds, byId),
  }));
}

/**
 * 两份歌单列表在界面看来是不是同一份。
 *
 * 每次收藏写入之后都会跑一趟对账，而它读回来的多半与界面上的一模一样。照样换成新对象
 * 会让整片歌单卡重新测量布局——用户正拖着一张卡时，那次重测会让它跳一下。
 */
export function samePlaylists(a: Playlist[], b: Playlist[]): boolean {
  if (a.length !== b.length) return false;
  return a.every((left, i) => {
    const right = b[i];
    return (
      left.id === right.id &&
      left.title === right.title &&
      left.description === right.description &&
      left.updatedAtMs === right.updatedAtMs &&
      left.trackIds.length === right.trackIds.length &&
      left.trackIds.every((id, j) => id === right.trackIds[j]) &&
      // 曲目本体是 trackIds 与当前曲库的纯函数，正常情况下不会单独变；仍比一遍，
      // 免得将来某条水合路径漏了一半时，这里悄悄把差异当成没差异。
      left.tracks.length === right.tracks.length &&
      left.tracks.every((track, j) => track === right.tracks[j])
    );
  });
}

/** 界面歌单 → 落盘形态。时间戳由后端重新盖，这里传的只是乐观值。 */
export function toStored(playlist: Playlist): StoredPlaylist {
  return {
    id: playlist.id,
    title: playlist.title,
    description: playlist.description,
    trackIds: playlist.trackIds,
    updatedAtMs: playlist.updatedAtMs,
  };
}

/**
 * 「上次更新」的显示文案。
 *
 * 当天更新单独说「今天更新」：用户刚改完就看到一个日期，会以为改动没生效。
 * 更细的相对时间（「3 天前」）要靠 `Intl.RelativeTimeFormat` 与一套分档规则，
 * 目前的界面只在元信息行里露一次，不值得为它引入那一整套。
 */
export function updatedLabelOf(updatedAtMs: number, t: TFunction, locale: Locale): string {
  // 后端在系统时钟早于 1970 时会退化成 0（见 `src-tauri/src/collections.rs`）。
  // 那不是 1970 年 1 月 1 日，别把哨兵值当日期显示出来。
  if (!Number.isFinite(updatedAtMs) || updatedAtMs <= 0) return t("playlist.updatedUnknown");
  const at = new Date(updatedAtMs);
  if (at.toDateString() === new Date().toDateString()) return t("playlist.updatedNow");
  return t("playlist.updatedOn", { d: at.toLocaleDateString(locale) });
}
