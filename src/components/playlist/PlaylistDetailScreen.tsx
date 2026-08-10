import { useMemo, useRef, useState, type UIEvent } from "react";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { AnimatePresence, motion, Reorder, useDragControls, useReducedMotion } from "framer-motion";
import { AnimatedIcon } from "@/components/common/AnimatedIcon";
import { Collage } from "@/components/common/Collage";
import { DetailNotFound } from "@/components/common/DetailNotFound";
import { FilterPill, useFilterPill } from "@/components/common/FilterPill";
import { Icon } from "@/components/common/Icon";
import { useMetadataEditor } from "@/components/common/EditMetadataDialog";
import { ItemContextMenu } from "@/components/common/ItemContextMenu";
import { ConfirmDialog, PromptDialog } from "@/components/common/Modal";
import { PlayPauseIcon } from "@/components/common/PlayPauseIcon";
import { TrackIndicator } from "@/components/common/TrackIndicator";
import { useElasticScroll } from "@/hooks/useElasticScroll";
import { PLAYLIST_TRACK_MENU } from "@/data/library";
import { collageOf } from "@/data/playlists";
import { usePlayerStore } from "@/store/player";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";
import { NEW_PLAYLIST, addTracksToPlaylistArg } from "@/lib/playlistActions";
import { updatedLabelOf } from "@/lib/playlists";
import { shuffled } from "@/lib/shuffle";
import { fmtTime } from "@/lib/time";
import type { MessageKey } from "@/i18n/messages";
import type { Id, Playlist, Track } from "@/types/player";

const STICKY_THRESHOLD = 210;
const COLS = "grid-cols-[44px_1fr_170px_190px_44px_60px]";

/** 过滤命中高亮：把标题按第一个命中拆三段，命中段 --ac。 */
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

const MORE_ITEM =
  "flex cursor-pointer items-center justify-between gap-3 rounded-lg px-2.5 py-2 text-[13px] outline-none data-[highlighted]:bg-hv";

/**
 * 歌单级「…」菜单：歌单是用户内容，这里放的是它的管理动作
 * （重命名 / 删除 / 整单插队 / 并入其他歌单）；专辑的「…」则复用其右键菜单。
 */
