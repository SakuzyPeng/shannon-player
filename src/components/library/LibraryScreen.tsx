import { Icon } from "@/components/common/Icon";
import { AlbumGrid } from "@/components/library/AlbumGrid";
import { AlbumList } from "@/components/library/AlbumList";
import { useElasticScroll } from "@/hooks/useElasticScroll";
import { NAV_ITEMS } from "@/data/library";
import { ALBUMS } from "@/data/library";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";

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
            view === v ? "bg-srf text-tx shadow-[0_1px_3px_rgba(60,40,20,0.1)]" : "text-tx2",
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
  const { scrollerRef, innerRef, thumbRef, onWheel, onScroll } = useElasticScroll();

  const navItem = NAV_ITEMS.find((n) => n.key === nav);
  const title = navItem ? t(navItem.labelKey) : t("nav.albums");
  const isAlbums = nav === "albums";

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* 标题栏（兼作窗口拖拽区） */}
      <div data-tauri-drag-region className="flex items-end gap-4 px-10 pb-5 pt-[34px]">
        <div data-tauri-drag-region className="flex flex-col">
          <h1 className="m-0 font-serif text-[40px] font-medium text-tx">{title}</h1>
          <div className="mt-[7px] text-[13px] text-tx2">
            {isAlbums
              ? t("header.albumSubtitle", { count: ALBUMS.length })
              : t("placeholder.body")}
          </div>
        </div>
        <div className="flex-1" data-tauri-drag-region />
        {isAlbums && (
          <>
            <Segmented />
            <div className="flex w-[190px] cursor-text items-center gap-2 rounded-full border border-bd bg-srf px-[15px] py-[9px] text-[13px] text-tx2">
              <Icon name="search" size={14} />
              {t("action.search")}
            </div>
          </>
        )}
      </div>

      {/* 滚动区 + 自绘滚动条 */}
      <div className="relative min-h-0 flex-1">
        <div
          ref={scrollerRef}
          onWheel={onWheel}
          onScroll={onScroll}
          className="no-scrollbar absolute inset-0 overflow-auto px-10 pb-[120px] pt-1 [overscroll-behavior:contain]"
        >
          <div ref={innerRef} className="will-change-transform">
            {isAlbums ? (
              view === "grid" ? (
                <AlbumGrid />
              ) : (
                <AlbumList />
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
          className="pointer-events-none absolute right-[5px] top-2 z-20 h-[120px] w-1.5 rounded-sm opacity-0"
          style={{ background: "rgba(112,92,66,0.4)" }}
        />
      </div>
    </div>
  );
}
