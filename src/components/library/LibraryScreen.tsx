import { useMemo, useRef, useState, type UIEvent } from "react";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { AnimatePresence, motion } from "framer-motion";
import { FilterPill, useFilterPill, type FilterState } from "@/components/common/FilterPill";
import { Icon } from "@/components/common/Icon";
import { MetaLine } from "@/components/common/MetaLine";
import { SegmentedContent, SegmentedControl } from "@/components/common/SegmentedControl";
import { AlbumGrid } from "@/components/library/AlbumGrid";
import { AlbumList } from "@/components/library/AlbumList";
import { useElasticScroll } from "@/hooks/useElasticScroll";
import { NAV_ITEMS } from "@/data/library";
import { ALBUMS } from "@/data/library";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import type { MessageKey } from "@/i18n/messages";

type AlbumSort = "recent" | "title" | "artist";

/** 与歌曲页对齐：标题栏滚出后显示吸顶栏。 */
const STICKY_THRESHOLD = 100;

const SORT_LABEL: Record<AlbumSort, MessageKey> = {
  recent: "songs.sortRecent",
  title: "songs.sortByTitle",
  artist: "songs.sortByArtist",
};

function Segmented() {
  const { t } = useT();
  const view = useUiStore((s) => s.view);
  const setView = useUiStore((s) => s.setView);
  return (
    <SegmentedControl
      value={view}
      onValueChange={setView}
      options={[
        { value: "grid", label: t("view.grid") },
        { value: "list", label: t("view.list") },
      ]}
      className="p-[3px] text-[12.5px]"
      buttonClassName="px-4 py-1.5"
    />
  );
}

function AlbumSortMenu({
  sort,
  onValueChange,
}: {
  sort: AlbumSort;
  onValueChange: (value: AlbumSort) => void;
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
          aria-label={t("songs.sortMenu")}
          className="surface-corners animate-menu-pop menu-shadow z-50 w-[170px] origin-top-right rounded-[14px] border border-bd bg-srf p-1.5"
        >
          <DropdownMenu.RadioGroup value={sort} onValueChange={(value) => onValueChange(value as AlbumSort)}>
            {(Object.keys(SORT_LABEL) as AlbumSort[]).map((mode) => (
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
 * 专辑页工具簇：视图切换 + 排序 + 页内过滤钮。
 * 过滤逻辑与歌曲页 / 歌手页 / 收藏页 / 歌单页统一（全局搜索仍在图标栏）。
 */
function AlbumToolbar({
  sort,
  onSortChange,
  filter,
  inputRef,
  compact,
}: {
  sort: AlbumSort;
  onSortChange: (value: AlbumSort) => void;
  filter: FilterState;
  inputRef: React.RefObject<HTMLInputElement | null>;
  compact: boolean;
}) {
  const { t } = useT();
  return (
    <>
      <Segmented />
      <AlbumSortMenu sort={sort} onValueChange={onSortChange} />
      <FilterPill
        filter={filter}
        height={compact ? 34 : 40}
        openWidth={compact ? 300 : 318}
        inputRef={inputRef}
        placeholder={t("albums.filterPlaceholder")}
        className={compact ? "ml-auto" : "mr-1.5"}
      />
    </>
  );
}

export function LibraryScreen() {
  const { t } = useT();
  const nav = useUiStore((s) => s.nav);
  const view = useUiStore((s) => s.view);
  const { scrollerRef, innerRef, thumbRef, onScroll } = useElasticScroll();
  const [sort, setSort] = useState<AlbumSort>("recent");
  const [barVisible, setBarVisible] = useState(false);
  const { filter, query } = useFilterPill();
  const inputRef = useRef<HTMLInputElement>(null);
  const barInputRef = useRef<HTMLInputElement>(null);

  const navItem = NAV_ITEMS.find((n) => n.key === nav);
  const title = navItem ? t(navItem.labelKey) : t("nav.albums");
  const isAlbums = nav === "albums";
  const albums = useMemo(() => {
    const list = [...ALBUMS];
    if (sort === "title") return list.sort((a, b) => a.title.localeCompare(b.title, "zh"));
    if (sort === "artist") {
      return list.sort(
        (a, b) => a.artist.localeCompare(b.artist, "zh") || a.title.localeCompare(b.title, "zh"),
      );
    }
    return list;
  }, [sort]);
  /** 页内过滤：命中专辑名或歌手名（与歌手页 / 歌曲页一致的匹配口径）。 */
  const filteredAlbums = useMemo(
    () =>
      query
        ? albums.filter(
            (a) =>
              a.title.toLowerCase().includes(query) || a.artist.toLowerCase().includes(query),
          )
        : albums,
    [albums, query],
  );
  const handleScroll = (e: UIEvent<HTMLDivElement>) => {
    onScroll(e);
    const visible = e.currentTarget.scrollTop > STICKY_THRESHOLD;
    if (visible !== barVisible) setBarVisible(visible);
  };

  return (
    <div className="relative min-h-0 flex-1">
      {isAlbums && (
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
            <Icon name="albums" size={15} strokeWidth={1.8} />
          </div>
          <span className="font-serif text-[16.5px] font-semibold text-tx">{title}</span>
          <div className="flex-1" />
          <AlbumToolbar
            sort={sort}
            onSortChange={setSort}
            filter={filter}
            inputRef={barInputRef}
            compact
          />
        </div>
      )}

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
              <h1 className="m-0 font-serif text-[40px] font-medium text-tx">{title}</h1>
              <MetaLine
                text={
                  isAlbums
                    ? t("header.albumSubtitle", { count: ALBUMS.length, sort: t(SORT_LABEL[sort]) })
                    : t("placeholder.body")
                }
                className="mt-[7px] text-[13px] text-tx2"
              />
            </div>
            <div className="flex-1" data-tauri-drag-region />
            {isAlbums && (
              <AlbumToolbar
                sort={sort}
                onSortChange={setSort}
                filter={filter}
                inputRef={inputRef}
                compact={false}
              />
            )}
          </div>

          {isAlbums ? (
            <AnimatePresence initial={false} mode="wait">
              {filteredAlbums.length > 0 ? (
                <motion.div
                  key="albums-content"
                  className="pt-2"
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  exit={{ opacity: 0 }}
                  transition={{ duration: 0.16 }}
                >
                  <SegmentedContent value={view}>
                    {view === "grid" ? (
                      <AlbumGrid albums={filteredAlbums} />
                    ) : (
                      <AlbumList albums={filteredAlbums} />
                    )}
                  </SegmentedContent>
                </motion.div>
              ) : (
                <motion.div
                  key="albums-empty"
                  className="flex flex-col items-center gap-2.5 pb-10 pt-[100px] text-center"
                  initial={{ opacity: 0, y: 8 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -6 }}
                  transition={{ duration: 0.18 }}
                >
                  <div className="font-serif text-lg font-semibold text-tx">
                    {t("albums.emptyTitle", { q: filter.q.trim() })}
                  </div>
                  <div className="text-[13px] text-tx2">{t("albums.emptyBody")}</div>
                </motion.div>
              )}
            </AnimatePresence>
          ) : (
            <div className="flex h-[520px] flex-col items-center justify-center gap-3 text-center text-tx2">
              <div className="font-serif text-[22px] text-tx">{t("placeholder.title", { name: title })}</div>
              <div className="text-[13px]">{t("placeholder.body")}</div>
            </div>
          )}
        </div>
      </div>
      <div
        ref={thumbRef}
        className="scroll-thumb pointer-events-none absolute right-[5px] top-2 z-20 h-[120px] w-1.5 rounded-[3px] opacity-0"
      />
    </div>
  );
}
