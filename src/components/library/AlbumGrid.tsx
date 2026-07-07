import { motion } from "framer-motion";
import { Icon } from "@/components/common/Icon";
import { AlbumContextMenu } from "@/components/library/AlbumContextMenu";
import { ALBUMS } from "@/data/library";
import { usePlayerStore } from "@/store/player";
import { useT } from "@/i18n";
import type { Album } from "@/types/player";

function playAlbum(album: Album) {
  usePlayerStore.getState().play({
    id: `${album.id}-t0`,
    title: album.title,
    artist: album.artist,
    album: album.title,
    albumId: album.id,
    cover: album.cover,
    durationSec: 240,
    favorited: false,
  });
}

export function AlbumGrid() {
  const { t } = useT();
  return (
    <div className="grid grid-cols-4 gap-x-7 gap-y-8">
      {ALBUMS.map((album) => (
        <AlbumContextMenu key={album.id} album={album}>
          <div className="min-w-0 cursor-pointer">
            <motion.div
              whileHover={{ y: -5, scale: 1.015 }}
              transition={{ type: "spring", stiffness: 380, damping: 18 }}
              className="cover-material group/cover relative grid aspect-square place-items-center rounded-2xl"
              style={{
                backgroundImage: `linear-gradient(145deg, ${album.cover.gradient[0]}, ${album.cover.gradient[1]})`,
              }}
            >
              <span className="font-serif text-[56px] font-medium text-[rgba(253,248,240,0.9)]">
                {album.cover.initial}
              </span>
              {/* hover 浮现播放键（唯一交互入口） */}
              <div
                className="absolute inset-0 flex items-end justify-end rounded-2xl p-3 opacity-0 transition-opacity duration-[220ms] group-hover/cover:opacity-100"
                style={{ background: "linear-gradient(to top, rgba(30,18,8,0.42), transparent 45%)" }}
              >
                <motion.button
                  aria-label={t("action.playAlbum", { title: album.title })}
                  whileHover={{ scale: 1.08 }}
                  whileTap={{ scale: 0.9 }}
                  onClick={(e) => {
                    e.stopPropagation();
                    playAlbum(album);
                  }}
                  className="grid size-[38px] place-items-center rounded-full bg-ac text-[#FFF9F0] shadow-[0_6px_16px_rgba(30,18,8,0.35)]"
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
        </AlbumContextMenu>
      ))}
    </div>
  );
}
