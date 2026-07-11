import { useMemo } from "react";
import { motion } from "framer-motion";
import { EqBars } from "@/components/common/EqBars";
import { Icon } from "@/components/common/Icon";
import { ItemContextMenu } from "@/components/common/ItemContextMenu";
import { useElasticScroll } from "@/hooks/useElasticScroll";
import { ALBUMS, TRACK_MENU, tracksOf } from "@/data/library";
import { usePlayerStore } from "@/store/player";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";
import { coverGradientStyle } from "@/lib/coverStyle";
import { fmtTime } from "@/lib/time";
import type { MessageKey } from "@/i18n/messages";
import type { Id, Track } from "@/types/player";

export function AlbumDetailScreen({ albumId }: { albumId: Id }) {
  const { t } = useT();
  const closeAlbum = useUiStore((s) => s.closeAlbum);
  const openArtist = useUiStore((s) => s.openArtist);
  const { scrollerRef, innerRef, thumbRef, onScroll } = useElasticScroll();

  const playing = usePlayerStore((s) => s.playing);
  const current = usePlayerStore((s) =>
    s.currentIndex >= 0 ? s.queue[s.currentIndex]?.track : null,
  );
  const favorites = usePlayerStore((s) => s.favorites);
  const favoriteAlbums = usePlayerStore((s) => s.favoriteAlbums);
  const playQueue = usePlayerStore((s) => s.playQueue);
  const togglePlay = usePlayerStore((s) => s.togglePlay);
  const toggleFavorite = usePlayerStore((s) => s.toggleFavorite);
  const toggleFavoriteAlbum = usePlayerStore((s) => s.toggleFavoriteAlbum);
  const enqueueNext = usePlayerStore((s) => s.enqueueNext);

  const album = ALBUMS.find((a) => a.id === albumId);
  const tracks = useMemo(() => (album ? tracksOf(album) : []), [album]);
  if (!album) return null;

  const collected = !!favoriteAlbums[album.id];
  const isThisAlbum = current?.albumId === album.id;
  const playingThis = isThisAlbum && playing;
  const totalSec = tracks.reduce((s, tk) => s + tk.durationSec, 0);

  const onPlayAlbum = () => {
    if (isThisAlbum) togglePlay();
    else playQueue(tracks, 0);
  };
  const onShuffleAlbum = () => {
    const shuffled = [...tracks].sort(() => Math.random() - 0.5);
    playQueue(shuffled, 0);
  };
  const onTrackAction = (track: Track, index: number, key: MessageKey) => {
    switch (key) {
      case "menu.play":
        playQueue(tracks, index);
        break;
      case "menu.playNext":
        enqueueNext(track);
        break;
      case "menu.favorite":
        toggleFavorite(track.id);
        break;
      case "menu.showLyrics":
        playQueue(tracks, index);
        useUiStore.getState().openLyrics();
        break;
    }
  };

  return (
    <div className="relative min-h-0 flex-1">
      <div
        ref={scrollerRef}
        onScroll={onScroll}
        className="no-scrollbar absolute inset-0 overflow-auto px-10 pb-[120px] [overscroll-behavior:contain]"
      >
        <div ref={innerRef}>
          {/* 面包屑返回（兼作窗口拖拽区） */}
          <div data-tauri-drag-region className="flex items-center pt-[22px]">
            <button
              onClick={closeAlbum}
              className="flex cursor-pointer items-center gap-1.5 rounded-full py-[5px] pl-2 pr-3 text-[12.5px] text-tx2 transition-colors hover:bg-hv hover:text-tx"
            >
              <Icon name="chevronLeft" size={13} strokeWidth={2} />
              {t("nav.albums")}
            </button>
          </div>

          {/* 专辑头部 */}
          <div className="flex items-center gap-9 pb-[30px] pt-[18px]">
            <div
              className="cover-corners cover-gradient cover-hero-material group/cover relative grid size-[232px] flex-shrink-0 place-items-center rounded-2xl"
              style={coverGradientStyle(album.cover)}
            >
              <span className="cover-initial font-serif text-[76px] font-medium">
                {album.cover.initial}
              </span>
              {/* hover 浮现操作爱心（收藏专辑的唯一交互入口） */}
              <div className="cover-corners cover-hero-overlay absolute inset-0 rounded-2xl opacity-0 transition-opacity duration-[220ms] group-hover/cover:opacity-100">
                <motion.button
                  whileHover={{ scale: 1.1 }}
                  whileTap={{ scale: 0.9 }}
                  title={collected ? t("album.uncollect") : t("album.collect")}
                  aria-label={collected ? t("album.uncollect") : t("album.collect")}
                  onClick={() => toggleFavoriteAlbum(album.id)}
                  className="collect-shadow absolute right-3 top-3 grid size-7 cursor-pointer place-items-center rounded-full bg-srf text-ac"
                >
                  <Icon name={collected ? "heart" : "favorites"} size={14} strokeWidth={2} />
                </motion.button>
              </div>
            </div>

            <div className="flex min-w-0 flex-col gap-2.5">
              <div className="text-[11px] font-bold tracking-[0.16em] text-tx2">
                {t("album.kicker")}
              </div>
              <div className="relative self-start">
                <h1 className="m-0 font-serif text-[42px] font-semibold leading-[1.15] text-tx">
                  {album.title}
                </h1>
                {/* 已收藏角标：绝对定位，不占版面、不可点 */}
                {collected && (
                  <span title={t("album.collected")} className="absolute -right-5 top-0.5 text-ac">
                    <Icon name="heart" size={14} />
                  </span>
                )}
              </div>
              <div className="text-sm text-tx2">
                <span
                  onClick={() => openArtist(album.artist)}
                  className="cursor-pointer font-semibold text-ac"
                >
                  {album.artist}
                </span>
                {" · "}
                {album.year} · {album.genre} · {t("unit.tracks", { n: album.trackCount })} ·{" "}
                {t("unit.minutes", { n: Math.floor(totalSec / 60) })}
              </div>
              <div className="mt-2.5 flex items-center gap-3">
                <motion.button
                  whileHover={{ filter: "brightness(1.08)" }}
                  whileTap={{ scale: 0.95 }}
                  onClick={onPlayAlbum}
                  className="flex cursor-pointer items-center gap-2 rounded-full bg-ac px-[26px] py-[11px] text-sm font-semibold text-on-ac"
                >
                  <Icon name={playingThis ? "pause" : "play"} size={14} style={{ marginLeft: 0 }} />
                  {playingThis ? t("player.pause") : t("player.play")}
                </motion.button>
                <button
                  onClick={onShuffleAlbum}
                  className="flex cursor-pointer items-center gap-2 rounded-full border border-bd bg-srf px-[22px] py-[11px] text-sm font-semibold text-tx transition-colors hover:bg-hv active:scale-95"
                >
                  <Icon name="shuffle" size={15} strokeWidth={1.8} />
                  {t("album.shufflePlay")}
                </button>
                <button
                  aria-label={t("album.more")}
                  title={t("album.more")}
                  className="grid size-10 cursor-pointer place-items-center rounded-full border border-bd bg-srf text-tx2 transition-colors hover:bg-hv hover:text-tx"
                >
                  <Icon name="more" size={16} />
                </button>
              </div>
            </div>
          </div>

          {/* 曲目列表 */}
          <div className="border-t border-bd">
            {tracks.map((track: Track, i: number) => {
              const isCur = current?.id === track.id;
              const liked = !!favorites[track.id];
              return (
                <ItemContextMenu
                  key={track.id}
                  label={`${track.title} — ${track.artist}`}
                  items={TRACK_MENU}
                  onAction={(key) => onTrackAction(track, i, key)}
                >
                  <div
                    onClick={() => playQueue(tracks, i)}
                    className="mt-0.5 grid cursor-pointer grid-cols-[44px_1fr_44px_64px] items-center gap-3.5 rounded-xl px-3.5 py-[11px] transition-colors hover:bg-hv"
                  >
                    {isCur ? (
                      <EqBars playing={playing} />
                    ) : (
                      <span className="pl-[3px] text-[13px] tabular-nums text-tx2">{i + 1}</span>
                    )}
                    <span
                      className={cn(
                        "truncate font-serif text-[15.5px]",
                        isCur ? "font-semibold text-ac" : "font-medium text-tx",
                      )}
                    >
                      {track.title}
                    </span>
                    <button
                      aria-label={liked ? t("player.unfavorite") : t("player.favorite")}
                      onClick={(e) => {
                        e.stopPropagation();
                        toggleFavorite(track.id);
                      }}
                      className={cn(
                        "grid size-[30px] cursor-pointer place-items-center rounded-full transition-colors hover:bg-ac/12",
                        liked ? "text-ac" : "text-tx2",
                      )}
                    >
                      <Icon name={liked ? "heart" : "favorites"} size={15} strokeWidth={1.8} />
                    </button>
                    <span className="text-right text-[13px] tabular-nums text-tx2">
                      {fmtTime(track.durationSec)}
                    </span>
                  </div>
                </ItemContextMenu>
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
