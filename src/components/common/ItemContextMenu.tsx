import * as ContextMenu from "@radix-ui/react-context-menu";
import type { ReactNode } from "react";
import type { AlbumMenuItem } from "@/data/library";
import { useT } from "@/i18n";
import type { MessageKey } from "@/i18n/messages";
import { cn } from "@/lib/cn";

interface Props {
  /** 标题行内容（专辑/曲目名 — 歌手，属内容不进 i18n）。 */
  label: string;
  items: AlbumMenuItem[];
  children: ReactNode;
  onAction?: (key: MessageKey) => void;
}

/** 右键菜单：222px、role=menu、方向键循环 + Enter/Esc（由 Radix 提供），危险项用 --danger。 */
export function ItemContextMenu({ label, items, children, onAction }: Props) {
  const { t } = useT();
  return (
    <ContextMenu.Root>
      <ContextMenu.Trigger asChild>{children}</ContextMenu.Trigger>
      <ContextMenu.Portal>
        <ContextMenu.Content className="surface-corners animate-menu-pop menu-shadow z-40 w-[222px] origin-top-left rounded-[14px] border border-bd bg-srf p-1.5">
          <div className="mb-[5px] truncate border-b border-bd px-2.5 pb-2 pt-[7px] font-serif text-[12px] text-tx2">
            {label}
          </div>
          {items.map((mi, i) =>
            mi.kind === "separator" ? (
              <ContextMenu.Separator key={i} className="mx-2 my-[5px] h-px bg-bd" />
            ) : (
              <ContextMenu.Item
                key={i}
                onSelect={() => onAction?.(mi.labelKey!)}
                className={cn(
                  "flex cursor-pointer items-center justify-between gap-3 rounded-lg px-2.5 py-[7px] text-[13px] outline-none data-[highlighted]:bg-hv",
                  mi.danger ? "text-danger" : "text-tx",
                )}
              >
                <span>{t(mi.labelKey!)}</span>
                {mi.kbd && <span className="text-[11px] text-tx2 opacity-85">{mi.kbd}</span>}
              </ContextMenu.Item>
            ),
          )}
        </ContextMenu.Content>
      </ContextMenu.Portal>
    </ContextMenu.Root>
  );
}
