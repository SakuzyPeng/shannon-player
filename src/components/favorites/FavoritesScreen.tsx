import { useMemo, useRef, useState, type ReactNode, type UIEvent } from "react";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { motion } from "framer-motion";
import { Collage } from "@/components/common/Collage";
import { EqBars } from "@/components/common/EqBars";
import { FilterPill, useFilterPill } from "@/components/common/FilterPill";
import { Icon } from "@/components/common/Icon";
import { ItemContextMenu } from "@/components/common/ItemContextMenu";
import { SegmentedContent, SegmentedControl } from "@/components/common/SegmentedControl";
import { useElasticScroll } from "@/hooks/useElasticScroll";
import { ALBUMS, TRACK_MENU, albumsOfArtist, allTracks, tracksOf } from "@/data/library";
import { PLAYLISTS, collageOf } from "@/data/playlists";
import { usePlayerStore } from "@/store/player";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";
import { coverGradientStyle } from "@/lib/coverStyle";
import { fmtTime } from "@/lib/time";
import type { MessageKey } from "@/i18n/messages";
import type { Album, Playlist, Track } from "@/types/player";

const STICKY_THRESHOLD = 100;
const SONG_COLS = "grid-cols-[44px_1fr_170px_190px_44px_60px]";

type FavTab = "songs" | "albums" | "artists" | "playlists";
type SortMode = "recent" | "title" | "artist";

const TABS: { key: FavTab; labelKey: MessageKey }[] = [
  { key: "songs", labelKey: "nav.songs" },
  { key: "albums", labelKey: "nav.albums" },
  { key: "artists", labelKey: "nav.artists" },
  { key: "playlists", labelKey: "favorites.playlists" },
];

const SORT_LABEL: Record<SortMode, MessageKey> = {
  recent: "favorites.sortRecent",
  title: "songs.sortByTitle",
  artist: "songs.sortByArtist",
};

const SORT_MODES: Record<FavTab, readonly SortMode[]> = {
  songs: ["recent", "title", "artist"],
  albums: ["recent", "title", "artist"],
  artists: ["recent", "title"],
  playlists: ["recent", "title"],
};

/** 过滤命中高亮：命中段着 --ac。 */
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

