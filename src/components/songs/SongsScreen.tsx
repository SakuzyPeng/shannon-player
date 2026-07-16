import { useMemo, useRef, useState, type ReactNode, type UIEvent } from "react";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { AnimatedIcon } from "@/components/common/AnimatedIcon";
import { FilterPill, useFilterPill } from "@/components/common/FilterPill";
import { Icon } from "@/components/common/Icon";
import { ItemContextMenu } from "@/components/common/ItemContextMenu";
import { TrackIndicator } from "@/components/common/TrackIndicator";
import { useElasticScroll } from "@/hooks/useElasticScroll";
import { ALBUMS, TRACK_MENU, allTracks } from "@/data/library";
import { usePlayerStore } from "@/store/player";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";
import { coverGradientStyle } from "@/lib/coverStyle";
import { fmtTime } from "@/lib/time";
import type { MessageKey } from "@/i18n/messages";
import type { Track } from "@/types/player";

/** 吸顶栏出现阈值（设计稿：scrollTop > 100）。 */
const STICKY_THRESHOLD = 100;
const STICKY_BAR_HEIGHT = 58;

type SortMode = "title" | "artist" | "recent";

const SORT_LABEL: Record<SortMode, MessageKey> = {
  title: "songs.sortByTitle",
  artist: "songs.sortByArtist",
  recent: "songs.sortRecent",
};

/** 过滤命中高亮：把标题按第一个命中拆三段，命中段 --ac。 */
function HighlightTitle({ text, query }: { text: string; query: string }) {
  if (query) {
    const hi = text.toLowerCase().indexOf(query);
    if (hi >= 0) {
      return (
        <>
          <span>{text.slice(0, hi)}</span>
          <span className="text-ac">{text.slice(hi, hi + query.length)}</span>
          <span>{text.slice(hi + query.length)}</span>
        </>
      );
    }
  }
  return <>{text}</>;
}

