import { metaJoin } from "@/lib/meta";
import { CoverArt } from "@/components/common/CoverArt";
import { useMemo, useState, type UIEvent } from "react";
import { motion } from "framer-motion";
import { AnimatedIcon } from "@/components/common/AnimatedIcon";
import { DetailNotFound } from "@/components/common/DetailNotFound";
import { useMetadataEditor } from "@/components/common/EditMetadataDialog";
import { Icon } from "@/components/common/Icon";
import { ItemContextMenu, ItemMoreMenu } from "@/components/common/ItemContextMenu";
import { PlayPauseIcon } from "@/components/common/PlayPauseIcon";
import { TrackIndicator } from "@/components/common/TrackIndicator";
import { useElasticScroll } from "@/hooks/useElasticScroll";
import { ALBUM_MENU, TRACK_MENU } from "@/data/library";
import { albums as libraryAlbums, tracksOf } from "@/lib/library";
import { usePlayerStore } from "@/store/player";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";
import { coverGradientStyle } from "@/lib/coverStyle";
import { addTracksToPlaylistArg } from "@/lib/playlistActions";
import { shuffled } from "@/lib/shuffle";
import { fmtTime } from "@/lib/time";
import type { MessageKey } from "@/i18n/messages";
import type { Id, Track } from "@/types/player";

/** 专辑标题滚出后显示精简吸顶栏，短专辑也有足够的可见区间。 */
const STICKY_THRESHOLD = 160;

