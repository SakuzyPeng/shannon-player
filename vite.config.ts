import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// @tauri-apps/cli 会以 TAURI_ENV_* 注入环境，这里遵循 Tauri 官方推荐配置。
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react(), tailwindcss()],

  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },

  // Tauri 需要一个固定端口 + 不清屏，方便看到 Rust 日志。
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: {
      // 别去监听 Rust 侧文件。
      ignored: ["**/src-tauri/**"],
    },
    fs: {
      // 允许经 /@fs/ 读取应用数据目录下的封面缩略图。
      // 真实封面只有原生窗口里才有，而 UI 验证走的是浏览器里的 dev server
      // （见 CLAUDE.md），开这个口子才能在浏览器里核对封面效果。仅 dev 生效。
      allow: [
        fileURLToPath(new URL(".", import.meta.url)),
        `${process.env.HOME}/Library/Application Support/com.shannon.player`,
      ],
    },
  },

  // 面向 Tauri 使用的现代 webview，产物无需兼容旧浏览器。
  build: {
    target: "safari15",
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
