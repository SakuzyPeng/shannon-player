import { useEffect } from "react";
import { setLoudnessQueue, type LoudnessQueueItem } from "@/lib/backend";
import { orderedQueue, usePlayerStore } from "@/store/player";
import { useUiStore } from "@/store/ui";

/**
 * 把「该按什么顺序分析响度」持续告诉后端。
 *
 * ## 顺序由前端给，增益由后端算
 *
 * 优先级就是**距当前播放位置的远近**，而队列的权威在前端（随机顺序、循环模式、
 * 用户的拖拽重排都在这边）。后端自己排就得复制一份队列状态，那是两处状态迟早分叉的
 * 经典写法。反过来，具体增益是后端知识（取决于分析结果与播放策略），前端不碰。
 *
 * 顺序是把队列从当前曲目**旋转**一圈：当前这首排头，其后依次，队首那段排到最后。
 * 于是「预取下一首」与「分析全库」是同一件事——队列跑空就是全库分析完成
 * （启动时整库入队，所以这个队列通常就是整个曲库）。
 *
 * ## 关掉就要真的停下
 *
 * 用户关掉响度归一化时推一个空队列。为一个已经关掉的功能继续在后台解码全库，
 * 用户看到的就是一个凭空吃 CPU 的播放器——而他刚刚才明确表示不需要它。
 *
 * 去抖 500 ms：拖拽重排会连续触发，每动一下就重排一次分析队列纯属浪费。
 */

const DEBOUNCE_MS = 500;

/** 按播放顺序列出待分析曲目，当前这首排头。 */
function analysisOrder(): LoudnessQueueItem[] {
  const s = usePlayerStore.getState();
  const ordered = orderedQueue(s.queue, s.shuffleOrder);
  const currentUid = s.queue[s.currentIndex]?.uid;
  const at = currentUid === undefined ? -1 : ordered.findIndex((item) => item.uid === currentUid);
  const rotated = at < 0 ? ordered : [...ordered.slice(at), ...ordered.slice(0, at)];

  // 同一首歌可以多次入队，但分析一次就够；没有 path 的（种子演示曲目）无从分析。
  const seen = new Set<string>();
  const items: LoudnessQueueItem[] = [];
  for (const { track } of rotated) {
    if (!track.path || seen.has(track.id)) continue;
    seen.add(track.id);
    items.push({ trackId: track.id, path: track.path });
  }
  return items;
}

export function useLoudnessQueue() {
  useEffect(() => {
    let debounce: ReturnType<typeof setTimeout> | null = null;

    const push = () => {
      const enabled = useUiStore.getState().settings.loudness;
      void setLoudnessQueue(enabled ? analysisOrder() : []);
    };

    const schedule = () => {
      if (debounce !== null) clearTimeout(debounce);
      debounce = setTimeout(push, DEBOUNCE_MS);
    };

    const unsubscribePlayer = usePlayerStore.subscribe((state, prev) => {
      // 只认这三样。进度每秒变 5 次，与分析顺序无关。
      if (
        state.queue !== prev.queue ||
        state.currentIndex !== prev.currentIndex ||
        state.shuffleOrder !== prev.shuffleOrder
      ) {
        schedule();
      }
    });

    const unsubscribeUi = useUiStore.subscribe((state, prev) => {
      if (state.settings.loudness !== prev.settings.loudness) schedule();
    });

    // 启动时先推一次：曲库恢复完成后队列才有内容，那次变化会自己触发；
    // 这一次覆盖的是「设置为开、队列已经就位」的情形。
    schedule();

    return () => {
      unsubscribePlayer();
      unsubscribeUi();
      if (debounce !== null) clearTimeout(debounce);
    };
  }, []);
}
