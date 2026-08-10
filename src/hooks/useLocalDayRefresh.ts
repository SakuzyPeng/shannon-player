import { useEffect, useState } from "react";

/** 当地自然日键；时区或日期变化都会得到不同的值。 */
function localDayKey(now: Date): string {
  return `${now.getFullYear()}-${now.getMonth()}-${now.getDate()}@${now.getTimezoneOffset()}`;
}

/** 到下一个当地零点的等待时间；额外留一点余量，避免定时器提前唤醒仍落在前一天。 */
function delayUntilNextLocalDay(now: Date): number {
  const next = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1);
  return Math.max(1_000, next.getTime() - now.getTime() + 50);
}

/**
 * 在当地日期变化时让 App 重渲染一次。
 *
 * 歌单的「今天 / 昨天 / 几周前」由渲染时的当前日期推导；没有这枚自然日时钟，应用
 * 停在同一页面跨过午夜后会一直显示旧文案。定时器按下一个当地零点逐次安排，因而天然
 * 适应夏令时的 23 / 25 小时日；窗口重新获得焦点时再对账一次，覆盖休眠、时区切换与
 * 后台定时器被系统节流的情况。
 */
export function useLocalDayRefresh() {
  const [, setDay] = useState(() => localDayKey(new Date()));

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null;

    const schedule = () => {
      if (timer !== null) clearTimeout(timer);
      const now = new Date();
      timer = setTimeout(() => {
        setDay(localDayKey(new Date()));
        schedule();
      }, delayUntilNextLocalDay(now));
    };

    const reconcile = () => {
      setDay(localDayKey(new Date()));
      schedule();
    };
    const onVisibilityChange = () => {
      if (!document.hidden) reconcile();
    };

    schedule();
    window.addEventListener("focus", reconcile);
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => {
      if (timer !== null) clearTimeout(timer);
      window.removeEventListener("focus", reconcile);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
  }, []);
}
