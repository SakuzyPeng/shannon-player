import * as ContextMenu from "@radix-ui/react-context-menu";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import type { ComponentType, ReactNode } from "react";
import type { AlbumMenuItem } from "@/data/library";
import { Icon } from "@/components/common/Icon";
import { usePlayerStore } from "@/store/player";
import { useT } from "@/i18n";
import type { MessageKey } from "@/i18n/messages";
import { cn } from "@/lib/cn";
import { NEW_PLAYLIST } from "@/lib/playlistActions";
import type { Id, Playlist } from "@/types/player";
import type { TFunction } from "@/i18n";

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
const CONTENT_CLS =
  "surface-corners animate-menu-pop menu-shadow z-40 w-[222px] rounded-[14px] border border-bd bg-srf p-1.5";
const SUB_CONTENT_CLS =
  "surface-corners animate-menu-pop menu-shadow z-40 w-[200px] origin-top-left rounded-[14px] border border-bd bg-srf p-1.5";

/**
 * 右键菜单与「…」下拉共用同一套菜单项渲染。
 *
 * Radix 的 ContextMenu 与 DropdownMenu 是两套独立 primitive，但导出的成员同名、
 * 用到的这几个 props 也一致，因此把命名空间当参数传入即可复用整段渲染——
 * 两个入口的内容天然保持一致（「…」本就是右键菜单的鼠标可点对应物）。
 */
interface MenuParts {
  Item: ComponentType<Record<string, unknown>>;
  Separator: ComponentType<Record<string, unknown>>;
  Sub: ComponentType<Record<string, unknown>>;
  SubTrigger: ComponentType<Record<string, unknown>>;
  SubContent: ComponentType<Record<string, unknown>>;
  Portal: ComponentType<Record<string, unknown>>;
}

function renderItems(
  P: MenuParts,
  {
    items,
    t,
    playlists,
    onAction,
    containsTrackId,
  }: {
    items: AlbumMenuItem[];
    t: TFunction;
    playlists: Playlist[];
    onAction?: (key: MessageKey, arg?: string) => void;
    containsTrackId?: Id;
  },
) {
  return items.map((mi, i) => {
    if (mi.kind === "separator") {
      return <P.Separator key={i} className="mx-2 my-[5px] h-px bg-bd" />;
    }
    // 「加入歌单」渲染为子菜单：列出全部歌单 + 新建
    if (mi.labelKey === "menu.addToPlaylist") {
      return (
        <P.Sub key={i}>
          <P.SubTrigger className={cn(ITEM_CLS, "text-tx")}>
            <span>{t(mi.labelKey)}</span>
            <Icon name="chevronRight" size={12} strokeWidth={2} className="text-tx2" />
          </P.SubTrigger>
          <P.Portal>
            <P.SubContent sideOffset={4} className={SUB_CONTENT_CLS}>
              {playlists.map((pl) => {
                const contains =
                  containsTrackId != null && pl.tracks.some((tk) => tk.id === containsTrackId);
                return (
                  <P.Item
                    key={pl.id}
                    onSelect={() => onAction?.("menu.addToPlaylist", pl.id)}
                    className={cn(ITEM_CLS, "text-tx")}
                  >
                    <span className="truncate">{pl.title}</span>
                    {contains && (
                      <Icon
                        name="check"
                        size={13}
                        strokeWidth={2.4}
                        className="flex-shrink-0 text-ac"
                      />
                    )}
                  </P.Item>
                );
              })}
              <P.Separator className="mx-2 my-[5px] h-px bg-bd" />
              <P.Item
                onSelect={() => onAction?.("menu.addToPlaylist", NEW_PLAYLIST)}
                className={cn(ITEM_CLS, "font-semibold text-ac")}
              >
                <span>{t("menu.newPlaylist")}</span>
              </P.Item>
            </P.SubContent>
          </P.Portal>
        </P.Sub>
      );
    }
    return (
      <P.Item
        key={i}
        onSelect={() => onAction?.(mi.labelKey!)}
        className={cn(ITEM_CLS, mi.danger ? "text-danger" : "text-tx")}
      >
        <span>{t(mi.labelKey!)}</span>
        {mi.kbd && <span className="text-[11px] text-tx2 opacity-85">{mi.kbd}</span>}
      </P.Item>
    );
  });
}

/** 菜单顶部的对象标题行。 */
function MenuLabel({ label }: { label: string }) {
  return (
    <div className="mb-[5px] truncate border-b border-bd px-2.5 pb-2 pt-[7px] font-serif text-[12px] text-tx2">
      {label}
    </div>
  );
}

/** 右键菜单：222px、role=menu、方向键循环 + Enter/Esc（由 Radix 提供），危险项用 --danger。 */
export function ItemContextMenu({ label, items, children, onAction, containsTrackId }: Props) {
  const { t } = useT();
  const playlists = usePlayerStore((s) => s.playlists);
  return (
    <ContextMenu.Root>
      <ContextMenu.Trigger asChild>{children}</ContextMenu.Trigger>
      <ContextMenu.Portal>
        <ContextMenu.Content className={cn(CONTENT_CLS, "origin-top-left")}>
          <MenuLabel label={label} />
          {renderItems(ContextMenu as unknown as MenuParts, {
            items,
            t,
            playlists,
            onAction,
            containsTrackId,
          })}
        </ContextMenu.Content>
      </ContextMenu.Portal>
    </ContextMenu.Root>
  );
}

/**
 * 「…」按钮版：内容与右键菜单完全一致（右键是隐藏交互，这里是它的显式入口）。
 * 用于专辑详情页——专辑属曲库内容，可做的动作就是 ALBUM_MENU 那些，不额外发明。
 */
export function ItemMoreMenu({ label, items, children, onAction, containsTrackId }: Props) {
  const { t } = useT();
  const playlists = usePlayerStore((s) => s.playlists);
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>{children}</DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          align="start"
          sideOffset={8}
          className={cn(CONTENT_CLS, "z-50 origin-top-left")}
        >
          <MenuLabel label={label} />
          {renderItems(DropdownMenu as unknown as MenuParts, {
            items,
            t,
            playlists,
            onAction,
            containsTrackId,
          })}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}
