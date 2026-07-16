import { useEffect, useRef } from "react";
import { useUiStore } from "@/store/ui";

/** 把 UI store 的主题解析成 light/dark 并写到 <html data-theme>；system 跟随系统偏好。 */
export function useApplyTheme(): void {
  const theme = useUiStore((s) => s.theme);
  const appliedOnce = useRef(false);

  useEffect(() => {
    const mql = window.matchMedia("(prefers-color-scheme: dark)");
    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
    const root = document.documentElement;
    let transitionTimer = 0;

    const resolve = () => {
      const dark = theme === "dark" || (theme === "system" && mql.matches);
      if (appliedOnce.current && !reduceMotion.matches) {
        root.classList.add("theme-transitioning");
        // 先让 transition 属性生效，再切换变量值。
        void root.offsetWidth;
      }
      root.setAttribute("data-theme", dark ? "dark" : "light");
      appliedOnce.current = true;
      window.clearTimeout(transitionTimer);
      transitionTimer = window.setTimeout(() => root.classList.remove("theme-transitioning"), 220);
    };

    resolve();
    if (theme === "system") {
      mql.addEventListener("change", resolve);
      return () => {
        mql.removeEventListener("change", resolve);
        window.clearTimeout(transitionTimer);
        root.classList.remove("theme-transitioning");
      };
    }
    return () => {
      window.clearTimeout(transitionTimer);
      root.classList.remove("theme-transitioning");
    };
  }, [theme]);
}