export function FavoritesScreen() {
  const { t } = useT();
  const { scrollerRef, innerRef, thumbRef, onScroll } = useElasticScroll();
  const [barVisible, setBarVisible] = useState(false);
  const [tab, setTab] = useState<FavTab>("songs");
  const [sort, setSort] = useState<SortMode>("recent");
  const { filter, query } = useFilterPill();
  const headerInputRef = useRef<HTMLInputElement | null>(null);
  const barInputRef = useRef<HTMLInputElement | null>(null);

  const playing = usePlayerStore((s) => s.playing);
  const current = usePlayerStore((s) =>
    s.currentIndex >= 0 ? s.queue[s.currentIndex]?.track : null,
  );
  const favorites = usePlayerStore((s) => s.favorites);
  const favoriteAlbums = usePlayerStore((s) => s.favoriteAlbums);
  const favoriteArtists = usePlayerStore((s) => s.favoriteArtists);
  const favoritePlaylists = usePlayerStore((s) => s.favoritePlaylists);
  const playQueue = usePlayerStore((s) => s.playQueue);
  const toggleFavorite = usePlayerStore((s) => s.toggleFavorite);
  const toggleFavoriteAlbum = usePlayerStore((s) => s.toggleFavoriteAlbum);
  const toggleFavoriteArtist = usePlayerStore((s) => s.toggleFavoriteArtist);
  const toggleFavoritePlaylist = usePlayerStore((s) => s.toggleFavoritePlaylist);
  const enqueueNext = usePlayerStore((s) => s.enqueueNext);
  const openAlbum = useUiStore((s) => s.openAlbum);
  const openArtist = useUiStore((s) => s.openArtist);
  const openPlaylist = useUiStore((s) => s.openPlaylist);

  // 收藏基集（未过滤，用于计数与空态判定）。
  const baseSongs = useMemo(
    () => allTracks().filter((tk) => favorites[tk.id]),
    [favorites],
  );
  const baseAlbums = useMemo(
    () => ALBUMS.filter((a) => favoriteAlbums[a.id]),
    [favoriteAlbums],
  );
  const baseArtists = useMemo(() => {
    const seen = new Set<string>();
    const out: string[] = [];
    for (const a of ALBUMS) {
      if (favoriteArtists[a.artist] && !seen.has(a.artist)) {
        seen.add(a.artist);
        out.push(a.artist);
      }
    }
    return out;
  }, [favoriteArtists]);
  const basePlaylists = useMemo(
    () => PLAYLISTS.filter((p) => favoritePlaylists[p.id]),
    [favoritePlaylists],
  );

  const baseCount: Record<FavTab, number> = {
    songs: baseSongs.length,
    albums: baseAlbums.length,
    artists: baseArtists.length,
    playlists: basePlaylists.length,
  };

  // 当前分段过滤 + 排序。
  const songEntries = useMemo(() => {
    let list = query
      ? baseSongs.filter((tk) =>
          `${tk.title} ${tk.artist} ${tk.album}`.toLowerCase().includes(query),
        )
      : baseSongs;
    if (sort === "title") {
      list = [...list].sort((a, b) => a.title.localeCompare(b.title, "zh"));
    } else if (sort === "artist") {
      list = [...list].sort(
        (a, b) =>
          a.artist.localeCompare(b.artist, "zh") ||
          a.album.localeCompare(b.album, "zh") ||
          a.title.localeCompare(b.title, "zh"),
      );
    }
    return list;
  }, [baseSongs, query, sort]);

  const albumEntries = useMemo(() => {
    let list = query
      ? baseAlbums.filter((a) => `${a.title} ${a.artist}`.toLowerCase().includes(query))
      : baseAlbums;
    if (sort === "title") {
      list = [...list].sort((a, b) => a.title.localeCompare(b.title, "zh"));
    } else if (sort === "artist") {
      list = [...list].sort(
        (a, b) => a.artist.localeCompare(b.artist, "zh") || a.title.localeCompare(b.title, "zh"),
      );
    }
    return list;
  }, [baseAlbums, query, sort]);
  const artistEntries = useMemo(() => {
    let list = query ? baseArtists.filter((n) => n.toLowerCase().includes(query)) : baseArtists;
    if (sort === "title") list = [...list].sort((a, b) => a.localeCompare(b, "zh"));
    return list;
  }, [baseArtists, query, sort]);
  const playlistEntries = useMemo(() => {
    let list = query
      ? basePlaylists.filter((p) => `${p.title} ${p.description}`.toLowerCase().includes(query))
      : basePlaylists;
    if (sort === "title") list = [...list].sort((a, b) => a.title.localeCompare(b.title, "zh"));
    return list;
  }, [basePlaylists, query, sort]);

  const grouped = tab === "songs" && sort === "artist";
  const songGroups = useMemo(() => {
    if (!grouped) return [];
    const out: Array<{
      artist: string;
      count: number;
      albums: Array<{ title: string; albumId?: string; rows: Array<[Track, number]> }>;
    }> = [];
    songEntries.forEach((tk, i) => {
      let g = out[out.length - 1];
      if (!g || g.artist !== tk.artist) {
        g = { artist: tk.artist, count: 0, albums: [] };
        out.push(g);
      }
      let ab = g.albums[g.albums.length - 1];
      if (!ab || ab.title !== tk.album) {
        ab = { title: tk.album, albumId: tk.albumId, rows: [] };
        g.albums.push(ab);
      }
      ab.rows.push([tk, i]);
      g.count += 1;
    });
    return out;
  }, [songEntries, grouped]);

  const shownCount =
    tab === "songs"
      ? songEntries.length
      : tab === "albums"
        ? albumEntries.length
        : tab === "artists"
          ? artistEntries.length
          : playlistEntries.length;
  const empty = shownCount === 0;
  const emptyTitleKey: Record<FavTab, MessageKey> = {
    songs: "favorites.emptySongs",
    albums: "favorites.emptyAlbums",
    artists: "favorites.emptyArtists",
    playlists: "favorites.emptyPlaylists",
  };
  const filtering = !!query && baseCount[tab] > 0;

  const handleScroll = (e: UIEvent<HTMLDivElement>) => {
    onScroll(e);
    const v = e.currentTarget.scrollTop > STICKY_THRESHOLD;
    if (v !== barVisible) setBarVisible(v);
  };

  const onTrackAction = (track: Track, index: number, key: MessageKey) => {
    switch (key) {
      case "menu.play":
        playQueue(songEntries, index);
        break;
      case "menu.playNext":
        enqueueNext(track);
        break;
      case "menu.favorite":
        toggleFavorite(track.id);
        break;
      case "menu.showLyrics":
        playQueue(songEntries, index);
        useUiStore.getState().openLyrics();
        break;
    }
  };

  const subtitle = t("favorites.subtitle", {
    songs: baseSongs.length,
    albums: baseAlbums.length,
    artists: baseArtists.length,
    playlists: basePlaylists.length,
  });

  const filterPlaceholder = t("favorites.filterPlaceholder");
  const selectTab = (next: FavTab) => {
    setTab(next);
    if (!SORT_MODES[next].includes(sort)) setSort("recent");
  };

  /** 曲目行（扁平与分组共用；列宽不同）。 */
  const renderSongRow = (track: Track, index: number, no: number, group: boolean): ReactNode => {
    const isCur = current?.id === track.id;
    return (
      <ItemContextMenu
        key={track.id}
        label={`${track.title} — ${track.artist}`}
        items={TRACK_MENU}
        onAction={(key) => onTrackAction(track, index, key)}
      >
        <div
          onClick={() => playQueue(songEntries, index)}
          className={cn(
            "library-row-divider grid cursor-pointer items-center gap-3 rounded-xl transition-colors hover:bg-hv",
            group
              ? "library-row-divider--grouped mt-px grid-cols-[44px_1fr_44px_60px] py-2 pl-[26px] pr-3.5"
              : `mt-0.5 ${SONG_COLS} px-3.5 py-[9px]`,
          )}
        >
          {isCur ? (
            <EqBars playing={playing} />
          ) : (
            <span className="pl-[3px] text-[13px] tabular-nums text-tx2">{no}</span>
          )}
          <span
            className={cn(
              "truncate font-serif",
              group ? "text-[15px]" : "text-[15.5px]",
              isCur ? "font-semibold text-ac" : "font-medium text-tx",
            )}
          >
            <Highlight text={track.title} query={query} />
          </span>
          {!group && <span className="truncate text-[13px] text-tx2">{track.artist}</span>}
          {!group && <span className="truncate text-[13px] text-tx2">{track.album}</span>}
          <button
            aria-label={t("player.unfavorite")}
            title={t("player.unfavorite")}
            onClick={(e) => {
              e.stopPropagation();
              toggleFavorite(track.id);
            }}
            className="grid size-[30px] cursor-pointer place-items-center rounded-full text-ac transition-colors hover:bg-ac/12"
          >
            <Icon name="heart" size={15} />
          </button>
          <span className="text-right text-[13px] tabular-nums text-tx2">
            {fmtTime(track.durationSec)}
          </span>
        </div>
      </ItemContextMenu>
    );
  };

  /** 分段切换器（头部与吸顶栏共用，尺寸不同）。 */
  const renderTabs = (compact: boolean) => (
    <SegmentedControl
      value={tab}
      onValueChange={selectTab}
      options={TABS.map((tb) => ({ value: tb.key, label: t(tb.labelKey) }))}
      className={cn("text-[12.5px]", compact ? "p-[2.5px] text-xs" : "p-[3px]")}
      buttonClassName={compact ? "px-[13px] py-1" : "px-4 py-1.5"}
    />
  );

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
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
        <div className="grid size-8 place-items-center rounded-full border border-bd bg-sb text-ac">
          <Icon name="heart" size={15} />
        </div>
        <span className="font-serif text-[16.5px] font-semibold text-tx">
          {t("nav.favorites")}
        </span>
        <span className="hidden whitespace-nowrap text-xs text-tx2 lg:inline">{subtitle}</span>
        <div className="ml-1.5">{renderTabs(true)}</div>
        <FilterPill
          filter={filter}
          height={34}
          openWidth={300}
          inputRef={barInputRef}
          placeholder={filterPlaceholder}
          className="ml-auto"
        />
      </div>

      <div className="relative min-h-0 flex-1">
        <div
          ref={scrollerRef}
          onScroll={handleScroll}
          className="no-scrollbar absolute inset-0 overflow-auto px-10 pb-[120px] [overscroll-behavior:contain]"
        >
          <div ref={innerRef} className="will-change-transform">
            {/* 标题栏（兼作窗口拖拽区） */}
            <div data-tauri-drag-region className="flex items-end gap-4 pb-[18px] pt-[34px]">
              <div className="flex flex-col gap-[7px]">
                <h1 className="m-0 font-serif text-[40px] font-medium text-tx">
                  {t("nav.favorites")}
                </h1>
                <div className="text-[13px] text-tx2">{subtitle}</div>
              </div>
              <div className="flex-1" data-tauri-drag-region />

              {renderTabs(false)}

              {/* 排序菜单：各分段仅暴露适用的排序项。 */}
              <DropdownMenu.Root>
                <DropdownMenu.Trigger asChild>
                  <button className="flex cursor-pointer items-center gap-1.5 rounded-full border border-bd bg-srf px-[15px] py-[9px] text-[13px] text-tx transition-colors hover:bg-hv">
                    {t(SORT_LABEL[sort])}
                    <Icon name="chevronDown" size={12} strokeWidth={2} />
                  </button>
                </DropdownMenu.Trigger>
                <DropdownMenu.Portal>
                  <DropdownMenu.Content
                    align="end"
                    sideOffset={6}
                    aria-label={t("songs.sortMenu")}
                    className="surface-corners animate-menu-pop menu-shadow z-50 w-[170px] origin-top-right rounded-[14px] border border-bd bg-srf p-1.5"
                  >
                    <DropdownMenu.RadioGroup value={sort} onValueChange={(v) => setSort(v as SortMode)}>
                      {SORT_MODES[tab].map((mode) => (
                        <DropdownMenu.RadioItem
                          key={mode}
                          value={mode}
                          className="flex cursor-pointer items-center justify-between gap-3 rounded-lg px-2.5 py-2 text-[13px] text-tx outline-none data-[highlighted]:bg-hv"
                        >
                          <span>{t(SORT_LABEL[mode])}</span>
                          {sort === mode && (
                            <Icon name="check" size={14} className="text-ac" strokeWidth={2.4} />
                          )}
                        </DropdownMenu.RadioItem>
                      ))}
                    </DropdownMenu.RadioGroup>
                  </DropdownMenu.Content>
                </DropdownMenu.Portal>
              </DropdownMenu.Root>

              {/* 过滤圆钮 */}
              <FilterPill
                filter={filter}
                height={40}
                openWidth={318}
                inputRef={headerInputRef}
                placeholder={filterPlaceholder}
                className="mr-1.5"
              />
            </div>

            <SegmentedContent value={tab}>
              {/* 空态 */}
              {empty && (
                <div className="flex flex-col items-center gap-3.5 pb-10 pt-[100px] text-center">
                  <div className="grid size-16 place-items-center rounded-full border border-bd bg-sb text-tx2">
                    <Icon name="favorites" size={26} strokeWidth={1.7} />
                  </div>
                  <div className="font-serif text-lg font-semibold text-tx">
                    {filtering
                      ? t("favorites.emptyFilter", { q: filter.q.trim() })
                      : t(emptyTitleKey[tab])}
                  </div>
                  {!filtering && (
                    <div className="max-w-[320px] text-[13px] leading-[1.6] text-tx2">
                      {t("favorites.emptyHint")}
                    </div>
                  )}
                </div>
              )}

              {/* 歌曲 · 分组 */}
              {tab === "songs" && !empty && grouped && (
                <div className="border-t border-bd">
                  {songGroups.map((g, gi) => (
                    <div key={g.artist} className={cn(gi > 0 && "mt-[18px] border-t border-bd")}>
                      <div className="flex items-baseline gap-2.5 px-0.5 pb-1 pt-[22px]">
                        <span className="font-serif text-[22px] font-semibold text-tx">
                          {g.artist}
                        </span>
                        <span className="text-xs text-tx2">
                          {t("songs.groupMeta", { albums: g.albums.length, n: g.count })}
                        </span>
                      </div>
                      {g.albums.map((ab) => {
                        const album = ALBUMS.find((a) => a.id === ab.albumId);
                        return (
                          <div key={`${g.artist}-${ab.title}`}>
                            <div className="flex items-center gap-[9px] px-0.5 pb-[7px] pt-3.5">
                              {album && (
                                <div
                                  className="cover-corners cover-gradient cover-thumb-material grid size-[26px] place-items-center rounded-md"
                                  style={coverGradientStyle(album.cover)}
                                >
                                  <span className="cover-initial font-serif text-[11px]">
                                    {album.cover.initial}
                                  </span>
                                </div>
                              )}
                              <span className="whitespace-nowrap font-serif text-sm font-semibold text-tx2">
                                {ab.title}
                              </span>
                              <div className="ml-1.5 h-px flex-1 bg-bd" />
                            </div>
                            {ab.rows.map(([tk, idx], n) => renderSongRow(tk, idx, n + 1, true))}
                          </div>
                        );
                      })}
                    </div>
                  ))}
                </div>
              )}

              {/* 歌曲 · 扁平 */}
              {tab === "songs" && !empty && !grouped && (
                <>
                  <div
                    className={`grid ${SONG_COLS} items-center gap-3 border-b border-bd px-3.5 pb-2 pt-1.5 text-[11px] font-semibold tracking-[0.08em] text-tx2`}
                  >
                    <span>#</span>
                    <span>{t("nav.songs")}</span>
                    <span>{t("songs.colArtist")}</span>
                    <span>{t("list.album")}</span>
                    <span />
                    <span className="text-right">{t("list.duration")}</span>
                  </div>
                  {songEntries.map((tk, i) => renderSongRow(tk, i, i + 1, false))}
                </>
              )}

              {/* 专辑 */}
              {tab === "albums" && !empty && (
                <div className="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-x-7 gap-y-8 pt-2">
                  {albumEntries.map((album) => (
                    <FavAlbumCard
                      key={album.id}
                      album={album}
                      query={query}
                      onOpen={() => openAlbum(album.id)}
                      onPlay={() => playQueue(tracksOf(album))}
                      onUnfav={() => toggleFavoriteAlbum(album.id)}
                    />
                  ))}
                </div>
              )}

              {/* 歌手 */}
              {tab === "artists" && !empty && (
                <div className="flex flex-wrap gap-x-[34px] gap-y-7 pt-3.5">
                  {artistEntries.map((name) => (
                    <FavArtistCard
                      key={name}
                      name={name}
                      query={query}
                      meta={t("favorites.artistMeta", { n: albumsOfArtist(name).length })}
                      onOpen={() => openArtist(name)}
                      onUnfav={() => toggleFavoriteArtist(name)}
                    />
                  ))}
                </div>
              )}

              {/* 歌单 */}
              {tab === "playlists" && !empty && (
                <div className="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-x-6 gap-y-8 pt-2">
                  {playlistEntries.map((pl) => (
                    <FavPlaylistCard
                      key={pl.id}
                      playlist={pl}
                      query={query}
                      onOpen={() => openPlaylist(pl.id)}
                      onUnfav={() => toggleFavoritePlaylist(pl.id)}
                    />
                  ))}
                </div>
              )}
            </SegmentedContent>
          </div>
        </div>

        <div
          ref={thumbRef}
          className="scroll-thumb pointer-events-none absolute right-[5px] top-2 z-20 h-[120px] w-1.5 rounded-[3px] opacity-0"
        />
      </div>
    </div>
  );
}

