import { ExpandToggle } from "@/components/common/ExpandToggle";
import { metaJoin } from "@/lib/meta";
import { CoverArt } from "@/components/common/CoverArt";
import { useMemo, useRef, useState, type RefObject, type UIEvent } from "react";
import { motion } from "framer-motion";
import { AnimatedIcon } from "@/components/common/AnimatedIcon";
import { Icon } from "@/components/common/Icon";
import { useMetadataEditor } from "@/components/common/EditMetadataDialog";
import { ItemContextMenu } from "@/components/common/ItemContextMenu";
import { PlayPauseIcon } from "@/components/common/PlayPauseIcon";
import { TrackIndicator } from "@/components/common/TrackIndicator";
import { useElasticScroll } from "@/hooks/useElasticScroll";
import { TRACK_MENU } from "@/data/library";
import { albumsRelatedToArtist, playsOf, topTracksOf, tracksByArtist, tracksOf } from "@/lib/library";
import { usePlayerStore } from "@/store/player";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";
import { coverGradientStyle } from "@/lib/coverStyle";
import { addTracksToPlaylistArg } from "@/lib/playlistActions";
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
  // 「显示全部」就地展开：热门歌曲与专辑默认是精选 + 横向滚动，
  // 展开后换成完整的纵向列表 / 自适应网格，不另开页面。
  const [allSongsOpen, setAllSongsOpen] = useState(false);
  const [allAlbumsOpen, setAllAlbumsOpen] = useState(false);
  const songsHeadRef = useRef<HTMLDivElement>(null);
  const albumsHeadRef = useRef<HTMLDivElement>(null);

  /**
   * 收起时把区块标题带回视野。
   *
   * 一百多行收成十行，文档骤然变矮，浏览器会把滚动位置钳到新的底部——
   * 用户点了「收起」，视线却被甩到页面别处。展开方向不需要处理：新增内容在
   * 标题下方，标题本身不动。
   */
  const toggleSection = (
    open: boolean,
    setOpen: (v: boolean) => void,
    head: RefObject<HTMLDivElement | null>,
  ) => {
    const collapsing = open;
    setOpen(!open);
    if (!collapsing) return;
    requestAnimationFrame(() => {
      const sc = scrollerRef.current;
      const el = head.current;
      if (!sc || !el) return;
      const top = el.getBoundingClientRect().top - sc.getBoundingClientRect().top + sc.scrollTop;
      if (sc.scrollTop > top) sc.scrollTo({ top: Math.max(0, top - 12) });
    });
  };

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
  const { dialog: editDialog, editTrack } = useMetadataEditor();

  // 「参与过的专辑」与「演唱过的曲目」——只看专辑艺人的话，
  // 合辑里的客串歌手会得到一个彻底空白的页面。
  const albums = useMemo(() => albumsRelatedToArtist(artistName), [artistName]);
  const topTracks = useMemo(() => topTracksOf(artistName), [artistName]);
  const allTracks = useMemo(() => tracksByArtist(artistName), [artistName]);
  if (albums.length === 0 && allTracks.length === 0) return null;

  // 列表与播放队列都用当前可见的这一组，避免「看到的」与「播的」不一致。
  const songs = allSongsOpen ? allTracks : topTracks;
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
  const onTrackAction = (track: Track, index: number, key: MessageKey, arg?: string) => {
    switch (key) {
      case "menu.addToPlaylist":
        if (arg) addTracksToPlaylistArg(arg, [track], t("playlist.newDefaultName"));
        break;
      case "menu.play":
        playQueue(songs, index);
        break;
      case "menu.playNext":
        enqueueNext(track);
        break;
      case "menu.favorite":
        toggleFavorite(track.id);
        break;
      case "menu.showLyrics":
        playQueue(songs, index);
        useUiStore.getState().openLyrics();
        break;
      case "menu.editTags":
        editTrack(track);
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
        {/* 吸顶栏里的返回：页面滚动后面包屑已经不在视野内，没有这个按钮就退不出去 */}
        <button
          aria-label={t("common.back")}
          onClick={closeArtist}
          className="grid size-[30px] flex-none cursor-pointer place-items-center rounded-full text-tx2 transition-colors hover:bg-hv hover:text-tx"
        >
          <Icon name="chevronLeft" size={15} strokeWidth={2} />
        </button>
        <div
          className="cover-gradient cover-thumb-material relative grid size-8 place-items-center rounded-full"
          style={coverGradientStyle(cover)}
        >
          <span className="cover-initial font-serif text-[14px]">{cover.initial}</span>
          <CoverArt cover={cover} px={32} />
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
          aria-label={playingThis ? t("player.pause") : t("artist.playAll")}
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
              <CoverArt cover={cover} px={172} />
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
                  <AnimatedIcon
                    name={followed ? "heart" : "favorites"}
                    size={14}
                    strokeWidth={2}
                    variant="pop"
                  />
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
                  onClick={onPlayAll}
                  className="play-action-material flex cursor-pointer items-center gap-2 rounded-full px-[26px] py-[11px] text-sm font-semibold text-on-ac"
                >
                  <PlayPauseIcon playing={playingThis} size={16} />
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
          <div ref={songsHeadRef} className="flex items-center border-t border-bd pb-2 pt-2.5">
            {/* 展开后列出的是全部曲目（按专辑顺序），再叫「热门歌曲」名不副实 */}
            <span className="font-serif text-xl font-semibold text-tx">
              {allSongsOpen ? t("nav.songs") : t("artist.topSongs")}
            </span>
            <div className="flex-1" />
            <ExpandToggle
              open={allSongsOpen}
              onToggle={() => toggleSection(allSongsOpen, setAllSongsOpen, songsHeadRef)}
              openLabel={t("artist.showAllSongs", { n: songCount })}
              closeLabel={t("artist.showLess")}
            />
          </div>
          <div
            className={cn(
              "no-scrollbar pb-2",
              allSongsOpen
                ? "flex flex-col"
                : "grid snap-x snap-mandatory auto-cols-[calc(50%-12px)] grid-flow-col grid-rows-[repeat(5,auto)] gap-x-6 overflow-x-auto",
            )}
          >
            {songs.map((track, i) => {
              const isCur = current?.id === track.id;
              const liked = !!favorites[track.id];
              // 展开时让紧接在原有十首之后的十来行浮现一下；再往后的直接显示——
              // 一百多行同时做入场动画会掉帧，而且屏幕上也看不到那么远。
              const entering = allSongsOpen && i >= topTracks.length && i < topTracks.length + 12;
              return (
                <ItemContextMenu
                  key={track.id}
                  label={`${track.title} — ${track.album}`}
                  items={TRACK_MENU}
                  onAction={(key, arg) => onTrackAction(track, i, key, arg)}
                  containsTrackId={track.id}
                >
                  <div
                    onClick={() => playQueue(songs, i)}
                    style={
                      entering ? { animationDelay: `${(i - topTracks.length) * 15}ms` } : undefined
                    }
                    className={cn(
                      "mt-0.5 grid snap-start cursor-pointer grid-cols-[40px_1fr_150px_40px_56px] items-center gap-3 rounded-xl px-3 py-2.5 transition-colors hover:bg-hv",
                      entering && "animate-row-in",
                    )}
                  >
                    <span className="text-[13px] tabular-nums text-tx2">
                      <TrackIndicator number={i + 1} active={isCur} playing={playing} />
                    </span>
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

          {/* 专辑横排 */}
          <div ref={albumsHeadRef} className="flex items-center pb-3.5 pt-[26px]">
            <span className="font-serif text-xl font-semibold text-tx">{t("nav.albums")}</span>
            <div className="flex-1" />
            <ExpandToggle
              open={allAlbumsOpen}
              onToggle={() => toggleSection(allAlbumsOpen, setAllAlbumsOpen, albumsHeadRef)}
              openLabel={t("artist.showAllAlbums", { n: albums.length })}
              closeLabel={t("artist.showLess")}
            />
          </div>
          <div
            className={cn(
              "no-scrollbar -mx-3 -mb-2 -mt-3.5 gap-6 px-3 pb-6 pt-3.5",
              allAlbumsOpen
                ? "grid grid-cols-[repeat(auto-fill,minmax(190px,1fr))]"
                : "flex snap-x snap-mandatory overflow-x-auto",
            )}
          >
            {albums.map((album) => (
              <ArtistAlbumCard
                key={album.id}
                album={album}
                fluid={allAlbumsOpen}
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
      {editDialog}
    </div>
  );
}

interface CardProps {
  album: Album;
  favorited: boolean;
  /** 网格展开态：宽度随列宽，不再固定 190px。 */
  fluid?: boolean;
  onOpen: () => void;
  onPlay: () => void;
  onToggleFavorite: () => void;
}

function ArtistAlbumCard({ album, favorited, fluid, onOpen, onPlay, onToggleFavorite }: CardProps) {
  const { t } = useT();
  return (
    <div
      className={cn(
        "relative min-w-0 cursor-pointer hover:z-10",
        // 横排时固定宽度并吸附，展开成网格后交给列宽决定
        fluid ? "w-full" : "w-[190px] flex-none snap-start",
      )}
      onClick={onOpen}
    >
      <motion.div
        layoutId={`album-cover-${album.id}`}
        // layoutId 会顺带开启 layout 动画，于是「展开 / 收起歌曲」导致下方专辑整体
        // 位移时，每张封面都会把这段位移做成 spring 动画——damping 18 还会过冲，
        // 看起来就是一排封面在弹跳。layoutDependency 固定为专辑 ID：位置变化不再
        // 重新测量，而卡片与详情页大封面之间的共享过渡（靠 layoutId 配对）不受影响。
        layoutDependency={album.id}
        whileHover={{ y: -5 }}
        transition={{ type: "spring", stiffness: 380, damping: 18 }}
        className="cover-corners cover-gradient cover-material group/card relative grid aspect-square place-items-center rounded-2xl"
        style={coverGradientStyle(album.cover)}
      >
        <span className="cover-initial font-serif text-5xl font-medium">{album.cover.initial}</span>
        <CoverArt cover={album.cover} px={200} />
        <div className="cover-corners artist-card-overlay absolute inset-0 flex items-end justify-end rounded-2xl p-3 opacity-0 transition-opacity duration-[220ms] group-hover/card:opacity-100">
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
            <AnimatedIcon
              name={favorited ? "heart" : "favorites"}
              size={14}
              strokeWidth={2}
              variant="pop"
            />
          </motion.button>
          <motion.button
            aria-label={t("action.playAlbum", { title: album.title })}
            onClick={(e) => {
              e.stopPropagation();
              onPlay();
            }}
            className="play-action-material play-action-compact grid size-7 place-items-center rounded-full text-on-ac"
          >
            <PlayPauseIcon playing={false} size={13} />
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
        {metaJoin(album.year, t("unit.tracks", { n: album.trackCount }))}
      </div>
    </div>
  );
}
