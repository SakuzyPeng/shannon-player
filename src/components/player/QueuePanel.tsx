import { useEffect, useRef } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { AnimatedIcon } from "@/components/common/AnimatedIcon";
import {
  RepeatControlIcon,
  ShuffleControlIcon,
} from "@/components/common/PlaybackControlButton";
import { usePlayerStore } from "@/store/player";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";
import { coverGradientStyle } from "@/lib/coverStyle";

interface Props {
  open: boolean;
  /** 点面板外或按 Esc 时回调（触发按钮需带 data-queue-trigger 以免关了又开）。 */
  onDismiss: () => void;
  /** 定位与变换原点由调用方给（歌词页右下浮起 / 播放条上方弹出）。 */
  className?: string;
}

/**
 * 播放队列面板（磨砂浮层）：随机 / 循环圆钮、接下来列表、清除。
 * 歌词页与播放条共用；数据与动作直接读播放器 store。
 */
export function QueuePanel({ open, onDismiss, className }: Props) {
  const { t } = useT();
  const reduceMotion = useReducedMotion();
  const panelRef = useRef<HTMLDivElement | null>(null);

  const queue = usePlayerStore((s) => s.queue);
  const currentIndex = usePlayerStore((s) => s.currentIndex);
  const shuffle = usePlayerStore((s) => s.shuffle);
  const repeat = usePlayerStore((s) => s.repeat);
  const favorites = usePlayerStore((s) => s.favorites);
  const toggleShuffle = usePlayerStore((s) => s.toggleShuffle);
  const cycleRepeat = usePlayerStore((s) => s.cycleRepeat);
  const toggleFavorite = usePlayerStore((s) => s.toggleFavorite);
  const clearUpNext = usePlayerStore((s) => s.clearUpNext);

  const track = currentIndex >= 0 ? (queue[currentIndex]?.track ?? null) : null;
  const upNext = queue.slice(currentIndex + 1);

  // 点面板外 / Esc 关闭；触发按钮标 data-queue-trigger，由其自身 onClick 负责开关。
  useEffect(() => {
    if (!open) return;
    const onPointerDown = (e: PointerEvent) => {
      const target = e.target as HTMLElement;
      if (panelRef.current?.contains(target)) return;
      if (target.closest("[data-queue-trigger]")) return;
      onDismiss();
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onDismiss();
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open, onDismiss]);

  return (
    <AnimatePresence initial={false}>
      {open && (
        <motion.div
          key="queue-panel"
          ref={panelRef}
          className={cn(
            "surface-corners queue-panel z-40 flex max-h-[min(470px,58vh)] w-[330px] flex-col overflow-hidden rounded-2xl",
            className,
          )}
          initial={{ opacity: 0, scale: reduceMotion ? 1 : 0.92, y: reduceMotion ? 0 : 10 }}
          animate={{ opacity: 1, scale: 1, y: 0 }}
          exit={{ opacity: 0, scale: reduceMotion ? 1 : 0.95, y: reduceMotion ? 0 : 8 }}
          transition={
            reduceMotion
              ? { duration: 0.01 }
              : { type: "spring", stiffness: 420, damping: 30, mass: 0.75 }
          }
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
              <ShuffleControlIcon active={shuffle} size={14} />
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
              <RepeatControlIcon mode={repeat} size={14} />
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
          {track && (
            <div className="px-[18px] pb-2.5 text-xs text-tx2">
              {t("queue.from", { name: track.album })}
            </div>
          )}
          <AnimatePresence initial={false} mode="wait">
            {upNext.length > 0 ? (
              <motion.div
                key="queue-list"
                className="no-scrollbar min-h-0 flex-1 overflow-y-auto px-2 pb-2.5"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                transition={{ duration: reduceMotion ? 0.01 : 0.16 }}
              >
                {upNext.map((item) => {
                  const qLiked = !!favorites[item.track.id];
                  return (
                    <div
                      key={item.uid}
                      className="flex cursor-pointer items-center gap-[11px] rounded-[11px] px-2.5 py-[7px] hover:bg-[var(--qhv)]"
                    >
                      <div
                        className="cover-corners cover-gradient grid size-[38px] flex-shrink-0 place-items-center rounded-lg shadow-[inset_0_0_0_1px_var(--qring)]"
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
                        <AnimatedIcon
                          name={qLiked ? "heart" : "favorites"}
                          size={13}
                          strokeWidth={1.8}
                          variant="pop"
                        />
                      </button>
                    </div>
                  );
                })}
              </motion.div>
            ) : (
              <motion.div
                key="queue-empty"
                className="px-[18px] pb-8 pt-[26px] text-center text-[13px] text-tx2"
                initial={{ opacity: 0, y: reduceMotion ? 0 : 5 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: reduceMotion ? 0 : -4 }}
                transition={{ duration: reduceMotion ? 0.01 : 0.16 }}
              >
                {t("queue.empty")}
              </motion.div>
            )}
          </AnimatePresence>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