function SongSortMenu({
  sort,
  onValueChange,
}: {
  sort: SortMode;
  onValueChange: (value: SortMode) => void;
}) {
  const { t } = useT();
  return (
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
          <DropdownMenu.RadioGroup value={sort} onValueChange={(value) => onValueChange(value as SortMode)}>
            {(Object.keys(SORT_LABEL) as SortMode[]).map((mode) => (
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
  );
}

export function SongsScreen() {
  const { t } = useT();
  const reduceMotion = useReducedMotion();
  const { scrollerRef, innerRef, thumbRef, onScroll } = useElasticScroll();
  const [barVisible, setBarVisible] = useState(false);

  const [sort, setSort] = useState<SortMode>("title");
  const { filter, query: q, rawQuery: filterQ } = useFilterPill();
  const headerInputRef = useRef<HTMLInputElement | null>(null);
  const barInputRef = useRef<HTMLInputElement | null>(null);

  const playing = usePlayerStore((s) => s.playing);
  const current = usePlayerStore((s) =>
    s.currentIndex >= 0 ? s.queue[s.currentIndex]?.track : null,
  );
  const favorites = usePlayerStore((s) => s.favorites);
  const playQueue = usePlayerStore((s) => s.playQueue);
  const toggleFavorite = usePlayerStore((s) => s.toggleFavorite);
  const enqueueNext = usePlayerStore((s) => s.enqueueNext);

  const tracks = useMemo(() => allTracks(), []);
  const totalSec = useMemo(() => tracks.reduce((s, tk) => s + tk.durationSec, 0), [tracks]);

  const entries = useMemo(() => {
    let list = tracks;
    if (q) {
      list = list.filter((tk) =>
        `${tk.title} ${tk.artist} ${tk.album}`.toLowerCase().includes(q),
      );
    }
    // 中文按拼音排序：界面用 localeCompare('zh')（后端将用 pinyin crate）。
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
    return list; // recent = 曲库顺序
  }, [tracks, q, sort]);

  const handleScroll = (e: UIEvent<HTMLDivElement>) => {
    onScroll(e);
    const v = e.currentTarget.scrollTop > STICKY_THRESHOLD;
    if (v !== barVisible) setBarVisible(v);
  };

  const onTrackAction = (track: Track, index: number, key: MessageKey) => {
    switch (key) {
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

  const subtitle = t("songs.subtitle", {
    n: tracks.length,
    h: Math.floor(totalSec / 3600),
    m: Math.round((totalSec % 3600) / 60),
  });

  /** 单行曲目（扁平与分组共用主体，网格列不同）。 */
  const renderRow = (track: Track, index: number, no: number, grouped: boolean): ReactNode => {
    const isCur = current?.id === track.id;
    const liked = !!favorites[track.id];
    return (
      <ItemContextMenu
        key={track.id}
        label={`${track.title} — ${track.artist}`}
        items={TRACK_MENU}
        onAction={(key) => onTrackAction(track, index, key)}
      >
        <div
          onClick={() => playQueue(entries, index)}
          className={cn(
            "library-row-divider grid cursor-pointer items-center gap-3 rounded-xl transition-colors hover:bg-hv",
            grouped
              ? "library-row-divider--grouped mt-px grid-cols-[44px_1fr_44px_60px] py-2 pl-[26px] pr-3.5"
              : "mt-0.5 grid-cols-[44px_1fr_170px_190px_44px_60px] px-3.5 py-[9px]",
          )}
        >
          <span className="text-[13px] tabular-nums text-tx2">
            <TrackIndicator number={no} active={isCur} playing={playing} />
          </span>
          <span
            className={cn(
              "truncate font-serif",
              grouped ? "text-[15px]" : "text-[15.5px]",
              isCur ? "font-semibold text-ac" : "font-medium text-tx",
            )}
          >
            <HighlightTitle text={track.title} query={q} />
          </span>
          {!grouped && <span className="truncate text-[13px] text-tx2">{track.artist}</span>}
          {!grouped && <span className="truncate text-[13px] text-tx2">{track.album}</span>}
          <button
            aria-label={liked ? t("player.unfavorite") : t("player.favorite")}
            title={liked ? t("player.unfavorite") : t("player.favorite")}
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
  };

  /** 按歌手分组视图的数据结构。 */
  const groups = useMemo(() => {
    if (sort !== "artist") return [];
    const out: Array<{
      artist: string;
      count: number;
      albums: Array<{ title: string; albumId?: string; rows: Array<[Track, number]> }>;
    }> = [];
    entries.forEach((tk, i) => {
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
  }, [entries, sort]);

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
          <Icon name="songs" size={15} strokeWidth={1.8} />
        </div>
        <span className="font-serif text-[16.5px] font-semibold text-tx">{t("nav.songs")}</span>
        <span className="whitespace-nowrap text-xs text-tx2">{subtitle}</span>
        <div className="ml-auto flex items-center gap-3">
          <SongSortMenu sort={sort} onValueChange={setSort} />
          <FilterPill
            filter={filter}
            height={34}
            openWidth={300}
            inputRef={barInputRef}
            placeholder={t("songs.filterPlaceholder")}
          />
        </div>
      </div>

      <div className="relative min-h-0 flex-1">
        <div
          ref={scrollerRef}
          onScroll={handleScroll}
          className="no-scrollbar absolute inset-0 overflow-auto px-10 pb-[120px] [overscroll-behavior:contain]"
        >
          <div ref={innerRef} className="will-change-transform">
            {/* 标题栏（兼作窗口拖拽区） */}
            <div data-tauri-drag-region className="flex items-end gap-3.5 pb-[18px] pt-[34px]">
              <div className="flex flex-col gap-[7px]">
                <h1 className="m-0 font-serif text-[40px] font-medium text-tx">
                  {t("nav.songs")}
                </h1>
                <div className="text-[13px] text-tx2">{subtitle}</div>
              </div>
              <div className="flex-1" data-tauri-drag-region />

              <SongSortMenu sort={sort} onValueChange={setSort} />

              {/* 过滤圆钮（hover 展开） */}
              <FilterPill
                filter={filter}
                height={40}
                openWidth={318}
                inputRef={headerInputRef}
                placeholder={t("songs.filterPlaceholder")}
                className="mr-1.5"
              />
            </div>

            {/* 空态 */}
            <AnimatePresence initial={false}>
            {entries.length === 0 && (
              <motion.div
                key="songs-empty"
                className="flex flex-col items-center gap-3.5 px-0 pb-10 pt-[100px] text-center"
                initial={{ opacity: 0, y: reduceMotion ? 0 : 8 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: reduceMotion ? 0 : -6 }}
                transition={{ duration: reduceMotion ? 0.01 : 0.18, ease: [0.22, 1, 0.36, 1] }}
              >
                <div className="grid size-16 place-items-center rounded-full border border-bd bg-sb text-tx2">
                  <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round">
                    <circle cx="10.5" cy="10.5" r="7" />
                    <path d="M16 16l5 5 M8 10.5h5" />
                  </svg>
                </div>
                <div className="font-serif text-lg font-semibold text-tx">
                  {t("songs.emptyTitle", { q: filterQ.trim() })}
                </div>
                <div className="max-w-[320px] text-[13px] leading-[1.6] text-tx2">
                  {t("songs.emptyBody")}
                </div>
              </motion.div>
            )}
            </AnimatePresence>

            {/* 按歌手分组 */}
            {entries.length > 0 && sort === "artist" && (
              <div className="border-t border-bd">
                {groups.map((g, gi) => (
                  <div key={g.artist} className={cn(gi > 0 && "mt-[18px] border-t border-bd")}>
                    <div
                      className="artist-group-sticky sticky z-10 flex items-baseline gap-2.5 border-b border-bd bg-bg px-0.5 pb-1.5 pt-[18px]"
                      style={{
                        top: barVisible ? STICKY_BAR_HEIGHT : 0,
                        transition: "top 0.25s var(--ease-spring)",
                      }}
                    >
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
                          {ab.rows.map(([tk, idx], n) => renderRow(tk, idx, n + 1, true))}
                        </div>
                      );
                    })}
                  </div>
                ))}
              </div>
            )}

            {/* 扁平列表 */}
            {entries.length > 0 && sort !== "artist" && (
              <>
                <div className="grid grid-cols-[44px_1fr_170px_190px_44px_60px] items-center gap-3 border-b border-bd px-3.5 pb-2 pt-1.5 text-[11px] font-semibold tracking-[0.08em] text-tx2">
                  <span>#</span>
                  <span>{t("nav.songs")}</span>
                  <span>{t("songs.colArtist")}</span>
                  <span>{t("list.album")}</span>
                  <span />
                  <span className="text-right">{t("list.duration")}</span>
                </div>
                {entries.map((tk, i) => renderRow(tk, i, i + 1, false))}
              </>
            )}
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
