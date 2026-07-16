import { useEffect, useMemo, useRef, useState, type UIEvent } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { Icon } from "@/components/common/Icon";
import { useElasticScroll } from "@/hooks/useElasticScroll";
import { ALBUMS, albumsOfArtist, allTracks } from "@/data/library";
import { PLAYLISTS } from "@/data/playlists";
import { usePlayerStore } from "@/store/player";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { coverGradientStyle } from "@/lib/coverStyle";
import { fmtTime } from "@/lib/time";
import type { MessageKey } from "@/i18n/messages";

type Scope = "all" | "songs" | "albums" | "artists" | "playlists";

const SCOPES: { key: Scope; labelKey: MessageKey }[] = [
  { key: "all", labelKey: "search.scopeAll" },
  { key: "songs", labelKey: "nav.songs" },
  { key: "albums", labelKey: "nav.albums" },
  { key: "artists", labelKey: "nav.artists" },
  { key: "playlists", labelKey: "favorites.playlists" },
];

const CAP = 8;

/** 命中高亮：首个命中段着 --ac。 */
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

/** 搜索页曲目缩略图上的白色均衡器（深色遮罩之上）。 */
function ThumbEq({ playing }: { playing: boolean }) {
  return (
    <div className="absolute inset-0 flex items-center justify-center gap-[2px] rounded-[9px] bg-[rgba(30,18,8,0.5)]">
      {[0, 0.25, 0.5].map((delay) => (
        <div
          key={delay}
          className="h-3 w-[2.5px] origin-bottom rounded-[1px] bg-[#F6EFE3]"
          style={{
            animation: `eq 0.9s ease-in-out ${delay}s infinite`,
            animationPlayState: playing ? "running" : "paused",
          }}
        />
      ))}
    </div>
  );
}

