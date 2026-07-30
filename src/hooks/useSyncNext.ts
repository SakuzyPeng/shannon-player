import { useEffect } from "react";
import { usePlayerStore } from "@/store/player";

/**
 * 把「下一首是谁」持续告诉引擎。
 *
 * 无缝换曲要求引擎**提前**拿到下一首：它得在当前这首解码完之前把文件打开、在环形缓冲
 * 里打好边界点。而「下一首是谁」取决于队列、随机顺序与循环模式，三者的权威都在前端。
 *
 * ## 为什么做成订阅，而不是在每个队列动作里调用
 *
 * 队列动作有十几个（入队、插播、移除、清空接下来、拖拽重排、切歌、开关随机…），
 * 漏掉任何一个的表现都是「偶尔不无缝」——那种缺陷难复现、更难归因。放在这里，
 * 将来新增的队列动作自动被覆盖，不需要谁记得补一行。
 *
 * 与 `useLoudnessQueue` 是同一个套路（顺序由前端给，具体处理交给后端），
 * 区别只在于那边关心「整条队列的顺序」，这边只关心「紧接着的那一个」。
 *
 * 去抖 80 ms：拖拽重排会连续触发，而每次重发都让引擎丢掉已经预解码好的那份、
 * 重新打开文件。上限要**远小于**「当前这首的剩余时长」，否则改动会赶不上边界。
 */
const DEBOUNCE_MS = 80;

export function useSyncNext() {
  useEffect(() => {
    let debounce: ReturnType<typeof setTimeout> | null = null;

    const schedule = () => {
      if (debounce !== null) clearTimeout(debounce);
      debounce = setTimeout(() => usePlayerStore.getState().syncNext(), DEBOUNCE_MS);
    };

    const unsubscribe = usePlayerStore.subscribe((state, prev) => {
      // 只认影响「下一首是谁」的那几样。进度每秒变 5 次，与它无关。
      if (
        state.queue !== prev.queue ||
        state.currentIndex !== prev.currentIndex ||
        state.shuffleOrder !== prev.shuffleOrder ||
        state.repeat !== prev.repeat ||
        // 引擎换了代际（装载完成、或越过边界接上了新曲）：上一份指定已经被用掉。
        state.activeLoadId !== prev.activeLoadId
      ) {
        schedule();
      }
    });

    return () => {
      unsubscribe();
      if (debounce !== null) clearTimeout(debounce);
    };
  }, []);
}
