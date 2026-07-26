import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "@/lib/backend";
import { isMacPlatform } from "@/lib/platform";

/**
 * 自绘窗口外观的状态开关（配套样式见 index.css 的「窗口圆角」一节）。
 *
 * **只有 Windows / Linux 需要它**。那两个平台关掉了系统标题栏
 * （`decorations: false`）以换取自绘交通灯与整块可拖拽的头部，代价是窗口连系统
 * 圆角一起丢了，成了个直角矩形——圆角只能自己画，而画之前得先让窗口透明
 * （`transparent: true`），否则圆角外的四个角是不透明底色，看着仍是直角。
 * macOS 走 `decorations: true` + `titleBarStyle: "Overlay"`（见
 * `tauri.macos.conf.json`），窗口是系统画的，圆角、投影本来就有，这里直接退出。
 *
 * 两个 data 属性：
 * - `data-window-chrome="custom"` —— 只在需要自绘窗口外观时打上。浏览器预览与
 *   macOS 都不打：前者的窗口是浏览器画的，再切一圈圆角只会在四角露出白色小三角。
 * - `data-window-fit="full"` —— 窗口铺满、四角不该留圆角时打上。Windows 11 的
 *   最大化窗口就是直角，全屏同理，两者都要查。
 */
export function useWindowChrome() {
  useEffect(() => {
    if (!isTauri() || isMacPlatform()) return;
    const root = document.documentElement;
    root.dataset.windowChrome = "custom";

    let cancelled = false;
    let unlisten: (() => void) | undefined;

    const sync = async () => {
      try {
        const win = getCurrentWindow();
        const [maximized, fullscreen] = await Promise.all([
          win.isMaximized(),
          win.isFullscreen(),
        ]);
        if (!cancelled) {
          root.dataset.windowFit = maximized || fullscreen ? "full" : "floating";
        }
      } catch {
        // IPC 或权限不可用时保持圆角：最坏是最大化时四角透出一点桌面，
        // 比整窗直角要好。
      }
    };

    // 去抖：最大化 / 全屏是离散事件，但拖拽窗口边缘会连续触发 resize，
    // 不去抖就是每帧两次 IPC。120ms 后再查，视觉上察觉不到延迟。
    let timer = 0;
    const schedule = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => void sync(), 120);
    };

    void sync();
    // 两个来源都监听：Tauri 的窗口事件语义更准，DOM 的 resize 不依赖任何权限，
    // 万一前者的权限没配上，圆角仍能跟着窗口状态走。去抖已经把重复合并掉了。
    window.addEventListener("resize", schedule);
    void getCurrentWindow()
      .onResized(schedule)
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
      window.removeEventListener("resize", schedule);
      unlisten?.();
      delete root.dataset.windowChrome;
      delete root.dataset.windowFit;
    };
  }, []);
}
