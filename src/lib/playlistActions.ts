import { usePlayerStore } from "@/store/player";
import type { Track } from "@/types/player";

/** 「添加到歌单」菜单项的 arg 哨兵：新建歌单并加入。 */
export const NEW_PLAYLIST = "__new__";

/** 统一处理菜单回调的 arg：歌单 ID → 加入；NEW_PLAYLIST → 以默认名新建并加入。 */
export function addTracksToPlaylistArg(arg: string, tracks: Track[], newPlaylistName: string): void {
  const player = usePlayerStore.getState();
  if (arg === NEW_PLAYLIST) player.createPlaylistWithTracks(newPlaylistName, tracks);
  else player.addToPlaylist(arg, tracks);
}
