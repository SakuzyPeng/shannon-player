import { useMemo, useRef, useState, type UIEvent } from "react";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { motion } from "framer-motion";
import { FilterPill, useFilterPill } from "@/components/common/FilterPill";
import { Icon } from "@/components/common/Icon";
import { useElasticScroll } from "@/hooks/useElasticScroll";
import { ALBUMS, albumsOfArtist } from "@/data/library";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { coverGradientStyle } from "@/lib/coverStyle";
import type { MessageKey } from "@/i18n/messages";

type ArtistSort = "name" | "albums";

/** 主标题基本滚出后显示吸顶栏，短歌手网格也保留可见区间。 */
const STICKY_THRESHOLD = 80;

const SORT_LABEL: Record<ArtistSort, MessageKey> = {
  name: "artists.sortByName",
  albums: "artists.sortByAlbums",
};

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

function ArtistSortMenu({
  sort,
  onValueChange,
}: {
  sort: ArtistSort;
  onValueChange: (value: ArtistSort) => void;
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
          aria-label={t("artists.sortMenu")}
          className="surface-corners animate-menu-pop menu-shadow z-50 w-[170px] origin-top-right rounded-[14px] border border-bd bg-srf p-1.5"
        >
          <DropdownMenu.RadioGroup value={sort} onValueChange={(value) => onValueChange(value as ArtistSort)}>
            {(Object.keys(SORT_LABEL) as ArtistSort[]).map((mode) => (
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

export function ArtistsScreen() {
  const { t } = useT();
  const { scrollerRef, innerRef, thumbRef, onScroll } = useElasticScroll();
  const { filter, query } = useFilterPill();
  const inputRef = useRef<HTMLInputElement | null>(null);
  const barInputRef = useRef<HTMLInputElement | null>(null);
  const [sort, setSort] = useState<ArtistSort>("name");
  const [barVisible, setBarVisible] = useState(false);

  const artists = useMemo(() => {
    const seen = new Set<string>();
    const out: string[] = [];
    for (const a of ALBUMS) {
      if (!seen.has(a.artist)) {
        seen.add(a.artist);
        out.push(a.artist);
      }
    }
    return out;
  }, []);
  const sortedArtists = useMemo(() => {
    const list = [...artists];
    if (sort === "albums") {
      return list.sort(
        (a, b) => albumsOfArtist(b).length - albumsOfArtist(a).length || a.localeCompare(b, "zh"),
      );
    }
    return list.sort((a, b) => a.localeCompare(b, "zh"));
  }, [artists, sort]);
  const filteredArtists = useMemo(
    () => (query ? sortedArtists.filter((name) => name.toLowerCase().includes(query)) : sortedArtists),
    [query, sortedArtists],
  );

  const handleScroll = (e: UIEvent<HTMLDivElement>) => {
    onScroll(e);
    const visible = e.currentTarget.scrollTop > STICKY_THRESHOLD;
    if (visible !== barVisible) setBarVisible(visible);
  };

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
          <Icon name="artists" size={15} strokeWidth={1.8} />
        </div>
        <span className="font-serif text-[16.5px] font-semibold text-tx">{t("nav.artists")}</span>
        <span className="hidden whitespace-nowrap text-xs text-tx2 lg:inline">
          {t("artists.subtitle", { n: artists.length })}
        </span>
        <div className="flex-1" />
        <ArtistSortMenu sort={sort} onValueChange={setSort} />
        <FilterPill
          filter={filter}
          height={34}
          openWidth={300}
          inputRef={barInputRef}
          placeholder={t("artists.filterPlaceholder")}
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
            <div data-tauri-drag-region className="flex flex-col">
              <h1 className="m-0 font-serif text-[40px] font-medium text-tx">{t("nav.artists")}</h1>
              <div className="mt-[7px] text-[13px] text-tx2">
                {t("artists.subtitle", { n: artists.length })}
              </div>
            </div>
            <div className="flex-1" data-tauri-drag-region />
            <ArtistSortMenu sort={sort} onValueChange={setSort} />
            <FilterPill
              filter={filter}
              height={40}
              openWidth={318}
              inputRef={inputRef}
              placeholder={t("artists.filterPlaceholder")}
              className="mr-1.5"
            />
          </div>

          {filteredArtists.length > 0 ? (
            <div className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] justify-items-center gap-x-6 gap-y-9 pt-2">
              {filteredArtists.map((name) => (
                <ArtistCard key={name} name={name} />
              ))}
            </div>
          ) : (
            <div className="flex flex-col items-center gap-2.5 pb-10 pt-[100px] text-center">
              <div className="font-serif text-lg font-semibold text-tx">
                {t("artists.emptyTitle", { q: filter.q.trim() })}
              </div>
              <div className="text-[13px] text-tx2">{t("artists.emptyBody")}</div>
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
