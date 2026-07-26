/** 元信息统一使用的分隔符（与 MetaLine 一致）。 */
const SEP = " · ";

/**
 * 用 ` · ` 连接元信息片段，自动跳过缺失的项。
 *
 * 年份、流派这类信息未必有：文件没写标签时后端如实留空，界面直接拼接就会出现
 * 「白鲸电台 · 」这样的孤零零分隔符，或者更糟的哨兵值「0」。缺了就整段省略。
 */
export function metaJoin(...parts: Array<string | number | null | undefined>): string {
  return parts
    .filter((p) => p !== null && p !== undefined && String(p).trim() !== "")
    .join(SEP);
}
