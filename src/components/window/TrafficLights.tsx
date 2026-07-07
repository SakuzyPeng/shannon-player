import type { MouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useT } from "@/i18n";

/** 自绘 macOS 交通灯，接入真实 Tauri 窗口控制。非 Tauri 环境（纯浏览器预览）静默降级。 */
export function TrafficLights() {
  const { t } = useT();
  const run =
    (fn: (w: ReturnType<typeof getCurrentWindow>) => Promise<unknown>) =>
    async (e: MouseEvent) => {
      e.stopPropagation();
      try {
        await fn(getCurrentWindow());
      } catch {
        /* 浏览器预览下无 Tauri IPC，忽略 */
      }
    };

  const glyph = "opacity-0 transition-opacity group-hover:opacity-100";

  return (
    <div className="group flex h-[26px] items-center gap-2 scale-[0.86]">
      <button
        aria-label={t("window.close")}
        onClick={run((w) => w.close())}
        className="traffic-close traffic-light grid size-3 place-items-center rounded-full"
      >
        <svg width="8" height="8" viewBox="0 0 24 24" className={`${glyph} traffic-close-glyph`} strokeWidth={2.6} strokeLinecap="round">
          <path d="M7 7l10 10 M17 7l-10 10" />
        </svg>
      </button>
      <button
        aria-label={t("window.minimize")}
        onClick={run((w) => w.minimize())}
        className="traffic-light traffic-minimize grid size-3 place-items-center rounded-full"
      >
        <svg width="8" height="8" viewBox="0 0 24 24" className={`${glyph} traffic-minimize-glyph`} strokeWidth={2.6} strokeLinecap="round">
          <path d="M6 12h12" />
        </svg>
      </button>
      <button
        aria-label={t("window.maximize")}
        onClick={run((w) => w.toggleMaximize())}
        className="traffic-light traffic-maximize grid size-3 place-items-center rounded-full"
      >
        <svg width="8" height="8" viewBox="0 0 24 24" className={`${glyph} traffic-maximize-glyph`} strokeWidth={2.4} strokeLinecap="round">
          <path d="M12 6v12 M6 12h12" />
        </svg>
      </button>
    </div>
  );
}
