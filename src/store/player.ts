import { create } from "zustand";
import type {
  AudioDevice,
  Id,
  PlaybackProgress,
  QueueItem,
  RepeatMode,
  Track,
} from "@/types/player";
import { DEMO_TRACK } from "@/data/library";

/** 生成队列项 uid（后期可换成后端下发的稳定 ID）。 */
let uidSeq = 0;
const nextUid = (): Id => `q-${uidSeq++}`;

interface PlayerState {
  /** ---- 播放队列 ---- */
  queue: QueueItem[];
  /** 当前播放项在 queue 中的下标（-1 表示空）。 */
  currentIndex: number;

  /** ---- 播放状态 ---- */
  playing: boolean;
  repeat: RepeatMode;
  shuffle: boolean;
  /** 音量 0..1。 */
  volume: number;
  muted: boolean;

  /** ---- 进度（秒） ---- */
  progress: PlaybackProgress;

  /** ---- 音频设备 ---- */
  devices: AudioDevice[];
  activeDeviceId: Id | null;

  /** ---- 动作 ---- */
  current: () => Track | null;
  play: (track?: Track) => void;
  pause: () => void;
  togglePlay: () => void;
  next: () => void;
  prev: () => void;
  toggleShuffle: () => void;
  cycleRepeat: () => void;
  toggleFavoriteCurrent: () => void;
  setVolume: (v: number) => void;
  toggleMuted: () => void;
  seek: (positionSec: number) => void;
  /** 供音频引擎/定时器回灌进度。 */
  setProgress: (p: Partial<PlaybackProgress>) => void;
  /** 「下一首播放」：插入到当前项之后。 */
  enqueueNext: (track: Track) => void;
  /** 追加到队尾。 */
  enqueue: (track: Track) => void;
  setActiveDevice: (id: Id) => void;
}

const REPEAT_CYCLE: RepeatMode[] = ["off", "all", "one"];

/** 初始队列：仅放入演示曲目，进度停在 43%（对齐设计稿）。 */
const initialQueue: QueueItem[] = [
  { uid: nextUid(), track: DEMO_TRACK, source: "user" },
];

export const usePlayerStore = create<PlayerState>((set, get) => ({
  queue: initialQueue,
  currentIndex: 0,

  playing: true,
  repeat: "off",
  shuffle: false,
  volume: 0.68,
  muted: false,

  progress: {
    positionSec: Math.round(DEMO_TRACK.durationSec * 0.43),
    durationSec: DEMO_TRACK.durationSec,
    bufferedSec: DEMO_TRACK.durationSec,
  },

  devices: [
    { id: "dev-default", label: "系统默认输出", isDefault: true },
    { id: "dev-speakers", label: "MacBook Pro 扬声器", isDefault: false },
  ],
  activeDeviceId: "dev-default",

  current: () => {
    const { queue, currentIndex } = get();
    return currentIndex >= 0 && currentIndex < queue.length
      ? queue[currentIndex].track
      : null;
  },

  play: (track) =>
    set((s) => {
      if (!track) return { playing: true };
      const idx = s.queue.findIndex((q) => q.track.id === track.id);
      if (idx >= 0) return { currentIndex: idx, playing: true };
      const item: QueueItem = { uid: nextUid(), track, source: "user" };
      return { queue: [...s.queue, item], currentIndex: s.queue.length, playing: true };
    }),

  pause: () => set({ playing: false }),
  togglePlay: () => set((s) => ({ playing: !s.playing })),

  next: () =>
    set((s) => {
      if (s.queue.length === 0) return s;
      if (s.repeat === "one") return { progress: { ...s.progress, positionSec: 0 } };
      const atEnd = s.currentIndex >= s.queue.length - 1;
      const nextIdx = atEnd ? (s.repeat === "all" ? 0 : s.currentIndex) : s.currentIndex + 1;
      return { currentIndex: nextIdx, progress: { ...s.progress, positionSec: 0 } };
    }),

  prev: () =>
    set((s) => {
      // 3 秒内回退到上一首，否则回到本曲开头。
      if (s.progress.positionSec > 3) return { progress: { ...s.progress, positionSec: 0 } };
      const prevIdx = s.currentIndex <= 0 ? 0 : s.currentIndex - 1;
      return { currentIndex: prevIdx, progress: { ...s.progress, positionSec: 0 } };
    }),

  toggleShuffle: () => set((s) => ({ shuffle: !s.shuffle })),
  cycleRepeat: () =>
    set((s) => ({ repeat: REPEAT_CYCLE[(REPEAT_CYCLE.indexOf(s.repeat) + 1) % 3] })),

  toggleFavoriteCurrent: () =>
    set((s) => {
      if (s.currentIndex < 0) return s;
      const queue = s.queue.slice();
      const item = queue[s.currentIndex];
      queue[s.currentIndex] = {
        ...item,
        track: { ...item.track, favorited: !item.track.favorited },
      };
      return { queue };
    }),

  setVolume: (v) => set({ volume: Math.max(0, Math.min(1, v)), muted: v === 0 }),
  toggleMuted: () => set((s) => ({ muted: !s.muted })),

  seek: (positionSec) =>
    set((s) => ({
      progress: {
        ...s.progress,
        positionSec: Math.max(0, Math.min(positionSec, s.progress.durationSec)),
      },
    })),

  setProgress: (p) => set((s) => ({ progress: { ...s.progress, ...p } })),

  enqueueNext: (track) =>
    set((s) => {
      const item: QueueItem = { uid: nextUid(), track, source: "user" };
      const queue = s.queue.slice();
      queue.splice(s.currentIndex + 1, 0, item);
      return { queue };
    }),

  enqueue: (track) =>
    set((s) => ({
      queue: [...s.queue, { uid: nextUid(), track, source: "auto" }],
    })),

  setActiveDevice: (id) => set({ activeDeviceId: id }),
}));