/** 收藏专辑卡（复用曲库卡语言，hover 浮现取消收藏 + 播放）。 */
function FavAlbumCard({
  album,
  query,
  onOpen,
  onPlay,
  onUnfav,
}: {
  album: Album;
  query: string;
  onOpen: () => void;
  onPlay: () => void;
  onUnfav: () => void;
}) {
  const { t } = useT();
  return (
    <div className="relative min-w-0 cursor-pointer hover:z-10" onClick={onOpen}>
      <motion.div
        whileHover={{ y: -5 }}
        transition={{ type: "spring", stiffness: 380, damping: 18 }}
        className="cover-corners cover-gradient cover-material group/cover relative grid aspect-square place-items-center rounded-2xl"
        style={coverGradientStyle(album.cover)}
      >
        <span className="cover-initial font-serif text-[56px] font-medium">
          {album.cover.initial}
        </span>
        <div className="cover-corners cover-overlay absolute inset-0 flex items-end justify-end rounded-2xl p-3 opacity-0 transition-opacity duration-[220ms] group-hover/cover:opacity-100">
          <motion.button
            aria-label={t("album.uncollect")}
            title={t("album.uncollect")}
            whileHover={{ scale: 1.12 }}
            whileTap={{ scale: 0.9 }}
            onClick={(e) => {
              e.stopPropagation();
              onUnfav();
            }}
            className="absolute right-2.5 top-2 grid size-9 place-items-center text-[#EE9560] drop-shadow-[0_1px_4px_rgba(30,18,8,0.55)]"
          >
            <Icon name="heart" size={20} />
          </motion.button>
          <motion.button
            aria-label={t("action.playAlbum", { title: album.title })}
            whileHover={{ scale: 1.08 }}
            whileTap={{ scale: 0.9 }}
            onClick={(e) => {
              e.stopPropagation();
              onPlay();
            }}
            className="cover-action-shadow grid size-[38px] place-items-center rounded-full bg-ac text-on-ac"
          >
            <Icon name="play" size={15} />
          </motion.button>
        </div>
      </motion.div>
      <div className="mt-3 flex items-center gap-1.5">
        <span className="truncate font-serif text-[15.5px] font-semibold text-tx">
          <Highlight text={album.title} query={query} />
        </span>
        <div className="flex-1" />
        <span title={t("album.collected")} className="flex-shrink-0 text-ac">
          <Icon name="heart" size={13} />
        </span>
      </div>
      <div className="mt-[3px] truncate text-[12.5px] text-tx2">
        {album.artist} · {album.year}
      </div>
    </div>
  );
}

