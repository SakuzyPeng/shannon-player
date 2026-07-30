import { useEffect } from "react";
import { saveSettings } from "@/lib/backend";
import { toSettings } from "@/lib/settings";
import { useUiStore } from "@/store/ui";

/**
 * 把界面设置持续写回后端。
 *
 * 比播放会话简单得多：设置只在用户点击时变化，没有「每秒 5 次的进度」那种连续量，
 * 因此只需要一层去抖——主题分段控件是三个按钮，手快连点两下不该写两次盘。
 *
 * ## 这里不需要「就绪守卫」
 *
 * 会话那边有 `sessionReady`，因为 StrictMode 的 mount → unmount → mount 会让 cleanup 里
 * 的补写在**应用刚启动**时执行，把种子队列写进文件。设置这边不会：落盘的值在
 * `createRoot` 之前就已经灌进 store（见 `src/main.tsx`），所以任何时刻写出去的都是
 * 「用户的设置」而不是「默认值」。这不是巧合，是把读取放在首帧之前换来的——
 * 那件事本来是为了避免主题白闪。
 */

const DEBOUNCE_MS = 300;

export function usePersistSettings() {
  useEffect(() => {
    let debounce: ReturnType<typeof setTimeout> | null = null;

    const persist = () => {
      void saveSettings(JSON.stringify(toSettings(useUiStore.getState())));
    };

    const unsubscribe = useUiStore.subscribe((state, prev) => {
      // 只认落盘的那几项。导航、详情页、歌词页开关都是「当下在看什么」，不是设置。
      if (
        state.theme === prev.theme &&
        state.view === prev.view &&
        state.language === prev.language &&
        state.settings === prev.settings
      ) {
        return;
      }
      if (debounce !== null) clearTimeout(debounce);
      debounce = setTimeout(persist, DEBOUNCE_MS);
    });

    // 关窗前补一次：去抖窗口里正好按了 Cmd+Q 的话，那次改动本来会连同定时器一起消失。
    const onUnload = () => persist();
    window.addEventListener("beforeunload", onUnload);

    return () => {
      unsubscribe();
      window.removeEventListener("beforeunload", onUnload);
      // 有待写的改动就立刻写掉，而不是把定时器一清了事。
      if (debounce !== null) {
        clearTimeout(debounce);
        persist();
      }
    };
  }, []);
}