function PlaylistMoreMenu({
  playlist,
  onRename,
  onDelete,
}: {
  playlist: Playlist;
  onRename: () => void;
  onDelete: () => void;
}) {
  const { t } = useT();
  const playlists = usePlayerStore((s) => s.playlists);
  const enqueueNext = usePlayerStore((s) => s.enqueueNext);
  return (
    // modal={false}：菜单项会打开对话框，两层 modal 叠加时 Radix 的 body
    // pointer-events 恢复顺序会互相覆盖（菜单先关恢复 ''，对话框后关又写回
    // 它记下的 'none'），导致关闭后整个界面点不动。让 body 锁定只由对话框管。
    <DropdownMenu.Root modal={false}>
      <DropdownMenu.Trigger asChild>
        <button
          aria-label={t("album.more")}
          title={t("album.more")}
          className="grid size-10 cursor-pointer place-items-center rounded-full border border-bd bg-srf text-tx2 transition-colors hover:bg-hv hover:text-tx data-[state=open]:bg-hv data-[state=open]:text-ac"
        >
          <Icon name="more" size={16} />
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          align="start"
          sideOffset={8}
          className="surface-corners animate-menu-pop menu-shadow z-50 w-[222px] origin-top-left rounded-[14px] border border-bd bg-srf p-1.5"
        >
          <DropdownMenu.Item onSelect={onRename} className={cn(MORE_ITEM, "text-tx")}>
            <span>{t("playlist.rename")}</span>
          </DropdownMenu.Item>
          <DropdownMenu.Item
            onSelect={() => [...playlist.tracks].reverse().forEach(enqueueNext)}
            className={cn(MORE_ITEM, "text-tx")}
          >
            <span>{t("menu.playNext")}</span>
          </DropdownMenu.Item>
          {/* 把本歌单曲目并入另一个歌单（不含自己）。 */}
          <DropdownMenu.Sub>
            <DropdownMenu.SubTrigger className={cn(MORE_ITEM, "text-tx")}>
              <span>{t("playlist.mergeInto")}</span>
              <Icon name="chevronRight" size={13} className="text-tx2" strokeWidth={2} />
            </DropdownMenu.SubTrigger>
            <DropdownMenu.Portal>
              <DropdownMenu.SubContent
                sideOffset={6}
                className="surface-corners animate-menu-pop menu-shadow z-50 w-[210px] origin-top-left rounded-[14px] border border-bd bg-srf p-1.5"
              >
                {playlists
                  .filter((p) => p.id !== playlist.id)
                  .map((p) => (
                    <DropdownMenu.Item
                      key={p.id}
                      onSelect={() =>
                        addTracksToPlaylistArg(p.id, playlist.tracks, "", playlist.trackIds)
                      }
                      className={cn(MORE_ITEM, "text-tx")}
                    >
                      <span className="min-w-0 truncate">{p.title}</span>
                      <span className="flex-none text-[11px] text-tx2">
                        {t("unit.tracks", { n: p.tracks.length })}
                      </span>
                    </DropdownMenu.Item>
                  ))}
                <DropdownMenu.Separator className="mx-2 my-[5px] h-px bg-bd" />
                <DropdownMenu.Item
                  onSelect={() =>
                    addTracksToPlaylistArg(
                      NEW_PLAYLIST,
                      playlist.tracks,
                      t("playlist.newDefaultName"),
                      playlist.trackIds,
                    )
                  }
                  className={cn(MORE_ITEM, "font-semibold text-ac")}
                >
                  <span>{t("menu.newPlaylist")}</span>
                </DropdownMenu.Item>
              </DropdownMenu.SubContent>
            </DropdownMenu.Portal>
          </DropdownMenu.Sub>
          <DropdownMenu.Separator className="mx-2 my-[5px] h-px bg-bd" />
          <DropdownMenu.Item onSelect={onDelete} className={cn(MORE_ITEM, "text-danger")}>
            <span>{t("playlist.delete")}</span>
          </DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

/**
 * 歌单曲目行。定义在模块级（而非渲染函数内）——内联定义会让 React 每次渲染
 * 都视作新组件类型而重挂载，拖拽状态会在第一帧丢失。
 */
function PlaylistRow({
  track,
  index,
  reorderable,
  isCur,
  playing,
  liked,
  query,
  onPlay,
  onToggleFavorite,
  onAction,
}: {
  track: Track;
  index: number;
  reorderable: boolean;
  isCur: boolean;
  playing: boolean;
  liked: boolean;
  query: string;
  onPlay: () => void;
  onToggleFavorite: () => void;
  onAction: (key: MessageKey, arg?: string) => void;
}) {
  const { t } = useT();
  const dragControls = useDragControls();

  const row = (
    <ItemContextMenu
      label={`${track.title} — ${track.artist}`}
      items={PLAYLIST_TRACK_MENU}
      onAction={onAction}
      containsTrackId={track.id}
    >
      <div
        onClick={onPlay}
        className={`group/row mt-0.5 grid ${COLS} cursor-pointer items-center gap-3 rounded-xl px-3.5 py-2.5 transition-colors hover:bg-hv`}
      >
        <span
          className={cn("text-[13px] tabular-nums text-tx2", reorderable && "cursor-grab")}
          onPointerDown={(e) => reorderable && dragControls.start(e)}
          // 拖拽柄不是播放入口：按下即进入拖拽，单击不应触发整行的播放。
          onClick={(e) => reorderable && e.stopPropagation()}
        >
          <TrackIndicator
            number={index + 1}
            active={isCur}
            playing={playing}
            showGripOnHover={reorderable}
            gripTitle={t("playlist.dragToReorder")}
          />
        </span>
        <span
          className={cn(
            "truncate font-serif text-[15.5px]",
            isCur ? "font-semibold text-ac" : "font-medium text-tx",
          )}
        >
          <Highlight text={track.title} query={query} />
        </span>
        <span className="truncate text-[13px] text-tx2">{track.artist}</span>
        <span className="truncate text-[13px] text-tx2">{track.album}</span>
        <button
          aria-label={liked ? t("player.unfavorite") : t("player.favorite")}
          onClick={(e) => {
            e.stopPropagation();
            onToggleFavorite();
          }}
          className={cn(
            "grid size-[30px] cursor-pointer place-items-center rounded-full transition-[transform,background-color,color] hover:bg-ac/12 active:scale-90",
            liked ? "text-ac" : "text-tx2",
          )}
        >
          <AnimatedIcon name={liked ? "heart" : "favorites"} size={15} strokeWidth={1.8} variant="pop" />
        </button>
        <span className="text-right text-[13px] tabular-nums text-tx2">
          {fmtTime(track.durationSec)}
        </span>
      </div>
    </ItemContextMenu>
  );

  if (!reorderable) return row;
  return (
    <Reorder.Item value={track} as="div" dragListener={false} dragControls={dragControls}>
      {row}
    </Reorder.Item>
  );
}

export function PlaylistDetailScreen({ playlistId }: { playlistId: Id }) {
  const { t, locale } = useT();
  const reduceMotion = useReducedMotion();
  const setNav = useUiStore((s) => s.setNav);
  const closePlaylist = useUiStore((s) => s.closePlaylist);
  const { scrollerRef, innerRef, thumbRef, onScroll } = useElasticScroll();
  const [barVisible, setBarVisible] = useState(false);
  const { filter, query } = useFilterPill();
  const headerInputRef = useRef<HTMLInputElement | null>(null);
  const barInputRef = useRef<HTMLInputElement | null>(null);

  const playing = usePlayerStore((s) => s.playing);
  const current = usePlayerStore((s) =>
    s.currentIndex >= 0 ? s.queue[s.currentIndex]?.track : null,
  );
  const favorites = usePlayerStore((s) => s.favorites);
  const collected = usePlayerStore((s) => !!s.favoritePlaylists[playlistId]);
  const playQueue = usePlayerStore((s) => s.playQueue);
  const togglePlay = usePlayerStore((s) => s.togglePlay);
  const toggleFavorite = usePlayerStore((s) => s.toggleFavorite);
  const toggleFavoritePlaylist = usePlayerStore((s) => s.toggleFavoritePlaylist);
  const enqueueNext = usePlayerStore((s) => s.enqueueNext);
  const { dialog: editDialog, editTrack } = useMetadataEditor();
  const reorderPlaylist = usePlayerStore((s) => s.reorderPlaylist);
  const removeFromPlaylist = usePlayerStore((s) => s.removeFromPlaylist);
  const renamePlaylist = usePlayerStore((s) => s.renamePlaylist);
  const deletePlaylist = usePlayerStore((s) => s.deletePlaylist);
  const [renameOpen, setRenameOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);

  const playlist = usePlayerStore((s) => s.playlists.find((p) => p.id === playlistId));
  const covers = useMemo(() => (playlist ? collageOf(playlist) : []), [playlist]);
  const allTracks = playlist?.tracks ?? [];
  const entries = useMemo(
    () =>
      query
        ? allTracks.filter((tk) =>
            `${tk.title} ${tk.artist} ${tk.album}`.toLowerCase().includes(query),
          )
        : allTracks,
    [allTracks, query],
  );
  // 歌单可以在详情页开着的时候被别处删掉（如「…」菜单的删除后再回退）。
  if (!playlist) return <DetailNotFound backLabel="nav.playlists" onBack={closePlaylist} />;

  const totalSec = allTracks.reduce((s, tk) => s + tk.durationSec, 0);
  const playingThis = playing && allTracks.some((tk) => tk.id === current?.id);

  const onPlayAll = () => {
    if (allTracks.some((tk) => tk.id === current?.id)) togglePlay();
    else playQueue(allTracks, 0);
  };
  const onShuffle = () => playQueue(shuffled(allTracks), 0);
  const onTrackAction = (track: Track, index: number, key: MessageKey, arg?: string) => {
    switch (key) {
      case "menu.addToPlaylist":
        if (arg) addTracksToPlaylistArg(arg, [track], t("playlist.newDefaultName"));
        break;
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
      case "menu.removeFromPlaylist":
        removeFromPlaylist(playlistId, track.id);
        break;
      case "menu.editTags":
        editTrack(track);
        break;
    }
  };
  /** 过滤中列表是子集，重排语义不明确；此时关闭拖拽。 */
  const reorderable = !query;
  const handleScroll = (e: UIEvent<HTMLDivElement>) => {
    onScroll(e);
    const v = e.currentTarget.scrollTop > STICKY_THRESHOLD;
    if (v !== barVisible) setBarVisible(v);
  };

  const meta = t("playlist.meta", {
    n: allTracks.length,
    m: Math.round(totalSec / 60),
    updated: updatedLabelOf(playlist.updatedAtMs, t, locale),
  });

  return (
    <div className="relative min-h-0 flex-1">
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
        {/* 吸顶栏里的返回：页面滚动后面包屑已经不在视野内，没有这个按钮就退不出去 */}
        <button
          aria-label={t("common.back")}
          onClick={closePlaylist}
          className="grid size-[30px] flex-none cursor-pointer place-items-center rounded-full text-tx2 transition-colors hover:bg-hv hover:text-tx"
        >
          <Icon name="chevronLeft" size={15} strokeWidth={2} />
        </button>
        <Collage covers={covers} size={32} radius={8} />
        <div className="relative">
          <span className="font-serif text-[16.5px] font-semibold text-tx">{playlist.title}</span>
          {collected && (
            <span className="absolute -right-[13px] top-px text-ac">
              <Icon name="heart" size={10} />
            </span>
          )}
        </div>
        <FilterPill
          filter={filter}
          height={34}
          openWidth={300}
          inputRef={barInputRef}
          placeholder={t("playlist.filterPlaceholder")}
          className="ml-auto"
        />
        {/* 与头部同一对动作，只是收成图标：滚下去之后大按钮已不在视野，
            随机播放不该因为翻了页就没得点。 */}
        <div className="flex flex-none items-center gap-2.5">
          <motion.button
            aria-label={playingThis ? t("player.pause") : t("player.play")}
            title={playingThis ? t("player.pause") : t("player.play")}
            onClick={onPlayAll}
            disabled={allTracks.length === 0}
            className="play-action-material play-action-compact grid size-[34px] cursor-pointer place-items-center rounded-full text-on-ac disabled:pointer-events-none disabled:opacity-40"
          >
            <PlayPauseIcon playing={playingThis} size={15} />
          </motion.button>
          <button
            aria-label={t("album.shufflePlay")}
            title={t("album.shufflePlay")}
            onClick={onShuffle}
            disabled={allTracks.length === 0}
            className="grid size-[34px] flex-none cursor-pointer place-items-center rounded-full border border-bd bg-srf text-tx transition-colors hover:bg-hv active:scale-95 disabled:pointer-events-none disabled:opacity-40"
          >
            <Icon name="shuffle" size={14} strokeWidth={1.8} />
          </button>
        </div>
      </div>

      <div
        ref={scrollerRef}
        onScroll={handleScroll}
        className="no-scrollbar absolute inset-0 overflow-auto px-10 pb-[120px] [overscroll-behavior:contain]"
      >
        <div ref={innerRef} className="will-change-transform">
          {/* 面包屑返回（兼作窗口拖拽区） */}
          <div data-tauri-drag-region className="flex items-center pt-[22px]">
            <button
              onClick={() => setNav("favorites")}
              className="flex cursor-pointer items-center gap-1.5 rounded-full py-[5px] pl-2 pr-3 text-[12.5px] text-tx2 transition-colors hover:bg-hv hover:text-tx"
            >
              <Icon name="chevronLeft" size={13} strokeWidth={2} />
              {t("nav.favorites")}
            </button>
          </div>

          {/* 歌单头部 */}
          <div className="flex items-center gap-9 pb-[30px] pt-[18px]">
            <div className="group/cover relative flex-shrink-0">
              <Collage covers={covers} size={232} radius={16} glyph={38} className="collage-hero-shadow" />
              <div className="cover-corners absolute inset-0 rounded-2xl opacity-0 transition-opacity duration-[220ms] group-hover/cover:opacity-100">
                <motion.button
                  whileHover={{ scale: 1.1 }}
                  whileTap={{ scale: 0.9 }}
                  title={collected ? t("album.uncollect") : t("album.collect")}
                  aria-label={collected ? t("album.uncollect") : t("album.collect")}
                  onClick={() => toggleFavoritePlaylist(playlist.id)}
                  className="collect-shadow absolute right-3 top-3 grid size-7 cursor-pointer place-items-center rounded-full bg-srf text-ac"
                >
                  <AnimatedIcon
                    name={collected ? "heart" : "favorites"}
                    size={14}
                    strokeWidth={2}
                    variant="pop"
                  />
                </motion.button>
              </div>
            </div>

            <div className="flex min-w-0 flex-1 flex-col gap-2.5">
              <div className="text-[11px] font-bold tracking-[0.16em] text-tx2">
                {t("playlist.kicker")}
              </div>
              <div className="relative self-start">
                <h1 className="m-0 font-serif text-[42px] font-semibold leading-[1.15] text-tx">
                  {playlist.title}
                </h1>
                {collected && (
                  <span title={t("album.collected")} className="absolute -right-5 top-0.5 text-ac">
                    <Icon name="heart" size={14} />
                  </span>
                )}
              </div>
              <div className="max-w-[520px] text-[13.5px] leading-[1.6] text-tx2">
                {playlist.description}
              </div>
              <div className="text-[13px] text-tx2">{meta}</div>
              <div className="mt-2 flex items-center gap-3">
                {/* 空歌单是常态（刚建的还没加歌），两个钮一并置灰，
                    否则点下去是往队列里塞一个空列表。 */}
                <motion.button
                  onClick={onPlayAll}
                  disabled={allTracks.length === 0}
                  className="play-action-material flex cursor-pointer items-center gap-2 rounded-full px-[26px] py-[11px] text-sm font-semibold text-on-ac disabled:pointer-events-none disabled:opacity-40"
                >
                  <PlayPauseIcon playing={playingThis} size={16} />
                  {playingThis ? t("player.pause") : t("player.play")}
                </motion.button>
                <button
                  onClick={onShuffle}
                  disabled={allTracks.length === 0}
                  className="flex cursor-pointer items-center gap-2 rounded-full border border-bd bg-srf px-[22px] py-[11px] text-sm font-semibold text-tx transition-colors hover:bg-hv active:scale-95 disabled:pointer-events-none disabled:opacity-40"
                >
                  <Icon name="shuffle" size={15} strokeWidth={1.8} />
                  {t("album.shufflePlay")}
                </button>
                <PlaylistMoreMenu
                  playlist={playlist}
                  onRename={() => setRenameOpen(true)}
                  onDelete={() => setDeleteOpen(true)}
                />
                <FilterPill
                  filter={filter}
                  height={40}
                  openWidth={318}
                  inputRef={headerInputRef}
                  placeholder={t("playlist.filterPlaceholder")}
                  className="ml-auto"
                />
              </div>
            </div>
          </div>

          {/* 曲目列表 */}
          <div className="border-t border-bd">
            <div className={`grid ${COLS} items-center gap-3 px-3.5 pb-2 pt-2.5 text-[11px] font-semibold tracking-[0.08em] text-tx2`}>
              <span>#</span>
              <span>{t("nav.songs")}</span>
              <span>{t("songs.colArtist")}</span>
              <span>{t("list.album")}</span>
              <span />
              <span className="text-right">{t("list.duration")}</span>
            </div>

            <AnimatePresence initial={false}>
            {entries.length === 0 && (
              <motion.div
                key="playlist-empty"
                className="flex flex-col items-center gap-2.5 py-11 text-center"
                initial={{ opacity: 0, y: reduceMotion ? 0 : 7 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: reduceMotion ? 0 : -5 }}
                transition={{ duration: reduceMotion ? 0.01 : 0.18, ease: [0.22, 1, 0.36, 1] }}
              >
                {/* 「过滤没命中」与「歌单本来就空」是两回事：后者没有过滤词，
                    套用前者的文案会渲染成「歌单里没有“”」，还引导去搜索——
                    而用户此刻需要的是知道怎么往里加歌。 */}
                {allTracks.length === 0 ? (
                  <>
                    <div className="font-serif text-[15px] font-semibold text-tx">
                      {t("playlist.noTracksTitle")}
                    </div>
                    <div className="max-w-[300px] text-[12.5px] leading-[1.6] text-tx2">
                      {t("playlist.noTracksBody")}
                    </div>
                  </>
                ) : (
                  <>
                    <div className="font-serif text-[15px] font-semibold text-tx">
                      {t("playlist.emptyTitle", { q: filter.q.trim() })}
                    </div>
                    <div className="text-[12.5px] text-tx2">
                      {t("playlist.emptyTryGlobal")}
                      <span
                        onClick={() => setNav("albums")}
                        className="cursor-pointer font-semibold text-ac"
                      >
                        {t("playlist.emptyGlobalSearch")}
                      </span>
                    </div>
                  </>
                )}
              </motion.div>
            )}
            </AnimatePresence>

            {/* 过滤中显示的是子集，重排语义不明确 —— 此时退回普通列表、不出现拖拽柄。 */}
            {reorderable ? (
              <Reorder.Group
                as="div"
                axis="y"
                values={entries}
                onReorder={(next) => reorderPlaylist(playlistId, next)}
              >
                {entries.map((track, i) => (
                  <PlaylistRow
                    key={track.id}
                    track={track}
                    index={i}
                    reorderable
                    isCur={current?.id === track.id}
                    playing={playing}
                    liked={!!favorites[track.id]}
                    query={query}
                    onPlay={() => playQueue(entries, i)}
                    onToggleFavorite={() => toggleFavorite(track.id)}
                    onAction={(key, arg) => onTrackAction(track, i, key, arg)}
                  />
                ))}
              </Reorder.Group>
            ) : (
              entries.map((track, i) => (
                <PlaylistRow
                  key={track.id}
                  track={track}
                  index={i}
                  reorderable={false}
                  isCur={current?.id === track.id}
                  playing={playing}
                  liked={!!favorites[track.id]}
                  query={query}
                  onPlay={() => playQueue(entries, i)}
                  onToggleFavorite={() => toggleFavorite(track.id)}
                  onAction={(key, arg) => onTrackAction(track, i, key, arg)}
                />
              ))
            )}
          </div>
        </div>
      </div>

      <div
        ref={thumbRef}
        className="scroll-thumb pointer-events-none absolute right-[5px] top-2 z-20 h-[120px] w-1.5 rounded-[3px] opacity-0"
      />

      <PromptDialog
        open={renameOpen}
        onOpenChange={setRenameOpen}
        title={t("playlist.renameTitle")}
        label={t("playlist.renameLabel")}
        initialValue={playlist.title}
        confirmLabel={t("dialog.save")}
        onConfirm={(title) => renamePlaylist(playlistId, title)}
      />
      <ConfirmDialog
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        title={t("playlist.deleteTitle")}
        description={t("playlist.deleteBody", { title: playlist.title })}
        confirmLabel={t("dialog.delete")}
        onConfirm={() => {
          // 先离开详情页再删除，避免渲染指向已不存在的歌单。
          closePlaylist();
          deletePlaylist(playlistId);
        }}
      />
      {editDialog}
    </div>
  );
}
