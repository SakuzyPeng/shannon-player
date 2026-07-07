/** 秒 → "m:ss"（用于时长与进度显示，配合 tabular-nums）。 */
export function fmtTime(sec: number): string {
  const m = Math.floor(sec / 60);
  const s = Math.floor(sec % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}
