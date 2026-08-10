import { useMemo, useRef, useState, type UIEvent } from "react";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { AnimatePresence, motion } from "framer-motion";
import { Collage } from "@/components/common/Collage";
import { FilterPill, useFilterPill } from "@/components/common/FilterPill";
import { Icon } from "@/components/common/Icon";
import { MetaLine } from "@/components/common/MetaLine";
import { useElasticScroll } from "@/hooks/useElasticScroll";
import { collageOf } from "@/data/playlists";
import { updatedLabelOf } from "@/lib/playlists";
import { usePlayerStore } from "@/store/player";
import { useUiStore, type PlaylistSort } from "@/store/ui";
import { useT } from "@/i18n";
import type { MessageKey } from "@/i18n/messages";
import type { Id, Playlist } from "@/types/player";

/** 与歌手页对齐：主标题基本滚出后显示吸顶栏。 */
const STICKY_THRESHOLD = 80;

const SORT_LABEL: Record<PlaylistSort, MessageKey> = {
  recent: "playlists.sortRecent",
  title: "playlists.sortByTitle",
  size: "playlists.sortBySize",
  custom: "playlists.sortCustom",
};

/** 过滤命中高亮（与歌单详情页同一实现口径）。 */
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

/**
 * 歌单索引卡：2×2 拼贴封面 + 标题 + 元信息，收藏者带角标。
 * 与收藏页的歌单卡同一视觉语言，区别是这里列出全部歌单（含未收藏）。
 */
function PlaylistCard({
  playlist,
  query,
  draggable,
  dragging,
  onDragStart,
  onDrag,
  onDragEnd,
  cardRef,
}: {
  playlist: Playlist;
  query: string;
  draggable: boolean;
  dragging: boolean;
  onDragStart: () => void;
  onDrag: (clientX: number, clientY: number) => void;
  onDragEnd: () => void;
  cardRef: (el: HTMLDivElement | null) => void;
}) {
  const { t, locale } = useT();
  const openPlaylist = useUiStore((s) => s.openPlaylist);
  const collected = usePlayerStore((s) => !!s.favoritePlaylists[playlist.id]);
  const toggleFavoritePlaylist = usePlayerStore((s) => s.toggleFavoritePlaylist);
  const totalMin = Math.round(
    playlist.tracks.reduce((sum, tk) => sum + tk.durationSec, 0) / 60,
  );
  return (
    <motion.div
      ref={cardRef}
      layout
      exit={{ opacity: 0, scale: 0.96 }}
      transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
      drag={draggable}
      dragSnapToOrigin
      dragElastic={0.12}
      dragMomentum={false}
      whileDrag={{ scale: 1.04, zIndex: 30 }}
      onDragStart={onDragStart}
      onDrag={(_, info) => onDrag(info.point.x, info.point.y)}
      onDragEnd={onDragEnd}
      // 拖拽结束的那一下 click 不应误入歌单详情。
      onClick={() => !dragging && openPlaylist(playlist.id)}
      style={{ touchAction: draggable ? "none" : undefined }}
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
            aria-label={collected ? t("album.uncollect") : t("album.collect")}
            title={collected ? t("album.uncollect") : t("album.collect")}
            whileHover={{ scale: 1.12 }}
            whileTap={{ scale: 0.9 }}
            onClick={(e) => {
              e.stopPropagation();
              toggleFavoritePlaylist(playlist.id);
            }}
            className="absolute right-2.5 top-2 grid size-9 place-items-center text-[#EE9560] drop-shadow-[0_1px_4px_rgba(30,18,8,0.55)]"
          >
            <Icon name={collected ? "heart" : "favorites"} size={20} />
          </motion.button>
        </div>
      </div>
      <div className="w-full px-0.5">
        <div className="flex items-center gap-1.5">
          <span className="min-w-0 truncate font-serif text-[15.5px] font-semibold text-tx">
            <Highlight text={playlist.title} query={query} />
          </span>
          {collected && (
            <span className="flex-none text-ac" title={t("album.collected")}>
              <Icon name="heart" size={12} />
            </span>
          )}
        </div>
        <MetaLine
          text={t("playlist.meta", {
            n: playlist.tracks.length,
            m: totalMin,
            updated: updatedLabelOf(playlist.updatedAtMs, t, locale),
          })}
          className="mt-0.5 block truncate text-[12.5px] text-tx2"
        />
      </div>
    </motion.div>
  );
}

