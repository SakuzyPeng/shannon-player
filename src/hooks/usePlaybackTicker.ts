import { useEffect } from "react";
import { usePlayerStore } from "@/store/player";

/**
 * 订阅播放引擎事件。
 *
 * 取代了早先的占位时钟——那时进度是前端每 100 ms 自加出来的，与实际发声无关。
 * 现在位置由引擎按**输出回调已消费的帧数**上报（还扣掉了设备延迟），
 * 这是唯一能跟发声对齐的口径：定时器估算会随重采样比率、设备缓冲、
 * 系统调度各自漂移，歌词逐字高亮那种精度下会明显对不上。
 *
 * 事件约 5 Hz，界面若要更顺滑应在事件之间自行插值，而不是把频率调高——
 * 事件的职责是**重锚定**，不是驱动动画。
 */
export function usePlaybackTicker() {
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;

    void usePlayerStore
      .getState()
      .attachEngine()
      .then((off) => {
        // 订阅是异步的，组件可能已经卸载——此时立刻退订，不留悬挂的监听。
        if (cancelled) off();
        else unlisten = off;
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
}
