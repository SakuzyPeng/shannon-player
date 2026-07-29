import type { Id, RepeatMode, Track } from "@/types/player";

/**
 * 播放会话的持久化格式。
 *
 * 后端只负责原子地存一段文本（见 `src-tauri/src/session.rs`），结构与版本由这里拥有
 * ——队列怎么排、随机顺序如何、进度到哪，后端在其中没有任何领域判断可做。
 *
 * ## 只存曲目 ID，不存 Track 对象
 *
 * 三个理由，第二个才是关键：
 *
 * 1. 体积。启动时整库入队，存完整对象等于把曲库抄一遍（封面、来源、格式全在里面）。
 * 2. **新鲜度**。曲目信息的权威在曲库，不在会话里。存了副本，用户改完元数据重启，
 *    队列里还是旧标题——而他刚刚才改过，这种不一致最让人怀疑软件是不是坏了。
 * 3. 失效。文件删掉、重扫后 ID 变了，按 ID 查不到自然剔除；存了副本反而会留下
 *    一条点了没反应的幽灵。
 *
 * 代价是恢复时要按 ID 回查曲库，因此**必须等曲库就绪之后再恢复会话**。
 */

/** 当前 schema 版本。结构不兼容地变化时 +1，旧版本一律当作没有会话。 */
const SESSION_VERSION = 1;

export interface PlaybackSession {
  version: number;
  /** 队列的曲目 ID，按播放顺序。**允许重复**——同一首歌可以多次入队。 */
  trackIds: Id[];
  /** 当前项在 `trackIds` 中的下标。 */
  currentIndex: number;
  /** 播放位置（秒）。恢复后不自动播放，按下播放时从这里续上。 */
  positionSec: number;
  repeat: RepeatMode;
  shuffle: boolean;
  /**
   * 随机顺序，存的是 `trackIds` 的**下标排列**而不是曲目 ID。
   *
   * 队列允许同一首歌出现多次，用 ID 表达顺序会产生歧义（"下一个是 t-7" 指的是哪一个
   * t-7？）。下标唯一，且恢复时剔除失效曲目后可以直接重映射。
   */
  shuffleOrder: number[] | null;
  volume: number;
  muted: boolean;
}

/** 从当前 store 状态构造会话。 */
export function toSession(state: {
  queue: { uid: Id; track: Track }[];
  currentIndex: number;
  progress: { positionSec: number };
  repeat: RepeatMode;
  shuffle: boolean;
  shuffleOrder: Id[] | null;
  volume: number;
  muted: boolean;
}): PlaybackSession {
  const uidToIndex = new Map(state.queue.map((item, i) => [item.uid, i]));
  return {
    version: SESSION_VERSION,
    trackIds: state.queue.map((item) => item.track.id),
    currentIndex: state.currentIndex,
    positionSec: state.progress.positionSec,
    repeat: state.repeat,
    shuffle: state.shuffle,
    shuffleOrder:
      state.shuffleOrder?.map((uid) => uidToIndex.get(uid)).filter((i): i is number => i !== undefined) ??
      null,
    volume: state.volume,
    muted: state.muted,
  };
}

/** 恢复结果：曲目已按 ID 查回，失效的那些连同随机顺序一起剔除。 */
export interface RestoredSession {
  tracks: Track[];
  currentIndex: number;
  positionSec: number;
  repeat: RepeatMode;
  shuffle: boolean;
  /** 随机顺序，已重映射为 `tracks` 的下标。 */
  shuffleOrder: number[] | null;
  volume: number;
  muted: boolean;
  /** 有多少曲目在曲库里找不到而被剔除（用于说明「队列少了几首」）。 */
  dropped: number;
}

/**
 * 解析并按曲库回查曲目。
 *
 * 任何不认识的内容都返回 `null` 而不是抛错：会话是可重建的数据，读不懂就当没有，
 * 不该让一份坏掉的会话挡住启动。
 */
export function fromSession(json: string, lookup: (id: Id) => Track | undefined): RestoredSession | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(json);
  } catch {
    return null;
  }
  if (!isSession(parsed) || parsed.version !== SESSION_VERSION) return null;

  // 回查曲库并记下「旧下标 → 新下标」，随机顺序要靠它重映射。
  const tracks: Track[] = [];
  const remap = new Map<number, number>();
  parsed.trackIds.forEach((id, oldIndex) => {
    const track = lookup(id);
    if (!track) return;
    remap.set(oldIndex, tracks.length);
    tracks.push(track);
  });
  if (tracks.length === 0) return null;

  // 当前曲目若已失效，退到它之后仍然存在的第一首——而不是回到队首。
  // 用户上次听到队列中段，重启后从头开始会比「少了一首」更突兀。
  let currentIndex = remap.get(parsed.currentIndex) ?? -1;
  if (currentIndex < 0) {
    for (let i = parsed.currentIndex + 1; i < parsed.trackIds.length; i++) {
      const mapped = remap.get(i);
      if (mapped !== undefined) {
        currentIndex = mapped;
        break;
      }
    }
  }
  const resolvedIndex = currentIndex < 0 ? 0 : currentIndex;

  return {
    tracks,
    currentIndex: resolvedIndex,
    // 当前曲目变了就别沿用旧位置：那个秒数属于另一首歌。
    positionSec: remap.get(parsed.currentIndex) === resolvedIndex ? Math.max(0, parsed.positionSec) : 0,
    repeat: parsed.repeat,
    shuffle: parsed.shuffle,
    shuffleOrder:
      parsed.shuffleOrder
        ?.map((oldIndex) => remap.get(oldIndex))
        .filter((i): i is number => i !== undefined) ?? null,
    volume: clamp01(parsed.volume),
    muted: parsed.muted,
    dropped: parsed.trackIds.length - tracks.length,
  };
}

function clamp01(v: number): number {
  return Number.isFinite(v) ? Math.max(0, Math.min(1, v)) : 1;
}

/** 结构校验。宁可判得严一点——放进来一个半对的会话，症状会出现在离这里很远的地方。 */
function isSession(v: unknown): v is PlaybackSession {
  if (typeof v !== "object" || v === null) return false;
  const s = v as Record<string, unknown>;
  return (
    typeof s.version === "number" &&
    Array.isArray(s.trackIds) &&
    s.trackIds.every((id) => typeof id === "string") &&
    typeof s.currentIndex === "number" &&
    typeof s.positionSec === "number" &&
    (s.repeat === "off" || s.repeat === "all" || s.repeat === "one") &&
    typeof s.shuffle === "boolean" &&
    (s.shuffleOrder === null ||
      (Array.isArray(s.shuffleOrder) && s.shuffleOrder.every((i) => typeof i === "number"))) &&
    typeof s.volume === "number" &&
    typeof s.muted === "boolean"
  );
}