function PlaylistSortMenu({
  sort,
  onValueChange,
}: {
  sort: PlaylistSort;
  onValueChange: (value: PlaylistSort) => void;
}) {
  const { t } = useT();
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <button className="flex flex-none cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-full border border-bd bg-srf px-[15px] py-[9px] text-[13px] text-tx transition-colors hover:bg-hv">
          {t(SORT_LABEL[sort])}
          <Icon name="chevronDown" size={12} strokeWidth={2} />
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          align="end"
          sideOffset={6}
          aria-label={t("artists.sortMenu")}
          className="surface-corners animate-menu-pop menu-shadow z-50 w-[170px] origin-top-right rounded-[14px] border border-bd bg-srf p-1.5"
        >
          <DropdownMenu.RadioGroup
            value={sort}
            onValueChange={(value) => onValueChange(value as PlaylistSort)}
          >
            {(Object.keys(SORT_LABEL) as PlaylistSort[]).map((mode) => (
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

/**
 * 歌单索引页：列出全部歌单（含未收藏与新建的）。
 *
 * 歌单是用户创建的内容，其可见性不该由「收藏」决定——收藏页的歌单分段
 * 只是「已收藏的歌单」这一子集，取消收藏后仍能在此找到。
 */
export function PlaylistsScreen() {
  const { t } = useT();
  const { scrollerRef, innerRef, thumbRef, onScroll } = useElasticScroll();
  const { filter, query } = useFilterPill();
  const inputRef = useRef<HTMLInputElement | null>(null);
  const barInputRef = useRef<HTMLInputElement | null>(null);
  // 排序模式存 store：自定义顺序必须跨页面导航保留（见 store/ui.ts）。
  const sort = useUiStore((s) => s.playlistSort);
  const setSort = useUiStore((s) => s.setPlaylistSort);
  const [barVisible, setBarVisible] = useState(false);

  const playlists = usePlayerStore((s) => s.playlists);
  const reorderPlaylists = usePlayerStore((s) => s.reorderPlaylists);
  const sorted = useMemo(() => {
    const list = [...playlists];
    if (sort === "title") return list.sort((a, b) => a.title.localeCompare(b.title, "zh"));
    if (sort === "size") {
      return list.sort(
        (a, b) => b.tracks.length - a.tracks.length || a.title.localeCompare(b.title, "zh"),
      );
    }
    // 「自定义顺序」直接用 store 里的数组顺序，不再排序。
    if (sort === "custom") return list;
    // 「最近更新」：时间戳倒序，同一毫秒（导入的旧数据可能整批相同）再按标题定序，
    // 免得每次渲染的次序都不一样。
    return list.sort(
      (a, b) => b.updatedAtMs - a.updatedAtMs || a.title.localeCompare(b.title, "zh"),
    );
  }, [playlists, sort]);
  const entries = useMemo(
    () => (query ? sorted.filter((p) => p.title.toLowerCase().includes(query)) : sorted),
    [query, sorted],
  );

  /* ---- 拖拽排序（歌单是用户内容，顺序应可自定义；专辑等曲库内容不提供） ----
     网格会换行，framer-motion 的 Reorder 只处理单轴，这里按指针落点判定目标格位。 */
  const [draggingId, setDraggingId] = useState<Id | null>(null);
  const cardEls = useRef(new Map<Id, HTMLDivElement>());
  /** 过滤中列表是子集，重排语义不明确 —— 此时禁用拖拽。 */
  const draggable = !query && entries.length > 1;

  const handleDragStart = (id: Id) => {
    setDraggingId(id);
    // 从任何排序模式起拖：先把当前可见顺序固化为自定义顺序，拖动才有意义。
    if (sort !== "custom") {
      reorderPlaylists(sorted);
      setSort("custom");
    }
  };

  const handleDrag = (id: Id, clientX: number, clientY: number) => {
    const list = usePlayerStore.getState().playlists;
    const from = list.findIndex((p) => p.id === id);
    if (from < 0) return;
    let to = -1;
    for (const [otherId, el] of cardEls.current) {
      if (otherId === id) continue;
      const r = el.getBoundingClientRect();
      if (clientX >= r.left && clientX <= r.right && clientY >= r.top && clientY <= r.bottom) {
        to = list.findIndex((p) => p.id === otherId);
        break;
      }
    }
    if (to < 0 || to === from) return;
    const next = [...list];
    const [moved] = next.splice(from, 1);
    next.splice(to, 0, moved);
    reorderPlaylists(next);
  };

  const handleScroll = (e: UIEvent<HTMLDivElement>) => {
    onScroll(e);
    const visible = e.currentTarget.scrollTop > STICKY_THRESHOLD;
    if (visible !== barVisible) setBarVisible(visible);
  };

  const subtitle = t("playlists.subtitle", { n: playlists.length });

  return (
    <div className="relative min-h-0 flex-1">
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
          <Icon name="playlists" size={15} strokeWidth={1.8} />
        </div>
        <span className="font-serif text-[16.5px] font-semibold text-tx">{t("nav.playlists")}</span>
        <span className="hidden whitespace-nowrap text-xs text-tx2 lg:inline">{subtitle}</span>
        <div className="flex-1" />
        <PlaylistSortMenu sort={sort} onValueChange={setSort} />
        <FilterPill
          filter={filter}
          height={34}
          openWidth={300}
          inputRef={barInputRef}
          placeholder={t("playlists.filterPlaceholder")}
          className="ml-auto"
        />
      </div>

      <div
        ref={scrollerRef}
        onScroll={handleScroll}
        className="no-scrollbar absolute inset-0 overflow-auto px-10 pb-[120px] [overscroll-behavior:contain]"
      >
        <div ref={innerRef} className="will-change-transform">
          {/* 标题栏（兼作窗口拖拽区） */}
          <div data-tauri-drag-region className="flex items-end gap-4 pb-5 pt-[34px]">
            {/* 标题列不参与压缩：页面身份优先，空间压力由过滤钮吸收。 */}
            <div data-tauri-drag-region className="flex flex-none flex-col">
              <h1 className="m-0 font-serif text-[40px] font-medium text-tx">
                {t("nav.playlists")}
              </h1>
              <MetaLine text={subtitle} className="mt-[7px] text-[13px] text-tx2" />
            </div>
            <div className="flex-1" data-tauri-drag-region />
            <PlaylistSortMenu sort={sort} onValueChange={setSort} />
            <FilterPill
              filter={filter}
              height={40}
              openWidth={318}
              inputRef={inputRef}
              placeholder={t("playlists.filterPlaceholder")}
              className="mr-1.5"
            />
          </div>

          <AnimatePresence initial={false} mode="wait">
            {entries.length > 0 ? (
              <motion.div
                key="playlists-grid"
                className="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-x-6 gap-y-8 pt-2"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                transition={{ duration: 0.16 }}
              >
                <AnimatePresence initial={false}>
                  {entries.map((pl) => (
                    <PlaylistCard
                      key={pl.id}
                      playlist={pl}
                      query={query}
                      draggable={draggable}
                      dragging={draggingId === pl.id}
                      onDragStart={() => handleDragStart(pl.id)}
                      onDrag={(x, y) => handleDrag(pl.id, x, y)}
                      onDragEnd={() => setDraggingId(null)}
                      cardRef={(el) => {
                        if (el) cardEls.current.set(pl.id, el);
                        else cardEls.current.delete(pl.id);
                      }}
                    />
                  ))}
                </AnimatePresence>
              </motion.div>
            ) : (
              <motion.div
                key="playlists-empty"
                className="flex flex-col items-center gap-2.5 pb-10 pt-[100px] text-center"
                initial={{ opacity: 0, y: 8 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -6 }}
                transition={{ duration: 0.18 }}
              >
                <div className="font-serif text-lg font-semibold text-tx">
                  {query ? t("playlists.emptyTitle", { q: filter.q.trim() }) : t("playlists.noneTitle")}
                </div>
                <div className="text-[13px] text-tx2">
                  {query ? t("playlists.emptyBody") : t("playlists.noneBody")}
                </div>
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      </div>
      <div
        ref={thumbRef}
        className="scroll-thumb pointer-events-none absolute right-[5px] top-2 z-20 h-[120px] w-1.5 rounded-[3px] opacity-0"
      />
    </div>
  );
}
