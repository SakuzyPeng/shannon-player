import { useEffect } from "react";
import { usePlayerStore } from "@/store/player";

const TICK_MS = 100;

/** 模拟播放时钟：播放中每 100ms 推进一次进度（真实音频引擎接入前的占位）。 */
export function usePlaybackTicker() {
  const playing = usePlayerStore((s) => s.playing);
  useEffect(() => {
    if (!playing) return;
    const id = window.setInterval(() => usePlayerStore.getState().tick(TICK_MS / 1000), TICK_MS);
    return () => window.clearInterval(id);
  }, [playing]);
}
