import { useEffect, useRef } from "react";
import { loadCollections } from "@/lib/backend";
import type { StoredPlaylist } from "@/lib/playlists";
import { useLibraryStore } from "@/store/library";
import { EMPTY_FAVORITES, usePlayerStore } from "@/store/player";
import type { Favorites } from "@/types/generated/collections";

/**
 * 启动时把落盘的收藏与歌单灌进 store。
 *
 * ## 为什么要等曲库
 *
 * 与播放会话同一条：两者存的都是**曲目 ID**，而界面要的是专辑 ID 与曲目本体，中间那层
 * 换算得查当前曲库。曲库还没到位时算出来的是一份空映射，红心与歌单曲目要等到下一次
 * 重算才出现。
 *
 * 收藏与歌单只从数据库恢复一次；派生部分（专辑 ID 映射、歌单曲目）单独订阅曲库版本，
 * 每次整库替换都重算。两件事分开，既不会重复 IPC，也不会依赖页面内容的 key 去误判 App
 * 本身会重挂载。**重算是无损的**：歌单保留全部落盘曲目 ID，当前曲库里查不回的那些留在
 * 原地等文件回来，不会被这一趟抹掉。
 *
 * ## 读失败按「什么都没有」处理
 *
 * 与播放会话一致：读不到就当空，不弹错误框。真正需要惊动用户的是**写**失败——那时
 * 他刚点了红心、正等着结果，而这里他什么都没做。
 *
 * 浏览器预览（无后端）拿到 `null`，此时保留 store 里的种子演示数据：界面不该因为在
 * 浏览器里跑就一片空白，那样 UI 开发没法继续。
 */
export function useRestoreCollections() {
  const libraryVersion = useLibraryStore((state) => state.version);
  const initialLibraryVersion = useRef(libraryVersion);

  useEffect(() => {
    // 初次挂载保留浏览器预览的种子数据；真正的曲库替换会递增版本，再按权威成员重算。
    if (libraryVersion === initialLibraryVersion.current) return;
    const player = usePlayerStore.getState();
    player.recomputeFavoriteAlbums();
    player.rehydratePlaylists();
  }, [libraryVersion]);

  useEffect(() => {
    let cancelled = false;
    const baseline = usePlayerStore.getState().collectionsRevision;
    // 收藏与歌单共用一个版本号，因此这一份快照要么一起采纳、要么一起放弃：只对上一半
    // 会让「新建歌单并收藏」在中途作废时留下一个指向不存在歌单的收藏标记。
    const adopt = (favorites: Favorites, playlists: StoredPlaylist[]) => {
      const player = usePlayerStore.getState();
      if (player.restoreFavorites(favorites, baseline)) player.restorePlaylists(playlists);
    };
    void (async () => {
      try {
        const loaded = await loadCollections();
        if (cancelled || !loaded) return;
        // 用户若在读取期间点过红心或改过歌单，旧快照就作废；写入队列排空后会再读一次
        // 权威状态。
        adopt(loaded[0], loaded[1]);
      } catch (error) {
        console.error("读取收藏与歌单失败，本次按空处理", error);
        if (!cancelled) adopt(EMPTY_FAVORITES, []);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);
}
