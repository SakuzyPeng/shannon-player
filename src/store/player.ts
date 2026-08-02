import { create } from "zustand";
import type {
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
import * as backend from "@/lib/backend";
import {
  engine,
  hintDuration,
  loadSession,
  saveSession,
  type NextTrackArgs,
} from "@/lib/backend";
// 专辑收藏落盘的是曲目 ID，界面按专辑 ID 读，中间这层换算要现查曲库。
// `lib/library.ts` 只依赖 `store/library.ts`，不回头 import 本文件，没有循环。
import { albums, tracksOf } from "@/lib/library";
import { fromSession, toSession } from "@/lib/session";
import { useUiStore } from "@/store/ui";
import type { Favorites } from "@/types/generated/collections";
import type { AudioDeviceInfo, PlaybackError, PlayerStatus } from "@/types/generated/player";

/** 生成队列项 uid（后期可换成后端下发的稳定 ID）。 */
let uidSeq = 0;
const nextUid = (): Id => `q-${uidSeq++}`;
let loadSeq = 0;
// 本次运行的随机盐，让装载 ID 跨进程唯一。**不要叫 session**——那个词现在专指
// 持久化的播放会话（`@/lib/session`），两个概念混在一起会让人以为 loadId 也会落盘。
const runSalt = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
const nextLoadId = (): Id => `load-${runSalt}-${loadSeq++}`;
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

/**
 * 恢复会话前后要比对的播放域快照。
 *
 * 恢复要跨两次 IPC（曲库 + 会话文件），期间用户完全可能已经动过播放器。判据必须
 * **按意图分组**，而不是「整个 store 有没有变过」：Zustand 每次 `set` 都换新对象，
 * 那种判据下调一次音量就会让整份会话作废——队列被整库入队顶掉，下一次持久化再把它
 * 写回文件，用户上次听到哪就永久没了。而调音量并不包含「忘掉我的队列」的意思。
 */
export interface PlaybackBaseline {
  queue: QueueItem[];
  currentIndex: number;
  loadedTrackId: Id | null;
  volume: number;
  muted: boolean;
  repeat: RepeatMode;
  shuffle: boolean;
}

/** 用户已经自己选了要放什么——整份会话作废，不能再把队列换掉。 */
function chosePlayback(before: PlaybackBaseline, after: PlaybackBaseline): boolean {
  return (
    before.queue !== after.queue ||
    before.currentIndex !== after.currentIndex ||
    before.loadedTrackId !== after.loadedTrackId
  );
}

/** 已下发给引擎的「下一首」。 */
export interface NextHandoff {
  queueUid: Id;
  trackId: Id;
  /** 为它**预先**生成的装载 ID：越过边界后的事件都由这一代认领。 */
  loadId: Id;
  revision: number;
}

interface PlayerState {
  /** ---- 播放队列 ---- */
  queue: QueueItem[];
  /** 当前播放项在 queue 中的下标（-1 表示空）。 */
  currentIndex: number;
  /**
   * 随机播放顺序（队列项 uid 的排列）。`null` = 顺序播放。
   *
   * 存一份顺序而不是「每次 next 随机取一首」：后者会重复、会漏，
   * 一个 10 首的队列放到第 10 首时仍有约 35% 的曲目一次没放过。
   * 洗一次牌再按牌序走，才是用户以为的「随机播放」。
   */
  shuffleOrder: Id[] | null;

  /** ---- 播放状态 ---- */
  playing: boolean;
  repeat: RepeatMode;
  shuffle: boolean;
  /** 音量 0..1。 */
  volume: number;
  muted: boolean;

  /** ---- 进度（秒） ---- */
  progress: PlaybackProgress;
  /** 引擎状态。与 `playing` 的区别：它还能表达装载中、播完、出错。 */
  status: PlayerStatus;
  /**
   * 引擎当前装载的曲目 ID；从未装载则为 null。
   *
   * 必须单独记，不能用 `current()` 推断：播放条显示哪首歌与引擎装载了哪首歌是
   * 两件事。曲库恢复后队列里换上了真实曲目，而引擎那边还是空的——此时按播放，
   * `engine.play()` 打在空引擎上是 no-op，界面却已经把图标切成了暂停。
   * 实测就是这个：按钮变了，进度一动不动。
   */
  loadedTrackId: Id | null;
  /**
   * 引擎尚未装载时待应用的位置，绑定到队列项 uid（队列允许同一曲目出现多次）。
   *
   * 会话恢复不自动播放，位置先留在这里；用户按播放时它随 `Load` 原子进入引擎，
   * 在预缓冲与解除暂停之前生效。显式切歌会换 uid，因此旧位置不会串到另一项。
   */
  pendingSeek: { queueUid: Id; positionSec: number } | null;
  /**
   * 会话恢复流程是否已经跑完（无论恢复成功还是回落到整库入队）。
   *
   * **在此之前一律不许写会话**。启动早期队列里是种子演示曲目，把它存下来会覆盖掉
   * 用户上次的真实会话——而种子曲目的 ID 在真实曲库里查不到，下次恢复就会整个失败，
   * 表现为「队列每次重启都回到第一首」。
   *
   * 这不是假想：React StrictMode 在开发模式下会 mount → unmount → mount，
   * `usePersistSession` 卸载时那句「退出前补存一次」于是在**应用刚启动**时就触发了，
   * 存下的正是种子队列。实测复现，一次就把会话打回原形。
   */
  sessionReady: boolean;
  /**
   * 当前装载请求的代际 ID。与曲目 ID 分开：单曲循环、失败重试都会连续装载同一首，
   * 只比曲目 ID 无法识别上一代迟到的状态与结束事件。
   */
  activeLoadId: Id | null;
  /** 当前显式装载创建的播放链 ID；gapless 交接只换 activeLoadId，不换这一个。 */
  activeChainId: Id | null;
  /**
   * 已发出、引擎还没回过任何事件的**显式**装载。
   *
   * 它的唯一职责是给迟到的越界通知定优先级：用户刚点了另一首歌，而引擎在处理这条
   * 装载之前恰好越过了上一首的边界，那条 `trackChanged` 描述的是一个已被取代的时刻。
   * 接受它会把界面切到一首正在被替换的歌上，更糟的是 `activeLoadId` 会被改成那一代，
   * 此后新曲的事件全被当成过期丢掉——界面就此冻住。
   *
   * 只能用这个标记，不能拿「和已下发的下一首比对」代替：队列换了下一首而边界**已经**
   * 越过时，那首歌确实正在响，此时丢掉事件就是另一个方向的音画分裂。
   */
  pendingLoadId: Id | null;
  /**
   * 已经下发给引擎的「下一首」。`null` = 已明确告知它「放完就停」。
   *
   * 记队列项 uid 而不是曲目 ID：同一首歌可以多次入队，而拖拽重排会换位置不换 uid，
   * 越过边界后正是靠它在当前队列里定位。
   */
  nextHandoff: NextHandoff | null;
  /**
   * 尚可能从引擎回来的 handoff 历史，按 loadId 对账。
   *
   * 改 next 时旧边界可能已经越过；只留最新一项会把旧曲事件误认成新指定。
   * 作废项不会回事件，因此保留一个小窗口并在交接、显式装载或失败时清理。
   */
  handoffHistory: NextHandoff[];
  /**
   * 队列版本号，每次重新指定下一首时 +1，随命令下发并由切歌事件回带。
   *
   * 只用于对账与诊断（这次交接依据的是哪一版队列），**不作为丢弃事件的判据**——
   * 已经发声的切歌是既成事实。
   */
  queueRevision: number;
  /**
   * 上一次播放失败的原因；成功装载时清空。
   *
   * 单独存而不是塞进 `status: "error"`：界面既要知道「出错了」，也要知道
   * 「错在哪一步」才能给出可操作的提示——找不到文件、格式不支持、设备被占用，
   * 用户要做的事完全不同。
   */
  error: PlaybackError | null;
  /**
   * 当前曲目没有本地文件路径（种子演示曲库就是这样）。
   *
   * 与 `error` 分开，因为它根本不是故障：用户还没扫描过曲库而已，
   * 提示语该是「去添加音乐文件夹」而不是「播放失败」。
   */
  needsLibrary: boolean;

  /** ---- 收藏（用户数据，落在 library.db，见 core/src/collections.rs） ---- */
  favorites: Record<Id, boolean>;
  /**
   * 专辑收藏的**派生视图**，键是专辑 ID，只供组件读。
   *
   * 权威在下面的 `favoriteAlbumTracks`：专辑 ID 由含目录的归组键哈希而来，改标签、
   * 挪文件、重扫都会变（见 `core/src/id.rs`），拿它当持久化的键，用户整理一次音乐
   * 文件夹收藏就没了。所以落盘的是曲目 ID，这份映射按当前曲库现算。
   */
  favoriteAlbums: Record<Id, boolean>;
  /** 被收藏专辑的曲目 ID 集合（权威）。「当前专辑里任意一首在集合内」即算已收藏。 */
  favoriteAlbumTracks: Record<Id, true>;
  /** 收藏歌手（当前以歌手名为键，后期换稳定 ID）。 */
  favoriteArtists: Record<string, boolean>;
  /** 收藏歌单（以歌单 ID 为键）。 */
  favoritePlaylists: Record<Id, boolean>;
  /** 歌单（用户数据，可变；种子初始化，后期由后端持久化）。 */
  playlists: Playlist[];

  /** ---- 音频设备 ---- */
  /**
   * 可用输出端点，来自后端。空数组表示还没问过——**不是**「一台设备都没有」。
   *
   * 不持久化：设备会插拔，存一份只会让菜单显示已经拔掉的耳机。用户的**选择**是另一
   * 回事，那份跟着界面设置走（`useUiStore.outputDevice`）。
   */
  devices: AudioDeviceInfo[];
  /**
   * 此刻实际在哪台端点上出声。由引擎事件回带，**不是**用户的选择。
   *
   * 两者必须分开：用户选的那台可能已经拔了，而「跟随系统默认」本身也不是一个端点。
   * 界面要如实回答「现在声音从哪出来」，只能问这一个。
   */
  effectiveDeviceId: Id | null;
  /** 前端已发出的最新设备选择版本；端点回执只接纳这一版。 */
  deviceRevision: number;
  /** 选定的端点用不了。播放**未受影响**，仍在原端点上放（见 `deviceRejected`）。 */
  deviceError: PlaybackError | null;

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
  /** 启动时把落盘的收藏灌进来（由 `useRestoreCollections` 调用）。 */
  restoreFavorites: (favorites: Favorites) => void;
  /** 按当前曲库重算专辑收藏的派生视图。曲库一换就要重算。 */
  recomputeFavoriteAlbums: () => void;
  /** 把曲目加入歌单（按曲目 ID 去重，重复加入为 no-op）；更新时间标记清空 = 「今天更新」。 */
  addToPlaylist: (playlistId: Id, tracks: Track[]) => void;
  /** 新建并收藏歌单后加入曲目；名称默认「新歌单」，重名自动加序号。返回新歌单 ID。 */
  createPlaylistWithTracks: (baseName: string, tracks: Track[]) => Id;
  /** 从歌单移除曲目。 */
  removeFromPlaylist: (playlistId: Id, trackId: Id) => void;
  /** 用新顺序替换歌单曲目（拖拽重排）。 */
  reorderPlaylist: (playlistId: Id, tracks: Track[]) => void;
  /** 用新顺序替换歌单列表本身（歌单页拖拽重排，即「自定义顺序」）。 */
  reorderPlaylists: (playlists: Playlist[]) => void;
  /** 重命名歌单。 */
  renamePlaylist: (playlistId: Id, title: string) => void;
  /** 删除歌单（同时清掉它的收藏标记）。 */
  deletePlaylist: (playlistId: Id) => void;
  setVolume: (v: number) => void;
  toggleMuted: () => void;
  seek: (positionSec: number) => void;
  /** 供音频引擎回灌进度。 */
  setProgress: (p: Partial<PlaybackProgress>) => void;
  /** 把当前曲目送进引擎。切歌后调用；`autoplay` 决定装载完是否立即出声。 */
  loadCurrent: (autoplay: boolean) => void;
  /**
   * 按当前队列重算「下一首」并下发给引擎（无缝换曲的前提）。
   *
   * 由 `useSyncNext` 在队列、循环、随机状态或当前曲目变化时调用，业务代码不用管——
   * 放在每个队列动作里就意味着将来新增的动作会漏掉它，而漏掉的表现是「偶尔不无缝」。
   */
  syncNext: () => void;
  /**
   * 真实曲库到位后接管队列：队列仍是种子演示曲目时换成曲库第一首。
   * **只换不播**——启动即出声是没人要的行为。
   */
  adoptLibrary: (tracks: Track[]) => void;
  /**
   * 从持久化的会话恢复队列与播放状态。**必须在曲库就绪之后调用**——
   * 会话只存曲目 ID，曲目本体要按 ID 回查曲库。
   *
   * 返回是否恢复成功；失败时调用方回落到 `adoptLibrary`。
   */
  restoreSession: (
    lookup: (id: Id) => Track | undefined,
    options?: { signal?: AbortSignal; baseline?: PlaybackBaseline },
  ) => Promise<boolean>;
  /** 真实曲库的恢复 / 首次接管已经结束，从此允许写播放会话。 */
  markSessionReady: () => void;
  /** 把当前会话写回后端。由 `usePersistSession` 节流调用，业务代码不用管。 */
  persistSession: () => void;
  /** 订阅引擎事件并接管播放状态。返回取消订阅函数；由 `App` 在挂载时调用一次。 */
  attachEngine: () => Promise<() => void>;
  /** 清空当前曲目之后的队列（歌词页队列面板「清除」）。 */
  clearUpNext: () => void;
  /** 用新顺序替换「接下来」队列（拖拽重排）。 */
  reorderUpNext: (items: QueueItem[]) => void;
  /** 跳到队列中的指定项并播放。 */
  playQueueItem: (uid: Id) => void;
  /** 移除队列中的指定项（当前播放项不可移除）。 */
  removeQueueItem: (uid: Id) => void;
  /** 「下一首播放」：插入到当前项之后。 */
  enqueueNext: (track: Track) => void;
  /** 追加到队尾。 */
  enqueue: (track: Track) => void;
  /** 重新问后端要一份端点列表。菜单每次打开都该调。 */
  refreshDevices: () => Promise<void>;
  /**
   * 把选定的端点下发给引擎；`null` = 跟随系统默认。
   *
   * 调用点是 `useSyncDevice`，不是设置页——「用户选了哪台」的权威在界面设置里，
   * 这里只负责落实。核对设备是否还在也归那个 hook（它拿得到列表）。
   */
  setDeviceOnEngine: (deviceId: Id | null, deviceRevision: number) => Promise<void>;
  dismissDeviceError: () => void;
}

const REPEAT_CYCLE: RepeatMode[] = ["off", "all", "one"];

/** 切到新曲目时的全新进度（位置归零、时长取新曲）。 */
function freshProgress(track: Track): PlaybackProgress {
  return { positionSec: 0, durationSec: track.durationSec, bufferedSec: track.durationSec };
}

/**
 * 洗牌（Fisher-Yates），把 `first` 固定在首位。
 *
 * 固定当前曲目是必须的：开随机时正在放的那首不该被换掉——用户按的是「之后随机」，
 * 不是「立刻换一首」。
 */
function shuffled(uids: Id[], first?: Id): Id[] {
  const rest = uids.filter((u) => u !== first);
  for (let i = rest.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [rest[i], rest[j]] = [rest[j], rest[i]];
  }
  return first !== undefined && uids.includes(first) ? [first, ...rest] : rest;
}

/**
 * 把队列投影成实际播放顺序。随机顺序是权威，但这里会防御性地去掉失效 uid、
 * 补回漏同步的新项，避免一处旧状态让后继计算直接停住。
 */
export function orderedQueue(
  queue: QueueItem[],
  order: Id[] | null,
): QueueItem[] {
  if (order === null) return queue;
  const byUid = new Map(queue.map((item) => [item.uid, item]));
  const seen = new Set<Id>();
  const ordered: QueueItem[] = [];
  for (const uid of order) {
    const item = byUid.get(uid);
    if (item && !seen.has(uid)) {
      seen.add(uid);
      ordered.push(item);
    }
  }
  for (const item of queue) {
    if (!seen.has(item.uid)) ordered.push(item);
  }
  return ordered;
}

/** 当前项之后真正会播放的队列；面板与 next() 共用同一份顺序事实。 */
export function upNextItems(
  queue: QueueItem[],
  currentIndex: number,
  order: Id[] | null,
): QueueItem[] {
  const ordered = orderedQueue(queue, order);
  const currentUid = queue[currentIndex]?.uid;
  if (currentUid === undefined) return ordered;
  const currentPos = ordered.findIndex((item) => item.uid === currentUid);
  return currentPos < 0 ? [] : ordered.slice(currentPos + 1);
}

/**
 * 求下一首的队列下标；没有下一首返回 -1。
 *
 * 随机与顺序的差别只在「排在谁后面」，循环规则完全共用——分开写两套是这类
 * 播放器最容易长出不一致行为的地方（比如随机模式下「单曲循环」失效）。
 */
function nextIndex(
  queue: QueueItem[],
  currentIndex: number,
  order: Id[] | null,
  repeat: RepeatMode,
  wrap: boolean,
): number {
  if (queue.length === 0) return -1;
  const ordered = orderedQueue(queue, order);
  const uid = queue[currentIndex]?.uid;
  const pos = ordered.findIndex((item) => item.uid === uid);
  if (pos < 0) return -1;
  const at = pos + 1;
  const nextUidValue = at < ordered.length
    ? ordered[at].uid
    : wrap && repeat === "all"
      ? ordered[0]?.uid
      : undefined;
  if (nextUidValue === undefined) return -1;
  const idx = queue.findIndex((q) => q.uid === nextUidValue);
  return idx;
}

/** 求上一首的队列下标；没有上一首返回 -1。 */
function prevIndex(queue: QueueItem[], currentIndex: number, order: Id[] | null): number {
  if (queue.length === 0) return -1;
  const ordered = orderedQueue(queue, order);
  const pos = ordered.findIndex((item) => item.uid === queue[currentIndex]?.uid);
  if (pos <= 0) return -1;
  return queue.findIndex((q) => q.uid === ordered[pos - 1].uid);
}

/**
 * 自然播完之后该接哪一首。
 *
 * 与手动 `next()` 的区别：这里遵守**真实的**循环模式（单曲循环 = 接自己），
 * 而手动切歌刻意忽略它——用户按下一首就是要下一首，不该被单曲循环困住。
 */
function successorItem(s: PlayerState): QueueItem | undefined {
  if (s.queue.length === 0 || s.currentIndex < 0) return undefined;
  const idx =
    s.repeat === "one"
      ? s.currentIndex
      : nextIndex(s.queue, s.currentIndex, s.shuffleOrder, s.repeat, true);
  const item = idx >= 0 ? s.queue[idx] : undefined;
  // 没有路径的曲目（种子演示曲库）真引擎放不了；假引擎不读文件，照常接。
  if (!item || (!item.track.path && engine.requiresPath)) return undefined;
  return item;
}

/** 把一个队列项变成随命令下发的指定，并生成它自己的装载 ID。 */
function designate(item: QueueItem, revision: number): { handoff: NextHandoff; args: NextTrackArgs } {
  const loadId = nextLoadId();
  // 假引擎在边界处要自己换到下一首，那一刻没有调用方可问时长。
  hintDuration(item.track.id, item.track.durationSec);
  return {
    handoff: { queueUid: item.uid, trackId: item.track.id, loadId, revision },
    args: {
      path: item.track.path ?? "",
      trackId: item.track.id,
      loadId,
    },
  };
}

const HANDOFF_HISTORY_LIMIT = 32;

function rememberHandoff(history: NextHandoff[], handoff: NextHandoff): NextHandoff[] {
  return [...history.filter((item) => item.loadId !== handoff.loadId), handoff].slice(
    -HANDOFF_HISTORY_LIMIT,
  );
}

/** 初始队列：仅放入演示曲目，进度停在 43%（对齐设计稿）。 */
const initialQueue: QueueItem[] = [
  { uid: nextUid(), track: DEMO_TRACK, source: "user" },
];

export const usePlayerStore = create<PlayerState>((set, get) => ({
  queue: initialQueue,
  currentIndex: 0,
  shuffleOrder: null,

  // 初始不自动播放：应用一启动就出声是没人要的行为，
  // 何况此时引擎尚未装载任何文件，`playing: true` 只会是个骗人的图标。
  playing: false,
  repeat: "off",
  shuffle: false,
  volume: 0.68,
  muted: false,

  progress: {
    positionSec: Math.round(DEMO_TRACK.durationSec * 0.43),
    durationSec: DEMO_TRACK.durationSec,
    bufferedSec: DEMO_TRACK.durationSec,
  },
  status: "idle",
  loadedTrackId: null,
  activeLoadId: null,
  activeChainId: null,
  pendingLoadId: null,
  nextHandoff: null,
  handoffHistory: [],
  queueRevision: 0,
  pendingSeek: null,
  sessionReady: false,
  error: null,
  needsLibrary: false,

  favorites: { ...SEED_FAVORITE_TRACKS },
  favoriteAlbums: { ...SEED_FAVORITE_ALBUMS },
  favoriteAlbumTracks: {},
  favoriteArtists: { ...SEED_FAVORITE_ARTISTS },
  favoritePlaylists: { "pl-nightdrive": true },
  playlists: PLAYLISTS.map((p) => ({ ...p, tracks: [...p.tracks] })),

  devices: [],
  effectiveDeviceId: null,
  deviceRevision: 0,
  deviceError: null,

  current: () => {
    const { queue, currentIndex } = get();
    return currentIndex >= 0 && currentIndex < queue.length
      ? queue[currentIndex].track
      : null;
  },

  play: (track) => {
    if (!track) {
      const s = get();
      const cur = s.current();
      // 引擎装的不是当前这首（启动后第一次按播放就是这种情况）——先装载。
      if (cur && s.loadedTrackId !== cur.id) {
        get().loadCurrent(true);
        return;
      }
      // 播完之后再按则从头开始，否则用户只能眼看着按钮没反应。
      if (s.status === "ended") {
        void engine.seek(0);
        void engine.play();
        set({ playing: true, progress: { ...s.progress, positionSec: 0 } });
        return;
      }
      void engine.play();
      set({ playing: true });
      return;
    }
    const s = get();
    const idx = s.queue.findIndex((q) => q.track.id === track.id);
    if (idx >= 0) {
      set({ currentIndex: idx, playing: true, pendingSeek: null, progress: freshProgress(track) });
    } else {
      const item: QueueItem = { uid: nextUid(), track, source: "user" };
      set({
        queue: [...s.queue, item],
        currentIndex: s.queue.length,
        shuffleOrder: s.shuffleOrder
          ? [...orderedQueue(s.queue, s.shuffleOrder).map((q) => q.uid), item.uid]
          : null,
        playing: true,
        pendingSeek: null,
        progress: freshProgress(track),
      });
    }
    get().loadCurrent(true);
  },

  playQueue: (tracks, startIndex = 0) => {
    if (tracks.length === 0) return;
    const queue: QueueItem[] = tracks.map((track) => ({ uid: nextUid(), track, source: "user" }));
    const idx = Math.max(0, Math.min(startIndex, queue.length - 1));
    // 换队列要重洗牌：沿用旧顺序的话，随机模式下会跳到一首已经不在队列里的歌。
    const shuffleOrder = get().shuffle ? shuffled(queue.map((q) => q.uid), queue[idx].uid) : null;
    set({
      queue,
      currentIndex: idx,
      shuffleOrder,
      playing: true,
      pendingSeek: null,
      progress: freshProgress(queue[idx].track),
    });
    get().loadCurrent(true);
  },

  pause: () => {
    void engine.pause();
    set({ playing: false });
  },

  togglePlay: () => {
    const s = get();
    if (s.playing) {
      s.pause();
    } else {
      s.play();
    }
  },

  next: () => {
    const s = get();
    if (s.queue.length === 0) return;
    // 手动切歌时「单曲循环」不该困住用户——他按的是下一首，就给下一首。
    // 自然播完的循环由引擎事件那条路处理（见 attachEngine）。
    const idx = nextIndex(s.queue, s.currentIndex, s.shuffleOrder, "all", true);
    if (idx < 0) return;
    set({ currentIndex: idx, pendingSeek: null, progress: freshProgress(s.queue[idx].track) });
    get().loadCurrent(true);
  },

  prev: () => {
    const s = get();
    if (s.queue.length === 0) return;
    // 3 秒内回退到上一首，否则回到本曲开头。
    if (s.progress.positionSec > 3) {
      get().seek(0);
      return;
    }
    const idx = prevIndex(s.queue, s.currentIndex, s.shuffleOrder);
    if (idx < 0) {
      get().seek(0);
      return;
    }
    set({ currentIndex: idx, pendingSeek: null, progress: freshProgress(s.queue[idx].track) });
    get().loadCurrent(true);
  },

  toggleShuffle: () =>
    set((s) => {
      const on = !s.shuffle;
      return {
        shuffle: on,
        shuffleOrder: on ? shuffled(s.queue.map((q) => q.uid), s.queue[s.currentIndex]?.uid) : null,
      };
    }),
  cycleRepeat: () =>
    set((s) => ({ repeat: REPEAT_CYCLE[(REPEAT_CYCLE.indexOf(s.repeat) + 1) % 3] })),

  // 四个收藏动作都是**先改界面、写失败再退回**。等 IPC 回来才亮红心会让点击有可见延迟，
  // 而收藏是个高频小动作；反过来，写失败却把红心留着，用户会以为存住了——所以退回是
  // 必须的。退回本身就是「没存上」的信号（已知不足：目前没有文字说明，见文档）。
  toggleFavorite: (id) => {
    const on = !get().favorites[id];
    set((s) => ({ favorites: { ...s.favorites, [id]: on } }));
    void backend.favoriteTrack(id, on).catch((error) => {
      console.error("保存收藏失败", error);
      set((s) => ({ favorites: { ...s.favorites, [id]: !on } }));
    });
  },

  toggleFavoriteAlbum: (id) => {
    const album = albums().find((a) => a.id === id);
    // 专辑没了（重扫后 ID 变了、或详情页开着时曲库被替换）就什么都不做：
    // 拿不到曲目 ID 就无从表达这次收藏，写一条空集合只会留下一个谁也命中不了的记录。
    if (!album) return;
    const trackIds = tracksOf(album).map((tk) => tk.id);
    if (trackIds.length === 0) return;
    const on = !get().favoriteAlbums[id];
    set((s) => {
      const albumTracks = { ...s.favoriteAlbumTracks };
      for (const tid of trackIds) {
        if (on) albumTracks[tid] = true;
        else delete albumTracks[tid];
      }
      return {
        favoriteAlbumTracks: albumTracks,
        favoriteAlbums: { ...s.favoriteAlbums, [id]: on },
      };
    });
    void backend.favoriteAlbum(trackIds, on).catch((error) => {
      console.error("保存收藏失败", error);
      set((s) => {
        const albumTracks = { ...s.favoriteAlbumTracks };
        for (const tid of trackIds) {
          if (on) delete albumTracks[tid];
          else albumTracks[tid] = true;
        }
        return {
          favoriteAlbumTracks: albumTracks,
          favoriteAlbums: { ...s.favoriteAlbums, [id]: !on },
        };
      });
    });
  },

  toggleFavoriteArtist: (name) => {
    const on = !get().favoriteArtists[name];
    set((s) => ({ favoriteArtists: { ...s.favoriteArtists, [name]: on } }));
    void backend.favoriteArtist(name, on).catch((error) => {
      console.error("保存收藏失败", error);
      set((s) => ({ favoriteArtists: { ...s.favoriteArtists, [name]: !on } }));
    });
  },

  toggleFavoritePlaylist: (id) => {
    const on = !get().favoritePlaylists[id];
    set((s) => ({ favoritePlaylists: { ...s.favoritePlaylists, [id]: on } }));
    void backend.favoritePlaylist(id, on).catch((error) => {
      console.error("保存收藏失败", error);
      set((s) => ({ favoritePlaylists: { ...s.favoritePlaylists, [id]: !on } }));
    });
  },

  restoreFavorites: (favorites) => {
    const toMap = <T extends string>(list: T[]): Record<T, boolean> =>
      Object.fromEntries(list.map((k) => [k, true])) as Record<T, boolean>;
    set({
      favorites: toMap(favorites.tracks),
      favoriteAlbumTracks: Object.fromEntries(
        favorites.albumTracks.map((k) => [k, true as const]),
      ),
      favoriteArtists: toMap(favorites.artists),
      favoritePlaylists: toMap(favorites.playlists),
    });
    get().recomputeFavoriteAlbums();
  },

  // 专辑收藏存的是曲目 ID，界面按专辑 ID 读，所以两者之间要现算一层。曲库一换就得重来：
  // 专辑 ID 变了、专辑本身也可能被重新归组。
  recomputeFavoriteAlbums: () => {
    const albumTracks = get().favoriteAlbumTracks;
    const derived: Record<Id, boolean> = {};
    if (Object.keys(albumTracks).length > 0) {
      for (const album of albums()) {
        // 「任意一首命中」而不是「全部」：专辑的曲目集合会变（补一首漏扫的、
        // 删一首重复的），按全部匹配会让收藏毫无征兆地消失。
        if (tracksOf(album).some((tk) => albumTracks[tk.id])) derived[album.id] = true;
      }
    }
    set({ favoriteAlbums: derived });
  },

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

  removeFromPlaylist: (playlistId, trackId) =>
    set((s) => ({
      playlists: s.playlists.map((p) =>
        p.id === playlistId
          ? { ...p, tracks: p.tracks.filter((tk) => tk.id !== trackId), updatedLabel: "" }
          : p,
      ),
    })),

  reorderPlaylist: (playlistId, tracks) =>
    set((s) => ({
      playlists: s.playlists.map((p) =>
        p.id === playlistId ? { ...p, tracks: [...tracks], updatedLabel: "" } : p,
      ),
    })),

  reorderPlaylists: (playlists) => set({ playlists: [...playlists] }),

  renamePlaylist: (playlistId, title) =>
    set((s) => ({
      playlists: s.playlists.map((p) =>
        p.id === playlistId ? { ...p, title, updatedLabel: "" } : p,
      ),
    })),

  deletePlaylist: (playlistId) =>
    set((s) => {
      // 收藏标记一并清除，避免留下指向已删歌单的孤立键。
      const { [playlistId]: _removed, ...favoritePlaylists } = s.favoritePlaylists;
      return { playlists: s.playlists.filter((p) => p.id !== playlistId), favoritePlaylists };
    }),

  setVolume: (v) => {
    const volume = Math.max(0, Math.min(1, v));
    void engine.setVolume(volume);
    set({ volume, muted: volume === 0 });
  },

  toggleMuted: () =>
    set((s) => {
      const muted = !s.muted;
      // 静音送 0 而不是记着音量不发命令：引擎那边的音量斜坡（15 ms）才是防爆音的地方。
      void engine.setVolume(muted ? 0 : s.volume);
      return { muted };
    }),

  seek: (positionSec) => {
    const s = get();
    const target = Math.max(0, Math.min(positionSec, s.progress.durationSec));
    const currentItem = s.queue[s.currentIndex];
    const engineHasCurrent =
      currentItem !== undefined &&
      s.loadedTrackId === currentItem.track.id &&
      s.activeLoadId !== null;
    if (engineHasCurrent) {
      void engine.seek(target);
    }
    // 乐观更新：等引擎的进度事件回来要 200 ms，拖动进度条时那是看得见的滞后。
    // 引擎尚未装载时则把位置绑定到当前队列项，首次 Load 会原子应用；0 秒等价于没有待定位。
    set({
      pendingSeek:
        !engineHasCurrent && currentItem !== undefined && target > 0
          ? { queueUid: currentItem.uid, positionSec: target }
          : null,
      progress: { ...s.progress, positionSec: target },
    });
  },

  setProgress: (p) => set((s) => ({ progress: { ...s.progress, ...p } })),

  /**
   * 把当前曲目送进引擎。
   *
   * 没有 `path` 的曲目（种子演示曲库）不是错误：用户只是还没扫描过。
   * 这时置 `needsLibrary` 让界面去提示「添加音乐文件夹」，而不是报一个
   * 让人以为文件坏了的播放失败。
   */
  loadCurrent: (autoplay) => {
    const s = get();
    const currentItem = s.queue[s.currentIndex];
    if (!currentItem) return;
    const { track } = currentItem;
    const initialPositionSec =
      s.pendingSeek?.queueUid === currentItem.uid ? s.pendingSeek.positionSec : null;
    // 只有真引擎才需要文件。假引擎不读文件，种子曲库在浏览器预览里照样能放，
    // 否则 `pnpm dev` 里的播放、切歌、循环模式会全是死的。
    if (!track.path && engine.requiresPath) {
      set({
        needsLibrary: true,
        playing: false,
        status: "idle",
        error: null,
        loadedTrackId: null,
        activeLoadId: null,
        activeChainId: null,
        pendingLoadId: null,
        nextHandoff: null,
        handoffHistory: [],
      });
      return;
    }
    // 浏览器预览的假引擎没有文件可读，时长得由这里告诉它。
    hintDuration(track.id, track.durationSec);
    // 乐观置 playing：装载到出声有几十毫秒，这期间按钮不该还停在「播放」上。
    // 真正的状态随后由引擎事件校正（装载失败时会被改回来）。
    const loadId = nextLoadId();
    const revision = s.queueRevision + 1;
    const successor = successorItem(s);
    const next = successor ? designate(successor, revision) : null;
    set({
      needsLibrary: false,
      error: null,
      loadedTrackId: track.id,
      activeLoadId: loadId,
      activeChainId: loadId,
      pendingLoadId: loadId,
      nextHandoff: next?.handoff ?? null,
      handoffHistory: next ? [next.handoff] : [],
      queueRevision: revision,
      playing: autoplay,
    });
    // 有效音量、初始位置与**下一首**都和 load 走同一条后端命令：拆成多个异步 invoke
    // 没有顺序保证——音量晚到让第一首落到默认满音量，位置晚到让续播先漏出曲首再跳，
    // 而下一首若早到，会被这条装载的 teardown 抹掉（无缝换曲于是时好时坏）。
    void engine
      .load({
        path: track.path ?? "",
        trackId: track.id,
        loadId,
        autoplay,
        initialVolume: s.muted ? 0 : s.volume,
        initialPositionSec,
        // 传设置而不是增益：倍率由后端按分析结果与播放策略算。开关是**装载时读一次**，
        // 因此中途改设置要到下一次装载才生效——管线领先播放约 1.5 秒，
        // 缓冲里那段 PCM 的增益已经写死了，假装立刻生效只会让人以为开关坏了。
        loudness: useUiStore.getState().settings.loudness,
        next: next?.args ?? null,
        queueRevision: revision,
      })
      .catch((error) => {
        // 命令根本没发出去。整份乐观装载都要撤掉；只清 pendingLoadId 会留下一个
        // 实际不存在的 activeLoadId，界面继续显示播放且初始 next 也永远不会重发。
        console.error("装载命令发送失败", error);
        set((st) =>
          st.activeLoadId === loadId
            ? {
                playing: false,
                status: "idle",
                loadedTrackId: null,
                activeLoadId: null,
                activeChainId: null,
                pendingLoadId: null,
                nextHandoff: null,
                handoffHistory: [],
              }
            : {},
        );
      });
  },

  syncNext: () => {
    const s = get();
    // 引擎那边什么都没在放，就谈不上「下一首」。
    if (s.activeLoadId === null || s.activeChainId === null || s.loadedTrackId === null) return;
    const successor = successorItem(s);
    // 与已经下发的那份一致就不重发：拖拽重排、在队尾增删常常并不改变下一首，
    // 而每次重发都会让引擎丢掉已经预解码好的那份、重新打开文件。
    if ((s.nextHandoff?.queueUid ?? null) === (successor?.uid ?? null)) return;
    const revision = s.queueRevision + 1;
    const next = successor ? designate(successor, revision) : null;
    set({
      nextHandoff: next?.handoff ?? null,
      handoffHistory: next ? rememberHandoff(s.handoffHistory, next.handoff) : s.handoffHistory,
      queueRevision: revision,
    });
    // 「没有下一首」也要明说：不说的话引擎会一直接着上次指定的那首，
    // 用户删掉队尾之后反而会绕回去。
    void engine
      .setNext(
        {
          chainId: s.activeChainId,
          queueRevision: revision,
          next: next?.args ?? null,
        },
        useUiStore.getState().settings.loudness,
      )
      .catch((error) => {
        console.error("更新下一首命令发送失败", error);
        // 只撤回仍是本次 revision 的乐观指定；更新的意图已经到来时不能反向覆盖。
        set((state) =>
          state.queueRevision === revision
            ? {
                nextHandoff: null,
                handoffHistory: state.handoffHistory.filter((h) => h.revision !== revision),
              }
            : {},
        );
      });
  },

  adoptLibrary: (tracks) => {
    if (tracks.length === 0) return;
    const s = get();
    // 只在用户还没自己选过曲目时接管。判据是「队列里的曲目都没有 path」——
    // 种子曲库没有真实文件，而真实曲目一定有。用「队列长度是否为 1」之类的
    // 结构判据会在用户手动播过一首演示曲后失灵。
    const untouched = s.queue.every((q) => !q.track.path);
    if (!untouched) return;
    // 整库进队列，不是只放第一首：只放一首的话「下一首」会原地打转
    // ——按钮亮着、按下去却什么都没发生，比按钮是灰的还费解。
    // 用户点专辑/歌单时 `playQueue` 会整个替换掉，这只是启动时的默认队列。
    const queue = tracks.map((track) => ({ uid: nextUid(), track, source: "auto" as const }));
    set({
      queue,
      currentIndex: 0,
      shuffleOrder: s.shuffle ? shuffled(queue.map((item) => item.uid), queue[0]?.uid) : null,
      playing: false,
      status: "idle",
      loadedTrackId: null,
      activeLoadId: null,
      activeChainId: null,
      pendingLoadId: null,
      nextHandoff: null,
      handoffHistory: [],
      needsLibrary: false,
      pendingSeek: null,
      progress: freshProgress(tracks[0]),
    });
  },

  restoreSession: async (lookup, options) => {
    // 基线默认取调用这一刻，但调用方应当把**整个恢复流程开始时**的快照传进来：
    // 曲库 IPC 本身也可能很慢，那段时间里的操作同样要优先于旧会话。
    const baseline = options?.baseline ?? get();
    const json = await loadSession();
    if (options?.signal?.aborted) return false;
    const now = get();
    // 只有「用户已经选了要放什么」才整份作废（也包括 StrictMode 另一轮恢复留下的队列）。
    if (chosePlayback(baseline, now)) return false;
    // 音量与模式是各自独立的意图：用户刚调好的值不该被旧会话盖回去，
    // 但那也不意味着他想丢掉队列。
    const keepVolume = now.volume !== baseline.volume || now.muted !== baseline.muted;
    const keepMode = now.repeat !== baseline.repeat || now.shuffle !== baseline.shuffle;
    const restored = json ? fromSession(json, lookup) : null;
    if (!restored) {
      // 没有会话、或会话已失效：调用方会回落到整库入队，并在两条路径汇合后
      // 统一调用 markSessionReady。就绪生命周期不再偷偷依赖 adoptLibrary 是否接管成功。
      return false;
    }

    const queue: QueueItem[] = restored.tracks.map((track) => ({
      uid: nextUid(),
      track,
      source: "auto" as const,
    }));
    const idx = Math.min(restored.currentIndex, queue.length - 1);
    const shuffle = keepMode ? now.shuffle : restored.shuffle;
    const volume = keepVolume ? now.volume : restored.volume;
    const muted = keepVolume ? now.muted : restored.muted;
    set({
      queue,
      currentIndex: idx,
      shuffleOrder: shuffle
        ? keepMode
          ? // 用户刚开的随机：他洗的是种子队列，那份顺序对恢复的队列没有意义，现洗一份。
            shuffled(
              queue.map((item) => item.uid),
              queue[idx]?.uid,
            )
          : // 存的是下标排列，这里映射回本次运行新生成的 uid。
            (restored.shuffleOrder?.map((i) => queue[i]?.uid).filter((u): u is Id => !!u) ?? null)
        : null,
      shuffle,
      repeat: keepMode ? now.repeat : restored.repeat,
      volume,
      muted,
      // 恢复不自动播放，但进度条要显示上次的位置。
      playing: false,
      status: "idle",
      loadedTrackId: null,
      activeLoadId: null,
      activeChainId: null,
      pendingLoadId: null,
      nextHandoff: null,
      handoffHistory: [],
      needsLibrary: false,
      error: null,
      progress: {
        positionSec: restored.positionSec,
        durationSec: queue[idx]?.track.durationSec ?? 0,
        bufferedSec: 0,
      },
      pendingSeek:
        restored.positionSec > 0
          ? { queueUid: queue[idx].uid, positionSec: restored.positionSec }
          : null,
    });
    // 音量要立刻同步给引擎：它是与装载无关的全局设置，
    // 等到第一次 load 才带过去的话，用户开播前调音量会打在默认值上。
    void engine.setVolume(muted ? 0 : volume);
    return true;
  },

  markSessionReady: () => set({ sessionReady: true }),

  persistSession: () => {
    const s = get();
    // 恢复流程没跑完就一个字都不写：此刻队列里是种子演示曲目，存下去会覆盖用户
    // 上次的真实会话，而种子 ID 在曲库里查不到，下次恢复必然失败。
    if (!s.sessionReady) return;
    if (s.queue.length === 0 || s.currentIndex < 0) return;
    void saveSession(JSON.stringify(toSession(s)));
  },

  attachEngine: async () => {
    return engine.onEvent((event) => {
      const s = get();

      // 无缝交接是**既成事实**：事件到达时新曲已经在响了。它带的是新曲那一代的装载 ID，
      // 与 activeLoadId 必然不同，因此不能走下面「只认当前代际」的过滤。
      if (event.type === "trackChanged") {
        // 唯一该丢掉它的情形：用户已经另点了一首（显式装载在途）。见 pendingLoadId。
        if (s.pendingLoadId !== null) return;
        // 事件可能晚于一次甚至多次 `setNext` 才抵达。只能按引擎回带的 loadId + revision
        // 去历史里认领，不能拿「此刻最新的 next」冒充：否则旧 B 的越界会被映射成新 C，
        // UI 先跳错曲，随后所有 B 的进度又因 loadId 不匹配而被丢掉，看起来像彻底冻住。
        const handoff = s.handoffHistory.find(
          (item) =>
            item.loadId === event.loadId &&
            item.revision === event.queueRevision &&
            (event.trackId == null || item.trackId === event.trackId),
        );
        const remainingHistory = s.handoffHistory.filter(
          (item) => item.revision > event.queueRevision && item.loadId !== event.loadId,
        );
        const remainingNext =
          s.nextHandoff?.loadId === event.loadId ? null : s.nextHandoff;
        // 显式 Load 会清空上一条播放链的历史。若它的迟到事件绕过了 pendingLoad 窗口，
        // 这里仍不能把“完全不认识”误当成“已知 handoff 后来被移出队列”去触发 fallback。
        if (!handoff) return;
        // 按 uid 对账，不做下标递增：随机顺序与拖拽重排都会让「下一个下标」失去意义。
        const idx = s.queue.findIndex((q) => q.uid === handoff.queueUid);
        if (idx < 0) {
          // 边界越过之后才收到「这首已被移出队列」——它已经在响，撤不回来了。
          // 但用户的意思是别放它，所以显式装载新队列此刻该放的那首：
          // 会有一次可解释的停顿，好过继续放一首他刚刚删掉的歌。
          const fallback = nextIndex(s.queue, s.currentIndex, s.shuffleOrder, s.repeat, true);
          if (fallback >= 0) {
            set({
              currentIndex: fallback,
              nextHandoff: null,
              handoffHistory: [],
              pendingSeek: null,
              progress: freshProgress(s.queue[fallback].track),
            });
            get().loadCurrent(true);
          } else {
            void engine.pause();
            set({ playing: false, nextHandoff: null, handoffHistory: [] });
          }
          return;
        }
        set({
          currentIndex: idx,
          loadedTrackId: event.trackId ?? s.queue[idx].track.id,
          activeLoadId: event.loadId,
          // 只消费事件认领的那份；若旧边界越过后用户已经指定了更新的 next，必须保留。
          nextHandoff: remainingNext,
          handoffHistory: remainingHistory,
          pendingSeek: null,
          // 不在这里猜 transport 状态：Pause 可能与越界事件交错，强行写 playing 会把
          // 已经生效的暂停覆盖掉。真正的状态由同一代的 status 事件负责。
          error: null,
          needsLibrary: false,
          progress: {
            positionSec: 0,
            // 时长以引擎读到的为准，标签里的值未必与实际码流一致。
            durationSec: event.format.durationSec ?? s.queue[idx].track.durationSec,
            bufferedSec: 0,
          },
        });
        get().syncNext();
        return;
      }

      // 端点事件与「现在放的是哪一首」无关：换端点不改变曲目，而且用户在**什么都
      // 没放**的时候也能改，那一刻引擎根本没有装载上下文可盖章。所以它们和
      // `trackChanged` 一样走在代际过滤之前，否则空闲时那一条会被整条丢掉。
      if (event.type === "outputChanged") {
        if (event.deviceRevision !== s.deviceRevision) return;
        // 不是新装载：队列位置与待定位都不因它改变，只更新「现在从哪台设备出声」。
        // 提示由下一次明确选择在 pick() 里撤掉；拒绝后的自动回滚也会收到确认，若在这里
        // 清错，用户只会看到菜单弹回去，却来不及看见为什么。
        set({ effectiveDeviceId: event.format.deviceId });
        return;
      }
      if (event.type === "deviceRejected") {
        if (event.deviceRevision !== s.deviceRevision) return;
        // **不是播放失败**：音乐仍在原端点上响着，所以只记提示，不动任何播放状态。
        set({ deviceError: event.error });
        // 回滚依据必须来自引擎真正保留的偏好，而不是前端的一格点击历史：连续快速选择时，
        // 多次预检会交错返回，一格历史既可能指向被拒项，也可能覆盖更早的有效选择。
        const fallback =
          event.preferredDeviceId === null
            ? null
            : {
                id: event.preferredDeviceId,
                label:
                  event.preferredDeviceLabel ??
                  s.devices.find((device) => device.id === event.preferredDeviceId)?.label ??
                  event.preferredDeviceId,
              };
        useUiStore.getState().setOutputDevice(fallback);
        return;
      }

      // 带着在途装载 ID 的事件一到，就说明那条命令已经进了引擎，标记可以撤了。
      // 不撤的话它会一直挡着越界通知（IPC 异常那条路已在 loadCurrent 里兜住）。
      if (event.loadId === s.pendingLoadId) set({ pendingLoadId: null });

      const currentId = s.current()?.id;
      // 以装载代际为第一判据：同一首连续重载时 trackId 相同，只有 loadId 能识别迟到事件。
      if (event.loadId !== s.activeLoadId) return;
      // trackId 再做一层契约防御；正常情况下它与当前曲目必然一致。
      if (event.trackId != null && currentId != null && event.trackId !== currentId) return;

      switch (event.type) {
        case "opened": {
          // 时长以**引擎读到的**为准：标签里的时长未必与实际码流一致，
          // 而进度条的量程必须跟发声的那份对齐，否则拖到末尾会差一截。
          set((st) => ({
            status: "loading",
            error: null,
            // 「跟随系统默认」不是一个端点，只有装载完成才知道它解析到了哪一台。
            effectiveDeviceId: event.format.deviceId,
            // 初始位置已经随 Load 在引擎侧生效；Opened 到达说明这份待办可以作废。
            pendingSeek: null,
            progress: { ...st.progress, durationSec: event.format.durationSec ?? st.progress.durationSec },
          }));
          break;
        }

        case "status":
          set({
            status: event.status,
            // `playing` 是给界面用的派生量；以引擎的状态为准，避免图标与实际不符。
            playing: event.status === "playing",
            ...(event.status === "error" ? {} : { needsLibrary: false }),
          });
          break;

        case "progress":
          set((st) => ({
            progress: {
              positionSec: event.positionSec,
              durationSec: event.durationSec ?? st.progress.durationSec,
              bufferedSec: event.bufferedSec,
            },
          }));
          break;

        case "ended": {
          // 到这里说明引擎手上**没有**下一首：要么队列到头了，要么那首没能预先接上
          // （文件没了、格式不支持、种子曲目没有路径）。无缝换曲走的是 trackChanged，
          // 这条分支于是成了兜底路径——它照常按循环规则推进，只是会有一声停顿。
          set({ nextHandoff: null, handoffHistory: [] });
          if (s.repeat === "one") {
            void engine.seek(0);
            void engine.play();
            set({ playing: true, progress: { ...s.progress, positionSec: 0 } });
            return;
          }
          const idx = nextIndex(s.queue, s.currentIndex, s.shuffleOrder, s.repeat, true);
          if (idx < 0) {
            // 队列到头：停在末尾而不是跳回开头，"off" 的含义就是不要再放了。
            set({ playing: false, status: "ended" });
            return;
          }
          set({ currentIndex: idx, pendingSeek: null, progress: freshProgress(s.queue[idx].track) });
          get().loadCurrent(true);
          break;
        }

        case "failed":
          // 播不了就停下并如实说明。**不自动跳下一首**——整库格式不支持时那会变成
          // 一场无声的快进，用户完全不知道发生了什么。
          set({
            status: "error",
            playing: false,
            error: event.error,
            loadedTrackId: null,
            activeLoadId: null,
            activeChainId: null,
            pendingLoadId: null,
            // 引擎已经拆掉整条链路，待接续的那份跟着作废。
            nextHandoff: null,
            handoffHistory: [],
          });
          break;
      }
    });
  },

  clearUpNext: () =>
    set((s) => {
      const currentUid = s.queue[s.currentIndex]?.uid;
      if (currentUid === undefined) return s;
      const removed = new Set(upNextItems(s.queue, s.currentIndex, s.shuffleOrder).map((q) => q.uid));
      const queue = s.queue.filter((item) => !removed.has(item.uid));
      return {
        queue,
        currentIndex: queue.findIndex((item) => item.uid === currentUid),
        shuffleOrder: s.shuffleOrder
          ? orderedQueue(queue, s.shuffleOrder).map((item) => item.uid)
          : null,
      };
    }),

  reorderUpNext: (items) =>
    set((s) => {
      if (s.shuffleOrder === null) {
        return { queue: [...s.queue.slice(0, s.currentIndex + 1), ...items] };
      }
      const ordered = orderedQueue(s.queue, s.shuffleOrder);
      const currentUid = s.queue[s.currentIndex]?.uid;
      const currentPos = ordered.findIndex((item) => item.uid === currentUid);
      if (currentPos < 0) return s;

      const future = ordered.slice(currentPos + 1);
      const allowed = new Set(future.map((item) => item.uid));
      const seen = new Set<Id>();
      const reordered = items.filter((item) => {
        if (!allowed.has(item.uid) || seen.has(item.uid)) return false;
        seen.add(item.uid);
        return true;
      });
      // 拖拽库通常会回传完整 values；仍补回遗漏项，避免一次异常回调等价于删除歌曲。
      const omitted = future.filter((item) => !seen.has(item.uid));
      return {
        shuffleOrder: [
          ...ordered.slice(0, currentPos + 1).map((item) => item.uid),
          ...reordered.map((item) => item.uid),
          ...omitted.map((item) => item.uid),
        ],
      };
    }),

  playQueueItem: (uid) => {
    const s = get();
    const idx = s.queue.findIndex((q) => q.uid === uid);
    if (idx < 0) return;
    set({
      currentIndex: idx,
      playing: true,
      pendingSeek: null,
      progress: freshProgress(s.queue[idx].track),
    });
    get().loadCurrent(true);
  },

  removeQueueItem: (uid) =>
    set((s) => {
      const idx = s.queue.findIndex((q) => q.uid === uid);
      // 当前播放项不可移除：移除它等于「跳到下一首」，语义应由 next() 承担。
      if (idx < 0 || idx === s.currentIndex) return s;
      const queue = s.queue.filter((q) => q.uid !== uid);
      return {
        queue,
        currentIndex: idx < s.currentIndex ? s.currentIndex - 1 : s.currentIndex,
        // 随机顺序里也要摘掉，否则轮到它时会找不到对应队列项而直接停住。
        shuffleOrder: s.shuffleOrder?.filter((u) => u !== uid) ?? null,
      };
    }),

  enqueueNext: (track) =>
    set((s) => {
      const item: QueueItem = { uid: nextUid(), track, source: "user" };
      const queue = s.queue.slice();
      queue.splice(s.currentIndex + 1, 0, item);
      // 「下一首播放」在随机模式下也该是下一首——插到随机序列的当前项之后，
      // 而不是丢进乱序里等运气。
      let shuffleOrder = s.shuffleOrder;
      if (shuffleOrder) {
        shuffleOrder = orderedQueue(s.queue, shuffleOrder).map((q) => q.uid);
        const pos = shuffleOrder.indexOf(s.queue[s.currentIndex]?.uid);
        shuffleOrder.splice(pos < 0 ? shuffleOrder.length : pos + 1, 0, item.uid);
      }
      return { queue, shuffleOrder };
    }),

  enqueue: (track) =>
    set((s) => {
      const item: QueueItem = { uid: nextUid(), track, source: "auto" };
      return {
        queue: [...s.queue, item],
        shuffleOrder: s.shuffleOrder
          ? [...orderedQueue(s.queue, s.shuffleOrder).map((q) => q.uid), item.uid]
          : null,
      };
    }),

  refreshDevices: async () => {
    try {
      set({ devices: await engine.listDevices() });
    } catch (error) {
      // 列不出设备不该弹错误：菜单里已经有「系统默认输出」那一项，用户照样能用。
      console.error("列举输出设备失败", error);
    }
  },

  setDeviceOnEngine: async (deviceId, deviceRevision) => {
    set({ deviceRevision });
    await engine.setDevice(deviceId, deviceRevision);
  },

  dismissDeviceError: () => set({ deviceError: null }),
}));
