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
        className="grid size-3 place-items-center rounded-full bg-[#FF5F57] shadow-[inset_0_0_0_0.5px_rgba(0,0,0,0.14)]"
      >
        <svg width="8" height="8" viewBox="0 0 24 24" className={glyph} stroke="#5A0000" strokeWidth={2.6} strokeLinecap="round">
          <path d="M7 7l10 10 M17 7l-10 10" />
        </svg>
      </button>
      <button
        aria-label={t("window.minimize")}
        onClick={run((w) => w.minimize())}
        className="grid size-3 place-items-center rounded-full bg-[#FEBC2E] shadow-[inset_0_0_0_0.5px_rgba(0,0,0,0.14)]"
      >
        <svg width="8" height="8" viewBox="0 0 24 24" className={glyph} stroke="#5A3A00" strokeWidth={2.6} strokeLinecap="round">
          <path d="M6 12h12" />
        </svg>
      </button>
      <button
        aria-label={t("window.maximize")}
        onClick={run((w) => w.toggleMaximize())}
        className="grid size-3 place-items-center rounded-full bg-[#28C840] shadow-[inset_0_0_0_0.5px_rgba(0,0,0,0.14)]"
      >
        <svg width="8" height="8" viewBox="0 0 24 24" className={glyph} stroke="#003D0B" strokeWidth={2.4} strokeLinecap="round">
          <path d="M12 6v12 M6 12h12" />
        </svg>
      </button>
    </div>
  );
}
