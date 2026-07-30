import { StrictMode } from "react";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import { isTauri, loadSettings } from "@/lib/backend";
import { installNativeChrome } from "@/lib/nativeChrome";
import { fromSettings } from "@/lib/settings";
import { applyTheme } from "@/lib/theme";
import { useUiStore } from "@/store/ui";
// 衬线字体随应用打包（可变字重 400–700），离线可用，不依赖运行时 CDN。
import "@fontsource-variable/lora";
import "@fontsource-variable/noto-serif-sc";
import "./index.css";

installNativeChrome();

// 浏览器预览（无 Tauri）里没有真实曲库，而 UI 验证恰恰走 dev server（见 CLAUDE.md）。
// 暴露 store 后可在控制台灌入真实快照来核对封面、长标题等真实数据下的表现：
//   const s = await (await fetch("/@fs/…/library-snapshot.json")).json();
//   __shannon.library.getState().setCoverDir("/@fs/…/covers");
//   __shannon.library.getState().setLibrary(s);
// 仅 dev 构建存在，生产产物里会被摇掉。
if (import.meta.env.DEV) {
  void Promise.all([
    import("@/store/library"),
    import("@/store/player"),
    import("@/store/ui"),
    import("@/lib/session"),
  ]).then(([library, player, ui, session]) => {
    (window as unknown as Record<string, unknown>).__shannon = {
      library: library.useLibraryStore,
      player: player.usePlayerStore,
      ui: ui.useUiStore,
      // 会话的解析是纯函数，挂出来便于拿真实 JSON 当场验证恢复逻辑。
      session,
    };
  });
}

/**
 * 先取回落盘的界面设置、同步应用主题、提交首屏，最后显示原生窗口。
 *
 * **只把读取放在 `createRoot` 前仍然不够**：等待 IPC 时 CSS 已经能把默认浅色背景画出来，
 * 而 `useApplyTheme` 要到挂载后的 effect 才写 `data-theme`。因此 Tauri 配置把窗口设为
 * `visible: false`；这里恢复 store 后立即把主题写到 `<html>`，再用 `flushSync` 提交首屏，
 * 两件事都完成后才 `show()`。浏览器预览没有原生窗口，仍然直接渲染。
 *
 * 顺带解决了另一件事：设置在首帧之前就位，`usePersistSettings` 于是不需要 `sessionReady`
 * 那样的就绪守卫——任何时刻写出去的都已经是用户的设置，而不是默认值。
 */
async function boot() {
  try {
    let json: string | null = null;
    try {
      json = await loadSettings();
    } catch (error) {
      // 设置可随手重建；IPC 异常也只能回落默认，不能让一份设置挡住整个应用启动。
      console.warn("读取界面设置失败，使用默认值", error);
    }

    const restored = json ? fromSettings(json) : null;
    if (restored) useUiStore.getState().hydrateSettings(restored);

    // 不能留给 useEffect：窗口一旦显示，首帧就必须已经拿到正确的 CSS 变量。
    applyTheme(useUiStore.getState().theme);

    const root = createRoot(document.getElementById("root")!);
    flushSync(() => {
      root.render(
        <StrictMode>
          <App />
        </StrictMode>,
      );
    });
  } finally {
    if (isTauri()) {
      try {
        await getCurrentWindow().show();
      } catch (error) {
        // 外壳另有 3 秒兜底显示；这里留日志，避免权限或 IPC 问题悄无声息。
        console.error("显示主窗口失败", error);
      }
    }
  }
}

void boot().catch((error) => console.error("应用启动失败", error));
