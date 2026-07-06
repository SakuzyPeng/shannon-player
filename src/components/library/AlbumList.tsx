import { AlbumContextMenu } from "@/components/library/AlbumContextMenu";
import { ALBUMS } from "@/data/library";
import { useT } from "@/i18n";

const COLS = "grid-cols-[64px_1fr_200px_70px_90px_90px]";

export function AlbumList() {
  const { t } = useT();
  const fmtMinutes = (sec: number) => t("unit.minutes", { n: Math.round(sec / 60) });

  return (
    <div className="flex flex-col overflow-hidden rounded-[14px] border border-bd bg-srf">
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
      {ALBUMS.map((album) => (
        <AlbumContextMenu key={album.id} album={album}>
          <div
            className={`grid ${COLS} cursor-pointer items-center gap-3.5 border-b border-bd px-[18px] py-2 transition-colors last:border-b-0 hover:bg-hv`}
          >
            <div
              className="grid size-11 place-items-center rounded-[9px] shadow-[inset_0_0_0_1px_rgba(38,22,8,0.16)]"
              style={{
                backgroundImage: `linear-gradient(145deg, ${album.cover.gradient[0]}, ${album.cover.gradient[1]})`,
              }}
            >
              <span className="font-serif text-[17px] text-[rgba(253,248,240,0.9)]">
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
        </AlbumContextMenu>
      ))}
    </div>
  );
}
