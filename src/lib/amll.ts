/** 领域歌词模型 → AMLL 核心 LyricLine 的转换。 */

import type { LyricLine as AmllLyricLine } from "@applemusic-like-lyrics/core";
import type { Lyrics } from "@/types/player";

interface Options {
  /** 是否携带译文（歌词页「译」开关）。 */
  withTranslation?: boolean;
}

export function toAmllLines(lyrics: Lyrics, opts: Options = {}): AmllLyricLine[] {
  return lyrics.lines.map((ln, i) => {
    const endTime = ln.endMs ?? lyrics.lines[i + 1]?.timeMs ?? ln.timeMs + 5000;
    const words =
      ln.words?.map((w) => ({ word: w.text, startTime: w.startMs, endTime: w.endMs })) ?? [
        { word: ln.text, startTime: ln.timeMs, endTime },
      ];
    return {
      words,
      startTime: ln.timeMs,
      endTime,
      translatedLyric: opts.withTranslation ? (ln.translation ?? "") : "",
      romanLyric: ln.romaji ?? "",
      isBG: false,
      isDuet: false,
    };
  });
}
