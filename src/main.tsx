import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { installNativeChrome } from "@/lib/nativeChrome";
// 衬线字体随应用打包（可变字重 400–700），离线可用，不依赖运行时 CDN。
import "@fontsource-variable/lora";
import "@fontsource-variable/noto-serif-sc";
import "./index.css";

installNativeChrome();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
