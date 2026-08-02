import { useEffect, useRef } from "react";
import { loadCollections } from "@/lib/backend";
import { useLibraryStore } from "@/store/library";
import { EMPTY_FAVORITES, usePlayerStore } from "@/store/player";

/**
 * 启动时把落盘的收藏灌进 store。
 *
 * ## 为什么要等曲库
 *
 * 与播放会话同一条：专辑收藏存的是**曲目 ID**，而界面按专辑 ID 读，中间那层换算得
 * 查当前曲库。曲库还没到位时算出来的是一份空映射，红心要等到下一次重算才亮。
 *
 * 收藏只从数据库恢复一次；专辑 ID 的派生则单独订阅曲库版本，每次整库替换都重算。
 * 两件事分开，既不会重复 IPC，也不会依赖页面内容的 key 去误判 App 本身会重挂载。
 *
 * ## 读失败按「没有收藏」处理
 *
 * 与播放会话一致：读不到就当空，不弹错误框。真正需要惊动用户的是**写**失败——那时
 * 他刚点了红心、正等着结果，而这里他什么都没做。
 *
 * 浏览器预览（无后端）拿到 `null`，此时保留 store 里的种子演示收藏：界面不该因为在
 * 浏览器里跑就一片空白，那样 UI 开发没法继续。
 */
export function useRestoreCollections() {
  const libraryVersion = useLibraryStore((state) => state.version);
  const initialLibraryVersion = useRef(libraryVersion);

  useEffect(() => {
    // 初次挂载保留浏览器预览的种子收藏；真正的曲库替换会递增版本，再按权威成员重算。
    if (libraryVersion === initialLibraryVersion.current) return;
    usePlayerStore.getState().recomputeFavoriteAlbums();
  }, [libraryVersion]);

  useEffect(() => {
    let cancelled = false;
    const baseline = usePlayerStore.getState().favoritesRevision;
    void (async () => {
      try {
        const loaded = await loadCollections();
        if (cancelled || !loaded) return;
        // 用户若在读取期间点过红心，旧快照就作废；写入队列排空后会再读一次权威状态。
        usePlayerStore.getState().restoreFavorites(loaded[0], baseline);
      } catch (error) {
        console.error("读取收藏失败，本次按没有收藏处理", error);
        if (!cancelled) {
          usePlayerStore.getState().restoreFavorites(EMPTY_FAVORITES, baseline);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);
}
