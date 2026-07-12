import { ItemContextMenu } from "@/components/common/ItemContextMenu";
import { ALBUM_MENU, tracksOf } from "@/data/library";
import { useUiStore } from "@/store/ui";
import { usePlayerStore } from "@/store/player";
import { useT } from "@/i18n";
import { coverGradientStyle } from "@/lib/coverStyle";
import type { MessageKey } from "@/i18n/messages";
import type { Album } from "@/types/player";

const COLS = "grid-cols-[64px_1fr_200px_70px_90px_90px]";

function handleAlbumAction(album: Album, key: MessageKey) {
  const player = usePlayerStore.getState();
  switch (key) {
    case "menu.play":
      player.playQueue(tracksOf(album));
      break;
    case "menu.playNext":
      tracksOf(album).slice().reverse().forEach(player.enqueueNext);
      break;
    case "menu.favorite":
      player.toggleFavoriteAlbum(album.id);
      break;
  }
}

export function AlbumList({ albums }: { albums: readonly Album[] }) {
  const { t } = useT();
  const openAlbum = useUiStore((s) => s.openAlbum);
  const fmtMinutes = (sec: number) => t("unit.minutes", { n: Math.round(sec / 60) });

  return (
    <div className="surface-corners flex flex-col overflow-hidden rounded-[14px] border border-bd bg-srf">
      <div
        className={`grid ${COLS} items-center gap-3.5 border-b border-bd px-[18px] py-2.5 text-[11px] font-semibold tracking-[0.08em] text-tx2`}
      >
        <span />
        <span>{t("list.album")}</span>
        <span>{t("list.artist")}</span>
        <span>{t("list.year")}</span>
        <span>{t("list.tracks")}</span>
        <span className="text-right">{t("list.duration")}</span>
      </div>
      {albums.map((album) => (
        <ItemContextMenu
          key={album.id}
          label={`${album.title} — ${album.artist}`}
          items={ALBUM_MENU}
          onAction={(key) => handleAlbumAction(album, key)}
        >
          <div
            onClick={() => openAlbum(album.id)}
            className={`grid ${COLS} cursor-pointer items-center gap-3.5 border-b border-bd px-[18px] py-2 transition-colors last:border-b-0 hover:bg-hv`}
          >
            <div
              className="cover-corners cover-gradient cover-thumb-material grid size-11 place-items-center rounded-[9px]"
              style={coverGradientStyle(album.cover)}
            >
              <span className="cover-initial font-serif text-[17px]">
                {album.cover.initial}
              </span>
            </div>
            <span className="truncate font-serif text-[14.5px] font-semibold text-tx">
              {album.title}
            </span>
            <span className="truncate text-[13px] text-tx2">{album.artist}</span>
            <span className="text-[13px] tabular-nums text-tx2">{album.year}</span>
            <span className="text-[13px] tabular-nums text-tx2">{t("unit.tracks", { n: album.trackCount })}</span>
            <span className="text-right text-[13px] tabular-nums text-tx2">
              {fmtMinutes(album.durationSec)}
            </span>
          </div>
        </ItemContextMenu>
      ))}
    </div>
  );
}