/** 收藏歌手卡（圆形头像，hover 浮现取消收藏，名旁收藏角标）。 */
function FavArtistCard({
  name,
  query,
  meta,
  onOpen,
  onUnfav,
}: {
  name: string;
  query: string;
  meta: string;
  onOpen: () => void;
  onUnfav: () => void;
}) {
  const { t } = useT();
  const cover = albumsOfArtist(name)[0]?.cover;
  return (
    <div
      onClick={onOpen}
      className="flex w-[150px] cursor-pointer flex-col items-center gap-[11px]"
    >
      <motion.div
        whileHover={{ y: -4 }}
        transition={{ type: "spring", stiffness: 380, damping: 20 }}
        className="cover-gradient artist-hero-material group/avatar relative grid size-[132px] place-items-center rounded-full"
        style={cover ? coverGradientStyle(cover) : undefined}
      >
        {cover && (
          <span className="cover-initial font-serif text-[44px] font-medium">{cover.initial}</span>
        )}
        <div className="artist-hero-overlay absolute inset-0 rounded-full opacity-0 transition-opacity duration-[220ms] group-hover/avatar:opacity-100">
          <motion.button
            aria-label={t("artist.unfollow")}
            title={t("artist.unfollow")}
            whileHover={{ scale: 1.12 }}
            whileTap={{ scale: 0.9 }}
            onClick={(e) => {
              e.stopPropagation();
              onUnfav();
            }}
            className="absolute right-3.5 top-1.5 grid size-[34px] place-items-center text-[#EE9560] drop-shadow-[0_1px_4px_rgba(30,18,8,0.55)]"
          >
            <Icon name="heart" size={19} />
          </motion.button>
        </div>
      </motion.div>
      <div className="relative">
        <span className="text-center font-serif text-[14.5px] font-semibold text-tx">
          <Highlight text={name} query={query} />
        </span>
        <span className="absolute -right-3.5 top-px text-ac">
          <Icon name="heart" size={10} />
        </span>
      </div>
      <div className="-mt-1.5 text-xs text-tx2">{meta}</div>
    </div>
  );
}

