import { usePlayerStore } from "@/store/player";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import type { PlaybackError } from "@/types/generated/player";
import type { Messages } from "@/i18n/messages";

/**
 * 播放失败 / 需要曲库的提示条，浮在播放条上方。
 *
 * 为什么要有它：引擎的失败是**静默**的——按下播放，什么也没发生。没有提示的话
 * 用户唯一能得到的信息就是「这个播放器坏了」，而实际原因可能只是文件被移走了。
 *
 * 措辞按错误类别分开，因为用户要做的事完全不同：文件没了要去找文件，
 * 格式不支持要去转码，设备被占用要去关别的应用。把它们统一成「播放失败」
 * 等于把唯一有用的那点信息丢掉。
 */

/** 错误类别 → 文案键。未知类别回落到解码失败（最中性的说法）。 */
const KIND_KEY: Record<string, keyof Messages> = {
  io: "player.error.io",
  unsupported: "player.error.unsupported",
  decode: "player.error.decode",
  noDevice: "player.error.noDevice",
  deviceConfig: "player.error.deviceConfig",
  stream: "player.error.stream",
};

function describe(error: PlaybackError, t: (k: keyof Messages, p?: Record<string, string | number>) => string) {
  const key = KIND_KEY[error.kind] ?? "player.error.decode";
  // 编码名是内容不是文案，不进 i18n；读不出来时退回容器名，两个都没有就留空
  // ——`{codec}` 插值成空串好过显示一个「undefined」。
  return t(key, { codec: error.codec ?? error.container ?? "" });
}

export function PlaybackNotice() {
  const { t } = useT();
  const error = usePlayerStore((s) => s.error);
  const needsLibrary = usePlayerStore((s) => s.needsLibrary);

  if (!error && !needsLibrary) return null;

  return (
    <div className="pointer-events-none absolute inset-x-[26px] bottom-[104px] z-30 flex justify-center">
      <div className="surface-corners pointer-events-auto flex max-w-full items-center gap-2.5 rounded-[13px] border border-bd bg-pb px-3.5 py-2.5 shadow-lg">
        {/* 图标集里没有告警图案，不为一条提示去扩充它——错误与提示的区分交给颜色。 */}
        <span
          aria-hidden
          className={`h-4 w-[3px] flex-none rounded-full ${error ? "bg-ac" : "bg-tx2/40"}`}
        />
        {/* 提示语可能较长，允许折行——它是说明文字，不是控件（见折行戒律③）。 */}
        <span className="min-w-0 text-[12.5px] leading-snug text-tx">
          {error ? describe(error, t) : t("player.needsLibrary")}
        </span>
        {needsLibrary && !error && (
          <button
            onClick={() => useUiStore.getState().setNav("settings")}
            className="flex-none cursor-pointer whitespace-nowrap rounded-lg px-2.5 py-1 text-[12px] font-medium text-ac transition-colors hover:bg-hv"
          >
            {t("nav.settings")}
          </button>
        )}
        <button
          onClick={() => usePlayerStore.setState({ error: null, needsLibrary: false })}
          className="flex-none cursor-pointer whitespace-nowrap rounded-lg px-2.5 py-1 text-[12px] font-medium text-tx2 transition-colors hover:bg-hv"
        >
          {t("player.error.dismiss")}
        </button>
      </div>
    </div>
  );
}
