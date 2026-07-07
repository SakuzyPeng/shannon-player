import { motion } from "framer-motion";
import { Icon } from "@/components/common/Icon";
import { ItemContextMenu } from "@/components/common/ItemContextMenu";
import { ALBUMS, ALBUM_MENU, tracksOf } from "@/data/library";
import { usePlayerStore } from "@/store/player";
import { useUiStore } from "@/store/ui";
import { useT } from "@/i18n";
import { coverGradientStyle } from "@/lib/coverStyle";
import type { MessageKey } from "@/i18n/messages";
import type { Album } from "@/types/player";

function enqueueAlbumNext(album: Album) {
  const { enqueueNext } = usePlayerStore.getState();
  tracksOf(album).slice().reverse().forEach(enqueueNext);
}

function handleAlbumAction(album: Album, key: MessageKey) {
  const player = usePlayerStore.getState();
  switch (key) {
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

export function AlbumGrid() {
  const { t } = useT();
  const openAlbum = useUiStore((s) => s.openAlbum);
  return (
    <div className="grid grid-cols-4 gap-x-7 gap-y-8">
      {ALBUMS.map((album) => (
        <ItemContextMenu
          key={album.id}
          label={`${album.title} — ${album.artist}`}
          items={ALBUM_MENU}
          onAction={(key) => handleAlbumAction(album, key)}
        >
          <div className="min-w-0 cursor-pointer" onClick={() => openAlbum(album.id)}>
            <motion.div
              whileHover={{ y: -5, scale: 1.015 }}
              transition={{ type: "spring", stiffness: 380, damping: 18 }}
              className="cover-gradient cover-material group/cover relative grid aspect-square place-items-center rounded-2xl"
              style={coverGradientStyle(album.cover)}
            >
              <span className="cover-initial font-serif text-[56px] font-medium">
                {album.cover.initial}
              </span>
              {/* hover 浮现播放键（唯一交互入口） */}
              <div
                className="cover-overlay absolute inset-0 flex items-end justify-end rounded-2xl p-3 opacity-0 transition-opacity duration-[220ms] group-hover/cover:opacity-100"
              >
                <motion.button
                  aria-label={t("action.playAlbum", { title: album.title })}
                  whileHover={{ scale: 1.08 }}
                  whileTap={{ scale: 0.9 }}
                  onClick={(e) => {
                    e.stopPropagation();
                    usePlayerStore.getState().playQueue(tracksOf(album));
                  }}
                  className="cover-action-shadow grid size-[38px] place-items-center rounded-full bg-ac text-on-ac"
                >
                  <Icon name="play" size={15} />
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
      ))}
    </div>
  );
}
