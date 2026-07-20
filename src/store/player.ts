import { create } from "zustand";
import type {
  AudioDevice,
  Id,
  PlaybackProgress,
  Playlist,
  QueueItem,
  RepeatMode,
  Track,
} from "@/types/player";
import {
  DEMO_TRACK,
  SEED_FAVORITE_ALBUMS,
  SEED_FAVORITE_ARTISTS,
  SEED_FAVORITE_TRACKS,
} from "@/data/library";
import { PLAYLISTS } from "@/data/playlists";

/** 生成队列项 uid（后期可换成后端下发的稳定 ID）。 */
let uidSeq = 0;
const nextUid = (): Id => `q-${uidSeq++}`;
let playlistSeq = 0;
const nextPlaylistId = (): Id => `pl-user-${Date.now()}-${playlistSeq++}`;

/** 保留首次出现的曲目，并把已有 ID 视为占用。 */
function uniqueTracksById(tracks: Track[], seen = new Set<Id>()): Track[] {
  return tracks.filter((track) => {
    if (seen.has(track.id)) return false;
    seen.add(track.id);
    return true;
  });
}

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

  /** ---- 收藏（用户数据，后期由后端持久化） ---- */
  favorites: Record<Id, boolean>;
  favoriteAlbums: Record<Id, boolean>;
  /** 收藏歌手（当前以歌手名为键，后期换稳定 ID）。 */
  favoriteArtists: Record<string, boolean>;
  /** 收藏歌单（以歌单 ID 为键）。 */
  favoritePlaylists: Record<Id, boolean>;
  /** 歌单（用户数据，可变；种子初始化，后期由后端持久化）。 */
  playlists: Playlist[];

  /** ---- 音频设备 ---- */
  devices: AudioDevice[];
  activeDeviceId: Id | null;

  /** ---- 动作 ---- */
  current: () => Track | null;
  play: (track?: Track) => void;
  /** 用整张专辑/歌单替换队列并从 startIndex 开始播放。 */
  playQueue: (tracks: Track[], startIndex?: number) => void;
  pause: () => void;
  togglePlay: () => void;
  next: () => void;
  prev: () => void;
  toggleShuffle: () => void;
  cycleRepeat: () => void;
  toggleFavorite: (id: Id) => void;
  toggleFavoriteAlbum: (id: Id) => void;
  toggleFavoriteArtist: (name: string) => void;
  toggleFavoritePlaylist: (id: Id) => void;
  /** 把曲目加入歌单（按曲目 ID 去重，重复加入为 no-op）；更新时间标记清空 = 「今天更新」。 */
  addToPlaylist: (playlistId: Id, tracks: Track[]) => void;
  /** 新建并收藏歌单后加入曲目；名称默认「新歌单」，重名自动加序号。返回新歌单 ID。 */
  createPlaylistWithTracks: (baseName: string, tracks: Track[]) => Id;
  setVolume: (v: number) => void;
  toggleMuted: () => void;
  seek: (positionSec: number) => void;
  /** 供音频引擎/定时器回灌进度。 */
  setProgress: (p: Partial<PlaybackProgress>) => void;
  /**
   * 模拟播放推进 dt 秒（真实音频引擎接入前的占位时钟）：
   * 到曲尾时按循环模式切曲，队列播完则停在末尾。
   */
  tick: (dtSec: number) => void;
  /** 清空当前曲目之后的队列（歌词页队列面板「清除」）。 */
  clearUpNext: () => void;
  /** 「下一首播放」：插入到当前项之后。 */
  enqueueNext: (track: Track) => void;
  /** 追加到队尾。 */
  enqueue: (track: Track) => void;
  setActiveDevice: (id: Id) => void;
}

const REPEAT_CYCLE: RepeatMode[] = ["off", "all", "one"];

