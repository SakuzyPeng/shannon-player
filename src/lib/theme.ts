import type { ThemeMode } from "@/types/player";

export type ResolvedTheme = "light" | "dark";

/** 把三态主题解析成真正应用到页面的明暗主题。 */
export function resolveTheme(theme: ThemeMode, systemDark: boolean): ResolvedTheme {
  return theme === "dark" || (theme === "system" && systemDark) ? "dark" : "light";
}

/**
 * 同步把主题写到 `<html>`。
 *
 * 启动时必须在原生窗口显示前调用；运行中则由 `useApplyTheme` 调用并负责过渡动画与
 * 系统主题监听。两处共用这一函数，避免首帧与后续切换各写一套解析规则。
 */
export function applyTheme(
  theme: ThemeMode,
  systemDark = window.matchMedia("(prefers-color-scheme: dark)").matches,
): void {
  document.documentElement.setAttribute("data-theme", resolveTheme(theme, systemDark));
}
