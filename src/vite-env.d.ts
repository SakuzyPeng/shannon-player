/// <reference types="vite/client" />

// Vite 注入的 import.meta.env（DEV / PROD 等）的类型声明。
// 项目此前没用到它，接入 dev-only 的 store 暴露（见 main.tsx）后才需要。

interface Window {
  /**
   * 仅 dev 构建存在：把 Zustand store 暴露到控制台，便于在浏览器预览里灌入
   * 真实曲库快照来核对 UI（用法见 `src/main.tsx`）。生产产物里不存在。
   */
  __shannon?: {
    library: unknown;
    player: unknown;
    ui: unknown;
  };
}
