import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { isTauri, onScanProgress, pickMusicFolder, scanLibrary } from "@/lib/backend";
import { useLibraryStore } from "@/store/library";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";

type Phase = "welcome" | "scanning" | "done";

/** 浏览器 dev 环境（无 Rust 后端）下的模拟规模。 */
const DEMO_TOTAL_SONGS = 1847;
const DEMO_ALBUMS = 132;
const DEMO_ARTISTS = 86;

/** 模拟扫描时滚动展示的文件路径（仅无后端时使用；有后端则显示真实路径）。 */
const FILES = [
  "~/Music/曲库/万能青年旅店/万能青年旅店/01 狗尿馆.flac",
  "~/Music/曲库/周杰伦/范特西/03 双截棍.flac",
  "~/Music/曲库/Radiohead/In Rainbows/04 Weird Fishes.flac",
  "~/Music/曲库/白鲸电台/长夜电波/02 午夜环线.flac",
  "~/Music/曲库/陈绮贞/华丽的冒险/07 旅行的意义.mp3",
  "~/Music/曲库/Frank Ocean/Blonde/09 Nights.m4a",
  "~/Music/曲库/朴树/我去2000年/05 New Boy.flac",
  "~/Music/曲库/Miles Davis/Kind of Blue/01 So What.flac",
  "~/Music/曲库/新裤子/生活因你而火热/03 没有理想的人不伤心.flac",
  "~/Music/曲库/罗大佑/之乎者也/01 之乎者也.flac",
];

