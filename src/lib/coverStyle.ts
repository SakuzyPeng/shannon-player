import type { CSSProperties } from "react";
import type { Cover } from "@/types/player";

type CoverGradientStyle = CSSProperties & {
  "--cover-c1": string;
  "--cover-c2": string;
};

export function coverGradientStyle(cover: Cover): CoverGradientStyle {
  return {
    "--cover-c1": cover.gradient[0],
    "--cover-c2": cover.gradient[1],
  };
}
