import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { Collage } from "@/components/common/Collage";
import { Icon } from "@/components/common/Icon";
import { collageOf } from "@/data/playlists";
import { usePlayerStore } from "@/store/player";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";
import { NEW_PLAYLIST, addTracksToPlaylistArg } from "@/lib/playlistActions";
import type { Track } from "@/types/player";

const ITEM_CLS =
  "flex cursor-pointer items-center gap-2.5 rounded-lg px-2.5 py-[7px] text-[13px] outline-none data-[highlighted]:bg-hv";

/** 播放条「加入歌单」菜单：列出全部歌单（迷你拼贴 + 曲数 + 已含勾）与新建入口，作用于当前曲目。 */
export function AddToPlaylistMenu({ track }: { track: Track }) {
  const { t } = useT();
  const playlists = usePlayerStore((s) => s.playlists);

  const onPick = (arg: string) => {
    addTracksToPlaylistArg(arg, [track], t("playlist.newDefaultName"));
  };

  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <button
          aria-label={t("player.addToPlaylist")}
          title={t("player.addToPlaylist")}
          className="grid size-[30px] cursor-pointer place-items-center rounded-full text-tx2 transition-colors hover:bg-hv hover:text-tx data-[state=open]:bg-hv data-[state=open]:text-ac"
        >
          <Icon name="addPlaylist" size={16} strokeWidth={1.8} />
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          side="top"
          align="end"
          sideOffset={10}
          aria-label={t("player.addToPlaylist")}
          className="surface-corners animate-menu-pop menu-shadow z-50 w-[240px] origin-bottom-right rounded-[14px] border border-bd bg-srf p-1.5"
        >
          <div className="mb-[5px] truncate border-b border-bd px-2.5 pb-2 pt-[7px] font-serif text-[12px] text-tx2">
            {track.title} — {track.artist}
          </div>
          {playlists.map((pl) => {
            const contains = pl.tracks.some((tk) => tk.id === track.id);
            return (
              <DropdownMenu.Item
                key={pl.id}
                onSelect={() => onPick(pl.id)}
                className={cn(ITEM_CLS, "text-tx")}
              >
                <Collage covers={collageOf(pl)} size={26} radius={6} />
                <span className="min-w-0 flex-1 truncate">{pl.title}</span>
                {contains ? (
                  <Icon name="check" size={13} strokeWidth={2.4} className="flex-shrink-0 text-ac" />
                ) : (
                  <span className="flex-shrink-0 text-[11px] tabular-nums text-tx2">
                    {t("unit.tracks", { n: pl.tracks.length })}
                  </span>
                )}
              </DropdownMenu.Item>
            );
          })}
          <DropdownMenu.Separator className="mx-2 my-[5px] h-px bg-bd" />
          <DropdownMenu.Item
            onSelect={() => onPick(NEW_PLAYLIST)}
            className={cn(ITEM_CLS, "font-semibold text-ac")}
          >
            <span className="grid size-[26px] flex-shrink-0 place-items-center rounded-md border border-bd bg-sb">
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round">
                <path d="M12 5v14 M5 12h14" />
              </svg>
            </span>
            <span>{t("menu.newPlaylist")}</span>
          </DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}
