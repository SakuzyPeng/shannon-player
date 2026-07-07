import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { TrafficLights } from "@/components/window/TrafficLights";
import { Icon, type IconName } from "@/components/common/Icon";
import { LANGUAGES, NAV_ITEMS } from "@/data/library";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";
import type { MessageKey } from "@/i18n/messages";
import type { Language } from "@/types/player";

const THEME_ICON: Record<string, { icon: IconName; labelKey: MessageKey }> = {
  light: { icon: "sun", labelKey: "theme.light" },
  dark: { icon: "moon", labelKey: "theme.dark" },
  system: { icon: "monitor", labelKey: "theme.system" },
};

export function IconRail() {
  const { t } = useT();
  const nav = useUiStore((s) => s.nav);
  const setNav = useUiStore((s) => s.setNav);
  const theme = useUiStore((s) => s.theme);
  const cycleTheme = useUiStore((s) => s.cycleTheme);
  const language = useUiStore((s) => s.language);
  const setLanguage = useUiStore((s) => s.setLanguage);

  const themeInfo = THEME_ICON[theme];

  return (
    <div
      data-tauri-drag-region
      className="flex w-[84px] flex-shrink-0 flex-col items-center gap-2.5 border-r border-bd bg-sb px-0 py-4 pb-[18px] transition-colors"
    >
      <TrafficLights />

      <div className="mb-2 mt-3 font-serif text-[18px] font-semibold text-ac">香</div>

      {NAV_ITEMS.map((item) => {
        const active = nav === item.key;
        const label = t(item.labelKey);
        return (
          <button
            key={item.key}
            aria-label={label}
            onClick={() => setNav(item.key)}
            className="flex cursor-pointer flex-col items-center gap-1"
          >
            <span
              className={cn(
                "grid size-[46px] place-items-center rounded-[13px] transition-transform duration-[180ms] ease-spring active:scale-90",
                active ? "bg-hv text-ac" : "text-tx2 hover:bg-hv",
              )}
              style={{ height: 44 }}
            >
              <Icon name={item.icon} size={20} />
            </span>
            <span className={cn("text-[10px]", active ? "font-semibold text-ac" : "text-tx2")}>
              {label}
            </span>
          </button>
        );
      })}

      <div className="flex-1" />

      {/* 语言（弹出菜单） */}
      <DropdownMenu.Root>
        <DropdownMenu.Trigger asChild>
          <button aria-label={t("rail.language")} title={t("rail.language")} className="flex cursor-pointer flex-col items-center gap-1">
            <span className="grid size-[46px] place-items-center rounded-[13px] text-tx2 transition-transform duration-[180ms] ease-spring hover:bg-hv active:scale-90 data-[state=open]:bg-hv" style={{ height: 44 }}>
              <Icon name="globe" size={20} />
            </span>
            <span className="text-[10px] text-tx2">{t("rail.language")}</span>
          </button>
        </DropdownMenu.Trigger>
        <DropdownMenu.Portal>
          <DropdownMenu.Content
            side="right"
            align="end"
            sideOffset={8}
            className="animate-menu-pop menu-shadow z-50 w-[186px] origin-bottom-left rounded-[14px] border border-bd bg-srf p-1.5"
          >
            {LANGUAGES.map((l) => (
              <DropdownMenu.Item
                key={l}
                onSelect={() => setLanguage(l as Language)}
                className="flex cursor-pointer items-center justify-between gap-3 rounded-lg px-2.5 py-2 text-[13px] text-tx outline-none data-[highlighted]:bg-hv"
              >
                <span>{l}</span>
                {language === l && <Icon name="check" size={14} className="text-ac" strokeWidth={2.4} />}
              </DropdownMenu.Item>
            ))}
          </DropdownMenu.Content>
        </DropdownMenu.Portal>
      </DropdownMenu.Root>

      {/* 外观（浅 / 深 / 系统 三态循环） */}
      <button
        onClick={cycleTheme}
        title={t("rail.appearance")}
        className="flex cursor-pointer flex-col items-center gap-1"
      >
        <span className="grid size-[46px] place-items-center rounded-[13px] text-tx2 transition-transform duration-[180ms] ease-spring hover:bg-hv active:scale-90" style={{ height: 44 }}>
          <Icon name={themeInfo.icon} size={20} />
        </span>
        <span className="text-[10px] text-tx2">{t(themeInfo.labelKey)}</span>
      </button>
    </div>
  );
}
