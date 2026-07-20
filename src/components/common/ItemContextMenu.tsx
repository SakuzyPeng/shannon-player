import * as ContextMenu from "@radix-ui/react-context-menu";
import type { ReactNode } from "react";
import type { AlbumMenuItem } from "@/data/library";
import { Icon } from "@/components/common/Icon";
import { usePlayerStore } from "@/store/player";
import { useT } from "@/i18n";
import type { MessageKey } from "@/i18n/messages";
import { cn } from "@/lib/cn";
import { NEW_PLAYLIST } from "@/lib/playlistActions";
import type { Id } from "@/types/player";

interface Props {
  /** 标题行内容（专辑/曲目名 — 歌手，属内容不进 i18n）。 */
  label: string;
  items: AlbumMenuItem[];
  children: ReactNode;
  /** arg：menu.addToPlaylist 时为歌单 ID 或 NEW_PLAYLIST。 */
  onAction?: (key: MessageKey, arg?: string) => void;
  /** 用于「已在歌单中」勾选标记（单曲菜单传曲目 ID；专辑菜单不传则不显示勾）。 */
  containsTrackId?: Id;
}

const ITEM_CLS =
  "flex cursor-pointer items-center justify-between gap-3 rounded-lg px-2.5 py-[7px] text-[13px] outline-none data-[highlighted]:bg-hv";

/** 右键菜单：222px、role=menu、方向键循环 + Enter/Esc（由 Radix 提供），危险项用 --danger。 */
export function ItemContextMenu({ label, items, children, onAction, containsTrackId }: Props) {
  const { t } = useT();
  const playlists = usePlayerStore((s) => s.playlists);
  return (
    <ContextMenu.Root>
      <ContextMenu.Trigger asChild>{children}</ContextMenu.Trigger>
      <ContextMenu.Portal>
        <ContextMenu.Content className="surface-corners animate-menu-pop menu-shadow z-40 w-[222px] origin-top-left rounded-[14px] border border-bd bg-srf p-1.5">
          <div className="mb-[5px] truncate border-b border-bd px-2.5 pb-2 pt-[7px] font-serif text-[12px] text-tx2">
            {label}
          </div>
          {items.map((mi, i) => {
            if (mi.kind === "separator") {
              return <ContextMenu.Separator key={i} className="mx-2 my-[5px] h-px bg-bd" />;
            }
            // 「添加到歌单」渲染为子菜单：列出全部歌单 + 新建
            if (mi.labelKey === "menu.addToPlaylist") {
              return (
                <ContextMenu.Sub key={i}>
                  <ContextMenu.SubTrigger className={cn(ITEM_CLS, "text-tx")}>
                    <span>{t(mi.labelKey)}</span>
                    <Icon name="chevronRight" size={12} strokeWidth={2} className="text-tx2" />
                  </ContextMenu.SubTrigger>
                  <ContextMenu.Portal>
                    <ContextMenu.SubContent
                      sideOffset={4}
                      className="surface-corners animate-menu-pop menu-shadow z-40 w-[200px] origin-top-left rounded-[14px] border border-bd bg-srf p-1.5"
                    >
                      {playlists.map((pl) => {
                        const contains =
                          containsTrackId != null &&
                          pl.tracks.some((tk) => tk.id === containsTrackId);
                        return (
                          <ContextMenu.Item
                            key={pl.id}
                            onSelect={() => onAction?.("menu.addToPlaylist", pl.id)}
                            className={cn(ITEM_CLS, "text-tx")}
                          >
                            <span className="truncate">{pl.title}</span>
                            {contains && (
                              <Icon name="check" size={13} strokeWidth={2.4} className="flex-shrink-0 text-ac" />
                            )}
                          </ContextMenu.Item>
                        );
                      })}
                      <ContextMenu.Separator className="mx-2 my-[5px] h-px bg-bd" />
                      <ContextMenu.Item
                        onSelect={() => onAction?.("menu.addToPlaylist", NEW_PLAYLIST)}
                        className={cn(ITEM_CLS, "font-semibold text-ac")}
                      >
                        <span>{t("menu.newPlaylist")}</span>
                      </ContextMenu.Item>
                    </ContextMenu.SubContent>
                  </ContextMenu.Portal>
                </ContextMenu.Sub>
              );
            }
            return (
              <ContextMenu.Item
                key={i}
                onSelect={() => onAction?.(mi.labelKey!)}
                className={cn(ITEM_CLS, mi.danger ? "text-danger" : "text-tx")}
              >
                <span>{t(mi.labelKey!)}</span>
                {mi.kbd && <span className="text-[11px] text-tx2 opacity-85">{mi.kbd}</span>}
              </ContextMenu.Item>
            );
          })}
        </ContextMenu.Content>
      </ContextMenu.Portal>
    </ContextMenu.Root>
  );
}
