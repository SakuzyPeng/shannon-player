import { AnimatePresence, motion } from "framer-motion";
import { ItemContextMenu } from "@/components/common/ItemContextMenu";
import { PlayPauseIcon } from "@/components/common/PlayPauseIcon";
import { ALBUM_MENU, tracksOf } from "@/data/library";
import { usePlayerStore } from "@/store/player";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { coverGradientStyle } from "@/lib/coverStyle";
import { addTracksToPlaylistArg } from "@/lib/playlistActions";
import type { MessageKey } from "@/i18n/messages";
import type { Album } from "@/types/player";

function enqueueAlbumNext(album: Album) {
  const { enqueueNext } = usePlayerStore.getState();
  tracksOf(album).slice().reverse().forEach(enqueueNext);
}

function handleAlbumAction(album: Album, key: MessageKey, arg?: string, newPlaylistName?: string) {
  const player = usePlayerStore.getState();
  switch (key) {
    case "menu.addToPlaylist":
      // 整张专辑加入（已在歌单中的曲目按 ID 去重）
      if (arg) addTracksToPlaylistArg(arg, tracksOf(album), newPlaylistName ?? "");
      break;
    case "menu.play":
      player.playQueue(tracksOf(album));
      break;
    case "menu.playNext":
      enqueueAlbumNext(album);
      break;
    case "menu.favorite":
      player.toggleFavoriteAlbum(album.id);
      break;
  }
}

export function AlbumGrid({ albums }: { albums: readonly Album[] }) {
  const { t } = useT();
  const openAlbum = useUiStore((s) => s.openAlbum);
  // auto-fill 随窗口宽度增减列数：1180（设计稿）恰为 4 列，980 下限 3 列，超宽自动加列。
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-x-7 gap-y-8">
      <AnimatePresence initial={false}>
      {albums.map((album) => (
        <motion.div
          key={album.id}
          layout="position"
          exit={{ opacity: 0, scale: 0.96 }}
          transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
        >
          <ItemContextMenu
            label={`${album.title} — ${album.artist}`}
            items={ALBUM_MENU}
            onAction={(key, arg) => handleAlbumAction(album, key, arg, t("playlist.newDefaultName"))}
          >
            <div
              className="relative min-w-0 cursor-pointer hover:z-10"
              onClick={() => openAlbum(album.id)}
            >
              <motion.div
                layoutId={`album-cover-${album.id}`}
                whileHover={{ y: -5 }}
                transition={{ type: "spring", stiffness: 380, damping: 18 }}
                className="cover-corners cover-gradient cover-material group/cover relative grid aspect-square place-items-center rounded-2xl"
                style={coverGradientStyle(album.cover)}
              >
                <span className="cover-initial font-serif text-[56px] font-medium">
                  {album.cover.initial}
                </span>
                {/* hover 浮现播放键（唯一交互入口） */}
                <div
                  className="cover-corners cover-overlay absolute inset-0 flex items-end justify-end rounded-2xl p-3 opacity-0 transition-opacity duration-[220ms] group-hover/cover:opacity-100"
                >
                  <motion.button
                    aria-label={t("action.playAlbum", { title: album.title })}
                    onClick={(e) => {
                      e.stopPropagation();
                      usePlayerStore.getState().playQueue(tracksOf(album));
                    }}
                    className="play-action-material play-action-compact grid size-[38px] place-items-center rounded-full text-on-ac"
                  >
                    <PlayPauseIcon playing={false} size={17} />
                  </motion.button>
                </div>
              </motion.div>
              <div className="mt-3 truncate font-serif text-[15.5px] font-semibold text-tx">
                {album.title}
              </div>
              <div className="mt-[3px] truncate text-[12.5px] text-tx2">
                {album.artist} · {album.year}
              </div>
            </div>
          </ItemContextMenu>
        </motion.div>
      ))}
      </AnimatePresence>
    </div>
  );
}
