import { useMemo, type UIEvent } from "react";
import { motion } from "framer-motion";
import { Icon } from "@/components/common/Icon";
import { useElasticScroll } from "@/hooks/useElasticScroll";
import { ALBUMS, albumsOfArtist } from "@/data/library";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { coverGradientStyle } from "@/lib/coverStyle";

/** 歌手索引卡：圆形头像（取最新专辑封面）+ 歌手名 + 专辑数。 */
function ArtistCard({ name }: { name: string }) {
  const { t } = useT();
  const openArtist = useUiStore((s) => s.openArtist);
  const albums = albumsOfArtist(name);
  const cover = albums[0]?.cover;
  return (
    <button
      onClick={() => openArtist(name)}
      className="group flex cursor-pointer flex-col items-center gap-[11px]"
    >
      <motion.div
        whileHover={{ y: -4 }}
        transition={{ type: "spring", stiffness: 380, damping: 20 }}
        className="cover-gradient cover-thumb-material grid size-[132px] place-items-center rounded-full transition-shadow duration-300 group-hover:shadow-[0_16px_32px_var(--cover-hover-shadow)]"
        style={cover ? coverGradientStyle(cover) : undefined}
      >
        {cover && (
          <span className="cover-initial font-serif text-[44px] font-medium">{cover.initial}</span>
        )}
      </motion.div>
      <div className="max-w-[150px] truncate text-center font-serif text-[14.5px] font-semibold text-tx">
        {name}
      </div>
      <div className="-mt-1.5 text-xs text-tx2">
        {t("favorites.artistMeta", { n: albums.length })}
      </div>
    </button>
  );
}

export function ArtistsScreen() {
  const { t } = useT();
  const setNav = useUiStore((s) => s.setNav);
  const { scrollerRef, innerRef, thumbRef, onScroll } = useElasticScroll();

  const artists = useMemo(() => {
    const seen = new Set<string>();
    const out: string[] = [];
    for (const a of ALBUMS) {
      if (!seen.has(a.artist)) {
        seen.add(a.artist);
        out.push(a.artist);
      }
    }
    return out.sort((x, y) => x.localeCompare(y, "zh"));
  }, []);

  const handleScroll = (e: UIEvent<HTMLDivElement>) => onScroll(e);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* 标题栏（兼作窗口拖拽区） */}
      <div data-tauri-drag-region className="flex items-end gap-4 px-10 pb-5 pt-[34px]">
        <div data-tauri-drag-region className="flex flex-col">
          <h1 className="m-0 font-serif text-[40px] font-medium text-tx">{t("nav.artists")}</h1>
          <div className="mt-[7px] text-[13px] text-tx2">
            {t("artists.subtitle", { n: artists.length })}
          </div>
        </div>
        <div className="flex-1" data-tauri-drag-region />
        <button
          onClick={() => setNav("search")}
          className="flex w-[190px] cursor-pointer items-center gap-2 rounded-full border border-bd bg-srf px-[15px] py-[9px] text-[13px] text-tx2 transition-colors hover:bg-hv hover:text-tx"
        >
          <Icon name="search" size={14} />
          {t("action.search")}
        </button>
      </div>

      {/* 滚动区 + 自绘滚动条 */}
      <div className="relative min-h-0 flex-1">
        <div
          ref={scrollerRef}
          onScroll={handleScroll}
          className="no-scrollbar absolute inset-0 overflow-auto px-10 pb-[120px] pt-2 [overscroll-behavior:contain]"
        >
          <div ref={innerRef} className="will-change-transform">
            <div className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] justify-items-center gap-x-6 gap-y-9">
              {artists.map((name) => (
                <ArtistCard key={name} name={name} />
              ))}
            </div>
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
