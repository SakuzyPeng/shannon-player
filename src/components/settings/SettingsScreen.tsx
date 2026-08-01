import { useRef, useState, type UIEvent } from "react";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { AnimatePresence, motion } from "framer-motion";
import { Icon } from "@/components/common/Icon";
import { SegmentedControl } from "@/components/common/SegmentedControl";
import { useElasticScroll } from "@/hooks/useElasticScroll";
import { LANGUAGES } from "@/data/library";
import { usePlayerStore } from "@/store/player";
import { useUiStore, type MusicFolder, type SettingKey } from "@/store/ui";
import { useT } from "@/i18n";
import type { MessageKey } from "@/i18n/messages";
import type { Language, ThemeMode } from "@/types/player";

const APP_VERSION = "0.1.0";

const SECTIONS: { key: string; labelKey: MessageKey }[] = [
  { key: "library", labelKey: "settings.secLibrary" },
  { key: "playback", labelKey: "settings.secPlayback" },
  { key: "lyrics", labelKey: "settings.secLyrics" },
  { key: "appearance", labelKey: "settings.secAppearance" },
  { key: "about", labelKey: "settings.secAbout" },
];

const LIB_TOGGLES: { key: SettingKey; labelKey: MessageKey; descKey: MessageKey }[] = [
  { key: "watch", labelKey: "settings.watch", descKey: "settings.watchDesc" },
  { key: "cloud", labelKey: "settings.cloud", descKey: "settings.cloudDesc" },
];
const PLAY_TOGGLES: { key: SettingKey; labelKey: MessageKey; descKey: MessageKey }[] = [
  { key: "loudness", labelKey: "settings.loudness", descKey: "settings.loudnessDesc" },
];
const LYRIC_TOGGLES: { key: SettingKey; labelKey: MessageKey; descKey: MessageKey }[] = [
  { key: "ttml", labelKey: "settings.onlineLyrics", descKey: "settings.onlineLyricsDesc" },
  { key: "karaoke", labelKey: "settings.wordByWord", descKey: "settings.wordByWordDesc" },
];
const THEME_SEG: { mode: ThemeMode; labelKey: MessageKey }[] = [
  { mode: "light", labelKey: "theme.light" },
  { mode: "dark", labelKey: "theme.dark" },
  { mode: "system", labelKey: "theme.system" },
];

/** 圆头开关（42×25，近临界弹簧位移）。 */
function Toggle({ on, onToggle, label }: { on: boolean; onToggle: () => void; label: string }) {
  return (
    <button
      role="switch"
      aria-checked={on}
      aria-label={label}
      onClick={onToggle}
      className="relative h-[25px] w-[42px] flex-shrink-0 cursor-pointer rounded-full transition-colors"
      style={{ background: on ? "var(--ac)" : "var(--bd)" }}
    >
      <span
        className="absolute top-[2.5px] size-5 rounded-full bg-[#FFFEFA] shadow-[0_1px_4px_rgba(60,40,20,0.3)] transition-[left] duration-[220ms] ease-spring"
        style={{ left: on ? 19.5 : 2.5 }}
      />
    </button>
  );
}

/** 开关行（标签 + 描述 + 开关）。 */
/** 快捷键清单（只读速查表，键位与 hooks/useGlobalHotkeys.ts 一一对应）。 */
const SHORTCUTS: { labelKey: MessageKey; keys: string[] }[] = [
  { labelKey: "shortcut.playPause", keys: ["Space"] },
  { labelKey: "shortcut.seek", keys: ["←", "→"] },
  { labelKey: "shortcut.prevNext", keys: ["⌘←", "⌘→"] },
  { labelKey: "shortcut.volume", keys: ["↑", "↓"] },
  { labelKey: "shortcut.mute", keys: ["M"] },
  { labelKey: "shortcut.search", keys: ["⌘F"] },
];

