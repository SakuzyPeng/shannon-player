import { usePlayerStore } from "@/store/player";
import type { Id, Track } from "@/types/player";

/** 「添加到歌单」菜单项的 arg 哨兵：新建歌单并加入。 */
export const NEW_PLAYLIST = "__new__";

/**
 * 统一处理菜单回调的 arg：歌单 ID → 加入；NEW_PLAYLIST → 以默认名新建并加入。
 *
 * 新建那条要等后端发号（见 `createPlaylistWithTracks`），但调用点是个菜单项、没有可等的
 * 地方，所以在这里 fire-and-forget。失败已经在 store 里记过日志，也不会在界面上留下
 * 一个库里不存在的歌单。
 *
 * 整单操作须把落盘的 `sourceTrackIds` 一并传来；`tracks` 只是当前曲库能水合出的子集。
 */
export function addTracksToPlaylistArg(
  arg: string,
  tracks: Track[],
  newPlaylistName: string,
  sourceTrackIds?: Id[],
): void {
  const player = usePlayerStore.getState();
  if (arg === NEW_PLAYLIST) {
    void player.createPlaylistWithTracks(newPlaylistName, tracks, sourceTrackIds);
  } else {
    player.addToPlaylist(arg, tracks, sourceTrackIds);
  }
}
