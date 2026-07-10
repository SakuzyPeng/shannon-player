import { motion } from "framer-motion";
import { Collage } from "@/components/common/Collage";
import { Icon } from "@/components/common/Icon";
import { AlbumGrid } from "@/components/library/AlbumGrid";
import { AlbumList } from "@/components/library/AlbumList";
import { useElasticScroll } from "@/hooks/useElasticScroll";
import { NAV_ITEMS } from "@/data/library";
import { ALBUMS } from "@/data/library";
import { PLAYLISTS, collageOf } from "@/data/playlists";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";

/** 收藏页临时入口：歌单卡片网格（完整收藏页在后续路线图实现）。 */
function PlaylistsHub() {
  const { t } = useT();
  const openPlaylist = useUiStore((s) => s.openPlaylist);
  return (
    <div className="pt-2">
      <div className="mb-4 text-[13px] font-semibold tracking-[0.08em] text-tx2">
        {t("favorites.playlists")}
      </div>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-x-6 gap-y-8">
        {PLAYLISTS.map((pl) => (
          <motion.button
            key={pl.id}
            whileHover={{ y: -5 }}
            transition={{ type: "spring", stiffness: 380, damping: 26 }}
            onClick={() => openPlaylist(pl.id)}
            className="group relative flex cursor-pointer flex-col items-start gap-3 rounded-2xl text-left hover:z-10"
          >
            <Collage
              covers={collageOf(pl)}
              size={180}
              radius={14}
              glyph={30}
              className="transition-shadow duration-300 group-hover:shadow-[0_16px_32px_var(--cover-hover-shadow)]"
            />
            <div className="w-full px-0.5">
              <div className="truncate font-serif text-[15.5px] font-semibold text-tx">
                {pl.title}
              </div>
              <div className="mt-0.5 truncate text-[12.5px] text-tx2">
                {t("playlist.meta", {
                  n: pl.tracks.length,
                  m: Math.round(pl.tracks.reduce((s, tk) => s + tk.durationSec, 0) / 60),
                  updated: pl.updatedLabel,
                })}
              </div>
            </div>
          </motion.button>
        ))}
      </div>
    </div>
  );
}

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
  const { scrollerRef, innerRef, thumbRef, onScroll } = useElasticScroll();

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
              : nav === "favorites"
                ? t("favorites.subtitle")
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
          onScroll={onScroll}
          className="no-scrollbar absolute inset-0 overflow-auto px-10 pb-[120px] pt-2 [overscroll-behavior:contain]"
        >
          <div ref={innerRef} className="will-change-transform">
            {isAlbums ? (
              view === "grid" ? (
                <AlbumGrid />
              ) : (
                <AlbumList />
              )
            ) : nav === "favorites" ? (
              <PlaylistsHub />
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
