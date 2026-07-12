import { useMemo, useState } from "react";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { Icon } from "@/components/common/Icon";
import { AlbumGrid } from "@/components/library/AlbumGrid";
import { AlbumList } from "@/components/library/AlbumList";
import { useElasticScroll } from "@/hooks/useElasticScroll";
import { NAV_ITEMS } from "@/data/library";
import { ALBUMS } from "@/data/library";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";
import type { MessageKey } from "@/i18n/messages";

type AlbumSort = "recent" | "title" | "artist";

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
    <div className="flex items-center rounded-full border border-bd bg-sb p-[3px] text-[12.5px]">
      {(["grid", "list"] as const).map((v) => (
        <button
          key={v}
          onClick={() => setView(v)}
          className={cn(
            "cursor-pointer rounded-full px-4 py-1.5 font-semibold transition-colors",
            view === v ? "segmented-active-shadow bg-srf text-tx" : "text-tx2",
          )}
        >
          {v === "grid" ? t("view.grid") : t("view.list")}
        </button>
      ))}
    </div>
  );
}

export function LibraryScreen() {
  const { t } = useT();
  const nav = useUiStore((s) => s.nav);
  const view = useUiStore((s) => s.view);
  const setNav = useUiStore((s) => s.setNav);
  const { scrollerRef, innerRef, thumbRef, onScroll } = useElasticScroll();
  const [sort, setSort] = useState<AlbumSort>("recent");

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

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* 标题栏（兼作窗口拖拽区） */}
      <div data-tauri-drag-region className="flex items-end gap-4 px-10 pb-5 pt-[34px]">
        <div data-tauri-drag-region className="flex flex-col">
          <h1 className="m-0 font-serif text-[40px] font-medium text-tx">{title}</h1>
          <div className="mt-[7px] text-[13px] text-tx2">
            {isAlbums
              ? t("header.albumSubtitle", { count: ALBUMS.length, sort: t(SORT_LABEL[sort]) })
              : t("placeholder.body")}
          </div>
        </div>
        <div className="flex-1" data-tauri-drag-region />
        {isAlbums && (
          <>
            <Segmented />
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
                  <DropdownMenu.RadioGroup value={sort} onValueChange={(v) => setSort(v as AlbumSort)}>
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
            <button
              onClick={() => setNav("search")}
              className="flex w-[190px] cursor-pointer items-center gap-2 rounded-full border border-bd bg-srf px-[15px] py-[9px] text-[13px] text-tx2 transition-colors hover:bg-hv hover:text-tx"
            >
              <Icon name="search" size={14} />
              {t("action.search")}
            </button>
          </>
        )}
      </div>

      {/* 滚动区 + 自绘滚动条 */}
      <div className="relative min-h-0 flex-1">
        <div
          ref={scrollerRef}
          onScroll={onScroll}
          className="no-scrollbar absolute inset-0 overflow-auto px-10 pb-[120px] pt-2 [overscroll-behavior:contain]"
        >
          <div ref={innerRef} className="will-change-transform">
            {isAlbums ? (
              view === "grid" ? (
                <AlbumGrid albums={albums} />
              ) : (
                <AlbumList albums={albums} />
              )
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
    </div>
  );
}
