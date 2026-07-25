/**
 * 桌面外壳手感修补：仅在 Tauri 原生窗口内生效。
 *
 * WebView 默认会在右键时弹出浏览器上下文菜单（重新加载 / 返回 / 检查元素等），
 * 这对一个本地播放器是非预期的。此处兜底拦截 contextmenu，只在输入类控件上放行
 * （保留复制 / 粘贴）。
 *
 * 必须挂在冒泡阶段并让出 defaultPrevented 的事件：Radix 的 composeEventHandlers
 * 默认 checkForDefaultPrevented，若在捕获阶段抢先 preventDefault，其 Trigger 会
 * 认为事件已被处理而跳过 handleOpen，导致应用自身的右键菜单一并打不开。冒泡阶段
 * 运行时 Radix 已开菜单并自行 preventDefault，此处直接放行即可。
 *
 * 浏览器 dev 环境（pnpm dev / Playwright 校验）不做拦截，保留原生右键便于调试。
 */
export function installNativeChrome(): void {
  const isTauri = "__TAURI_INTERNALS__" in window;
  if (!isTauri) return;

  document.addEventListener("contextmenu", (e) => {
    // 应用自身的右键菜单已接管（Radix Trigger 会 preventDefault），无需再拦。
    if (e.defaultPrevented) return;
    const target = e.target as HTMLElement | null;
    if (target?.closest('input, textarea, [contenteditable="true"]')) return;
    e.preventDefault();
  });
}
