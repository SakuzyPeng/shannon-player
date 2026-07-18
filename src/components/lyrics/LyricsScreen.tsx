import { useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { LyricPlayer } from "@applemusic-like-lyrics/react";
// AMLL 歌词播放器基础样式随歌词页按需加载；主题适配覆盖见 index.css 尾部。
import "@applemusic-like-lyrics/core/style.css";
import { AnimatedIcon } from "@/components/common/AnimatedIcon";
import { Icon } from "@/components/common/Icon";
import {
  RepeatControlButton,
  ShuffleControlButton,
  SkipControlButton,
} from "@/components/common/PlaybackControlButton";
import { PlayPauseIcon } from "@/components/common/PlayPauseIcon";
import { QueuePanel } from "@/components/player/QueuePanel";
import { VolumeSlider } from "@/components/common/VolumeSlider";
import { TrafficLights } from "@/components/window/TrafficLights";
import { ALBUMS } from "@/data/library";
import { lyricsOf } from "@/data/lyrics";
import { usePlayerStore } from "@/store/player";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { toAmllLines } from "@/lib/amll";
import { cn } from "@/lib/cn";
import { coverGradientStyle } from "@/lib/coverStyle";
import { fmtTime } from "@/lib/time";

/** 控制簇自动隐藏：鼠标静止 3s 后淡出（Token 文档 §4）。 */
const IDLE_HIDE_MS = 3000;

export function LyricsScreen() {
  const { t } = useT();
  const reduceMotion = useReducedMotion();
  const closeLyrics = useUiStore((s) => s.closeLyrics);

  const queue = usePlayerStore((s) => s.queue);
  const currentIndex = usePlayerStore((s) => s.currentIndex);
  const track = currentIndex >= 0 ? (queue[currentIndex]?.track ?? null) : null;
  const playing = usePlayerStore((s) => s.playing);
  const positionSec = usePlayerStore((s) => s.progress.positionSec);
  const durationSec = usePlayerStore((s) => s.progress.durationSec);
  const volume = usePlayerStore((s) => s.volume);
  const muted = usePlayerStore((s) => s.muted);
  const shuffle = usePlayerStore((s) => s.shuffle);
  const repeat = usePlayerStore((s) => s.repeat);
  const favorites = usePlayerStore((s) => s.favorites);

  const togglePlay = usePlayerStore((s) => s.togglePlay);
  const next = usePlayerStore((s) => s.next);
  const prev = usePlayerStore((s) => s.prev);
  const toggleShuffle = usePlayerStore((s) => s.toggleShuffle);
  const cycleRepeat = usePlayerStore((s) => s.cycleRepeat);
  const toggleFavorite = usePlayerStore((s) => s.toggleFavorite);
  const seek = usePlayerStore((s) => s.seek);
  const setVolume = usePlayerStore((s) => s.setVolume);
  const toggleMuted = usePlayerStore((s) => s.toggleMuted);

  const [lyricsOn, setLyricsOn] = useState(true);
  const [queueOpen, setQueueOpen] = useState(false);
  const [transOn, setTransOn] = useState(false);
  const [uiVisible, setUiVisible] = useState(true);
  const idleTimer = useRef(0);
  const queueOpenRef = useRef(queueOpen);
  queueOpenRef.current = queueOpen;

  const lyrics = track ? lyricsOf(track.id) : null;
  const amllLines = useMemo(
    () => (lyrics ? toAmllLines(lyrics, { withTranslation: transOn }) : []),
    [lyrics, transOn],
  );

  const pokeUi = () => {
    setUiVisible(true);
    window.clearTimeout(idleTimer.current);
    idleTimer.current = window.setTimeout(() => {
      if (!queueOpenRef.current) setUiVisible(false);
    }, IDLE_HIDE_MS);
  };
  const hideUi = () => {
    window.clearTimeout(idleTimer.current);
    if (!queueOpenRef.current) setUiVisible(false);
  };
  useEffect(() => () => window.clearTimeout(idleTimer.current), []);

  if (!track) return null;

  const album = ALBUMS.find((a) => a.id === track.albumId);
  const liked = !!favorites[track.id];
  const ctrlShown = queueOpen || uiVisible;

  const onSeek = (e: MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    seek(((e.clientX - rect.left) / rect.width) * durationSec);
  };
  return (
    <div
      onMouseMove={pokeUi}
      onMouseLeave={hideUi}
      className="fixed inset-0 z-40 flex bg-bg text-tx transition-colors"
      style={coverGradientStyle(track.cover)}
    >
      {/* 窗口边缘主色氛围 */}
      <div className="lyrics-edge-tint pointer-events-none absolute inset-0 z-20" />

      {/* 左上：交通灯 + 返回 */}
      <div data-tauri-drag-region className="absolute left-[18px] top-[18px] z-30 flex items-center gap-3.5">
        <TrafficLights />
        <button
          onClick={closeLyrics}
          className="flex cursor-pointer items-center gap-1.5 rounded-full border border-bd bg-srf px-3 py-[5px] text-xs text-tx2 transition-colors hover:bg-hv hover:text-tx"
        >
          <Icon name="chevronDown" size={12} strokeWidth={2} />
          {t("lyrics.back")}
        </button>
      </div>

      {/* 左栏：封面 + 迷你控制 */}
      <div
        className="lyrics-left-panel box-border flex flex-shrink-0 flex-col items-center justify-center bg-sb px-14"
        style={{
          width: lyricsOn ? 424 : "100%",
          borderRight: lyricsOn ? "1px solid var(--bd)" : "none",
          transition: "width 0.35s cubic-bezier(0.34,1.2,0.64,1)",
        }}
      >
        {/* 封面随视口高度收敛（min 高度 640 时约 230px），保证左栏音量条不贴底 */}
        <div className="group/cover relative size-[min(300px,36vh)]">
          <div className="lyrics-cover-glow pointer-events-none absolute -inset-5 rounded-[40px]" />
          <div className="cover-corners lyrics-cover-material cover-gradient relative grid size-[min(300px,36vh)] place-items-center rounded-[20px]">
            <span className="cover-initial font-serif text-[clamp(64px,11vh,96px)] font-medium">
              {track.cover.initial}
            </span>
            <div className="cover-corners lyrics-cover-overlay absolute inset-0 rounded-[20px] opacity-0 transition-opacity duration-[220ms] group-hover/cover:opacity-100">
              <motion.button
                whileHover={{ scale: 1.1 }}
                whileTap={{ scale: 0.9 }}
                title={liked ? t("player.unfavorite") : t("player.favorite")}
                aria-label={liked ? t("player.unfavorite") : t("player.favorite")}
                onClick={() => toggleFavorite(track.id)}
                className="collect-shadow absolute right-3.5 top-3.5 grid size-7 cursor-pointer place-items-center rounded-full bg-srf text-ac"
              >
                <AnimatedIcon
                  name={liked ? "heart" : "favorites"}
                  size={14}
                  strokeWidth={2}
                  variant="pop"
                />
              </motion.button>
            </div>
          </div>
        </div>

        <div className="mt-[30px] flex justify-center">
          <div className="relative">
            <h1 className="m-0 text-center font-serif text-[29px] font-semibold text-tx">
              {track.title}
            </h1>
            <AnimatePresence initial={false}>
              {liked && (
                <motion.span
                  key="liked"
                  title={t("album.collected")}
                  className="absolute -right-[17px] top-0.5 text-ac"
                  initial={{ opacity: 0, scale: reduceMotion ? 1 : 0.55 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={{ opacity: 0, scale: reduceMotion ? 1 : 1.3 }}
                  transition={reduceMotion ? { duration: 0.01 } : { type: "spring", stiffness: 520, damping: 24 }}
                >
                  <Icon name="heart" size={12} />
                </motion.span>
              )}
            </AnimatePresence>
          </div>
        </div>
        <div className="mt-2 text-center text-[13px] text-tx2">
          {track.artist} — {track.album}
          {album ? ` · ${album.year}` : ""}
        </div>

        <div className="mt-[34px] flex w-full max-w-[312px] flex-col gap-[7px]">
          <div onClick={onSeek} className="h-1 cursor-pointer overflow-hidden rounded-[2px] bg-bd">
            <div
              className="h-full rounded-[2px] bg-ac transition-[width] duration-100 ease-linear"
              style={{ width: `${durationSec ? (positionSec / durationSec) * 100 : 0}%` }}
            />
          </div>
          <div className="flex justify-between text-[11px] tabular-nums text-tx2">
            <span>{fmtTime(positionSec)}</span>
            <span>{fmtTime(durationSec)}</span>
          </div>
        </div>

        <div className="mt-[18px] flex items-center justify-center gap-3.5">
          <ShuffleControlButton
            active={shuffle}
            label={t("player.shuffle")}
            onClick={toggleShuffle}
            buttonSize={32}
            iconSize={15}
          />
          <SkipControlButton
            direction="previous"
            label={t("player.previous")}
            onClick={prev}
            buttonSize={36}
            iconSize={18}
          />
          <motion.button
            aria-label={playing ? t("player.pause") : t("player.play")}
            onClick={togglePlay}
            className="play-action-material grid size-[50px] cursor-pointer place-items-center rounded-full text-on-ac"
          >
            <PlayPauseIcon playing={playing} size={21} />
          </motion.button>
          <SkipControlButton
            direction="next"
            label={t("player.next")}
            onClick={next}
            buttonSize={36}
            iconSize={18}
          />
          <RepeatControlButton
            mode={repeat}
            label={`${t("player.repeat")}: ${repeat}`}
            onClick={cycleRepeat}
            buttonSize={32}
            iconSize={15}
          />
        </div>

        <div className="mt-[22px] flex items-center gap-2">
          <button
            type="button"
            aria-label={muted ? t("player.unmute") : t("player.mute")}
            aria-pressed={muted}
            title={muted ? t("player.unmute") : t("player.mute")}
            data-state={muted ? "muted" : "audible"}
            onClick={toggleMuted}
            className="grid size-7 shrink-0 place-items-center rounded-full text-tx2 transition-[color,background-color,transform] duration-150 hover:bg-hv hover:text-tx active:scale-90 data-[state=muted]:text-ac data-[state=muted]:hover:text-ac"
          >
            <Icon name={muted ? "volumeMuted" : "volume"} size={15} />
          </button>
          <VolumeSlider
            value={volume}
            muted={muted}
            label={t("player.volume")}
            onChange={setVolume}
            className="w-[104px]"
          />
        </div>
      </div>

      {/* 右栏：AMLL 歌词 / 空态 */}
      <AnimatePresence initial={false}>
        {lyricsOn && (
        <motion.div
          key="lyrics-panel"
          className="relative flex min-w-0 flex-1 flex-col justify-center overflow-hidden"
          initial={{ opacity: 0, x: reduceMotion ? 0 : 18 }}
          animate={{ opacity: 1, x: 0 }}
          exit={{ opacity: 0, x: reduceMotion ? 0 : 18 }}
          transition={reduceMotion ? { duration: 0.01 } : { duration: 0.3, ease: [0.22, 1, 0.36, 1] }}
        >
          <div className="lyrics-stage-bg pointer-events-none absolute inset-0" />

          <AnimatePresence initial={false} mode="wait">
          {lyrics ? (
            <motion.div
              key="lyrics"
              className="absolute inset-0"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: reduceMotion ? 0.01 : 0.18 }}
            >
              <LyricPlayer
                style={{ width: "100%", height: "100%" }}
                lyricLines={amllLines}
                currentTime={Math.round(positionSec * 1000)}
                playing={playing}
                onLyricLineClick={(e) => seek(e.line.getLine().startTime / 1000)}
              />
            </motion.div>
          ) : (
            <motion.div
              key="lyrics-empty"
              className="relative flex flex-col items-center text-center"
              initial={{ opacity: 0, y: reduceMotion ? 0 : 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: reduceMotion ? 0 : -6 }}
              transition={{ duration: reduceMotion ? 0.01 : 0.18 }}
            >
              <div className="grid size-16 place-items-center rounded-full border border-bd bg-sb text-tx2">
                <Icon name="queue" size={26} strokeWidth={1.6} />
              </div>
              <div className="mt-5 font-serif text-[19px] font-semibold text-tx">
                {t("lyrics.none.title")}
              </div>
              <div className="mt-2.5 whitespace-pre-line text-[13px] leading-[1.7] text-tx2">
                {t("lyrics.none.body")}
              </div>
              <div className="mt-6 flex gap-2.5">
                <motion.button
                  whileHover={{ filter: "brightness(1.07)" }}
                  whileTap={{ scale: 0.95 }}
                  className="flex cursor-pointer items-center gap-[7px] rounded-full bg-ac px-5 py-2.5 text-[13px] font-semibold text-on-ac"
                >
                  <Icon name="search" size={14} />
                  {t("lyrics.none.search")}
                </motion.button>
                <button className="cursor-pointer rounded-full border border-bd bg-srf px-5 py-2.5 text-[13px] font-semibold text-tx transition-colors hover:bg-hv">
                  {t("lyrics.none.import")}
                </button>
              </div>
              <div className="mt-4 text-[11.5px] text-tx2 opacity-80">
                {t("lyrics.none.source")}
              </div>
            </motion.div>
          )}
          </AnimatePresence>

          {/* 歌词工具簇（自动隐藏） */}
          <div
            className="float-shadow absolute right-[30px] top-[26px] z-30 flex items-center gap-1 rounded-full border border-bd bg-srf p-1"
            style={{
              opacity: uiVisible ? 1 : 0,
              pointerEvents: uiVisible ? "auto" : "none",
              transition: "opacity 0.6s ease",
            }}
          >
            <button
              title={t("lyrics.translation")}
              onClick={() => setTransOn((v) => !v)}
              className={cn(
                "grid size-8 cursor-pointer place-items-center rounded-full text-[13px] font-semibold transition-colors hover:bg-hv",
                transOn ? "bg-hv text-ac" : "text-tx2 hover:text-tx",
              )}
            >
              {t("lyrics.glyphTranslation")}
            </button>
            <button title={t("lyrics.romaji")} className="grid size-8 cursor-pointer place-items-center rounded-full text-[13px] font-semibold text-tx2 transition-colors hover:bg-hv hover:text-tx">
              {t("lyrics.glyphRomaji")}
            </button>
            <button title={t("lyrics.fontSize")} className="grid size-8 cursor-pointer place-items-center rounded-full text-[13px] font-semibold text-tx2 transition-colors hover:bg-hv hover:text-tx">
              Aa
            </button>
            <button title={t("lyrics.settings")} className="grid size-8 cursor-pointer place-items-center rounded-full text-tx2 transition-colors hover:bg-hv hover:text-tx">
              <Icon name="settings" size={15} strokeWidth={1.8} />
            </button>
          </div>
        </motion.div>
        )}
      </AnimatePresence>

      {/* 右下浮动按钮：歌词开关 + 队列（自动隐藏） */}
      <div
        className="absolute bottom-[26px] right-[30px] z-30 flex items-center gap-2"
        style={{
          opacity: ctrlShown ? 1 : 0,
          pointerEvents: ctrlShown ? "auto" : "none",
          transition: "opacity 0.6s ease",
        }}
      >
        <button
          title={lyricsOn ? t("lyrics.hide") : t("lyrics.show")}
          onClick={() => {
            setLyricsOn((v) => !v);
            pokeUi();
          }}
          className={cn(
            "float-shadow grid size-10 cursor-pointer place-items-center rounded-full border border-bd transition-transform active:scale-90",
            lyricsOn ? "bg-hv text-ac" : "bg-srf text-tx2",
          )}
        >
          <Icon name="songs" size={17} strokeWidth={1.8} />
        </button>
        <button
          title={t("player.queue")}
          data-queue-trigger
          onClick={() => {
            setQueueOpen((v) => !v);
            setUiVisible(true);
          }}
          className={cn(
            "float-shadow grid size-10 cursor-pointer place-items-center rounded-full border border-bd transition-transform active:scale-90",
            queueOpen ? "bg-hv text-ac" : "bg-srf text-tx2",
          )}
        >
          <Icon name="queue" size={17} strokeWidth={1.8} />
        </button>
      </div>
      {/* 队列面板（共享组件：磨砂浮层，点外 / Esc 关闭） */}
      <QueuePanel
        open={queueOpen}
        onDismiss={() => setQueueOpen(false)}
        className="absolute bottom-[78px] right-[30px] origin-bottom-right"
      />
    </div>
  );
}
