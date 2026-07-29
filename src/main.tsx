import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { installNativeChrome } from "@/lib/nativeChrome";
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

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