/** 收藏歌单卡（2×2 拼贴，hover 浮现取消收藏）。 */
function FavPlaylistCard({
  playlist,
  query,
  onOpen,
  onUnfav,
}: {
  playlist: Playlist;
  query: string;
  onOpen: () => void;
  onUnfav: () => void;
}) {
  const { t } = useT();
  return (
    <div
      onClick={onOpen}
      className="group relative flex cursor-pointer flex-col items-start gap-3 rounded-2xl text-left hover:z-10"
    >
      <div className="relative">
        <Collage
          covers={collageOf(playlist)}
          size={180}
          radius={14}
          glyph={30}
          className="transition-shadow duration-300 group-hover:shadow-[0_16px_32px_var(--cover-hover-shadow)]"
        />
        <div className="absolute inset-0 rounded-[14px] opacity-0 transition-opacity duration-[220ms] group-hover:opacity-100">
          <motion.button
            aria-label={t("album.uncollect")}
            title={t("album.uncollect")}
            whileHover={{ scale: 1.12 }}
            whileTap={{ scale: 0.9 }}
            onClick={(e) => {
              e.stopPropagation();
              onUnfav();
            }}
            className="absolute right-2.5 top-2 grid size-9 place-items-center text-[#EE9560] drop-shadow-[0_1px_4px_rgba(30,18,8,0.55)]"
          >
            <Icon name="heart" size={20} />
          </motion.button>
        </div>
      </div>
      <div className="w-full px-0.5">
        <div className="truncate font-serif text-[15.5px] font-semibold text-tx">
          <Highlight text={playlist.title} query={query} />
        </div>
        <div className="mt-0.5 truncate text-[12.5px] text-tx2">
          {t("playlist.meta", {
            n: playlist.tracks.length,
            m: Math.round(playlist.tracks.reduce((s, tk) => s + tk.durationSec, 0) / 60),
            updated: playlist.updatedLabel,
          })}
        </div>
      </div>
    </div>
  );
}