/** 切到新曲目时的全新进度（位置归零、时长取新曲）。 */
function freshProgress(track: Track): PlaybackProgress {
  return { positionSec: 0, durationSec: track.durationSec, bufferedSec: track.durationSec };
}

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

  favorites: { ...SEED_FAVORITE_TRACKS },
  favoriteAlbums: { ...SEED_FAVORITE_ALBUMS },
  favoriteArtists: { ...SEED_FAVORITE_ARTISTS },
  favoritePlaylists: { "pl-nightdrive": true },
  playlists: PLAYLISTS.map((p) => ({ ...p, tracks: [...p.tracks] })),

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
      if (idx >= 0) return { currentIndex: idx, playing: true, progress: freshProgress(track) };
      const item: QueueItem = { uid: nextUid(), track, source: "user" };
      return {
        queue: [...s.queue, item],
        currentIndex: s.queue.length,
        playing: true,
        progress: freshProgress(track),
      };
    }),

  playQueue: (tracks, startIndex = 0) =>
    set(() => {
      if (tracks.length === 0) return {};
      const queue: QueueItem[] = tracks.map((track) => ({ uid: nextUid(), track, source: "user" }));
      const idx = Math.max(0, Math.min(startIndex, queue.length - 1));
      return { queue, currentIndex: idx, playing: true, progress: freshProgress(queue[idx].track) };
    }),

  pause: () => set({ playing: false }),
  togglePlay: () => set((s) => ({ playing: !s.playing })),

  next: () =>
    set((s) => {
      if (s.queue.length === 0) return s;
      if (s.repeat === "one") return { progress: { ...s.progress, positionSec: 0 } };
      const atEnd = s.currentIndex >= s.queue.length - 1;
      const nextIdx = atEnd ? (s.repeat === "all" ? 0 : s.currentIndex) : s.currentIndex + 1;
      return { currentIndex: nextIdx, progress: freshProgress(s.queue[nextIdx].track) };
    }),

  prev: () =>
    set((s) => {
      // 3 秒内回退到上一首，否则回到本曲开头。
      if (s.progress.positionSec > 3) return { progress: { ...s.progress, positionSec: 0 } };
      const prevIdx = s.currentIndex <= 0 ? 0 : s.currentIndex - 1;
      return { currentIndex: prevIdx, progress: freshProgress(s.queue[prevIdx].track) };
    }),

  toggleShuffle: () => set((s) => ({ shuffle: !s.shuffle })),
  cycleRepeat: () =>
    set((s) => ({ repeat: REPEAT_CYCLE[(REPEAT_CYCLE.indexOf(s.repeat) + 1) % 3] })),

  toggleFavorite: (id) =>
    set((s) => ({ favorites: { ...s.favorites, [id]: !s.favorites[id] } })),

  toggleFavoriteAlbum: (id) =>
    set((s) => ({ favoriteAlbums: { ...s.favoriteAlbums, [id]: !s.favoriteAlbums[id] } })),

  toggleFavoriteArtist: (name) =>
    set((s) => ({ favoriteArtists: { ...s.favoriteArtists, [name]: !s.favoriteArtists[name] } })),

  toggleFavoritePlaylist: (id) =>
    set((s) => ({ favoritePlaylists: { ...s.favoritePlaylists, [id]: !s.favoritePlaylists[id] } })),

  addToPlaylist: (playlistId, tracks) =>
    set((s) => ({
      playlists: s.playlists.map((p) => {
        if (p.id !== playlistId) return p;
        const added = uniqueTracksById(tracks, new Set(p.tracks.map((tk) => tk.id)));
        if (added.length === 0) return p;
        // updatedLabel 置空 → 展示端回退为「今天更新」
        return { ...p, tracks: [...p.tracks, ...added], updatedLabel: "" };
      }),
    })),

  createPlaylistWithTracks: (baseName, tracks) => {
    const id = nextPlaylistId();
    set((s) => {
      // 重名自动加序号：新歌单 / 新歌单 2 / 新歌单 3 …
      const names = new Set(s.playlists.map((p) => p.title));
      let title = baseName;
      for (let n = 2; names.has(title); n++) title = `${baseName} ${n}`;
      const playlist: Playlist = {
        id,
        title,
        description: "",
        updatedLabel: "",
        tracks: uniqueTracksById(tracks),
      };
      return {
        playlists: [...s.playlists, playlist],
        // 当前没有独立歌单总览页；新建后先加入收藏页，确保用户能立即找到。
        favoritePlaylists: { ...s.favoritePlaylists, [id]: true },
      };
    });
    return id;
  },

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

  tick: (dtSec) =>
    set((s) => {
      if (!s.playing || s.currentIndex < 0) return s;
      const pos = s.progress.positionSec + dtSec;
      if (pos < s.progress.durationSec) {
        return { progress: { ...s.progress, positionSec: pos } };
      }
      if (s.repeat === "one") return { progress: { ...s.progress, positionSec: 0 } };
      const atEnd = s.currentIndex >= s.queue.length - 1;
      if (atEnd && s.repeat === "off") {
        return { playing: false, progress: { ...s.progress, positionSec: s.progress.durationSec } };
      }
      const nextIdx = atEnd ? 0 : s.currentIndex + 1;
      return { currentIndex: nextIdx, progress: freshProgress(s.queue[nextIdx].track) };
    }),

  clearUpNext: () =>
    set((s) => ({ queue: s.queue.slice(0, s.currentIndex + 1) })),

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
