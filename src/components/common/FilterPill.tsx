import { useRef, useState, type RefObject } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { Icon } from "@/components/common/Icon";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";

/** 过滤圆钮的受控状态 + 交互回调。由 useFilterPill 提供，FilterPill 消费。 */
export interface FilterState {
  open: boolean;
  focused: boolean;
  q: string;
  onEnter: (input: HTMLInputElement | null) => void;
  onLeave: () => void;
  onChange: (v: string) => void;
  onFocus: () => void;
  onBlur: () => void;
  onEscape: () => void;
  onClear: () => void;
}

/**
 * 过滤圆钮的状态机（曲库/歌单/搜索共用）：hover 展开、聚焦保持、失焦或清空收起。
 * 返回受控 state 与当前过滤词（trim + lowercase，未展开时为空）。
 */
export function useFilterPill() {
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const [focused, setFocused] = useState(false);
  const focusTimer = useRef(0);

  const filter: FilterState = {
    open,
    focused,
    q,
    onEnter: (input) => {
      setOpen(true);
      window.clearTimeout(focusTimer.current);
      focusTimer.current = window.setTimeout(() => input?.focus({ preventScroll: true }), 200);
    },
    onLeave: () => {
      if (!focused && !q.trim()) setOpen(false);
    },
    onChange: setQ,
    onFocus: () => setFocused(true),
    onBlur: () => {
      setFocused(false);
      if (!q.trim()) setOpen(false);
    },
    onEscape: () => {
      setOpen(false);
      setQ("");
      setFocused(false);
    },
    onClear: () => {
      setOpen(false);
      setQ("");
      setFocused(false);
    },
  };

  const query = open ? q.trim().toLowerCase() : "";
  return { filter, query, rawQuery: q };
}

/** 展开态被压缩时的下限：低于此宽度输入框已不可用，宁可让头部横向留白告急。 */
const MIN_OPEN_WIDTH = 132;

interface Props {
  filter: FilterState;
  /** 收起时的圆钮尺寸（= 高度）；展开宽度。设计稿：标题栏 40→318、吸顶栏 34→300。 */
  height: number;
  openWidth: number;
  inputRef: RefObject<HTMLInputElement | null>;
  placeholder: string;
  className?: string;
}

/** 过滤圆钮：hover 从圆形展开为输入框，聚焦强调圈，命中清空按钮。 */
export function FilterPill({ filter, height, openWidth, inputRef, placeholder, className }: Props) {
  const { t } = useT();
  const reduceMotion = useReducedMotion();
  return (
    <div
      onMouseEnter={() => filter.onEnter(inputRef.current)}
      onMouseLeave={filter.onLeave}
      onClick={() => filter.onEnter(inputRef.current)}
      className={cn(
        "box-border flex items-center gap-2 overflow-hidden rounded-full border bg-srf",
        // 收起时是固定尺寸的圆钮（压缩会挤成椭圆）；展开后作为头部唯一可收缩项，
        // 窗口变窄时由它让位，避免排序钮等控件被挤到换行或整行横向溢出。
        filter.open ? "shrink" : "flex-none",
        filter.focused ? "filter-focus-ring border-ac" : "border-bd",
        className,
      )}
      style={{
        height,
        width: filter.open ? openWidth : height,
        minWidth: filter.open ? MIN_OPEN_WIDTH : height,
        paddingLeft: 10,
        paddingRight: 7,
        transition: "width 0.3s cubic-bezier(0.34,1.3,0.64,1), border-color 0.2s ease",
      }}
    >
      <span
        className="flex-shrink-0 text-tx2"
        style={!filter.open && height >= 40 ? { transform: "translate(2px, 1px)" } : undefined}
      >
        <Icon name="search" size={height >= 40 ? 15 : 14} />
      </span>
      <input
        ref={inputRef}
        value={filter.q}
        onChange={(e) => filter.onChange(e.target.value)}
        onFocus={filter.onFocus}
        onBlur={filter.onBlur}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            filter.onEscape();
            e.currentTarget.blur();
          }
        }}
        placeholder={placeholder}
        className="min-w-0 flex-1 border-none bg-transparent text-[12.5px] text-tx outline-none"
        style={{ opacity: filter.open ? 1 : 0, transition: "opacity 0.2s ease" }}
      />
      <AnimatePresence initial={false}>
        {filter.q.trim().length > 0 && (
        <motion.button
          key="clear"
          title={t("songs.filterClear")}
          aria-label={t("songs.filterClear")}
          onMouseDown={(e) => e.preventDefault()}
          onClick={(e) => {
            e.stopPropagation();
            filter.onClear();
          }}
          className="grid h-5 flex-shrink-0 cursor-pointer place-items-center overflow-hidden rounded-full bg-hv text-tx2 hover:text-tx"
          initial={{ opacity: 0, width: 0, scale: reduceMotion ? 1 : 0.65 }}
          animate={{ opacity: 1, width: 20, scale: 1 }}
          exit={{ opacity: 0, width: 0, scale: reduceMotion ? 1 : 0.65 }}
          transition={{ duration: reduceMotion ? 0.01 : 0.16, ease: [0.22, 1, 0.36, 1] }}
        >
          <Icon name="close" size={9} strokeWidth={2.6} />
        </motion.button>
        )}
      </AnimatePresence>
    </div>
  );
}