export function AlbumDetailScreen({ albumId }: { albumId: Id }) {
  const { t } = useT();
  const closeAlbum = useUiStore((s) => s.closeAlbum);
  const openArtist = useUiStore((s) => s.openArtist);
  const { scrollerRef, innerRef, thumbRef, onScroll } = useElasticScroll();
  const [barVisible, setBarVisible] = useState(false);

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
  const { dialog: editDialog, editTrack, editAlbum } = useMetadataEditor();

  const album = libraryAlbums().find((a) => a.id === albumId);
  const tracks = useMemo(() => (album ? tracksOf(album) : []), [album]);
  /**
   * 按碟分组，同时保留每首在整张专辑里的位置——播放要用全局位置，
   * 显示序号要用碟内音轨号。两者混用就会出现「第二碟第一首显示成 16」。
   */
  const discs = useMemo(() => {
    const byDisc = new Map<number, { track: Track; index: number }[]>();
    tracks.forEach((track, index) => {
      const d = track.discNo ?? 1;
      if (!byDisc.has(d)) byDisc.set(d, []);
      byDisc.get(d)!.push({ track, index });
    });
    return [...byDisc.entries()].sort((a, b) => a[0] - b[0]);
  }, [tracks]);
  // 单碟专辑不显示碟标题，维持设计稿原样。
  const multiDisc = discs.length > 1;
  /**
   * 曲目艺人不止一位时才显示歌手列。
   *
   * 设计稿是按「一张专辑一位歌手」画的，行里只有标题；但合辑、致敬盘、社团专辑
   * 里每首歌的演唱者都不同（实测 28 张里有 10 张如此，最多的一张有 10 位），
   * 不显示就完全看不出谁唱的。单人专辑仍旧不显示——每行重复同一个名字是噪音。
   */
  const showTrackArtist = useMemo(
    () => new Set(tracks.map((tk) => tk.artist)).size > 1,
    [tracks],
  );
  // 专辑 ID 是聚合派生的，重扫后会变——旧 ID 打不开时给退路，别留白屏。
  if (!album) return <DetailNotFound backLabel="nav.albums" onBack={closeAlbum} />;

  const collected = !!favoriteAlbums[album.id];
  const isThisAlbum = current?.albumId === album.id;
  const playingThis = isThisAlbum && playing;
  const totalSec = tracks.reduce((s, tk) => s + tk.durationSec, 0);

  const onPlayAlbum = () => {
    if (isThisAlbum) togglePlay();
    else playQueue(tracks, 0);
  };
  const onShuffleAlbum = () => playQueue(shuffled(tracks), 0);
  /** 专辑级动作（「…」菜单）：与专辑卡右键菜单一致。 */
  const onAlbumAction = (key: MessageKey, arg?: string) => {
    switch (key) {
      case "menu.addToPlaylist":
        if (arg) addTracksToPlaylistArg(arg, tracks, t("playlist.newDefaultName"));
        break;
      case "menu.play":
        playQueue(tracks, 0);
        break;
      case "menu.playNext":
        [...tracks].reverse().forEach(enqueueNext);
        break;
      case "menu.favorite":
        toggleFavoriteAlbum(album!.id);
        break;
      case "menu.editTags":
        editAlbum(album!, tracks.length);
        break;
    }
  };
  const onTrackAction = (track: Track, index: number, key: MessageKey, arg?: string) => {
    switch (key) {
      case "menu.addToPlaylist":
        if (arg) addTracksToPlaylistArg(arg, [track], t("playlist.newDefaultName"));
        break;
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
      case "menu.editTags":
        editTrack(track);
        break;
    }
  };
  const handleScroll = (e: UIEvent<HTMLDivElement>) => {
    onScroll(e);
    const visible = e.currentTarget.scrollTop > STICKY_THRESHOLD;
    if (visible !== barVisible) setBarVisible(visible);
  };

  return (
    <div className="relative min-h-0 flex-1">
      {/* 吸顶栏：保留当前专辑语境与主播放操作。 */}
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
          onClick={closeAlbum}
          className="grid size-[30px] flex-none cursor-pointer place-items-center rounded-full text-tx2 transition-colors hover:bg-hv hover:text-tx"
        >
          <Icon name="chevronLeft" size={15} strokeWidth={2} />
        </button>
        <div
          className="cover-corners cover-gradient cover-thumb-material relative grid size-8 flex-shrink-0 place-items-center rounded-[7px]"
          style={coverGradientStyle(album.cover)}
        >
          <span className="cover-initial font-serif text-[14px]">{album.cover.initial}</span>
          <CoverArt cover={album.cover} px={32} />
        </div>
        <div className="min-w-0">
          <div className="flex items-center gap-1.5">
            <span className="truncate font-serif text-[16.5px] font-semibold text-tx">{album.title}</span>
            {collected && <Icon name="heart" size={10} className="flex-shrink-0 text-ac" />}
          </div>
          <div className="truncate text-[11px] text-tx2">{album.artist}</div>
        </div>
        <div className="flex-1" />
        {/* 与头部同一对动作，只是收成图标：滚下去之后大按钮已不在视野，
            随机播放不该因为翻了页就没得点。 */}
        <div className="flex flex-none items-center gap-2.5">
          <motion.button
            aria-label={playingThis ? t("player.pause") : t("player.play")}
            title={playingThis ? t("player.pause") : t("player.play")}
            onClick={onPlayAlbum}
            className="play-action-material play-action-compact grid size-[34px] cursor-pointer place-items-center rounded-full text-on-ac"
          >
            <PlayPauseIcon playing={playingThis} size={15} />
          </motion.button>
          <button
            aria-label={t("album.shufflePlay")}
            title={t("album.shufflePlay")}
            onClick={onShuffleAlbum}
            className="grid size-[34px] flex-none cursor-pointer place-items-center rounded-full border border-bd bg-srf text-tx transition-colors hover:bg-hv active:scale-95"
          >
            <Icon name="shuffle" size={14} strokeWidth={1.8} />
          </button>
        </div>
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
              onClick={closeAlbum}
              className="flex cursor-pointer items-center gap-1.5 rounded-full py-[5px] pl-2 pr-3 text-[12.5px] text-tx2 transition-colors hover:bg-hv hover:text-tx"
            >
              <Icon name="chevronLeft" size={13} strokeWidth={2} />
              {t("nav.albums")}
            </button>
          </div>

          {/* 专辑头部 */}
          <div className="flex items-center gap-9 pb-[30px] pt-[18px]">
            <motion.div
              layoutId={`album-cover-${album.id}`}
              transition={{ type: "spring", stiffness: 360, damping: 34, mass: 0.8 }}
              className="cover-corners cover-gradient cover-hero-material group/cover relative grid size-[232px] flex-shrink-0 place-items-center rounded-2xl"
              style={coverGradientStyle(album.cover)}
            >
              <span className="cover-initial font-serif text-[76px] font-medium">
                {album.cover.initial}
              </span>
              <CoverArt cover={album.cover} px={232} />
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
                  <AnimatedIcon
                    name={collected ? "heart" : "favorites"}
                    size={14}
                    strokeWidth={2}
                    variant="pop"
                  />
                </motion.button>
              </div>
            </motion.div>

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
                {metaJoin(
                  album.year,
                  album.genre,
                  t("unit.tracks", { n: album.trackCount }),
                  t("unit.minutes", { n: Math.floor(totalSec / 60) }),
                )}
              </div>
              <div className="mt-2.5 flex items-center gap-3">
                <motion.button
                  onClick={onPlayAlbum}
                  className="play-action-material flex cursor-pointer items-center gap-2 rounded-full px-[26px] py-[11px] text-sm font-semibold text-on-ac"
                >
                  <PlayPauseIcon playing={playingThis} size={16} />
                  {playingThis ? t("player.pause") : t("player.play")}
                </motion.button>
                <button
                  onClick={onShuffleAlbum}
                  className="flex cursor-pointer items-center gap-2 rounded-full border border-bd bg-srf px-[22px] py-[11px] text-sm font-semibold text-tx transition-colors hover:bg-hv active:scale-95"
                >
                  <Icon name="shuffle" size={15} strokeWidth={1.8} />
                  {t("album.shufflePlay")}
                </button>
                {/* 「…」与专辑卡右键菜单同内容：专辑属曲库内容，可做的动作就是这些。 */}
                <ItemMoreMenu
                  label={`${album.title} — ${album.artist}`}
                  items={ALBUM_MENU}
                  onAction={onAlbumAction}
                >
                  <button
                    aria-label={t("album.more")}
                    title={t("album.more")}
                    className="grid size-10 cursor-pointer place-items-center rounded-full border border-bd bg-srf text-tx2 transition-colors hover:bg-hv hover:text-tx data-[state=open]:bg-hv data-[state=open]:text-ac"
                  >
                    <Icon name="more" size={16} />
                  </button>
                </ItemMoreMenu>
              </div>
            </div>
          </div>

          {/* 曲目列表：多碟专辑按碟分节，序号用真实音轨号 */}
          <div className="border-t border-bd">
            {discs.map(([discNo, items]) => (
              <div key={discNo}>
                {multiDisc && (
                  <div className="px-3.5 pb-1 pt-5 text-[11.5px] font-semibold tracking-[0.06em] text-tx2">
                    {t("album.disc", { n: discNo })}
                  </div>
                )}
                {items.map(({ track, index }) => {
              const isCur = current?.id === track.id;
              const liked = !!favorites[track.id];
              const i = index;
              return (
                <ItemContextMenu
                  key={track.id}
                  label={`${track.title} — ${track.artist}`}
                  items={TRACK_MENU}
                  onAction={(key, arg) => onTrackAction(track, i, key, arg)}
                  containsTrackId={track.id}
                >
                  <div
                    onClick={() => playQueue(tracks, i)}
                    className={cn(
                      "mt-0.5 grid cursor-pointer items-center gap-3.5 rounded-xl px-3.5 py-[11px] transition-colors hover:bg-hv",
                      showTrackArtist
                        ? "grid-cols-[44px_1fr_minmax(96px,180px)_44px_64px]"
                        : "grid-cols-[44px_1fr_44px_64px]",
                    )}
                  >
                    <span className="text-[13px] tabular-nums text-tx2">
                      {/*
                        音轨号来自标签，**没写就留空位**。曾经退回碟内序位，
                        但那是个会撞号的哨兵值：同一张专辑里只要有几首缺标签，
                        编出来的序位就会与其它曲目的真实音轨号重号（实测一张
                        11 首的专辑同时出现了两个「10」和两个「11」）。
                        缺失值宁可留空，也不填一个看起来像真的的数字。
                      */}
                      <TrackIndicator
                        number={track.trackNo ?? "·"}
                        active={isCur}
                        playing={playing}
                      />
                    </span>
                    <span
                      className={cn(
                        "truncate font-serif text-[15.5px]",
                        isCur ? "font-semibold text-ac" : "font-medium text-tx",
                      )}
                    >
                      {track.title}
                    </span>
                    {showTrackArtist && (
                      // 合辑里「这首谁唱的，我想听他别的」是很自然的下一步。
                      // 默认与其他次要信息同色，hover 才提示可点——每行常驻一个强调色
                      // 会把视线从标题上抢走；stopPropagation 是为了不触发整行的播放。
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          openArtist(track.artist);
                        }}
                        title={track.artist}
                        className="min-w-0 cursor-pointer truncate text-left text-[13px] text-tx2 transition-colors hover:text-ac"
                      >
                        {track.artist}
                      </button>
                    )}
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
