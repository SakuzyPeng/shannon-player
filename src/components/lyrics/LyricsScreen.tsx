import { useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { motion } from "framer-motion";
import { LyricPlayer } from "@applemusic-like-lyrics/react";
import { Icon } from "@/components/common/Icon";
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
  const closeLyrics = useUiStore((s) => s.closeLyrics);

  const queue = usePlayerStore((s) => s.queue);
  const currentIndex = usePlayerStore((s) => s.currentIndex);
  const track = currentIndex >= 0 ? (queue[currentIndex]?.track ?? null) : null;
  const upNext = useMemo(() => queue.slice(currentIndex + 1), [queue, currentIndex]);
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
  const clearUpNext = usePlayerStore((s) => s.clearUpNext);

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
  const onVol = (e: MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    setVolume((e.clientX - rect.left) / rect.width);
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
        className="box-border flex flex-shrink-0 flex-col items-center justify-center bg-sb px-14 pb-12 pt-14"
        style={{
          width: lyricsOn ? 424 : "100%",
          borderRight: lyricsOn ? "1px solid var(--bd)" : "none",
          transition: "width 0.35s cubic-bezier(0.34,1.2,0.64,1)",
        }}
      >
        <div className="group/cover relative size-[300px]">
          <div className="lyrics-cover-glow pointer-events-none absolute -inset-5 rounded-[40px]" />
          <div className="lyrics-cover-material cover-gradient relative grid size-[300px] place-items-center rounded-[20px]">
            <span className="cover-initial font-serif text-8xl font-medium">
              {track.cover.initial}
            </span>
            <div className="lyrics-cover-overlay absolute inset-0 rounded-[20px] opacity-0 transition-opacity duration-[220ms] group-hover/cover:opacity-100">
              <motion.button
                whileHover={{ scale: 1.1 }}
                whileTap={{ scale: 0.9 }}
                title={liked ? t("player.unfavorite") : t("player.favorite")}
                aria-label={liked ? t("player.unfavorite") : t("player.favorite")}
                onClick={() => toggleFavorite(track.id)}
                className="collect-shadow absolute right-3.5 top-3.5 grid size-7 cursor-pointer place-items-center rounded-full bg-srf text-ac"
              >
                <Icon name={liked ? "heart" : "favorites"} size={14} strokeWidth={2} />
              </motion.button>
            </div>
          </div>
        </div>

        <div className="mt-[30px] flex justify-center">
          <div className="relative">
            <h1 className="m-0 text-center font-serif text-[29px] font-semibold text-tx">
              {track.title}
            </h1>
            {liked && (
              <span title={t("album.collected")} className="absolute -right-[17px] top-0.5 text-ac">
                <Icon name="heart" size={12} />
              </span>
            )}
          </div>
        </div>
        <div className="mt-2 text-center text-[13px] text-tx2">
          {track.artist} — {track.album}
          {album ? ` · ${album.year}` : ""}
        </div>

        <div className="mt-[34px] flex w-full max-w-[312px] flex-col gap-[7px]">
          <div onClick={onSeek} className="h-1 cursor-pointer overflow-hidden rounded-[2px] bg-bd">
            <div
              className="h-full rounded-[2px] bg-ac"
              style={{ width: `${durationSec ? (positionSec / durationSec) * 100 : 0}%` }}
            />
          </div>
          <div className="flex justify-between text-[11px] tabular-nums text-tx2">
            <span>{fmtTime(positionSec)}</span>
            <span>{fmtTime(durationSec)}</span>
          </div>
        </div>

        <div className="mt-[18px] flex items-center justify-center gap-3.5">
          <button
            aria-label={t("player.shuffle")}
            onClick={toggleShuffle}
            className={cn(
              "grid size-8 cursor-pointer place-items-center rounded-full transition-transform hover:bg-hv active:scale-90",
              shuffle ? "text-ac" : "text-tx2 hover:text-tx",
            )}
          >
            <Icon name="shuffle" size={15} strokeWidth={1.8} />
          </button>
          <button aria-label={t("player.previous")} onClick={prev} className="grid size-9 cursor-pointer place-items-center rounded-full text-tx transition-transform hover:bg-hv active:scale-90">
            <Icon name="prev" size={18} />
          </button>
          <motion.button
            aria-label={playing ? t("player.pause") : t("player.play")}
            onClick={togglePlay}
            whileTap={{ scale: 0.9 }}
            whileHover={{ filter: "brightness(1.08)" }}
            className="grid size-[50px] cursor-pointer place-items-center rounded-full bg-ac text-on-ac"
          >
            <Icon name={playing ? "pause" : "play"} size={19} />
          </motion.button>
          <button aria-label={t("player.next")} onClick={next} className="grid size-9 cursor-pointer place-items-center rounded-full text-tx transition-transform hover:bg-hv active:scale-90">
            <Icon name="next" size={18} />
          </button>
          <button
            aria-label={`${t("player.repeat")}: ${repeat}`}
            onClick={cycleRepeat}
            className={cn(
              "grid size-8 cursor-pointer place-items-center rounded-full transition-transform hover:bg-hv active:scale-90",
              repeat !== "off" ? "text-ac" : "text-tx2 hover:text-tx",
            )}
          >
            <Icon name="repeat" size={15} strokeWidth={1.8} />
          </button>
        </div>

        <div className="mt-[22px] flex items-center gap-3">
          <span className="text-tx2">
            <Icon name="volume" size={15} />
          </span>
          <div onClick={onVol} className="h-1 w-[90px] cursor-pointer rounded-[2px] bg-bd">
            <div
              className="h-full rounded-[2px] bg-tx2"
              style={{ width: `${(muted ? 0 : volume) * 100}%` }}
            />
          </div>
        </div>
      </div>

      {/* 右栏：AMLL 歌词 / 空态 */}
      {lyricsOn && (
        <div className="relative flex min-w-0 flex-1 flex-col justify-center overflow-hidden">
          <div className="lyrics-stage-bg pointer-events-none absolute inset-0" />

          {lyrics ? (
            <LyricPlayer
              style={{ width: "100%", height: "100%" }}
              lyricLines={amllLines}
              currentTime={Math.round(positionSec * 1000)}
              playing={playing}
              onLyricLineClick={(e) => seek(e.line.getLine().startTime / 1000)}
            />
          ) : (
            <div className="relative flex flex-col items-center text-center">
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
            </div>
          )}

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
        </div>
      )}

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

      {/* 队列面板（磨砂浮层） */}
      {queueOpen && (
        <div
          className="queue-panel absolute bottom-[78px] right-[30px] z-40 flex max-h-[470px] w-[330px] origin-bottom-right flex-col overflow-hidden rounded-2xl"
          style={{ animation: "queuePop 0.28s var(--ease-spring)" }}
        >
          <div className="flex gap-2.5 px-4 pt-3.5">
            <button
              onClick={toggleShuffle}
              className={cn(
                "flex flex-1 cursor-pointer items-center justify-center gap-[7px] rounded-full border py-2 text-[12.5px] font-semibold transition-transform active:scale-95",
                shuffle
                  ? "border-ac bg-ac text-on-ac"
                  : "border-[var(--qpillbd)] bg-[var(--qpill)] text-tx",
              )}
            >
              <Icon name="shuffle" size={14} strokeWidth={1.8} />
              {t("player.shuffle")}
            </button>
            <button
              onClick={cycleRepeat}
              className={cn(
                "flex flex-1 cursor-pointer items-center justify-center gap-[7px] rounded-full border py-2 text-[12.5px] font-semibold transition-transform active:scale-95",
                repeat !== "off"
                  ? "border-ac bg-ac text-on-ac"
                  : "border-[var(--qpillbd)] bg-[var(--qpill)] text-tx",
              )}
            >
              <Icon name="repeat" size={14} strokeWidth={1.8} />
              {t("player.repeat")}
            </button>
          </div>
          <div className="flex items-baseline gap-2 px-[18px] pb-0.5 pt-[15px]">
            <span className="font-serif text-[17px] font-semibold text-tx">{t("queue.title")}</span>
            <div className="flex-1" />
            <button
              onClick={clearUpNext}
              className="cursor-pointer rounded-lg px-2 py-1 text-[12.5px] font-semibold text-ac hover:bg-[var(--qhv)]"
            >
              {t("queue.clear")}
            </button>
          </div>
          <div className="px-[18px] pb-2.5 text-xs text-tx2">
            {t("queue.from", { name: track.album })}
          </div>
          {upNext.length > 0 ? (
            <div className="no-scrollbar min-h-0 flex-1 overflow-y-auto px-2 pb-2.5">
              {upNext.map((item) => {
                const qLiked = !!favorites[item.track.id];
                return (
                  <div
                    key={item.uid}
                    className="flex cursor-pointer items-center gap-[11px] rounded-[11px] px-2.5 py-[7px] hover:bg-[var(--qhv)]"
                  >
                    <div
                      className="cover-gradient grid size-[38px] flex-shrink-0 place-items-center rounded-lg shadow-[inset_0_0_0_1px_var(--qring)]"
                      style={coverGradientStyle(item.track.cover)}
                    >
                      <span className="cover-initial font-serif text-[15px]">
                        {item.track.cover.initial}
                      </span>
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="truncate font-serif text-[13.5px] font-semibold text-tx">
                        {item.track.title}
                      </div>
                      <div className="mt-0.5 truncate text-[11.5px] text-tx2">
                        {item.track.artist} — {item.track.album}
                      </div>
                    </div>
                    <button
                      aria-label={qLiked ? t("player.unfavorite") : t("player.favorite")}
                      onClick={(e) => {
                        e.stopPropagation();
                        toggleFavorite(item.track.id);
                      }}
                      className={cn(
                        "grid size-[26px] flex-shrink-0 cursor-pointer place-items-center rounded-full hover:bg-[var(--qhv)]",
                        qLiked ? "text-ac" : "text-tx2",
                      )}
                    >
                      <Icon name={qLiked ? "heart" : "favorites"} size={13} strokeWidth={1.8} />
                    </button>
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="px-[18px] pb-8 pt-[26px] text-center text-[13px] text-tx2">
              {t("queue.empty")}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