export function SearchScreen() {
  const { t } = useT();
  const reduceMotion = useReducedMotion();
  const { scrollerRef, innerRef, thumbRef, onScroll } = useElasticScroll();
  const inputRef = useRef<HTMLInputElement | null>(null);
  const [q, setQ] = useState("");
  const [focused, setFocused] = useState(false);
  const [scope, setScope] = useState<Scope>("all");

  const playing = usePlayerStore((s) => s.playing);
  const current = usePlayerStore((s) =>
    s.currentIndex >= 0 ? s.queue[s.currentIndex]?.track : null,
  );
  const playQueue = usePlayerStore((s) => s.playQueue);
  const openAlbum = useUiStore((s) => s.openAlbum);
  const openArtist = useUiStore((s) => s.openArtist);
  const openPlaylist = useUiStore((s) => s.openPlaylist);
  const recents = useUiStore((s) => s.searchRecents);
  const pushSearchRecent = useUiStore((s) => s.pushSearchRecent);

  // 进入搜索页自动聚焦输入框。
  useEffect(() => {
    inputRef.current?.focus({ preventScroll: true });
  }, []);

  const query = q.trim().toLowerCase();
  const hasQ = query.length > 0;
  const inScope = (k: Scope) => scope === "all" || scope === k;

  const songs = useMemo(() => {
    if (!hasQ || !inScope("songs")) return [];
    return allTracks()
      .filter((tk) => `${tk.title} ${tk.artist}`.toLowerCase().includes(query))
      .slice(0, CAP);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, hasQ, scope]);

  const albums = useMemo(() => {
    if (!hasQ || !inScope("albums")) return [];
    return ALBUMS.filter((a) => a.title.toLowerCase().includes(query)).slice(0, CAP);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, hasQ, scope]);

  const artists = useMemo(() => {
    if (!hasQ || !inScope("artists")) return [];
    const seen = new Set<string>();
    const out: string[] = [];
    for (const a of ALBUMS) {
      if (a.artist.toLowerCase().includes(query) && !seen.has(a.artist)) {
        seen.add(a.artist);
        out.push(a.artist);
      }
    }
    return out.slice(0, CAP);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, hasQ, scope]);

  const lists = useMemo(() => {
    if (!hasQ || !inScope("playlists")) return [];
    return PLAYLISTS.filter((p) => p.title.toLowerCase().includes(query)).slice(0, CAP);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, hasQ, scope]);

  const total = songs.length + albums.length + artists.length + lists.length;
  const showRecent = !hasQ;
  const showEmpty = hasQ && total === 0;

  const handleKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" && query) pushSearchRecent(q.trim());
    else if (e.key === "Escape") {
      if (q) setQ("");
      else e.currentTarget.blur();
    }
  };

  const handleScroll = (e: UIEvent<HTMLDivElement>) => onScroll(e);

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      {/* 搜索框 + 范围筛选 */}
      <div data-tauri-drag-region className="px-10 pb-[18px] pt-[30px]">
        <div
          className="mx-auto flex max-w-[560px] items-center gap-[11px] rounded-full border bg-srf px-[18px] py-3 transition-[box-shadow,border-color] duration-200"
          style={{
            borderColor: focused ? "var(--ac)" : "var(--bd)",
            boxShadow: focused
              ? "0 0 0 3px color-mix(in srgb, var(--ac) 18%, transparent)"
              : "0 2px 8px var(--stickybar-shadow)",
          }}
        >
          <Icon name="search" size={17} className="flex-shrink-0 text-tx2" />
          <input
            ref={inputRef}
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onFocus={() => setFocused(true)}
            onBlur={() => setFocused(false)}
            onKeyDown={handleKey}
            placeholder={t("search.placeholder")}
            className="min-w-0 flex-1 border-none bg-transparent text-[15px] text-tx outline-none placeholder:text-tx2/80"
          />
          <AnimatePresence initial={false}>
            {hasQ && (
            <motion.button
              key="clear"
              aria-label={t("search.clear")}
              title={t("search.clear")}
              onClick={() => {
                setQ("");
                inputRef.current?.focus();
              }}
              className="grid size-[22px] flex-shrink-0 cursor-pointer place-items-center rounded-full bg-hv text-tx2 transition-colors hover:text-tx"
              initial={{ opacity: 0, scale: reduceMotion ? 1 : 0.65 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: reduceMotion ? 1 : 0.65 }}
              transition={{ duration: reduceMotion ? 0.01 : 0.16 }}
            >
              <Icon name="close" size={11} strokeWidth={2.4} />
            </motion.button>
            )}
          </AnimatePresence>
        </div>
        <div className="mx-auto mt-2.5 flex max-w-[560px] gap-[7px]">
          {SCOPES.map((sp) => {
            const on = scope === sp.key;
            return (
              <button
                key={sp.key}
                onClick={() => setScope(sp.key)}
                className="cursor-pointer rounded-full border px-[15px] py-1.5 text-[12.5px] font-semibold transition-[transform,background-color,color,border-color] duration-[180ms] active:scale-[0.94]"
                style={{
                  background: on ? "var(--ac)" : "var(--srf)",
                  color: on ? "var(--on-ac)" : "var(--tx2)",
                  borderColor: on ? "var(--ac)" : "var(--bd)",
                }}
              >
                {t(sp.labelKey)}
              </button>
            );
          })}
        </div>
      </div>

      {/* 结果区 */}
      <div className="relative min-h-0 flex-1">
        <div
          ref={scrollerRef}
          onScroll={handleScroll}
          className="no-scrollbar absolute inset-0 overflow-auto px-10 pb-[120px] [overscroll-behavior:contain]"
        >
          <div ref={innerRef} className="relative mx-auto max-w-[760px] will-change-transform">
            <AnimatePresence initial={false} mode="popLayout">
            {/* 最近搜索 */}
            {showRecent && (
              <motion.section
                key="recent"
                initial={{ opacity: 0, y: reduceMotion ? 0 : 7 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: reduceMotion ? 0 : -5 }}
                transition={{ duration: reduceMotion ? 0.01 : 0.18, ease: [0.22, 1, 0.36, 1] }}
              >
                <div className="pb-2.5 pt-[26px] text-[12.5px] font-semibold tracking-[0.1em] text-tx2">
                  {t("search.recent")}
                </div>
                <div className="flex flex-wrap gap-2">
                  {recents.map((rc) => (
                    <button
                      key={rc}
                      onClick={() => {
                        setQ(rc);
                        inputRef.current?.focus();
                      }}
                      className="flex cursor-pointer items-center gap-[7px] rounded-full border border-bd bg-srf px-[15px] py-2 text-[13px] text-tx transition-colors hover:bg-hv"
                    >
                      <svg
                        width="13"
                        height="13"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        strokeWidth="1.8"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        className="text-tx2"
                      >
                        <circle cx="12" cy="12" r="9" />
                        <path d="M12 7v5l3 3" />
                      </svg>
                      {rc}
                    </button>
                  ))}
                </div>
              </motion.section>
            )}

            {/* 无结果 */}
            {showEmpty && (
              <motion.section
                key="empty"
                className="flex flex-col items-center gap-3.5 pb-10 pt-[90px] text-center"
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
                  {t("search.emptyTitle", { q: q.trim() })}
                </div>
                <div className="max-w-[320px] text-[13px] leading-[1.6] text-tx2">
                  {t("search.emptyBody")}
                </div>
              </motion.section>
            )}

            {/* 歌曲 */}
            {songs.length > 0 && (
              <motion.section
                key="songs"
                initial={{ opacity: 0, y: reduceMotion ? 0 : 7 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: reduceMotion ? 0 : -5 }}
                transition={{ duration: reduceMotion ? 0.01 : 0.18, ease: [0.22, 1, 0.36, 1] }}
              >
                <div className="pb-1.5 pt-6 font-serif text-[19px] font-semibold text-tx">
                  {t("nav.songs")}
                </div>
                <AnimatePresence initial={false}>
                {songs.map((tk, i) => {
                  const isCur = current?.id === tk.id;
                  return (
                    <motion.div
                      key={tk.id}
                      layout="position"
                      exit={{ opacity: 0, x: reduceMotion ? 0 : -6 }}
                      onClick={() => playQueue(songs, i)}
                      className="-mx-3 flex cursor-pointer items-center gap-[13px] rounded-xl px-3 py-2 transition-colors hover:bg-hv"
                    >
                      <div
                        className="cover-corners cover-gradient cover-thumb-material relative grid size-[42px] flex-shrink-0 place-items-center rounded-[9px]"
                        style={coverGradientStyle(tk.cover)}
                      >
                        <span className="cover-initial font-serif text-base">{tk.cover.initial}</span>
                        {isCur && <ThumbEq playing={playing} />}
                      </div>
                      <div className="min-w-0 flex-1">
                        <div className="truncate font-serif text-[14.5px] font-semibold text-tx">
                          <Highlight text={tk.title} query={query} />
                        </div>
                        <div className="mt-0.5 truncate text-xs text-tx2">
                          <Highlight text={tk.artist} query={query} /> — {tk.album}
                        </div>
                      </div>
                      <span className="text-[12.5px] tabular-nums text-tx2">
                        {fmtTime(tk.durationSec)}
                      </span>
                    </motion.div>
                  );
                })}
                </AnimatePresence>
              </motion.section>
            )}

            {/* 专辑 */}
            {albums.length > 0 && (
              <motion.section
                key="albums"
                initial={{ opacity: 0, y: reduceMotion ? 0 : 7 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: reduceMotion ? 0 : -5 }}
                transition={{ duration: reduceMotion ? 0.01 : 0.18, ease: [0.22, 1, 0.36, 1] }}
              >
                <div className="pb-3 pt-[26px] font-serif text-[19px] font-semibold text-tx">
                  {t("nav.albums")}
                </div>
                <div className="grid grid-cols-[repeat(auto-fill,minmax(140px,1fr))] gap-[22px]">
                  <AnimatePresence initial={false}>
                  {albums.map((al) => (
                    <motion.div
                      key={al.id}
                      layout="position"
                      exit={{ opacity: 0, scale: reduceMotion ? 1 : 0.95 }}
                      onClick={() => openAlbum(al.id)}
                      className="min-w-0 cursor-pointer"
                    >
                      <motion.div
                        layoutId={`album-cover-${al.id}`}
                        whileHover={{ y: -4 }}
                        transition={{ type: "spring", stiffness: 380, damping: 20 }}
                        className="cover-corners cover-gradient cover-material grid aspect-square place-items-center rounded-2xl"
                        style={coverGradientStyle(al.cover)}
                      >
                        <span className="cover-initial font-serif text-[40px] font-medium">
                          {al.cover.initial}
                        </span>
                      </motion.div>
                      <div className="mt-2 truncate font-serif text-[13.5px] font-semibold text-tx">
                        <Highlight text={al.title} query={query} />
                      </div>
                      <div className="mt-0.5 truncate text-[11.5px] text-tx2">
                        {al.artist} · {al.year}
                      </div>
                    </motion.div>
                  ))}
                  </AnimatePresence>
                </div>
              </motion.section>
            )}

            {/* 歌手 */}
            {artists.length > 0 && (
              <motion.section
                key="artists"
                initial={{ opacity: 0, y: reduceMotion ? 0 : 7 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: reduceMotion ? 0 : -5 }}
                transition={{ duration: reduceMotion ? 0.01 : 0.18, ease: [0.22, 1, 0.36, 1] }}
              >
                <div className="pb-3 pt-[26px] font-serif text-[19px] font-semibold text-tx">
                  {t("nav.artists")}
                </div>
                <div className="grid grid-cols-[repeat(auto-fill,minmax(120px,1fr))] justify-items-center gap-[22px]">
                  <AnimatePresence initial={false}>
                  {artists.map((name) => {
                    const cover = albumsOfArtist(name)[0]?.cover;
                    return (
                      <motion.div
                        key={name}
                        layout="position"
                        exit={{ opacity: 0, scale: reduceMotion ? 1 : 0.94 }}
                        onClick={() => openArtist(name)}
                        className="flex w-[120px] cursor-pointer flex-col items-center gap-[9px]"
                      >
                        <motion.div
                          whileHover={{ y: -4 }}
                          transition={{ type: "spring", stiffness: 380, damping: 20 }}
                          className="cover-gradient cover-thumb-material grid size-[104px] place-items-center rounded-full"
                          style={cover ? coverGradientStyle(cover) : undefined}
                        >
                          {cover && (
                            <span className="cover-initial font-serif text-[34px] font-medium">
                              {cover.initial}
                            </span>
                          )}
                        </motion.div>
                        <div className="text-center font-serif text-[13.5px] font-semibold text-tx">
                          <Highlight text={name} query={query} />
                        </div>
                      </motion.div>
                    );
                  })}
                  </AnimatePresence>
                </div>
              </motion.section>
            )}

            {/* 歌单 */}
            {lists.length > 0 && (
              <motion.section
                key="playlists"
                initial={{ opacity: 0, y: reduceMotion ? 0 : 7 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: reduceMotion ? 0 : -5 }}
                transition={{ duration: reduceMotion ? 0.01 : 0.18, ease: [0.22, 1, 0.36, 1] }}
              >
                <div className="pb-1.5 pt-[26px] font-serif text-[19px] font-semibold text-tx">
                  {t("favorites.playlists")}
                </div>
                <AnimatePresence initial={false}>
                {lists.map((pl) => (
                  <motion.div
                    key={pl.id}
                    layout="position"
                    exit={{ opacity: 0, x: reduceMotion ? 0 : -6 }}
                    onClick={() => openPlaylist(pl.id)}
                    className="-mx-3 flex cursor-pointer items-center gap-[13px] rounded-xl px-3 py-2 transition-colors hover:bg-hv"
                  >
                    <div className="grid size-[42px] flex-shrink-0 place-items-center rounded-[9px] border border-bd bg-sb text-tx2">
                      <Icon name="queue" size={18} strokeWidth={1.7} />
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="truncate font-serif text-[14.5px] font-semibold text-tx">
                        <Highlight text={pl.title} query={query} />
                      </div>
                      <div className="mt-0.5 truncate text-xs text-tx2">
                        {t("playlist.meta", {
                          n: pl.tracks.length,
                          m: Math.round(pl.tracks.reduce((s, tk) => s + tk.durationSec, 0) / 60),
                          updated: pl.updatedLabel,
                        })}
                      </div>
                    </div>
                  </motion.div>
                ))}
                </AnimatePresence>
              </motion.section>
            )}
            </AnimatePresence>
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
