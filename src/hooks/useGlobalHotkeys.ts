import { useEffect } from "react";
import { usePlayerStore } from "@/store/player";
import { useUiStore } from "@/store/ui";

/** 方向键单次快进 / 快退的秒数。 */
const SEEK_STEP = 5;
/** 方向键单次音量增减。 */
const VOLUME_STEP = 0.05;

/**
 * 焦点位于输入类控件或浮层（菜单 / 对话框）内时让位：
 * 那里的空格、方向键属于控件自身的语义（输入、菜单导航）。
 */
function shouldYield(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el?.closest) return false;
  return !!el.closest(
    'input, textarea, [contenteditable="true"], [role="menu"], [role="dialog"], [role="menuitem"]',
  );
}

/**
 * 全局播放快捷键（桌面播放器的基本盘）：
 *
 * - `空格`            播放 / 暂停
 * - `←` / `→`         快退 / 快进 5 秒
 * - `⌘/Ctrl + ←/→`   上一首 / 下一首
 * - `↑` / `↓`         音量 ±5%
 * - `M`               静音开关
 * - `⌘/Ctrl + F`      唤起搜索页
 *
 * 全部会 preventDefault，避免空格滚动页面、方向键滚动列表。
 */
export function useGlobalHotkeys(): void {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.altKey || shouldYield(e.target)) return;
      const mod = e.metaKey || e.ctrlKey;
      const player = usePlayerStore.getState();

      if (mod && (e.key === "f" || e.key === "F")) {
        e.preventDefault();
        useUiStore.getState().setNav("search");
        return;
      }
      // 其余传输控制均不带 ⌘/Ctrl（上一首 / 下一首除外）。
      switch (e.key) {
        case " ":
          e.preventDefault();
          player.togglePlay();
          break;
        case "ArrowLeft":
          e.preventDefault();
          if (mod) player.prev();
          else player.seek(player.progress.positionSec - SEEK_STEP);
          break;
        case "ArrowRight":
          e.preventDefault();
          if (mod) player.next();
          else player.seek(player.progress.positionSec + SEEK_STEP);
          break;
        case "ArrowUp":
          if (mod) return;
          e.preventDefault();
          player.setVolume(player.volume + VOLUME_STEP);
          break;
        case "ArrowDown":
          if (mod) return;
          e.preventDefault();
          player.setVolume(player.volume - VOLUME_STEP);
          break;
        case "m":
        case "M":
          if (mod) return;
          e.preventDefault();
          player.toggleMuted();
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
