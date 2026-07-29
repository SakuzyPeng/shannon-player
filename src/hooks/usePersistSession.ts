import { useEffect } from "react";
import { usePlayerStore } from "@/store/player";

/**
 * 把播放会话持续写回后端。
 *
 * ## 两种变化，两种节奏
 *
 * 队列、切歌、循环与随机状态是**离散**的，变一次写一次（去抖 500 ms，
 * 覆盖住拖拽重排那种连续触发）；播放位置是**连续**的，每秒 5 个事件，
 * 跟着写盘等于把 SSD 当秒表用。位置因此单独按 5 秒节流。
 *
 * 位置的精度到这里就够了：恢复时最多差 5 秒，而用户对"上次听到哪"的记忆本来就
 * 粗于此。为省这几秒去每 200 ms 写一次盘，是拿一个没人察觉的收益换真实的磨损。
 *
 * ## 退出时必须再写一次
 *
 * 节流意味着最后 5 秒的进度还在内存里。`beforeunload` 时补一次同步保存，
 * 否则「听到一半关掉应用」——也就是最常见的退出方式——恰好是丢得最多的那种。
 */

/** 离散状态的去抖窗口。 */
const DEBOUNCE_MS = 500;
/** 播放位置的节流间隔。 */
const POSITION_THROTTLE_MS = 5000;

export function usePersistSession() {
  useEffect(() => {
    let debounce: ReturnType<typeof setTimeout> | null = null;
    let lastPositionWrite = 0;

    const persist = () => usePlayerStore.getState().persistSession();

    const unsubscribe = usePlayerStore.subscribe((state, prev) => {
      // 位置单独走节流：它每秒变 5 次，不能与其它变化同等对待。
      const positionOnly =
        state.queue === prev.queue &&
        state.currentIndex === prev.currentIndex &&
        state.repeat === prev.repeat &&
        state.shuffle === prev.shuffle &&
        state.shuffleOrder === prev.shuffleOrder &&
        state.volume === prev.volume &&
        state.muted === prev.muted;

      if (positionOnly) {
        if (state.progress.positionSec === prev.progress.positionSec) return;
        const now = performance.now();
        if (now - lastPositionWrite < POSITION_THROTTLE_MS) return;
        lastPositionWrite = now;
        persist();
        return;
      }

      if (debounce !== null) clearTimeout(debounce);
      debounce = setTimeout(persist, DEBOUNCE_MS);
    });

    // 关窗前补一次：节流会把最后几秒留在内存里，而「听到一半关掉」正是最常见的退出方式。
    const onUnload = () => persist();
    window.addEventListener("beforeunload", onUnload);

    return () => {
      unsubscribe();
      if (debounce !== null) clearTimeout(debounce);
      window.removeEventListener("beforeunload", onUnload);
      // 卸载时也存一次，覆盖开发期热重载与将来可能的多窗口场景。
      //
      // 这一句在 StrictMode 下会在**应用刚启动时**就执行（mount → unmount → mount），
      // 那时队列还是种子演示曲目。真正挡住它的是 `persistSession` 里的 `sessionReady`
      // 守卫——不能指望「卸载」一定意味着「用完了」。
      persist();
    };
  }, []);
}
