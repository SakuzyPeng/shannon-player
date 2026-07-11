import { useCallback, useEffect, useRef, type UIEvent } from "react";

/**
 * 原生滚动 + 自绘滚动条。
 *
 * 经权衡的决定：滚动「手感」交还各平台原生——macOS 触控板橡皮筋、Windows/Linux 滚轮
 * 惯性各自沿用系统实现，最贴合用户在本机的肌肉记忆；本 hook 不再拦截 wheel、不再自积分
 * 速度或用 transform 造橡皮筋。只统一「视觉」：容器隐藏系统滚动条（`.no-scrollbar`），
 * 改绘一份跨平台一致的 6px thumb。
 *
 * thumb 完全由原生 scroll 事件驱动：按 scrollTop / scrollHeight 直接映射高度与位置
 * （1:1 跟随，无平滑滞后），静止 0.9s 后淡出；内容未溢出时不显示。
 *
 * 返回签名与旧「弹性引擎」保持一致，故所有调用点无需改动。`innerRef` 现仅作内容容器
 * （不再承载 transform），保留以兼容既有结构。
 */
export function useElasticScroll() {
  const scrollerRef = useRef<HTMLDivElement | null>(null);
  const innerRef = useRef<HTMLDivElement | null>(null);
  const thumbRef = useRef<HTMLDivElement | null>(null);
  const hideTimer = useRef(0);

  const updateThumb = useCallback(() => {
    const el = scrollerRef.current;
    const th = thumbRef.current;
    if (!el || !th) return;
    const inset = 8;
    const ratio = el.clientHeight / el.scrollHeight;
    // 内容未溢出：无需滚动条。
    if (ratio >= 1) {
      th.style.opacity = "0";
      return;
    }
    const trackH = el.clientHeight - inset * 2;
    const h = Math.max(30, trackH * ratio);
    const maxScroll = Math.max(1, el.scrollHeight - el.clientHeight);
    const progress = Math.max(0, Math.min(el.scrollTop, maxScroll)) / maxScroll;
    const top = (trackH - h) * progress;
    th.style.height = h.toFixed(2) + "px";
    th.style.top = (inset + top).toFixed(2) + "px";
    th.style.opacity = "1";
    window.clearTimeout(hideTimer.current);
    hideTimer.current = window.setTimeout(() => {
      if (thumbRef.current) thumbRef.current.style.opacity = "0";
    }, 900);
  }, []);

  /** 挂在滚动容器 onScroll 上：原生滚动（含触控板 / 滚轮 / 键盘 / 程序定位）驱动自绘 thumb。 */
  const onScroll = useCallback(
    (_e: UIEvent<HTMLDivElement>) => {
      updateThumb();
    },
    [updateThumb],
  );

  useEffect(() => () => window.clearTimeout(hideTimer.current), []);

  return { scrollerRef, innerRef, thumbRef, onScroll };
}
