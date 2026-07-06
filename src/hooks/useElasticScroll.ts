import { useCallback, useRef, type UIEvent, type WheelEvent } from "react";

/**
 * 过弹性滚动 + 自绘滚动条（严格移植设计稿的弹簧模型）：
 *  - 拉伸段 k=320 c=30；释放段 k=560 c=45；位移 84·tanh(x/200)
 *  - 滚轮判定窗口 40ms（|ΔY|<4 不续期）
 *  - 滚动条 6px 宽、越界按速度压缩、指数平滑 1-e^(-22t)、静止 0.9s 后淡出
 */
export function useElasticScroll() {
  const scrollerRef = useRef<HTMLDivElement | null>(null);
  const innerRef = useRef<HTMLDivElement | null>(null);
  const thumbRef = useRef<HTMLDivElement | null>(null);

  // 物理量
  const s = useRef({
    stretch: 0,
    vel: 0,
    velPeak: 0,
    cur: 0,
    sprV: 0,
    lastWheelT: 0,
    raf: 0,
    lastT: 0,
    thumbTimer: 0,
    thT: 0,
    thH: null as number | null,
    thTop: null as number | null,
    thTargetH: null as number | null,
    thTargetTop: null as number | null,
  }).current;

  const updateThumb = useCallback(
    (damped: number) => {
      const el = scrollerRef.current;
      const th = thumbRef.current;
      if (!el || !th) return;
      const inset = 8;
      const trackH = el.clientHeight - inset * 2;
      let h = Math.max(30, trackH * (el.clientHeight / el.scrollHeight));
      const maxScroll = Math.max(1, el.scrollHeight - el.clientHeight);
      let top = (trackH - h) * (el.scrollTop / maxScroll);
      if (damped !== 0) {
        const speedBoost = Math.min(0.85, s.velPeak / 160);
        const ratio = Math.min(0.68, (Math.abs(s.cur) / 260) * (0.35 + speedBoost));
        h = Math.max(22, h * (1 - ratio));
        top = damped > 0 ? 0 : trackH - h;
      }
      s.thTargetH = h;
      s.thTargetTop = top;
      const now = performance.now();
      const dth = Math.min(0.05, (now - (s.thT || now)) / 1000) || 0.016;
      s.thT = now;
      const f = 1 - Math.exp(-22 * dth);
      s.thH = s.thH == null ? h : s.thH + (h - s.thH) * f;
      s.thTop = s.thTop == null ? top : s.thTop + (top - s.thTop) * f;
      th.style.transition = "opacity 0.3s ease";
      th.style.height = s.thH.toFixed(2) + "px";
      th.style.top = (inset + s.thTop).toFixed(2) + "px";
      th.style.opacity = "1";
      window.clearTimeout(s.thumbTimer);
      s.thumbTimer = window.setTimeout(() => {
        th.style.opacity = "0";
      }, 900);
    },
    [s],
  );

  const applyStretch = useCallback(() => {
    const inner = innerRef.current;
    if (!inner) return;
    const damped = 84 * Math.tanh(s.cur / 200);
    inner.style.transition = "none";
    inner.style.transform = "translateY(" + damped.toFixed(2) + "px)";
    updateThumb(Math.abs(damped) < 0.3 ? 0 : damped);
  }, [s, updateThumb]);

  const tick = useCallback(
    (t: number) => {
      const dt = Math.min(0.032, (t - (s.lastT || t)) / 1000) || 0.016;
      s.lastT = t;
      const releasing = performance.now() - s.lastWheelT > 40;
      const target = releasing ? 0 : s.stretch;
      const k = releasing ? 560 : 320;
      const c = releasing ? 45 : 30;
      s.sprV += (target - s.cur) * k * dt;
      s.sprV *= Math.exp(-c * dt);
      s.cur += s.sprV * dt;
      const thumbSettled =
        s.thTargetH == null ||
        (Math.abs((s.thH ?? 0) - s.thTargetH) < 0.5 &&
          Math.abs((s.thTop ?? 0) - (s.thTargetTop ?? 0)) < 0.5);
      if (releasing && thumbSettled && Math.abs(s.cur) < 0.4 && Math.abs(s.sprV) < 2) {
        s.cur = 0;
        s.sprV = 0;
        s.stretch = 0;
        s.vel = 0;
        s.velPeak = 0;
        applyStretch();
        s.raf = 0;
        s.lastT = 0;
        return;
      }
      applyStretch();
      s.raf = requestAnimationFrame(tick);
    },
    [s, applyStretch],
  );

  const onWheel = useCallback(
    (e: WheelEvent<HTMLDivElement>) => {
      const el = scrollerRef.current;
      if (!el || !innerRef.current) return;
      const atTop = el.scrollTop <= 0;
      const atBottom = el.scrollTop + el.clientHeight >= el.scrollHeight - 1;
      const pulling = (atTop && e.deltaY < 0) || (atBottom && e.deltaY > 0);
      if (!pulling) {
        s.vel = 0;
        return;
      }
      s.vel = s.vel * 0.65 + Math.abs(e.deltaY) * 0.35;
      s.velPeak = Math.max(s.velPeak || 0, s.vel);
      s.stretch = Math.max(-260, Math.min(260, s.stretch - e.deltaY * 0.9));
      if (Math.abs(e.deltaY) >= 4) s.lastWheelT = performance.now();
      if (!s.raf) {
        s.lastT = 0;
        s.raf = requestAnimationFrame(tick);
      }
    },
    [s, tick],
  );

  const onScroll = useCallback(
    (_e: UIEvent<HTMLDivElement>) => {
      updateThumb(0);
    },
    [updateThumb],
  );

  return { scrollerRef, innerRef, thumbRef, onWheel, onScroll };
}
