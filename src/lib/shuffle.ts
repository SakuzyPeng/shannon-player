/**
 * 洗牌（Fisher-Yates）。
 *
 * 不用 `sort(() => Math.random() - 0.5)`：那个比较器不满足排序要求（同一对元素
 * 每次比较可能给出不同结果），各引擎的实现细节会让某些位置的曲目出现概率明显高于
 * 其他位置——「随机播放」总是从那几首开始，用户是感觉得到的。
 *
 * 返回新数组，不改动入参：调用点的 `tracks` 多半是 useMemo 的派生结果，就地打乱
 * 会污染缓存。
 */
export function shuffled<T>(list: readonly T[]): T[] {
  const out = [...list];
  for (let i = out.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}
