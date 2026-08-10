import type { ReactNode } from "react";
import { useLibraryStore } from "@/store/library";
import { usePlayerStore } from "@/store/player";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import type { PlaybackError } from "@/types/generated/player";
import type { Messages } from "@/i18n/messages";

/**
 * 浮在播放条上方的提示条。
 *
 * 为什么要有它：这些失败原本都是**静默**的——按下播放什么也没发生、点了红心它自己灭掉、
 * 改完元数据重启就没了。没有提示的话，用户唯一能得到的信息是「这个播放器坏了」，
 * 而真实原因（文件被移走、磁盘满、装过新版本）各不相同，他要做的事也完全不同。
 *
 * 一次只显示一条。同时冒出两三条提示会互相抢注意力，而它们的紧急程度差别很大：
 * 播放失败是用户此刻正在做的事，存储损坏是他的数据出了问题——后者更重要，
 * 但前者是他刚刚按下的那一下，先回答他才不至于以为按键失灵。
 */

/** 播放错误类别 → 文案键。未知类别回落到解码失败（最中性的说法）。 */
const KIND_KEY: Record<string, keyof Messages> = {
  io: "player.error.io",
  unsupported: "player.error.unsupported",
  decode: "player.error.decode",
  noDevice: "player.error.noDevice",
  deviceConfig: "player.error.deviceConfig",
  stream: "player.error.stream",
};

type Translate = (k: keyof Messages, p?: Record<string, string | number>) => string;

function describePlayback(error: PlaybackError, t: Translate) {
  const key = KIND_KEY[error.kind] ?? "player.error.decode";
  // 编码名是内容不是文案，不进 i18n；读不出来时退回容器名，两个都没有就留空
  // ——`{codec}` 插值成空串好过显示一个「undefined」。
  return t(key, { codec: error.codec ?? error.container ?? "" });
}

/** 当前该显示哪一条。返回 `null` 表示没有要说的。 */
function pick(t: Translate): {
  text: string;
  /** 第二行细节，目前只有损坏残骸的路径。路径是内容不是文案，不翻译。 */
  detail?: string;
  /** 强调色竖条：真故障用重音色，非故障用弱色。 */
  urgent: boolean;
  action?: ReactNode;
  onDismiss: () => void;
} | null {
  const player = usePlayerStore.getState();
  const library = useLibraryStore.getState();

  if (player.error) {
    return {
      text: describePlayback(player.error, t),
      urgent: true,
      onDismiss: () => usePlayerStore.setState({ error: null }),
    };
  }

  const storage = library.storageDismissed ? null : library.storage;
  if (storage && storage.kind !== "ok") {
    const dismiss = () => useLibraryStore.getState().dismissStorage();
    if (storage.kind === "unavailable") {
      return {
        text: t("storage.unavailable", { message: storage.message }),
        urgent: true,
        onDismiss: dismiss,
      };
    }
    if (storage.kind === "schemaTooNew") {
      return {
        text: t("storage.schemaTooNew", {
          found: storage.found,
          supported: storage.supported,
        }),
        urgent: true,
        onDismiss: dismiss,
      };
    }
    return {
      text: t("storage.corrupt"),
      // 残骸路径必须给出来：那份文件里有用户手改的元数据，只说「损坏」等于让他知道
      // 出了事却无从补救。
      detail: t("storage.corruptPath", { path: storage.backup }),
      urgent: true,
      onDismiss: dismiss,
    };
  }

  if (player.collectionsWriteFailed) {
    return {
      text: t("collections.writeFailed"),
      urgent: true,
      onDismiss: () => usePlayerStore.setState({ collectionsWriteFailed: false }),
    };
  }

  if (player.needsLibrary) {
    return {
      text: t("player.needsLibrary"),
      urgent: false,
      action: (
        <button
          onClick={() => useUiStore.getState().setNav("settings")}
          className="flex-none cursor-pointer whitespace-nowrap rounded-lg px-2.5 py-1 text-[12px] font-medium text-ac transition-colors hover:bg-hv"
        >
          {t("nav.settings")}
        </button>
      ),
      onDismiss: () => usePlayerStore.setState({ needsLibrary: false }),
    };
  }

  return null;
}

export function NoticeBar() {
  const { t } = useT();
  // 逐项订阅，让 `pick` 里的 `getState()` 读到的总是最新值：整份 store 订阅会让
  // 每次播放进度更新都重渲染这条提示。
  const error = usePlayerStore((s) => s.error);
  const needsLibrary = usePlayerStore((s) => s.needsLibrary);
  const writeFailed = usePlayerStore((s) => s.collectionsWriteFailed);
  const storage = useLibraryStore((s) => s.storage);
  const storageDismissed = useLibraryStore((s) => s.storageDismissed);
  void [error, needsLibrary, writeFailed, storage, storageDismissed];

  const notice = pick(t);
  if (!notice) return null;

  return (
    <div className="pointer-events-none absolute inset-x-[26px] bottom-[104px] z-30 flex justify-center">
      <div className="surface-corners pointer-events-auto flex max-w-full items-center gap-2.5 rounded-[13px] border border-bd bg-pb px-3.5 py-2.5 shadow-lg">
        {/* 图标集里没有告警图案，不为一条提示去扩充它——错误与提示的区分交给颜色。 */}
        <span
          aria-hidden
          className={`h-4 w-[3px] flex-none rounded-full ${notice.urgent ? "bg-ac" : "bg-tx2/40"}`}
        />
        {/* 提示语可能较长，允许折行——它是说明文字，不是控件（见折行戒律③）。 */}
        <span className="min-w-0 text-[12.5px] leading-snug text-tx">
          {notice.text}
          {notice.detail && (
            // 路径可能很长且不含空格，`break-all` 让它断在任意字符上，
            // 否则会把整条提示撑出窗口。
            <span className="mt-0.5 block break-all text-[11.5px] text-tx2">{notice.detail}</span>
          )}
        </span>
        {notice.action}
        <button
          onClick={notice.onDismiss}
          className="flex-none cursor-pointer whitespace-nowrap rounded-lg px-2.5 py-1 text-[12px] font-medium text-tx2 transition-colors hover:bg-hv"
        >
          {t("player.error.dismiss")}
        </button>
      </div>
    </div>
  );
}
