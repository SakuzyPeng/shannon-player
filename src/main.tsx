import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
// 衬线字体随应用打包（可变字重 400–700），离线可用，不依赖运行时 CDN。
import "@fontsource-variable/lora";
import "@fontsource-variable/noto-serif-sc";
// AMLL 歌词播放器基础样式（主题适配覆盖见 index.css 尾部）。
import "@applemusic-like-lyrics/core/style.css";
import "./index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
