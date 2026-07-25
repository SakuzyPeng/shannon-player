import * as Dialog from "@radix-ui/react-dialog";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";

/**
 * 模态基座：遮罩 + 居中面板，沿用菜单的表面材质（surface-corners / menu-shadow）
 * 与 spring 弹出曲线。Esc 与点遮罩关闭由 Radix 提供，遵从减少动态效果偏好。
 */
function ModalShell({
  open,
  onOpenChange,
  title,
  description,
  children,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="animate-dialog-overlay fixed inset-0 z-50 bg-[var(--cover-overlay)]" />
        <Dialog.Content className="surface-corners animate-dialog menu-shadow fixed left-1/2 top-1/2 z-50 w-[min(400px,calc(100vw-64px))] -translate-x-1/2 -translate-y-1/2 rounded-[18px] border border-bd bg-srf p-5">
          <Dialog.Title className="m-0 font-serif text-[17px] font-semibold text-tx">
            {title}
          </Dialog.Title>
          {description && (
            <Dialog.Description className="mb-0 mt-1.5 text-[12.5px] leading-relaxed text-tx2">
              {description}
            </Dialog.Description>
          )}
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

const BTN =
  "flex-none cursor-pointer whitespace-nowrap rounded-full px-[15px] py-[7px] text-[12.5px] font-semibold transition-colors";

/** 取消 / 确认按钮行。 */
function Actions({
  onCancel,
  confirmLabel,
  onConfirm,
  confirmDisabled,
  danger,
}: {
  onCancel: () => void;
  confirmLabel: string;
  onConfirm: () => void;
  confirmDisabled?: boolean;
  danger?: boolean;
}) {
  const { t } = useT();
  return (
    <div className="mt-4 flex items-center justify-end gap-2">
      <button
        type="button"
        onClick={onCancel}
        className={cn(BTN, "border border-bd bg-srf text-tx hover:bg-hv")}
      >
        {t("dialog.cancel")}
      </button>
      <button
        type="submit"
        onClick={onConfirm}
        disabled={confirmDisabled}
        className={cn(
          BTN,
          "text-on-ac transition-[filter] hover:brightness-[1.08] disabled:cursor-not-allowed disabled:opacity-45",
          danger ? "bg-danger" : "bg-ac",
        )}
      >
        {confirmLabel}
      </button>
    </div>
  );
}

/** 文本输入对话框（重命名等）：打开时自动聚焦并全选，回车提交，空值不可提交。 */
export function PromptDialog({
  open,
  onOpenChange,
  title,
  label,
  initialValue,
  confirmLabel,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  label?: string;
  initialValue: string;
  confirmLabel: string;
  onConfirm: (value: string) => void;
}) {
  const [value, setValue] = useState(initialValue);
  const inputRef = useRef<HTMLInputElement>(null);

  // 每次打开都以当前值为起点（上次编辑的残留值不应带入）。
  useEffect(() => {
    if (open) setValue(initialValue);
  }, [open, initialValue]);

  const trimmed = value.trim();
  const submit = () => {
    if (!trimmed) return;
    onConfirm(trimmed);
    onOpenChange(false);
  };

  return (
    <ModalShell open={open} onOpenChange={onOpenChange} title={title} description={label}>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <input
          ref={inputRef}
          autoFocus
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onFocus={(e) => e.currentTarget.select()}
          className="mt-3.5 w-full rounded-[11px] border border-bd bg-bg px-3 py-2 text-[13.5px] text-tx outline-none focus:border-ac"
        />
        <Actions
          onCancel={() => onOpenChange(false)}
          confirmLabel={confirmLabel}
          onConfirm={submit}
          confirmDisabled={!trimmed}
        />
      </form>
    </ModalShell>
  );
}

/** 破坏性操作二次确认（删除歌单等）：确认键用 --danger。 */
export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  confirmLabel,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description: string;
  confirmLabel: string;
  onConfirm: () => void;
}) {
  return (
    <ModalShell open={open} onOpenChange={onOpenChange} title={title} description={description}>
      <Actions
        onCancel={() => onOpenChange(false)}
        confirmLabel={confirmLabel}
        onConfirm={() => {
          onConfirm();
          onOpenChange(false);
        }}
        danger
      />
    </ModalShell>
  );
}
