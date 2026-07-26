import { useState } from "react";
import { coverSrc } from "@/lib/cover";
import { cn } from "@/lib/cn";
import { useLibraryStore } from "@/store/library";
import type { Cover } from "@/types/player";

/**
 * 真实封面图层。
 *
 * 叠在现有的「占位渐变 + 首字母」之上，而不是取代它：渐变始终铺底，图加载完成后
 * 淡入覆盖。这样无封面、文件缺失、加载中三种情况都自动回落到占位，调用点不必写
 * 任何错误分支。
 *
 * 圆角用 `border-radius: inherit` 跟随父容器——各处封面的圆角半径不同
 * （列表 9px、网格 2xl、歌词页 20px），继承比逐处传参可靠。
 *
 * 用法：放进已有的封面 `div` 内、首字母之后（后出现的绝对定位元素在上层），
 * 父容器需要 `relative`。
 */
export function CoverArt({
  cover,
  px,
  className,
}: {
  cover: Cover;
  /** 该处封面的 CSS 显示边长，用于挑缩略图档位。 */
  px: number;
  className?: string;
}) {
  const dir = useLibraryStore((s) => s.coverDir);
  const [loaded, setLoaded] = useState(false);
  const src = coverSrc(cover, dir, px);
  if (!src) return null;
  return (
    <img
      src={src}
      alt=""
      aria-hidden
      loading="lazy"
      decoding="async"
      draggable={false}
      onLoad={() => setLoaded(true)}
      style={{ borderRadius: "inherit" }}
      className={cn(
        "pointer-events-none absolute inset-0 h-full w-full object-cover transition-opacity duration-200",
        loaded ? "opacity-100" : "opacity-0",
        className,
      )}
    />
  );
}
