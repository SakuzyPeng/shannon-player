import { motion } from "framer-motion";
import { Icon } from "@/components/common/Icon";
import { usePlayerStore } from "@/store/player";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";
import { coverGradientStyle } from "@/lib/coverStyle";
import { fmtTime } from "@/lib/time";
import type { MouseEvent } from "react";

export function PlayBar() {
  const track = usePlayerStore((s) => (s.currentIndex >= 0 ? s.queue[s.currentIndex]?.track : null));
  const playing = usePlayerStore((s) => s.playing);
  const progress = usePlayerStore((s) => s.progress);
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
  const { t } = useT();

  if (!track) return null;

  const favorited = !!favorites[track.id];

  const pct = progress.durationSec ? (progress.positionSec / progress.durationSec) * 100 : 0;
  const remaining = Math.max(0, progress.durationSec - progress.positionSec);

  const onSeek = (e: MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    seek(((e.clientX - rect.left) / rect.width) * progress.durationSec);
  };
  const onVol = (e: MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    setVolume((e.clientX - rect.left) / rect.width);
  };

  return (
    <div className="surface-corners playbar-shadow absolute inset-x-[26px] bottom-[22px] z-30 flex h-[76px] items-center gap-4 rounded-[19px] border border-bd bg-pb px-[18px] transition-colors">
      {/* 左：封面 + 曲目（点击进歌词页）+ 收藏 */}
      <div className="flex w-[236px] items-center gap-3">
        <button
          aria-label={t("menu.showLyrics")}
          title={t("menu.showLyrics")}
          onClick={() => useUiStore.getState().openLyrics()}
          className="flex min-w-0 cursor-pointer items-center gap-3 text-left"
        >
          <div
            className="cover-corners cover-gradient play-cover-material grid size-12 flex-shrink-0 place-items-center rounded-xl"
            style={coverGradientStyle(track.cover)}
          >
            <span className="cover-initial font-serif text-[19px]">{track.cover.initial}</span>
          </div>
          <div className="min-w-0">
            <div className="truncate font-serif text-[14.5px] font-semibold text-tx">{track.title}</div>
            <div className="mt-0.5 truncate text-[12px] text-tx2">{track.artist}</div>
          </div>
        </button>
        {/* 爱心常驻 --ac（对齐设计稿）；实心 = 已收藏，描边 = 未收藏 */}
        <button
          aria-label={favorited ? t("player.unfavorite") : t("player.favorite")}
          onClick={() => toggleFavorite(track.id)}
          className="grid size-[30px] flex-shrink-0 place-items-center rounded-full text-ac transition-transform hover:bg-hv active:scale-90"
        >
          <Icon name={favorited ? "heart" : "favorites"} size={16} />
        </button>
      </div>

      {/* 中：控制簇 + 进度 */}
      <div className="mx-auto flex max-w-[520px] flex-1 flex-col gap-1.5">
        <div className="flex items-center justify-center gap-[13px]">
          <button
            aria-label={t("player.shuffle")}
            onClick={toggleShuffle}
            className={cn(
              "grid size-[30px] place-items-center rounded-full transition-transform hover:bg-hv active:scale-90",
              shuffle ? "text-ac" : "text-tx2 hover:text-tx",
            )}
          >
            <Icon name="shuffle" size={15} strokeWidth={1.8} />
          </button>
          <button aria-label={t("player.previous")} onClick={prev} className="grid size-8 place-items-center rounded-full text-tx transition-transform hover:bg-hv active:scale-90">
            <Icon name="prev" size={17} />
          </button>
          <motion.button
            aria-label={playing ? t("player.pause") : t("player.play")}
            onClick={togglePlay}
            whileTap={{ scale: 0.9 }}
            whileHover={{ filter: "brightness(1.08)" }}
            className="grid size-11 place-items-center rounded-full bg-ac text-on-ac"
          >
            <Icon name={playing ? "pause" : "play"} size={17} />
          </motion.button>
          <button aria-label={t("player.next")} onClick={next} className="grid size-8 place-items-center rounded-full text-tx transition-transform hover:bg-hv active:scale-90">
            <Icon name="next" size={17} />
          </button>
          <button
            aria-label={`${t("player.repeat")}: ${repeat}`}
            onClick={cycleRepeat}
            className={cn(
              "grid size-[30px] place-items-center rounded-full transition-transform hover:bg-hv active:scale-90",
              repeat !== "off" ? "text-ac" : "text-tx2 hover:text-tx",
            )}
          >
            <Icon name="repeat" size={15} strokeWidth={1.8} />
          </button>
        </div>
        <div className="flex items-center gap-2.5">
          <span className="text-[11px] tabular-nums text-tx2">{fmtTime(progress.positionSec)}</span>
          <div onClick={onSeek} className="h-1 flex-1 cursor-pointer overflow-hidden rounded-[2px] bg-bd">
            <div className="h-full rounded-[2px] bg-ac" style={{ width: `${pct}%` }} />
          </div>
          <span className="text-[11px] tabular-nums text-tx2">-{fmtTime(remaining)}</span>
        </div>
      </div>

      {/* 右：队列 + 添加 + 音量 */}
      <div className="flex w-[236px] items-center justify-end gap-2.5">
        <button aria-label={t("player.queue")} className="grid size-[30px] place-items-center rounded-full text-tx2 transition-colors hover:bg-hv hover:text-tx">
          <Icon name="queue" size={16} strokeWidth={1.8} />
        </button>
        <button aria-label={t("player.addToPlaylist")} className="grid size-[30px] place-items-center rounded-full text-tx2 transition-colors hover:bg-hv hover:text-tx">
          <Icon name="addPlaylist" size={16} strokeWidth={1.8} />
        </button>
        <span className="text-tx2">
          <Icon name="volume" size={16} />
        </span>
        <div onClick={onVol} className="h-1 w-[72px] cursor-pointer rounded-[2px] bg-bd">
          <div className="h-full rounded-[2px] bg-tx2" style={{ width: `${(muted ? 0 : volume) * 100}%` }} />
        </div>
      </div>
    </div>
  );
}
