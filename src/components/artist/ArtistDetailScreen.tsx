import { useMemo, useState, type UIEvent } from "react";
import { motion } from "framer-motion";
import { EqBars } from "@/components/common/EqBars";
import { Icon } from "@/components/common/Icon";
import { ItemContextMenu } from "@/components/common/ItemContextMenu";
import { useElasticScroll } from "@/hooks/useElasticScroll";
import { TRACK_MENU, albumsOfArtist, playsOf, topTracksOf, tracksOf } from "@/data/library";
import { usePlayerStore } from "@/store/player";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";
import { coverGradientStyle } from "@/lib/coverStyle";
import { fmtTime } from "@/lib/time";
import type { MessageKey } from "@/i18n/messages";
import type { Album, Track } from "@/types/player";

/** 吸顶栏出现阈值（设计稿：scrollTop > 210）。 */
const STICKY_THRESHOLD = 210;

export function ArtistDetailScreen({ artistName }: { artistName: string }) {
  const { t } = useT();
  const closeArtist = useUiStore((s) => s.closeArtist);
  const openAlbum = useUiStore((s) => s.openAlbum);
  const { scrollerRef, innerRef, thumbRef, onScroll } = useElasticScroll();
  const [barVisible, setBarVisible] = useState(false);

  const playing = usePlayerStore((s) => s.playing);
  const current = usePlayerStore((s) =>
    s.currentIndex >= 0 ? s.queue[s.currentIndex]?.track : null,
  );
  const favorites = usePlayerStore((s) => s.favorites);
  const favoriteAlbums = usePlayerStore((s) => s.favoriteAlbums);
  const followed = usePlayerStore((s) => !!s.favoriteArtists[artistName]);
  const playQueue = usePlayerStore((s) => s.playQueue);
  const togglePlay = usePlayerStore((s) => s.togglePlay);
  const toggleFavorite = usePlayerStore((s) => s.toggleFavorite);
  const toggleFavoriteAlbum = usePlayerStore((s) => s.toggleFavoriteAlbum);
  const toggleFavoriteArtist = usePlayerStore((s) => s.toggleFavoriteArtist);
  const enqueueNext = usePlayerStore((s) => s.enqueueNext);

  const albums = useMemo(() => albumsOfArtist(artistName), [artistName]);
  const topTracks = useMemo(() => topTracksOf(artistName), [artistName]);
  const allTracks = useMemo(() => albums.flatMap(tracksOf), [albums]);
  if (albums.length === 0) return null;

  const cover = albums[0].cover; // 头像用最新专辑封面（对齐设计稿「鲸」）
  const songCount = allTracks.length;
  const isThisArtist = current?.artist === artistName;
  const playingThis = isThisArtist && playing;

  const onPlayAll = () => {
    if (isThisArtist) togglePlay();
    else playQueue(allTracks, 0);
  };
  const onShuffle = () => {
    playQueue([...allTracks].sort(() => Math.random() - 0.5), 0);
  };
  const onTrackAction = (track: Track, index: number, key: MessageKey) => {
    switch (key) {
      case "menu.play":
        playQueue(topTracks, index);
        break;
      case "menu.playNext":
        enqueueNext(track);
        break;
      case "menu.favorite":
        toggleFavorite(track.id);
        break;
      case "menu.showLyrics":
        playQueue(topTracks, index);
        useUiStore.getState().openLyrics();
        break;
    }
  };
  const handleScroll = (e: UIEvent<HTMLDivElement>) => {
    onScroll(e);
    const v = e.currentTarget.scrollTop > STICKY_THRESHOLD;
    if (v !== barVisible) setBarVisible(v);
  };

  return (
    <div className="relative min-h-0 flex-1">
      {/* 吸顶栏：滚过头部后弹入 */}
      <div
        className="sticky-bar-shadow absolute inset-x-0 top-0 z-20 flex h-[58px] items-center gap-3 border-b border-bd bg-bg px-6"
        style={{
          opacity: barVisible ? 1 : 0,
          transform: `translateY(${barVisible ? 0 : -12}px)`,
          pointerEvents: barVisible ? "auto" : "none",
          transition: "opacity 0.25s ease, transform 0.25s var(--ease-spring)",
        }}
      >
        <div
          className="cover-gradient cover-thumb-material grid size-8 place-items-center rounded-full"
          style={coverGradientStyle(cover)}
        >
          <span className="cover-initial font-serif text-[14px]">{cover.initial}</span>
        </div>
        <div className="relative">
          <span className="font-serif text-[16.5px] font-semibold text-tx">{artistName}</span>
          {followed && (
            <span className="absolute -right-[13px] top-px text-ac">
              <Icon name="heart" size={10} />
            </span>
          )}
        </div>
        <div className="flex-1" />
        <motion.button
          whileHover={{ filter: "brightness(1.08)" }}
          whileTap={{ scale: 0.9 }}
          aria-label={playingThis ? t("player.pause") : t("artist.playAll")}
          onClick={onPlayAll}
          className="grid size-[34px] cursor-pointer place-items-center rounded-full bg-ac text-on-ac"
        >
          <Icon name={playingThis ? "pause" : "play"} size={13} style={{ marginLeft: playingThis ? 0 : 1 }} />
        </motion.button>
      </div>

      <div
        ref={scrollerRef}
        onScroll={handleScroll}
        className="no-scrollbar absolute inset-0 overflow-auto px-10 pb-[120px] [overscroll-behavior:contain]"
      >
        <div ref={innerRef} className="will-change-transform">
          {/* 面包屑返回（兼作窗口拖拽区） */}
          <div data-tauri-drag-region className="flex items-center pt-[22px]">
            <button
              onClick={closeArtist}
              className="flex cursor-pointer items-center gap-1.5 rounded-full py-[5px] pl-2 pr-3 text-[12.5px] text-tx2 transition-colors hover:bg-hv hover:text-tx"
            >
              <Icon name="chevronLeft" size={13} strokeWidth={2} />
              {t("nav.artists")}
            </button>
          </div>

          {/* 歌手头部 */}
          <div className="flex items-center gap-8 pb-[26px] pt-[18px]">
            <div
              className="cover-gradient artist-hero-material group/avatar relative grid size-[172px] flex-shrink-0 place-items-center rounded-full"
              style={coverGradientStyle(cover)}
            >
              <span className="cover-initial font-serif text-[60px] font-medium">
                {cover.initial}
              </span>
              {/* hover 浮现操作爱心（收藏歌手的唯一交互入口） */}
              <div className="artist-hero-overlay absolute inset-0 rounded-full opacity-0 transition-opacity duration-[220ms] group-hover/avatar:opacity-100">
                <motion.button
                  whileHover={{ scale: 1.1 }}
                  whileTap={{ scale: 0.9 }}
                  title={followed ? t("artist.unfollow") : t("artist.follow")}
                  aria-label={followed ? t("artist.unfollow") : t("artist.follow")}
                  onClick={() => toggleFavoriteArtist(artistName)}
                  className="collect-shadow absolute right-3.5 top-3.5 grid size-7 cursor-pointer place-items-center rounded-full bg-srf text-ac"
                >
                  <Icon name={followed ? "heart" : "favorites"} size={14} strokeWidth={2} />
                </motion.button>
              </div>
            </div>

            <div className="flex min-w-0 flex-col gap-[9px]">
              <div className="text-[11px] font-bold tracking-[0.16em] text-tx2">
                {t("artist.kicker")}
              </div>
              <div className="relative self-start">
                <h1 className="m-0 font-serif text-[42px] font-semibold leading-[1.15] text-tx">
                  {artistName}
                </h1>
                {followed && (
                  <span title={t("album.collected")} className="absolute -right-5 top-0.5 text-ac">
                    <Icon name="heart" size={14} />
                  </span>
                )}
              </div>
              <div className="text-sm text-tx2">
                {t("artist.meta", { albums: albums.length, songs: songCount, plays: playsOf(artistName) })}
              </div>
              <div className="mt-2 flex items-center gap-3">
                <motion.button
                  whileHover={{ filter: "brightness(1.08)" }}
                  whileTap={{ scale: 0.95 }}
                  onClick={onPlayAll}
                  className="flex cursor-pointer items-center gap-2 rounded-full bg-ac px-[26px] py-[11px] text-sm font-semibold text-on-ac"
                >
                  <Icon name={playingThis ? "pause" : "play"} size={14} style={{ marginLeft: 0 }} />
                  {playingThis ? t("player.pause") : t("artist.playAll")}
                </motion.button>
                <button
                  onClick={onShuffle}
                  className="flex cursor-pointer items-center gap-2 rounded-full border border-bd bg-srf px-[22px] py-[11px] text-sm font-semibold text-tx transition-colors hover:bg-hv active:scale-95"
                >
                  <Icon name="shuffle" size={15} strokeWidth={1.8} />
                  {t("album.shufflePlay")}
                </button>
              </div>
            </div>
          </div>

          {/* 热门歌曲 */}
          <div className="flex items-center border-t border-bd pb-2 pt-2.5">
            <span className="font-serif text-xl font-semibold text-tx">{t("artist.topSongs")}</span>
            <div className="flex-1" />
            <button className="flex cursor-pointer items-center gap-1 rounded-full px-3 py-1.5 text-[12.5px] text-tx2 transition-colors hover:bg-hv hover:text-tx">
              {t("artist.showAllSongs", { n: songCount })}
              <Icon name="chevronRight" size={12} strokeWidth={2} />
            </button>
          </div>
          <div className="no-scrollbar grid snap-x snap-mandatory auto-cols-[calc(50%-12px)] grid-flow-col grid-rows-[repeat(5,auto)] gap-x-6 overflow-x-auto pb-2">
            {topTracks.map((track, i) => {
              const isCur = current?.id === track.id;
              const liked = !!favorites[track.id];
              return (
                <ItemContextMenu
                  key={track.id}
                  label={`${track.title} — ${track.album}`}
                  items={TRACK_MENU}
                  onAction={(key) => onTrackAction(track, i, key)}
                >
                  <div
                    onClick={() => playQueue(topTracks, i)}
                    className="mt-0.5 grid snap-start cursor-pointer grid-cols-[40px_1fr_150px_40px_56px] items-center gap-3 rounded-xl px-3 py-2.5 transition-colors hover:bg-hv"
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
                    <span className="truncate text-[13px] text-tx2">{track.album}</span>
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

          {/* 专辑横排 */}
          <div className="flex items-center pb-3.5 pt-[26px]">
            <span className="font-serif text-xl font-semibold text-tx">{t("nav.albums")}</span>
            <div className="flex-1" />
            <button className="flex cursor-pointer items-center gap-1 rounded-full px-3 py-1.5 text-[12.5px] text-tx2 transition-colors hover:bg-hv hover:text-tx">
              {t("artist.showAllAlbums", { n: albums.length })}
              <Icon name="chevronRight" size={12} strokeWidth={2} />
            </button>
          </div>
          <div className="no-scrollbar -mx-3 -mb-2 -mt-3.5 flex snap-x snap-mandatory gap-6 overflow-x-auto px-3 pb-6 pt-3.5">
            {albums.map((album) => (
              <ArtistAlbumCard
                key={album.id}
                album={album}
                favorited={!!favoriteAlbums[album.id]}
                onOpen={() => openAlbum(album.id)}
                onPlay={() => playQueue(tracksOf(album), 0)}
                onToggleFavorite={() => toggleFavoriteAlbum(album.id)}
              />
            ))}
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

interface CardProps {
  album: Album;
  favorited: boolean;
  onOpen: () => void;
  onPlay: () => void;
  onToggleFavorite: () => void;
}

function ArtistAlbumCard({ album, favorited, onOpen, onPlay, onToggleFavorite }: CardProps) {
  const { t } = useT();
  return (
    <div
      className="relative w-[190px] min-w-0 flex-none cursor-pointer snap-start hover:z-10"
      onClick={onOpen}
    >
      <motion.div
        whileHover={{ y: -5 }}
        transition={{ type: "spring", stiffness: 380, damping: 18 }}
        className="cover-gradient cover-material group/card relative grid aspect-square place-items-center rounded-2xl"
        style={coverGradientStyle(album.cover)}
      >
        <span className="cover-initial font-serif text-5xl font-medium">{album.cover.initial}</span>
        <div className="artist-card-overlay absolute inset-0 flex items-end justify-end rounded-2xl p-3 opacity-0 transition-opacity duration-[220ms] group-hover/card:opacity-100">
          <motion.button
            whileHover={{ scale: 1.1 }}
            whileTap={{ scale: 0.9 }}
            title={favorited ? t("album.uncollect") : t("album.collect")}
            aria-label={favorited ? t("album.uncollect") : t("album.collect")}
            onClick={(e) => {
              e.stopPropagation();
              onToggleFavorite();
            }}
            className="collect-shadow absolute right-2.5 top-2.5 grid size-7 cursor-pointer place-items-center rounded-full bg-srf text-ac"
          >
            <Icon name={favorited ? "heart" : "favorites"} size={14} strokeWidth={2} />
          </motion.button>
          <motion.button
            whileTap={{ scale: 0.9 }}
            aria-label={t("action.playAlbum", { title: album.title })}
            onClick={(e) => {
              e.stopPropagation();
              onPlay();
            }}
            className="cover-action-shadow grid size-7 place-items-center rounded-full bg-ac text-on-ac"
          >
            <Icon name="play" size={12} style={{ marginLeft: 1 }} />
          </motion.button>
        </div>
      </motion.div>
      <div className="mt-3 flex min-w-0 items-center gap-1.5">
        <span className="truncate font-serif text-[15.5px] font-semibold text-tx">
          {album.title}
        </span>
        <div className="flex-1" />
        {favorited && (
          <span title={t("album.collected")} className="grid flex-shrink-0 place-items-center text-ac">
            <Icon name="heart" size={13} />
          </span>
        )}
      </div>
      <div className="mt-[3px] text-[12.5px] text-tx2">
        {album.year} · {t("unit.tracks", { n: album.trackCount })}
      </div>
    </div>
  );
}
