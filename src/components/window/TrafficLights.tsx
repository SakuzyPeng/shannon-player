import { type MouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useT } from "@/i18n";
import { isMacPlatform } from "@/lib/platform";

/**
 * 交通灯占位尺寸。
 *
 * 自绘那版是三个 12px 圆 + 两段 8px 间隔 = 52px；macOS 27 的系统按钮更大
 * （直径约 14、中心距 23，整组约 60px），所以按 52 去算居中会偏右 4px——
 * 用 84px 侧栏当标尺实测出来的。位置在 `tauri.macos.conf.json` 的
 * `trafficLightPosition` 里对齐，那两个数字**只在 macOS 27 上实测过**，
 * Tauri 自己也提醒标题栏高度随系统版本变，换版本要重新量。
 */
const RAIL_HEIGHT = 26;
const GROUP_WIDTH = 60;

/**
 * 窗口控制按钮。
 *
 * **macOS 用系统自带的，这里只留一块占位**。曾经三平台统一自绘，理由是设计稿画的
 * 就是 macOS 交通灯；但绿灯在 macOS 上远不止「最大化」——hover 会展开窗口平铺面板
 * （移动与调整大小 / 填充与排列 / 全屏幕），其中「排列」要摆布**其他应用**的窗口，
 * 是系统私有能力，自绘再怎么仿也拿不到。于是 macOS 改走
 * `decorations: true` + `titleBarStyle: "Overlay"`（见 `tauri.macos.conf.json`）：
 * 系统画窗口，交通灯浮在内容上，连带窗口圆角、投影、双击标题栏缩放、边缘拖拽
 * 全部回归原生，位置用 `trafficLightPosition` 对齐到本占位。
 *
 * Windows / Linux 没有这套语义，继续自绘（外观本就仿 macOS，视觉不分叉），
 * 绿灯即最大化。
 */
export function TrafficLights() {
  const { t } = useT();

  // 系统按钮浮在内容之上，不占布局；这里留出等大的空白，让「香」字与导航项
  // 从它下面开始排，否则会被压在按钮底下。
  if (isMacPlatform()) {
    return <div aria-hidden style={{ height: RAIL_HEIGHT, width: GROUP_WIDTH }} />;
  }

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
    <div className="group flex items-center gap-2 scale-[0.86]" style={{ height: RAIL_HEIGHT }}>
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
