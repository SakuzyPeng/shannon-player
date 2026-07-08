import { useCallback, useEffect, useRef, type UIEvent } from "react";

/**
 * 自定义滚动引擎：全平台一致的 macOS 式滚动。
 *
 * 不依赖系统原生滚动手感（原生只作布局与 scrollTop 载体），wheel 事件被拦截后
 * 由本引擎积分位置与速度：
 *  - 触控板（连续小增量）：1:1 跟手，同时测量速度；冲过边缘时速度灌入回弹弹簧。
 *  - 滚轮鼠标（离散步进）：每格转成速度冲量，指数摩擦平滑滑行（总位移与原生等距，
 *    v0 = Δ · λ），连拨自然加速——Windows / Linux 也获得惯性滚动。
 *  - 边缘橡皮筋：动量自动转化为过冲，近临界弹簧（k=560 c=45，来自设计稿）带初速
 *    弹回；贴边继续拉为阻尼拉伸。视觉位移 84·tanh(x/200)（设计稿映射）。
 *  - 自绘滚动条：6px 宽、越界按速度压缩、指数平滑 1-e^(-22t)、静止 0.9s 后淡出。
 */
export function useElasticScroll() {
  const scrollerRef = useRef<HTMLDivElement | null>(null);
  const innerRef = useRef<HTMLDivElement | null>(null);
  const thumbRef = useRef<HTMLDivElement | null>(null);

  // ---- 物理常量 ----
  const FRICTION = 6; // 惯性摩擦（1/s）：单格总位移 = Δ，滑行约 0.6s
  const SPRING_K = 560; // 回弹弹簧刚度（设计稿释放段）
  const SPRING_C = 45; // 回弹阻尼（近临界，无振荡）
  const MAX_OVER = 260; // 原始过冲上限（视觉再经 tanh 压缩）
  const INPUT_WINDOW_MS = 40; // 设计稿：输入活跃窗口，期间弹簧让位于直接拉伸
  const FLING_VEL = 1000; // 高于此速度冲过边缘视为甩动（fling）→ 立即回弹

  // 物理量（跨帧可变，不进 React 状态）
  const s = useRef({
    pos: 0, // 虚拟滚动位置（可越界）
    vel: 0, // 速度 px/s
    velPeak: 0,
    momentum: false, // 是否处于离散滚轮惯性模式
    bouncing: false, // 回弹锁定：甩动冲边后立即弹回，忽略系统动量尾巴事件
    lastBounceDy: 0,
    lastInputT: 0,
    lastEventT: 0,
    raf: 0,
    lastT: 0,
    thumbTimer: 0,
    thT: 0,
    thH: null as number | null,
    thTop: null as number | null,
    thTargetH: null as number | null,
    thTargetTop: null as number | null,
  }).current;

  const overOf = useCallback(
    (el: HTMLDivElement) => {
      const max = Math.max(0, el.scrollHeight - el.clientHeight);
      return s.pos < 0 ? s.pos : s.pos > max ? s.pos - max : 0;
    },
    [s],
  );

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
        const speedBoost = Math.min(0.85, s.velPeak / 4000);
        const ratio = Math.min(0.68, (Math.abs(damped) / 84) * (0.35 + speedBoost));
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

  const applyFrame = useCallback(() => {
    const el = scrollerRef.current;
    const inner = innerRef.current;
    if (!el || !inner) return;
    const max = Math.max(0, el.scrollHeight - el.clientHeight);
    el.scrollTop = Math.max(0, Math.min(s.pos, max));
    const over = overOf(el);
    // 视觉映射：过顶（over<0）内容下移，过底上移
    const damped = over === 0 ? 0 : -Math.sign(over) * 84 * Math.tanh(Math.abs(over) / 200);
    inner.style.transition = "none";
    inner.style.transform = `translateY(${damped.toFixed(2)}px)`;
    updateThumb(Math.abs(damped) < 0.3 ? 0 : damped);
  }, [s, overOf, updateThumb]);

  const tick = useCallback(
    (t: number) => {
      const el = scrollerRef.current;
      if (!el) {
        s.raf = 0;
        return;
      }
      const dt = Math.min(0.032, (t - (s.lastT || t)) / 1000) || 0.016;
      s.lastT = t;
      const over = overOf(el);
      const inputActive = performance.now() - s.lastInputT < INPUT_WINDOW_MS;

      if (over !== 0 && (s.bouncing || !inputActive)) {
        // 橡皮筋回弹：带入场速度的近临界弹簧
        s.vel += -SPRING_K * over * dt;
        s.vel *= Math.exp(-SPRING_C * dt);
        s.pos += s.vel * dt;
        // 弹回穿越边界后小速度即收敛，避免界内残余滑动
        const newOver = overOf(el);
        if (newOver === 0 || Math.sign(newOver) !== Math.sign(over)) {
          if (Math.abs(s.vel) < 120) {
            const max = Math.max(0, el.scrollHeight - el.clientHeight);
            s.pos = over < 0 ? 0 : max;
            s.vel = 0;
            s.bouncing = false;
          }
        }
      } else if (over === 0) {
        if (s.momentum) {
          // 离散滚轮惯性：指数摩擦滑行
          s.pos += s.vel * dt;
          s.vel *= Math.exp(-FRICTION * dt);
        } else {
          // 触控板界内不叠加惯性（速度仅保留给边缘转化），快速衰减
          s.vel *= Math.exp(-20 * dt);
        }
      }

      applyFrame();

      const thumbSettled =
        s.thTargetH == null ||
        (Math.abs((s.thH ?? 0) - s.thTargetH) < 0.5 &&
          Math.abs((s.thTop ?? 0) - (s.thTargetTop ?? 0)) < 0.5);
      const overNow = overOf(el);
      if (!inputActive && thumbSettled && overNow === 0 && Math.abs(s.vel) < 15) {
        s.vel = 0;
        s.velPeak = 0;
        s.momentum = false;
        s.raf = 0;
        s.lastT = 0;
        return;
      }
      s.raf = requestAnimationFrame(tick);
    },
    [s, overOf, applyFrame],
  );

  const ensureRaf = useCallback(() => {
    if (!s.raf) {
      s.lastT = 0;
      s.raf = requestAnimationFrame(tick);
    }
  }, [s, tick]);

  // 拦截 wheel（必须非 passive，React 合成事件在根节点是 passive 的）
  useEffect(() => {
    const el = scrollerRef.current;
    if (!el) return;
    s.pos = el.scrollTop;

    const onWheelNative = (e: globalThis.WheelEvent) => {
      e.preventDefault();
      const now = performance.now();
      const dtEv = Math.min(0.1, (now - s.lastEventT) / 1000) || 0.016;
      s.lastEventT = now;
      const max = Math.max(0, el.scrollHeight - el.clientHeight);
      // 归一化步进单位（行 / 页 → 像素）
      const dy =
        e.deltaMode === 1 ? e.deltaY * 16 : e.deltaMode === 2 ? e.deltaY * el.clientHeight : e.deltaY;
      // 离散滚轮：整数大步进（Windows 滚轮多为 ±100/120 的整数倍）
      const discrete =
        e.deltaMode !== 0 || (Number.isInteger(e.deltaY) && Math.abs(e.deltaY) >= 40);
      const over = s.pos < 0 ? s.pos : s.pos > max ? s.pos - max : 0;

      if (over !== 0) {
        // 回弹锁定中：macOS 松手后系统仍持续发送衰减的动量尾巴事件，
        // 若任其刷新输入窗口，橡皮筋会被钉在极限处「顿一下」才回弹。
        // 同向且未增强的事件视为动量尾巴，直接忽略；增量突然变大（手指
        // 主动再推）或反向输入则解除锁定、交还直接控制。
        if (s.bouncing) {
          const sameDir = Math.sign(dy) === Math.sign(over);
          if (sameDir && Math.abs(dy) <= Math.abs(s.lastBounceDy) * 1.5) {
            s.lastBounceDy = dy;
            return;
          }
          s.bouncing = false;
        }
        if (Math.abs(dy) >= 4) s.lastInputT = now;
        // 贴边继续拉：阻尼累加（视觉经 tanh 压缩），并抑制惯性
        const next = s.pos + dy * 0.9;
        s.pos = over < 0 ? Math.max(-MAX_OVER, next) : Math.min(max + MAX_OVER, next);
        s.vel = 0;
        s.momentum = false;
      } else if (discrete) {
        if (Math.abs(dy) >= 4) s.lastInputT = now;
        // 一格 = 等距总位移的速度冲量（v0 = Δ · λ），连拨叠加
        s.vel += dy * FRICTION;
        s.momentum = true;
      } else {
        if (Math.abs(dy) >= 4) s.lastInputT = now;
        // 触控板：1:1 跟手，速度用于边缘橡皮筋转化
        s.pos = Math.max(-MAX_OVER, Math.min(max + MAX_OVER, s.pos + dy));
        s.vel = 0.7 * s.vel + 0.3 * (dy / dtEv);
        s.momentum = false;
        // 高速冲过边缘 = 甩动：进入回弹锁定，动量立即转化为橡皮筋弹回
        const afterOver = s.pos < 0 ? s.pos : s.pos > max ? s.pos - max : 0;
        if (afterOver !== 0 && Math.abs(s.vel) > FLING_VEL) {
          s.bouncing = true;
          s.lastBounceDy = dy;
        }
      }
      s.velPeak = Math.max(s.velPeak, Math.abs(s.vel));
      ensureRaf();
    };

    // 外部滚动（键盘 / 程序定位）时回灌位置
    const onScrollSync = () => {
      const expected = Math.max(
        0,
        Math.min(Math.round(s.pos), Math.max(0, el.scrollHeight - el.clientHeight)),
      );
      if (Math.abs(el.scrollTop - expected) > 1 && !s.raf) {
        s.pos = el.scrollTop;
        s.vel = 0;
      }
    };

    el.addEventListener("wheel", onWheelNative, { passive: false });
    el.addEventListener("scroll", onScrollSync, { passive: true });
    return () => {
      el.removeEventListener("wheel", onWheelNative);
      el.removeEventListener("scroll", onScrollSync);
      if (s.raf) cancelAnimationFrame(s.raf);
      s.raf = 0;
      window.clearTimeout(s.thumbTimer);
    };
    // scrollerRef 在组件整个生命周期指向同一元素（mount 后不变）
  }, [s, ensureRaf]);

  /** 供消费者挂在滚动容器上：驱动自绘滚动条（吸顶栏等自行另加逻辑）。 */
  const onScroll = useCallback(
    (_e: UIEvent<HTMLDivElement>) => {
      updateThumb(0);
    },
    [updateThumb],
  );

  return { scrollerRef, innerRef, thumbRef, onScroll };
}
