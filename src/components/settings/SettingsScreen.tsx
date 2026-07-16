import { useRef, useState, type UIEvent } from "react";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { AnimatePresence, motion } from "framer-motion";
import { Icon } from "@/components/common/Icon";
import { SegmentedControl } from "@/components/common/SegmentedControl";
import { useElasticScroll } from "@/hooks/useElasticScroll";
import { LANGUAGES } from "@/data/library";
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
        <div ref={innerRef} className="mx-auto max-w-[680px] will-change-transform">
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
          {PLAY_TOGGLES.map((tg) => (
            <ToggleRow key={tg.key} labelKey={tg.labelKey} descKey={tg.descKey} settingKey={tg.key} />
          ))}

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

      {/* 右侧目录（滚动高亮 + 点击跳转） */}
      <div className="absolute right-9 top-24 z-20 hidden flex-col gap-0.5 xl:flex">
        {SECTIONS.map((sc) => {
          const active = activeSec === sc.key;
          return (
            <button
              key={sc.key}
              onClick={() => jumpTo(sc.key)}
              className="flex cursor-pointer items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-[12.5px] transition-colors hover:bg-hv"
              style={{ color: active ? "var(--tx)" : "var(--tx2)", fontWeight: active ? 600 : 400 }}
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

      <div
        ref={thumbRef}
        className="scroll-thumb pointer-events-none absolute right-[5px] top-2 z-20 h-[120px] w-1.5 rounded-[3px] opacity-0"
      />
    </div>
  );
}
