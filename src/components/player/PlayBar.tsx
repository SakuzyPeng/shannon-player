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
import { usePlayerStore } from "@/store/player";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";
import { coverGradientStyle } from "@/lib/coverStyle";
import { fmtTime } from "@/lib/time";
import { useState, type MouseEvent } from "react";

export function PlayBar() {
  const [queueOpen, setQueueOpen] = useState(false);
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
  const toggleMuted = usePlayerStore((s) => s.toggleMuted);
  const { t } = useT();

  if (!track) return null;

  const favorited = !!favorites[track.id];

  const pct = progress.durationSec ? (progress.positionSec / progress.durationSec) * 100 : 0;
  const remaining = Math.max(0, progress.durationSec - progress.positionSec);

  const onSeek = (e: MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    seek(((e.clientX - rect.left) / rect.width) * progress.durationSec);
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
          <AnimatedIcon name={favorited ? "heart" : "favorites"} size={16} variant="pop" />
        </button>
      </div>

      {/* 中：控制簇 + 进度 */}
      <div className="mx-auto flex max-w-[520px] flex-1 flex-col gap-1.5">
        <div className="flex items-center justify-center gap-[13px]">
          <ShuffleControlButton
            active={shuffle}
            label={t("player.shuffle")}
            onClick={toggleShuffle}
            buttonSize={30}
            iconSize={15}
          />
          <SkipControlButton
            direction="previous"
            label={t("player.previous")}
            onClick={prev}
            buttonSize={32}
            iconSize={17}
          />
          <button
            aria-label={playing ? t("player.pause") : t("player.play")}
            onClick={togglePlay}
            className="play-action-material grid size-11 place-items-center rounded-full text-on-ac"
          >
            <PlayPauseIcon playing={playing} size={19} />
          </button>
          <SkipControlButton
            direction="next"
            label={t("player.next")}
            onClick={next}
            buttonSize={32}
            iconSize={17}
          />
          <RepeatControlButton
            mode={repeat}
            label={`${t("player.repeat")}: ${repeat}`}
            onClick={cycleRepeat}
            buttonSize={30}
            iconSize={15}
          />
        </div>
        <div className="flex items-center gap-2.5">
          <span className="text-[11px] tabular-nums text-tx2">{fmtTime(progress.positionSec)}</span>
          <div onClick={onSeek} className="h-1 flex-1 cursor-pointer overflow-hidden rounded-[2px] bg-bd">
            <div
              className="h-full rounded-[2px] bg-ac transition-[width] duration-100 ease-linear"
              style={{ width: `${pct}%` }}
            />
          </div>
          <span className="text-[11px] tabular-nums text-tx2">-{fmtTime(remaining)}</span>
        </div>
      </div>

      {/* 右：队列 + 添加 + 音量 */}
      <div className="flex w-[236px] items-center justify-end gap-2.5">
        <button
          aria-label={t("player.queue")}
          aria-expanded={queueOpen}
          data-queue-trigger
          onClick={() => setQueueOpen((v) => !v)}
          className={cn(
            "grid size-[30px] cursor-pointer place-items-center rounded-full transition-colors",
            queueOpen ? "bg-hv text-ac" : "text-tx2 hover:bg-hv hover:text-tx",
          )}
        >
          <Icon name="queue" size={16} strokeWidth={1.8} />
        </button>
        <button aria-label={t("player.addToPlaylist")} className="grid size-[30px] place-items-center rounded-full text-tx2 transition-colors hover:bg-hv hover:text-tx">
          <Icon name="addPlaylist" size={16} strokeWidth={1.8} />
        </button>
        <button
          type="button"
          aria-label={muted ? t("player.unmute") : t("player.mute")}
          aria-pressed={muted}
          title={muted ? t("player.unmute") : t("player.mute")}
          data-state={muted ? "muted" : "audible"}
          onClick={toggleMuted}
          className="grid size-7 shrink-0 place-items-center rounded-full text-tx2 transition-[color,background-color,transform] duration-150 hover:bg-hv hover:text-tx active:scale-90 data-[state=muted]:text-ac data-[state=muted]:hover:text-ac"
        >
          <Icon name={muted ? "volumeMuted" : "volume"} size={16} />
        </button>
        <VolumeSlider
          value={volume}
          muted={muted}
          label={t("player.volume")}
          onChange={setVolume}
          className="w-[88px]"
        />
      </div>

      {/* 队列面板：从播放条右上方弹出（与歌词页共享组件） */}
      <QueuePanel
        open={queueOpen}
        onDismiss={() => setQueueOpen(false)}
        className="absolute bottom-[calc(100%+12px)] right-0 origin-bottom-right"
      />
    </div>
  );
}
