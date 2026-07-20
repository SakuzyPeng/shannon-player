import { useMemo, useRef, useState, type UIEvent } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { AnimatedIcon } from "@/components/common/AnimatedIcon";
import { Collage } from "@/components/common/Collage";
import { FilterPill, useFilterPill } from "@/components/common/FilterPill";
import { Icon } from "@/components/common/Icon";
import { ItemContextMenu } from "@/components/common/ItemContextMenu";
import { PlayPauseIcon } from "@/components/common/PlayPauseIcon";
import { TrackIndicator } from "@/components/common/TrackIndicator";
import { useElasticScroll } from "@/hooks/useElasticScroll";
import { PLAYLIST_TRACK_MENU } from "@/data/library";
import { collageOf } from "@/data/playlists";
import { usePlayerStore } from "@/store/player";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";
import { addTracksToPlaylistArg } from "@/lib/playlistActions";
import { fmtTime } from "@/lib/time";
import type { MessageKey } from "@/i18n/messages";
import type { Id, Track } from "@/types/player";

const STICKY_THRESHOLD = 210;
const COLS = "grid-cols-[44px_1fr_170px_190px_44px_60px]";

/** 过滤命中高亮：把标题按第一个命中拆三段，命中段 --ac。 */
function Highlight({ text, query }: { text: string; query: string }) {
  if (query) {
    const hi = text.toLowerCase().indexOf(query);
    if (hi >= 0) {
      return (
        <>
          {text.slice(0, hi)}
          <span className="text-ac">{text.slice(hi, hi + query.length)}</span>
          {text.slice(hi + query.length)}
        </>
      );
    }
  }
  return <>{text}</>;
}

