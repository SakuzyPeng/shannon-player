/* ============================================================
   香农播放器 · 歌词种子数据
   示例歌词为虚构歌曲「午夜环线」（白鲸电台 — 长夜电波）的原创文本，
   避免版权内容；逐字时间轴为均匀模拟。后期由 AMLL TTML DB /
   本地 .ttml / .lrc 导入替换。
   ============================================================ */

import { ALBUMS } from "@/data/library";
import type { Id, LyricLine, Lyrics } from "@/types/player";

/** 造一行逐字歌词：字符在前 fillSec 秒内均匀填充，行持续到下一行开始。 */
function line(startSec: number, endSec: number, text: string, translation: string): LyricLine {
  const startMs = Math.round(startSec * 1000);
  const endMs = Math.round(endSec * 1000);
  const chars = Array.from(text);
  const fillMs = Math.min(6000, endMs - startMs);
  const per = fillMs / chars.length;
  return {
    timeMs: startMs,
    endMs,
    text,
    translation,
    words: chars.map((c, i) => ({
      text: c,
      startMs: Math.round(startMs + i * per),
      endMs: Math.round(startMs + (i + 1) * per),
    })),
  };
}

/** 「午夜环线」歌词（原创示例，文本来自歌词页设计稿）。曲长 3:56。 */
const MIDNIGHT_LOOP_LINES: LyricLine[] = [
  ["信号灯换了三次颜色", "The signals changed their colors three times"],
  ["车厢里只剩下我和风", "Only the wind and I are left in the car"],
  ["报站声念着陌生站名", "The announcer reads out unfamiliar stops"],
  ["玻璃上映出整座城的灯", "The glass reflects the city lights"],
  ["环线开往午夜的尽头", "The loop line runs to the end of midnight"],
  ["谁的耳机漏出一点歌声", "Someone's earphones leak a little song"],
  ["我数着桥洞下的水纹", "I count the ripples under the bridge"],
  ["让今晚慢一点到站", "Let tonight arrive a little slower"],
  ["星星在高架桥外排队", "Stars are queuing beyond the viaduct"],
  ["黎明还有四站地远", "Dawn is still four stops away"],
].map(([t, tr], i) => line(12 + i * 21.5, 12 + i * 21.5 + 20, t, tr));

const nightWaveId = ALBUMS.find((a) => a.title === "长夜电波")?.id;

/** 歌词库（按曲目 ID 索引）。无词条的曲目走「暂无歌词」空态。 */
const LYRICS_BY_TRACK: Record<Id, Lyrics> = nightWaveId
  ? {
      [`${nightWaveId}-t1`]: {
        trackId: `${nightWaveId}-t1`, // 午夜环线
        lines: MIDNIGHT_LOOP_LINES,
        synced: true,
      },
    }
  : {};

export function lyricsOf(trackId: Id): Lyrics | null {
  return LYRICS_BY_TRACK[trackId] ?? null;
}
