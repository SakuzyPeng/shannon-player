import { useEffect } from "react";
import { loadCollections } from "@/lib/backend";
import { usePlayerStore } from "@/store/player";

/**
 * 启动时把落盘的收藏灌进 store。
 *
 * ## 为什么要等曲库
 *
 * 与播放会话同一条：专辑收藏存的是**曲目 ID**，而界面按专辑 ID 读，中间那层换算得
 * 查当前曲库。曲库还没到位时算出来的是一份空映射，红心要等到下一次重算才亮。
 *
 * 这里不自己等——`App` 以曲库版本为 key 强制重挂载（见 `src/App.tsx`），所以整库
 * 一换本 hook 就重新跑一遍，恢复与重算都跟着发生。曲目、歌手、歌单三类收藏不依赖
 * 曲库，先亮上没有坏处。
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
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const loaded = await loadCollections();
        if (cancelled || !loaded) return;
        usePlayerStore.getState().restoreFavorites(loaded[0]);
      } catch (error) {
        console.error("读取收藏失败，本次按没有收藏处理", error);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);
}