export function PlaylistDetailScreen({ playlistId }: { playlistId: Id }) {
  const { t } = useT();
  const reduceMotion = useReducedMotion();
  const setNav = useUiStore((s) => s.setNav);
  const { scrollerRef, innerRef, thumbRef, onScroll } = useElasticScroll();
  const [barVisible, setBarVisible] = useState(false);
  const { filter, query } = useFilterPill();
  const headerInputRef = useRef<HTMLInputElement | null>(null);
  const barInputRef = useRef<HTMLInputElement | null>(null);

  const playing = usePlayerStore((s) => s.playing);
  const current = usePlayerStore((s) =>
    s.currentIndex >= 0 ? s.queue[s.currentIndex]?.track : null,
  );
  const favorites = usePlayerStore((s) => s.favorites);
  const collected = usePlayerStore((s) => !!s.favoritePlaylists[playlistId]);
  const playQueue = usePlayerStore((s) => s.playQueue);
  const togglePlay = usePlayerStore((s) => s.togglePlay);
  const toggleFavorite = usePlayerStore((s) => s.toggleFavorite);
  const toggleFavoritePlaylist = usePlayerStore((s) => s.toggleFavoritePlaylist);
  const enqueueNext = usePlayerStore((s) => s.enqueueNext);

  const playlist = usePlayerStore((s) => s.playlists.find((p) => p.id === playlistId));
  const covers = useMemo(() => (playlist ? collageOf(playlist) : []), [playlist]);
  const allTracks = playlist?.tracks ?? [];
  const entries = useMemo(
    () =>
      query
        ? allTracks.filter((tk) =>
            `${tk.title} ${tk.artist} ${tk.album}`.toLowerCase().includes(query),
          )
        : allTracks,
    [allTracks, query],
  );
  if (!playlist) return null;

  const totalSec = allTracks.reduce((s, tk) => s + tk.durationSec, 0);
  const playingThis = playing && allTracks.some((tk) => tk.id === current?.id);

  const onPlayAll = () => {
    if (allTracks.some((tk) => tk.id === current?.id)) togglePlay();
    else playQueue(allTracks, 0);
  };
  const onShuffle = () => playQueue([...allTracks].sort(() => Math.random() - 0.5), 0);
  const onTrackAction = (track: Track, index: number, key: MessageKey, arg?: string) => {
    switch (key) {
      case "menu.addToPlaylist":
        if (arg) addTracksToPlaylistArg(arg, [track], t("playlist.newDefaultName"));
        break;
      case "menu.play":
        playQueue(entries, index);
        break;
      case "menu.playNext":
        enqueueNext(track);
        break;
      case "menu.favorite":
        toggleFavorite(track.id);
        break;
      case "menu.showLyrics":
        playQueue(entries, index);
        useUiStore.getState().openLyrics();
        break;
    }
  };
  const handleScroll = (e: UIEvent<HTMLDivElement>) => {
    onScroll(e);
    const v = e.currentTarget.scrollTop > STICKY_THRESHOLD;
    if (v !== barVisible) setBarVisible(v);
  };

  const meta = t("playlist.meta", {
    n: allTracks.length,
    m: Math.round(totalSec / 60),
    updated: playlist.updatedLabel || t("playlist.updatedNow"),
  });

  return (
    <div className="relative min-h-0 flex-1">
      {/* 吸顶栏 */}
      <div
        className="sticky-bar-shadow absolute inset-x-0 top-0 z-20 flex h-[58px] items-center gap-3 border-b border-bd bg-bg px-6"
        style={{
          opacity: barVisible ? 1 : 0,
          transform: `translateY(${barVisible ? 0 : -12}px)`,
          pointerEvents: barVisible ? "auto" : "none",
          transition: "opacity 0.25s ease, transform 0.25s var(--ease-spring)",
        }}
      >
        <Collage covers={covers} size={32} radius={8} />
        <div className="relative">
          <span className="font-serif text-[16.5px] font-semibold text-tx">{playlist.title}</span>
          {collected && (
            <span className="absolute -right-[13px] top-px text-ac">
              <Icon name="heart" size={10} />
            </span>
          )}
        </div>
        <FilterPill
          filter={filter}
          height={34}
          openWidth={300}
          inputRef={barInputRef}
          placeholder={t("playlist.filterPlaceholder")}
          className="ml-auto"
        />
        <motion.button
          aria-label={playingThis ? t("player.pause") : t("player.play")}
          onClick={onPlayAll}
          className="play-action-material play-action-compact grid size-[34px] cursor-pointer place-items-center rounded-full text-on-ac"
        >
          <PlayPauseIcon playing={playingThis} size={15} />
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
              onClick={() => setNav("favorites")}
              className="flex cursor-pointer items-center gap-1.5 rounded-full py-[5px] pl-2 pr-3 text-[12.5px] text-tx2 transition-colors hover:bg-hv hover:text-tx"
            >
              <Icon name="chevronLeft" size={13} strokeWidth={2} />
              {t("nav.favorites")}
            </button>
          </div>

          {/* 歌单头部 */}
          <div className="flex items-center gap-9 pb-[30px] pt-[18px]">
            <div className="group/cover relative flex-shrink-0">
              <Collage covers={covers} size={232} radius={16} glyph={38} className="collage-hero-shadow" />
              <div className="cover-corners absolute inset-0 rounded-2xl opacity-0 transition-opacity duration-[220ms] group-hover/cover:opacity-100">
                <motion.button
                  whileHover={{ scale: 1.1 }}
                  whileTap={{ scale: 0.9 }}
                  title={collected ? t("album.uncollect") : t("album.collect")}
                  aria-label={collected ? t("album.uncollect") : t("album.collect")}
                  onClick={() => toggleFavoritePlaylist(playlist.id)}
                  className="collect-shadow absolute right-3 top-3 grid size-7 cursor-pointer place-items-center rounded-full bg-srf text-ac"
                >
                  <AnimatedIcon
                    name={collected ? "heart" : "favorites"}
                    size={14}
                    strokeWidth={2}
                    variant="pop"
                  />
                </motion.button>
              </div>
            </div>

            <div className="flex min-w-0 flex-1 flex-col gap-2.5">
              <div className="text-[11px] font-bold tracking-[0.16em] text-tx2">
                {t("playlist.kicker")}
              </div>
              <div className="relative self-start">
                <h1 className="m-0 font-serif text-[42px] font-semibold leading-[1.15] text-tx">
                  {playlist.title}
                </h1>
                {collected && (
                  <span title={t("album.collected")} className="absolute -right-5 top-0.5 text-ac">
                    <Icon name="heart" size={14} />
                  </span>
                )}
              </div>
              <div className="max-w-[520px] text-[13.5px] leading-[1.6] text-tx2">
                {playlist.description}
              </div>
              <div className="text-[13px] text-tx2">{meta}</div>
              <div className="mt-2 flex items-center gap-3">
                <motion.button
                  onClick={onPlayAll}
                  className="play-action-material flex cursor-pointer items-center gap-2 rounded-full px-[26px] py-[11px] text-sm font-semibold text-on-ac"
                >
                  <PlayPauseIcon playing={playingThis} size={16} />
                  {playingThis ? t("player.pause") : t("player.play")}
                </motion.button>
                <button
                  onClick={onShuffle}
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
                <FilterPill
                  filter={filter}
                  height={40}
                  openWidth={318}
                  inputRef={headerInputRef}
                  placeholder={t("playlist.filterPlaceholder")}
                  className="ml-auto"
                />
              </div>
            </div>
          </div>

          {/* 曲目列表 */}
          <div className="border-t border-bd">
            <div className={`grid ${COLS} items-center gap-3 px-3.5 pb-2 pt-2.5 text-[11px] font-semibold tracking-[0.08em] text-tx2`}>
              <span>#</span>
              <span>{t("nav.songs")}</span>
              <span>{t("songs.colArtist")}</span>
              <span>{t("list.album")}</span>
              <span />
              <span className="text-right">{t("list.duration")}</span>
            </div>

            <AnimatePresence initial={false}>
            {entries.length === 0 && (
              <motion.div
                key="playlist-empty"
                className="flex flex-col items-center gap-2.5 py-11 text-center"
                initial={{ opacity: 0, y: reduceMotion ? 0 : 7 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: reduceMotion ? 0 : -5 }}
                transition={{ duration: reduceMotion ? 0.01 : 0.18, ease: [0.22, 1, 0.36, 1] }}
              >
                <div className="font-serif text-[15px] font-semibold text-tx">
                  {t("playlist.emptyTitle", { q: filter.q.trim() })}
                </div>
                <div className="text-[12.5px] text-tx2">
                  {t("playlist.emptyTryGlobal")}
                  <span
                    onClick={() => setNav("albums")}
                    className="cursor-pointer font-semibold text-ac"
                  >
                    {t("playlist.emptyGlobalSearch")}
                  </span>
                </div>
              </motion.div>
            )}
            </AnimatePresence>

            {entries.map((track, i) => {
              const isCur = current?.id === track.id;
              const liked = !!favorites[track.id];
              return (
                <ItemContextMenu
                  key={track.id}
                  label={`${track.title} — ${track.artist}`}
                  items={PLAYLIST_TRACK_MENU}
                  onAction={(key, arg) => onTrackAction(track, i, key, arg)}
                  containsTrackId={track.id}
                >
                  <div
                    onClick={() => playQueue(entries, i)}
                    className={`group/row mt-0.5 grid ${COLS} cursor-pointer items-center gap-3 rounded-xl px-3.5 py-2.5 transition-colors hover:bg-hv`}
                  >
                    <span className="cursor-grab text-[13px] tabular-nums text-tx2">
                      <TrackIndicator
                        number={i + 1}
                        active={isCur}
                        playing={playing}
                        showGripOnHover
                        gripTitle={t("playlist.dragToReorder")}
                      />
                    </span>
                    <span
                      className={cn(
                        "truncate font-serif text-[15.5px]",
                        isCur ? "font-semibold text-ac" : "font-medium text-tx",
                      )}
                    >
                      <Highlight text={track.title} query={query} />
                    </span>
                    <span className="truncate text-[13px] text-tx2">{track.artist}</span>
                    <span className="truncate text-[13px] text-tx2">{track.album}</span>
                    <button
                      aria-label={liked ? t("player.unfavorite") : t("player.favorite")}
                      onClick={(e) => {
                        e.stopPropagation();
                        toggleFavorite(track.id);
                      }}
                      className={cn(
                        "grid size-[30px] cursor-pointer place-items-center rounded-full transition-[transform,background-color,color] hover:bg-ac/12 active:scale-90",
                        liked ? "text-ac" : "text-tx2",
                      )}
                    >
                      <AnimatedIcon
                        name={liked ? "heart" : "favorites"}
                        size={15}
                        strokeWidth={1.8}
                        variant="pop"
                      />
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
