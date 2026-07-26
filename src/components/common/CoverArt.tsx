import { useState, type CSSProperties } from "react";
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
 * `corner-shape` 也必须显式继承：`.cover-corners` 把封面卡设成了 superellipse
 * 连续圆角，而 `corner-shape` 不是可继承属性。只继承半径的话，图片是标准圆角、
 * 容器是 superellipse，图片在四角比容器少盖住一圈，底下的深色占位渐变就露出来，
 * 浅色封面上看着像四道黑边。显式 `inherit` 让两者形状一致；歌手头像那种没有
 * `.cover-corners` 的圆形容器则继承到默认的 `round`，不会被误变成方角。
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
      // 加载完成后底层不再需要占位渐变，见 index.css 的 .cover-gradient:has(...)：
      // 万一圆角形状仍有细微差异，露出的也不会是深色色块。
      data-cover-loaded={loaded ? "true" : undefined}
      // corner-shape 尚未进 CSSProperties 类型定义，故断言。
      style={{ borderRadius: "inherit", cornerShape: "inherit" } as CSSProperties}
      className={cn(
        "pointer-events-none absolute inset-0 h-full w-full object-cover transition-opacity duration-200",
        loaded ? "opacity-100" : "opacity-0",
        className,
      )}
    />
  );
}
