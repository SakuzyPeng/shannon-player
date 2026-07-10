import { coverGradientStyle } from "@/lib/coverStyle";
import type { Cover } from "@/types/player";

interface Props {
  /** 4 张封面（顺序：左上、右上、左下、右下）。 */
  covers: Cover[];
  /** 边长（px）。 */
  size: number;
  /** 圆角（px）。 */
  radius: number;
  /** 首字字号（px）；为 0 则不显示首字（迷你拼贴）。 */
  glyph?: number;
  className?: string;
}

/** 歌单 2×2 拼贴封面：四象限各一张专辑封面渐变。 */
export function Collage({ covers, size, radius, glyph = 0, className }: Props) {
  return (
    <div
      className={`cover-corners grid grid-cols-2 grid-rows-2 overflow-hidden shadow-[inset_0_0_0_1px_var(--cover-hairline)] ${className ?? ""}`}
      style={{ width: size, height: size, borderRadius: radius }}
    >
      {covers.slice(0, 4).map((cover, i) => (
        <div key={i} className="cover-gradient grid place-items-center" style={coverGradientStyle(cover)}>
          {glyph > 0 && (
            <span className="cover-initial font-serif" style={{ fontSize: glyph }}>
              {cover.initial}
            </span>
          )}
        </div>
      ))}
    </div>
  );
}
