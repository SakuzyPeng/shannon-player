import { useEffect } from "react";
import { useUiStore } from "@/store/ui";

/** 把 UI store 的主题解析成 light/dark 并写到 <html data-theme>；system 跟随系统偏好。 */
export function useApplyTheme(): void {
  const theme = useUiStore((s) => s.theme);

  useEffect(() => {
    const mql = window.matchMedia("(prefers-color-scheme: dark)");

    const resolve = () => {
      const dark = theme === "dark" || (theme === "system" && mql.matches);
      document.documentElement.setAttribute("data-theme", dark ? "dark" : "light");
    };

    resolve();
    if (theme === "system") {
      mql.addEventListener("change", resolve);
      return () => mql.removeEventListener("change", resolve);
    }
  }, [theme]);
}