export function FirstRunScreen() {
  const { t } = useT();
  const reduceMotion = useReducedMotion();
  const closeOnboarding = useUiStore((s) => s.closeOnboarding);
  const setNav = useUiStore((s) => s.setNav);
  const setLibrary = useLibraryStore((s) => s.setLibrary);
  const [phase, setPhase] = useState<Phase>("welcome");
  const [pct, setPct] = useState(0);
  const [fileIdx, setFileIdx] = useState(0);
  /** 真实扫描的实时统计（有后端时替代模拟数字）。 */
  const [stats, setStats] = useState<{ tracks: number; albums: number; artists: number } | null>(
    null,
  );
  const [currentFile, setCurrentFile] = useState("");
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setInterval> | null>(null);

  // 无后端（浏览器 dev）时保留模拟时钟，否则界面在此环境下完全没有反馈。
  useEffect(() => {
    if (phase !== "scanning" || isTauri()) return;
    timer.current = setInterval(() => {
      setPct((prev) => {
        const next = Math.min(100, prev + 1.6 + Math.random() * 2.2);
        if (next >= 100) {
          if (timer.current) clearInterval(timer.current);
          setPhase("done");
          return 100;
        }
        return next;
      });
      setFileIdx((i) => (i + 1) % FILES.length);
    }, 180);
    return () => {
      if (timer.current) clearInterval(timer.current);
    };
  }, [phase]);

  /** 有后端：选文件夹 → 真实扫描，进度由 Rust 事件回灌。 */
  const startScan = async () => {
    setPct(0);
    setFileIdx(0);
    setError(null);

    if (!isTauri()) {
      setStats(null);
      setPhase("scanning");
      return;
    }

    const folder = await pickMusicFolder();
    if (!folder) return; // 用户取消选择，留在欢迎页
    setStats(null);
    setCurrentFile("");
    setPhase("scanning");

    const unlisten = await onScanProgress((p) => {
      setPct(p.total > 0 ? (p.done / p.total) * 100 : 0);
      setCurrentFile(p.current);
      if (p.tracks > 0 || p.albums > 0) {
        setStats({ tracks: p.tracks, albums: p.albums, artists: 0 });
      }
    });
    try {
      const snapshot = await scanLibrary([folder]);
      if (snapshot) {
        setLibrary(snapshot);
        const artists = new Set(snapshot.albums.map((a) => a.artist)).size;
        setStats({ tracks: snapshot.tracks.length, albums: snapshot.albums.length, artists });
      }
      setPct(100);
      setPhase("done");
    } catch (e) {
      setError(String(e));
      setPhase("welcome");
    } finally {
      unlisten();
    }
  };
  const finishNow = () => {
    if (timer.current) clearInterval(timer.current);
    setPhase("done");
  };
  const cancel = () => {
    if (timer.current) clearInterval(timer.current);
    setPct(0);
    setFileIdx(0);
    setPhase("welcome");
  };
  const startListening = () => {
    closeOnboarding();
    setNav("albums");
  };

  // 扫描中的「已找到」：有后端用真实统计，否则按进度插值出模拟数字。
  const found = stats
    ? stats.tracks
    : Math.min(DEMO_TOTAL_SONGS, Math.round((pct / 100) * DEMO_TOTAL_SONGS));
  const foundAlbums = stats ? stats.albums : Math.max(1, Math.round(found / 14));

  return (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center px-10">
      <AnimatePresence initial={false} mode="wait">
      {phase === "welcome" && (
        <motion.div
          key="welcome"
          initial={{ opacity: 0, y: 12 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: reduceMotion ? 0 : -10 }}
          transition={{ duration: reduceMotion ? 0.01 : 0.28, ease: "easeOut" }}
          className="flex max-w-[460px] flex-col items-center text-center"
        >
          <div className="grid size-[108px] place-items-center rounded-[28px] border border-bd bg-sb shadow-[0_14px_36px_var(--float-shadow)]">
            <span className="font-serif text-[52px] font-semibold text-ac">香</span>
          </div>
          <h1 className="mt-[30px] font-serif text-[34px] font-semibold text-tx">
            {t("firstRun.welcomeTitle")}
          </h1>
          <p className="mt-3.5 whitespace-pre-line text-[14.5px] leading-[1.8] text-tx2">
            {t("firstRun.welcomeBody")}
          </p>
          <motion.button
            whileHover={{ filter: "brightness(1.07)" }}
            whileTap={{ scale: 0.95 }}
            onClick={startScan}
            className="mt-[34px] flex cursor-pointer items-center gap-2.5 rounded-full bg-ac px-[30px] py-[13px] text-[15px] font-semibold text-on-ac shadow-[0_10px_26px_color-mix(in_srgb,var(--ac)_35%,transparent)]"
          >
            <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round">
              <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z M12 10v6 M9 13h6" />
            </svg>
            {t("firstRun.addFolder")}
          </motion.button>
          <div className="mt-[18px] text-xs text-tx2 opacity-85">{t("firstRun.formats")}</div>
          {/* 扫描失败要看得见：静默回到欢迎页等于假装什么都没发生。 */}
          {error && (
            <div className="mt-3 max-w-[420px] text-xs leading-relaxed text-danger">
              {t("firstRun.scanFailed")} — {error}
            </div>
          )}
        </motion.div>
      )}

      {phase === "scanning" && (
        <motion.div
          key="scanning"
          initial={{ opacity: 0, y: 12 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: reduceMotion ? 0 : -10 }}
          transition={{ duration: reduceMotion ? 0.01 : 0.28, ease: "easeOut" }}
          className="flex w-full max-w-[520px] flex-col items-center text-center"
        >
          <div
            className="grid size-24 place-items-center rounded-full shadow-[inset_0_0_0_1px_rgba(38,22,8,0.2),0_14px_36px_var(--cover-hover-shadow)]"
            style={{
              background: "linear-gradient(150deg,#4A6070,#26343E)",
              animation: "disc-spin 3.2s linear infinite",
            }}
          >
            <div className="size-[26px] rounded-full bg-bg shadow-[inset_0_0_0_1px_rgba(38,22,8,0.2)]" />
          </div>
          <h1 className="mt-7 font-serif text-[27px] font-semibold text-tx">
            {t("firstRun.scanningTitle")}
          </h1>
          <div className="mt-2.5 text-[13.5px] text-tx2">
            {t("firstRun.foundLabel")}{" "}
            <span className="font-bold tabular-nums text-ac">{found.toLocaleString()}</span>{" "}
            {t("firstRun.foundUnit", { m: foundAlbums })}
          </div>
          <div className="mt-[26px] h-[5px] w-full max-w-[400px] overflow-hidden rounded-[3px] bg-bd">
            <div
              className="h-full rounded-[3px] bg-ac transition-[width] duration-300 ease-out"
              style={{ width: `${Math.round(pct)}%` }}
            />
          </div>
          <div className="mt-3 max-w-[420px] truncate font-mono text-[11.5px] text-tx2 opacity-80">
            {currentFile || FILES[fileIdx]}
          </div>
          <div className="mt-[30px] flex gap-2.5">
            <button
              onClick={finishNow}
              className="cursor-pointer rounded-full border border-bd bg-srf px-5 py-[9px] text-[13px] font-semibold text-tx transition-colors hover:bg-hv"
            >
              {t("firstRun.background")}
            </button>
            <button
              onClick={cancel}
              className="cursor-pointer rounded-full px-5 py-[9px] text-[13px] text-tx2 transition-colors hover:bg-hv hover:text-tx"
            >
              {t("firstRun.cancel")}
            </button>
          </div>
        </motion.div>
      )}

      {phase === "done" && (
        <motion.div
          key="done"
          initial={{ opacity: 0, y: 12 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: reduceMotion ? 0 : -10 }}
          transition={{ duration: reduceMotion ? 0.01 : 0.28, ease: "easeOut" }}
          className="flex max-w-[480px] flex-col items-center text-center"
        >
          <div className="flex">
            <div
              className="grid size-[88px] place-items-center rounded-2xl shadow-[inset_0_0_0_1px_rgba(38,22,8,0.16),0_12px_30px_rgba(60,40,20,0.2)]"
              style={{ background: "linear-gradient(150deg,#7A4A3A,#46291F)", transform: "rotate(-8deg) translateX(14px)" }}
            >
              <span className="font-serif text-[34px] text-[rgba(250,248,244,0.88)]">范</span>
            </div>
            <div
              className="relative z-[2] grid size-[88px] place-items-center rounded-2xl shadow-[inset_0_0_0_1px_rgba(38,22,8,0.16),0_14px_34px_rgba(60,40,20,0.26)]"
              style={{ background: "linear-gradient(150deg,#4A6070,#26343E)" }}
            >
              <span className="font-serif text-[34px] text-[rgba(240,246,250,0.88)]">鲸</span>
            </div>
            <div
              className="grid size-[88px] place-items-center rounded-2xl shadow-[inset_0_0_0_1px_rgba(38,22,8,0.16),0_12px_30px_rgba(60,40,20,0.2)]"
              style={{ background: "linear-gradient(150deg,#3E5C50,#20332B)", transform: "rotate(8deg) translateX(-14px)" }}
            >
              <span className="font-serif text-[34px] text-[rgba(250,248,244,0.88)]">In</span>
            </div>
          </div>
          <h1 className="mt-[30px] font-serif text-[30px] font-semibold text-tx">
            {t("firstRun.doneTitle")}
          </h1>
          <div className="mt-3 whitespace-pre-line text-sm leading-[1.8] text-tx2">
            {t("firstRun.doneBody", {
              n: (stats?.tracks ?? DEMO_TOTAL_SONGS).toLocaleString(),
              m: stats?.albums ?? DEMO_ALBUMS,
              a: stats?.artists ?? DEMO_ARTISTS,
            })}
          </div>
          <motion.button
            whileHover={{ filter: "brightness(1.07)" }}
            whileTap={{ scale: 0.95 }}
            onClick={startListening}
            className="mt-8 flex cursor-pointer items-center gap-2.5 rounded-full bg-ac px-[34px] py-[13px] text-[15px] font-semibold text-on-ac shadow-[0_10px_26px_color-mix(in_srgb,var(--ac)_35%,transparent)]"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
              <path d="M8 5v14l11-7z" />
            </svg>
            {t("firstRun.startListening")}
          </motion.button>
        </motion.div>
      )}
      </AnimatePresence>
    </div>
  );
}
