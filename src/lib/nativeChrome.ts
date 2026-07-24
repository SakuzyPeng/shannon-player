/**
 * 桌面外壳手感修补：仅在 Tauri 原生窗口内生效。
 *
 * WebView 默认会在右键时弹出浏览器上下文菜单（重新加载 / 返回 / 检查元素等），
 * 这对一个本地播放器是非预期的。此处全局拦截 contextmenu，只在输入类控件上放行
 * （保留复制 / 粘贴）；应用自身的 Radix 右键菜单由其 Trigger 各自处理，不受影响。
 *
 * 浏览器 dev 环境（pnpm dev / Playwright 校验）不做拦截，保留原生右键便于调试。
 */
export function installNativeChrome(): void {
  const isTauri = "__TAURI_INTERNALS__" in window;
  if (!isTauri) return;

  document.addEventListener(
    "contextmenu",
    (e) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest('input, textarea, [contenteditable="true"]')) return;
      e.preventDefault();
    },
    { capture: true },
  );
}