function ShortcutList() {
  const { t } = useT();
  return (
    <div className="border-b border-bd px-0.5 py-[15px]">
      <div className="text-sm font-semibold text-tx">{t("settings.shortcuts")}</div>
      <div className="mt-[3px] text-[12.5px] text-tx2">{t("settings.shortcutsDesc")}</div>
      <div className="mt-3 flex flex-col gap-1.5">
        {SHORTCUTS.map((sc) => (
          <div key={sc.labelKey} className="flex items-center justify-between gap-4">
            <span className="min-w-0 truncate text-[12.5px] text-tx2">{t(sc.labelKey)}</span>
            <span className="flex flex-none items-center gap-1">
              {sc.keys.map((k) => (
                <kbd
                  key={k}
                  className="rounded-md border border-bd bg-srf px-1.5 py-0.5 font-ui text-[11px] tabular-nums text-tx"
                >
                  {k}
                </kbd>
              ))}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

/**
 * 输出设备行。
 *
 * 界面上要同时说清三件互不相同的事，所以不能只画一个下拉：
 *
 * 1. **用户选了哪台**（偏好，跟着设置持久化）；
 * 2. **系统默认是哪台**（设备枚举的事实，不等同于用户偏好）；
 * 3. **此刻真正从哪台输出**（引擎打开的端点，也不等同于用户偏好）；
 * 4. **这台现在还在不在**——不在就照常列出来并标注「当前不可用」，而不是悄悄退回默认。
 *    偏好保留是为了插回去还能自动生效，但那件事必须让用户看得见，否则他会觉得声音
 *    某天自己跑了；
 * 5. **换不过去时的说明**——那不是播放失败，音乐仍在原来那台上响着，措辞要分开。
 */
function OutputDeviceRow() {
  const { t } = useT();
  const devices = usePlayerStore((s) => s.devices);
  const refreshDevices = usePlayerStore((s) => s.refreshDevices);
  const effectiveDeviceId = usePlayerStore((s) => s.effectiveDeviceId);
  const deviceError = usePlayerStore((s) => s.deviceError);
  const dismissDeviceError = usePlayerStore((s) => s.dismissDeviceError);
  const preferred = useUiStore((s) => s.outputDevice);
  const setOutputDevice = useUiStore((s) => s.setOutputDevice);

  const missing = preferred !== null && !devices.some((d) => d.id === preferred.id);
  const current = preferred
    ? missing
      ? t("settings.outputDeviceUnavailable", { name: preferred.label })
      : preferred.label
    : t("settings.outputDeviceSystem");

  const pick = (device: { id: string; label: string } | null) => {
    dismissDeviceError();
    setOutputDevice(device);
  };

  return (
    <div className="border-b border-bd px-0.5 py-[15px]">
      <div className="flex items-center gap-4">
        <div className="min-w-0 flex-1">
          <div className="text-sm font-semibold text-tx">{t("settings.outputDevice")}</div>
          <div className="mt-[3px] text-[12.5px] text-tx2">{t("settings.outputDeviceDesc")}</div>
        </div>
        {/* 每次展开都重新问系统：设备会插拔，用缓存的列表会显示已经拔掉的耳机。 */}
        <DropdownMenu.Root onOpenChange={(open) => open && void refreshDevices()}>
          <DropdownMenu.Trigger asChild>
            <button className="flex max-w-[240px] flex-none cursor-pointer items-center gap-2 rounded-full border border-bd bg-srf px-[14px] py-2 text-[13px] text-tx transition-colors hover:bg-hv">
              <span className="min-w-0 truncate">{current}</span>
              <Icon name="chevronDown" size={12} strokeWidth={2} />
            </button>
          </DropdownMenu.Trigger>
          <DropdownMenu.Portal>
            <DropdownMenu.Content
              align="end"
              sideOffset={6}
              className="surface-corners animate-menu-pop menu-shadow z-50 w-[260px] origin-top-right rounded-[14px] border border-bd bg-srf p-1.5"
            >
              <DropdownMenu.Item
                onSelect={() => pick(null)}
                className="flex cursor-pointer items-center justify-between gap-3 rounded-lg px-2.5 py-2 text-[13px] text-tx outline-none data-[highlighted]:bg-hv"
              >
                <span className="min-w-0 truncate">{t("settings.outputDeviceSystem")}</span>
                {preferred === null && (
                  <Icon name="check" size={14} className="flex-none text-ac" strokeWidth={2.4} />
                )}
              </DropdownMenu.Item>
              {devices.map((device) => (
                <DropdownMenu.Item
                  key={device.id}
                  onSelect={() => pick({ id: device.id, label: device.label })}
                  className="flex cursor-pointer items-center justify-between gap-3 rounded-lg px-2.5 py-2 text-[13px] text-tx outline-none data-[highlighted]:bg-hv"
                >
                  <span className="flex min-w-0 flex-1 items-center gap-1.5">
                    <span className="min-w-0 truncate">{device.label}</span>
                    {device.isDefault && (
                      <span className="flex-none text-[10px] text-tx2">
                        {t("settings.outputDeviceDefaultBadge")}
                      </span>
                    )}
                    {effectiveDeviceId === device.id && (
                      <span className="flex-none text-[10px] font-semibold text-ac">
                        {t("settings.outputDeviceActiveBadge")}
                      </span>
                    )}
                  </span>
                  {preferred?.id === device.id && (
                    <Icon name="check" size={14} className="flex-none text-ac" strokeWidth={2.4} />
                  )}
                </DropdownMenu.Item>
              ))}
              {missing && preferred && (
                // 选不了但要看得见：它是用户的偏好，插回去就会重新生效。
                <DropdownMenu.Item
                  disabled
                  className="flex items-center justify-between gap-3 rounded-lg px-2.5 py-2 text-[13px] text-tx2 opacity-60 outline-none"
                >
                  <span className="min-w-0 truncate">
                    {t("settings.outputDeviceUnavailable", { name: preferred.label })}
                  </span>
                  <Icon name="check" size={14} className="flex-none text-ac" strokeWidth={2.4} />
                </DropdownMenu.Item>
              )}
            </DropdownMenu.Content>
          </DropdownMenu.Portal>
        </DropdownMenu.Root>
      </div>
      {deviceError && (
        <div className="mt-2.5 text-[12.5px] text-danger">
          {t("settings.outputDeviceRejected", { reason: deviceError.message })}
        </div>
      )}
    </div>
  );
}

function ToggleRow({
  labelKey,
  descKey,
  settingKey,
}: {
  labelKey: MessageKey;
  descKey: MessageKey;
  settingKey: SettingKey;
}) {
  const { t } = useT();
  const on = useUiStore((s) => s.settings[settingKey]);
  const toggleSetting = useUiStore((s) => s.toggleSetting);
  return (
    <div className="flex items-center gap-4 border-b border-bd px-0.5 py-[15px]">
      <div className="min-w-0 flex-1">
        <div className="text-sm font-semibold text-tx">{t(labelKey)}</div>
        <div className="mt-[3px] text-[12.5px] text-tx2">{t(descKey)}</div>
      </div>
      <Toggle on={on} onToggle={() => toggleSetting(settingKey)} label={t(labelKey)} />
    </div>
  );
}

function FolderRow({ folder }: { folder: MusicFolder }) {
  const { t } = useT();
  const removeMusicFolder = useUiStore((s) => s.removeMusicFolder);
  const status = folder.watching ? t("settings.statusWatching") : t("settings.statusScanned");
  return (
    <motion.div
      layout="position"
      exit={{ opacity: 0, height: 0, paddingTop: 0, paddingBottom: 0, borderColor: "transparent" }}
      transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
      className="group/folder flex items-center gap-3 overflow-hidden border-b border-bd px-4 py-[11px] transition-colors hover:bg-hv"
    >
      <svg
        width="16"
        height="16"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
        strokeLinecap="round"
        strokeLinejoin="round"
        className="flex-shrink-0 text-tx2"
      >
        <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
      </svg>
      <div className="min-w-0 flex-1">
        <div className="truncate text-[13.5px] font-medium text-tx">{folder.path}</div>
        <div className="mt-0.5 text-[11.5px] text-tx2">
          {t("settings.folderTracks", { n: folder.tracks.toLocaleString() })} · {status}
        </div>
      </div>
      <button
        aria-label={t("settings.removeFolder")}
        title={t("settings.removeFolder")}
        onClick={() => removeMusicFolder(folder.path)}
        className="grid size-7 flex-shrink-0 cursor-pointer place-items-center rounded-full text-tx2 transition-colors hover:bg-[rgba(176,72,58,0.12)] hover:text-[#B0483A]"
      >
        <Icon name="close" size={14} strokeWidth={1.8} />
      </button>
    </motion.div>
  );
}

function SectionTitle({
  labelKey,
  refCb,
}: {
  labelKey: MessageKey;
  refCb: (el: HTMLDivElement | null) => void;
}) {
  const { t } = useT();
  return (
    <div ref={refCb} className="pb-1 pt-8 font-serif text-[19px] font-semibold text-tx">
      {t(labelKey)}
    </div>
  );
}

export function SettingsScreen() {
  const { t } = useT();
  const { scrollerRef, innerRef, thumbRef, onScroll } = useElasticScroll();
  const [activeSec, setActiveSec] = useState("library");
  const secEls = useRef<Record<string, HTMLDivElement | null>>({});

  const theme = useUiStore((s) => s.theme);
  const setTheme = useUiStore((s) => s.setTheme);
  const language = useUiStore((s) => s.language);
  const setLanguage = useUiStore((s) => s.setLanguage);
  const folders = useUiStore((s) => s.musicFolders);

  const secRef = (k: string) => (el: HTMLDivElement | null) => {
    secEls.current[k] = el;
  };

  const recomputeActive = () => {
    const sc = scrollerRef.current;
    if (!sc) return;
    if (sc.scrollTop + sc.clientHeight >= sc.scrollHeight - 2) {
      setActiveSec(SECTIONS[SECTIONS.length - 1].key);
      return;
    }
    const pos = sc.scrollTop + sc.clientHeight * 0.45;
    let cur = SECTIONS[0].key;
    for (const { key } of SECTIONS) {
      const el = secEls.current[key];
      if (el && el.offsetTop <= pos) cur = key;
    }
    setActiveSec(cur);
  };

  const handleScroll = (e: UIEvent<HTMLDivElement>) => {
    onScroll(e);
    recomputeActive();
  };

  const jumpTo = (key: string) => {
    const el = secEls.current[key];
    const sc = scrollerRef.current;
    if (!el || !sc) return;
    const maxScroll = Math.max(0, sc.scrollHeight - sc.clientHeight);
    sc.scrollTo({ top: Math.min(Math.max(0, el.offsetTop - 18), maxScroll), behavior: "smooth" });
  };

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div
        ref={scrollerRef}
        onScroll={handleScroll}
        className="no-scrollbar absolute inset-0 overflow-auto px-10 pb-[120px] [overscroll-behavior:contain]"
      >
        {/* 内容列 680 + 右侧目录栏 220 组成 900 的版心，整体居中：
            目录因此在 [980, ∞) 全区间都有固定栏位，不再与正文抢位置。 */}
        <div ref={innerRef} className="mx-auto max-w-[900px] pr-[220px] will-change-transform">
          <h1
            data-tauri-drag-region
            className="m-0 pb-1.5 pt-[34px] font-serif text-[40px] font-medium text-tx"
          >
            {t("nav.settings")}
          </h1>

          {/* 曲库 */}
          <SectionTitle labelKey="settings.secLibrary" refCb={secRef("library")} />
          <div className="pb-1 pt-3 text-[12.5px] text-tx2">{t("settings.musicFolders")}</div>
          <div className="surface-corners flex flex-col overflow-hidden rounded-[13px] border border-bd bg-srf">
            <AnimatePresence initial={false}>
              {folders.map((f) => (
                <FolderRow key={f.path} folder={f} />
              ))}
            </AnimatePresence>
            <motion.button
              layout="position"
              onClick={() => useUiStore.getState().openOnboarding()}
              className="flex cursor-pointer items-center gap-2.5 px-4 py-[11px] text-[13px] font-semibold text-ac transition-colors hover:bg-hv"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
                <path d="M12 5v14 M5 12h14" />
              </svg>
              {t("settings.addFolder")}
            </motion.button>
          </div>
          {LIB_TOGGLES.map((tg) => (
            <ToggleRow key={tg.key} labelKey={tg.labelKey} descKey={tg.descKey} settingKey={tg.key} />
          ))}

          {/* 播放 */}
          <SectionTitle labelKey="settings.secPlayback" refCb={secRef("playback")} />
          <OutputDeviceRow />
          {PLAY_TOGGLES.map((tg) => (
            <ToggleRow key={tg.key} labelKey={tg.labelKey} descKey={tg.descKey} settingKey={tg.key} />
          ))}
          <ShortcutList />
          {/* 快捷键当前为固定映射：自定义键位需要持久化，等后端接入用户数据后再做。 */}

          {/* 歌词 */}
          <SectionTitle labelKey="settings.secLyrics" refCb={secRef("lyrics")} />
          {LYRIC_TOGGLES.map((tg) => (
            <ToggleRow key={tg.key} labelKey={tg.labelKey} descKey={tg.descKey} settingKey={tg.key} />
          ))}

          {/* 外观与语言 */}
          <SectionTitle labelKey="settings.secAppearance" refCb={secRef("appearance")} />
          <div className="flex items-center gap-4 border-b border-bd px-0.5 py-[15px]">
            <div className="flex-1">
              <div className="text-sm font-semibold text-tx">{t("settings.appearance")}</div>
              <div className="mt-[3px] text-[12.5px] text-tx2">{t("settings.appearanceDesc")}</div>
            </div>
            <SegmentedControl
              value={theme}
              onValueChange={setTheme}
              options={THEME_SEG.map((segment) => ({
                value: segment.mode,
                label: t(segment.labelKey),
              }))}
              className="p-[3px] text-[12.5px]"
              buttonClassName="px-[15px] py-1.5"
            />
          </div>
          <div className="flex items-center gap-4 border-b border-bd px-0.5 py-[15px]">
            <div className="flex-1">
              <div className="text-sm font-semibold text-tx">{t("settings.language")}</div>
              <div className="mt-[3px] text-[12.5px] text-tx2">{t("settings.languageDesc")}</div>
            </div>
            <DropdownMenu.Root>
              <DropdownMenu.Trigger asChild>
                <button className="flex cursor-pointer items-center gap-2 rounded-full border border-bd bg-srf px-[14px] py-2 text-[13px] text-tx transition-colors hover:bg-hv">
                  {language}
                  <Icon name="chevronDown" size={12} strokeWidth={2} />
                </button>
              </DropdownMenu.Trigger>
              <DropdownMenu.Portal>
                <DropdownMenu.Content
                  align="end"
                  sideOffset={6}
                  className="surface-corners animate-menu-pop menu-shadow z-50 w-[186px] origin-top-right rounded-[14px] border border-bd bg-srf p-1.5"
                >
                  {LANGUAGES.map((l) => (
                    <DropdownMenu.Item
                      key={l}
                      onSelect={() => setLanguage(l as Language)}
                      className="flex cursor-pointer items-center justify-between gap-3 rounded-lg px-2.5 py-2 text-[13px] text-tx outline-none data-[highlighted]:bg-hv"
                    >
                      <span>{l}</span>
                      {language === l && (
                        <Icon name="check" size={14} className="text-ac" strokeWidth={2.4} />
                      )}
                    </DropdownMenu.Item>
                  ))}
                </DropdownMenu.Content>
              </DropdownMenu.Portal>
            </DropdownMenu.Root>
          </div>

          {/* 关于 */}
          <SectionTitle labelKey="settings.secAbout" refCb={secRef("about")} />
          <div className="flex items-center gap-4 px-0.5 pb-1 pt-4">
            <div className="grid size-[52px] place-items-center rounded-[14px] border border-bd bg-sb font-serif text-2xl font-semibold text-ac">
              香
            </div>
            <div className="flex-1">
              <div className="font-serif text-base font-semibold text-tx">{t("settings.appName")}</div>
              <div className="mt-[3px] text-[12.5px] text-tx2">
                {APP_VERSION} · AGPL-3.0 · {t("settings.aboutTagline")}
              </div>
            </div>
          </div>
          <div className="flex flex-wrap gap-2 px-0.5 pt-3">
            <button className="cursor-pointer rounded-full border border-bd bg-srf px-[15px] py-[7px] text-[12.5px] font-semibold text-tx transition-colors hover:bg-hv">
              {t("settings.sourceCode")}
            </button>
            <button className="cursor-pointer rounded-full border border-bd bg-srf px-[15px] py-[7px] text-[12.5px] font-semibold text-tx transition-colors hover:bg-hv">
              {t("settings.backers")}
            </button>
            <button className="flex cursor-pointer items-center gap-1.5 rounded-full bg-ac px-[15px] py-[7px] text-[12.5px] font-semibold text-on-ac transition-[filter] hover:brightness-[1.08]">
              <Icon name="heart" size={13} />
              {t("settings.donate")}
            </button>
          </div>
        </div>
      </div>

      {/* 右侧目录（滚动高亮 + 点击跳转）：与正文共用同一 900 版心并右对齐，
          因此在任意窗口宽度下都与内容列保持固定间距，不再需要断点隐藏。 */}
      <div className="pointer-events-none absolute inset-x-0 top-24 z-20 flex justify-center px-10">
        <div className="flex w-full max-w-[900px] justify-end">
          <div className="pointer-events-auto flex flex-col gap-0.5">
            {SECTIONS.map((sc) => {
              const active = activeSec === sc.key;
              return (
                <button
                  key={sc.key}
                  onClick={() => jumpTo(sc.key)}
                  className="flex cursor-pointer items-center gap-2.5 whitespace-nowrap rounded-lg px-2.5 py-1.5 text-[12.5px] transition-colors hover:bg-hv"
                  style={{
                    color: active ? "var(--tx)" : "var(--tx2)",
                    fontWeight: active ? 600 : 400,
                  }}
                >
                  <span
                    className="h-3.5 w-[3px] rounded-[1.5px] transition-colors"
                    style={{ background: active ? "var(--ac)" : "transparent" }}
                  />
                  {t(sc.labelKey)}
                </button>
              );
            })}
          </div>
        </div>
      </div>

      <div
        ref={thumbRef}
        className="scroll-thumb pointer-events-none absolute right-[5px] top-2 z-20 h-[120px] w-1.5 rounded-[3px] opacity-0"
      />
    </div>
  );
}
