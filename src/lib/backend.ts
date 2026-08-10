import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { LibrarySnapshot, ScanProgress } from "@/types/generated/library";
import type { Favorites, Playlist } from "@/types/generated/collections";
import type { TrackMetadataPatch } from "@/types/generated/overrides";
import type { AudioDeviceInfo, PlayerEvent } from "@/types/generated/player";

/**
 * Rust 后端适配层。
 *
 * 所有 IPC 都经这里，组件不直接 invoke——这样浏览器 dev 环境（无 Tauri）
 * 只需在此处回落，不必在每个调用点写环境判断。
 *
 * 类型全部来自 `src/types/generated/`（由 core crate 的 ts-rs 产出），
 * Rust 结构一改，这里就编译报错。
 */

/** 是否运行在 Tauri 原生窗口内。 */
export const isTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** 扫描进度事件名，与 `src-tauri/src/lib.rs` 的 EVENT_SCAN_PROGRESS 一致。 */
const EVENT_SCAN_PROGRESS = "library://scan-progress";

/** 弹出系统文件夹选择器；返回所选路径，取消则为 null。 */
export async function pickMusicFolder(): Promise<string | null> {
  if (!isTauri()) return null;
  const picked = await open({ directory: true, multiple: false, recursive: true });
  return typeof picked === "string" ? picked : null;
}

/** 扫描给定文件夹，返回曲库快照。浏览器环境返回 null（调用方回落到种子数据）。 */
export async function scanLibrary(folders: string[]): Promise<LibrarySnapshot | null> {
  if (!isTauri()) return null;
  return invoke<LibrarySnapshot>("scan_library", { folders });
}

/** 取后端当前持有的曲库快照；尚未扫描时为 null。 */
export async function getLibrary(): Promise<LibrarySnapshot | null> {
  if (!isTauri()) return null;
  return invoke<LibrarySnapshot | null>("get_library");
}

/** 只统计候选音频文件数（不解析），用于开扫前显示规模。 */
export async function countAudioFiles(folders: string[]): Promise<number> {
  if (!isTauri()) return 0;
  return invoke<number>("count_audio_files", { folders });
}

/** 封面缩略图目录。前端据此按显示尺寸挑档位拼 URL，见 `src/lib/cover.ts`。 */
export async function getCoverDir(): Promise<string | null> {
  if (!isTauri()) return null;
  return invoke<string>("get_cover_dir");
}

/** 上次扫描用的音乐文件夹（设置页显示的是它，不是写死的示例路径）。 */
export async function getMusicFolders(): Promise<string[]> {
  if (!isTauri()) return [];
  return invoke<string[]>("get_music_folders");
}

/**
 * 元数据修改的字段三态，与后端 `Overrides::merge` 对齐：
 * 字段缺席 = 没动；文本空字符串 / 数字 null = 撤销；有值 = 改成这个值。
 *
 * 只提交用户真正动过的字段很重要——把界面上显示的推断值原样回写，
 * 等于把「猜的」固化成「用户指定的」，以后文件标签修好了也不会再更新。
 */
export type MetadataPatch = TrackMetadataPatch;

/** 改写单曲元数据，返回重新聚合后的曲库（后端走缓存，不重扫）。 */
export async function setTrackMetadata(
  trackId: string,
  patch: MetadataPatch,
): Promise<LibrarySnapshot | null> {
  if (!isTauri()) return null;
  return invoke<LibrarySnapshot>("set_track_metadata", { trackId, patch });
}

/** 改写整张专辑（后端展开为逐曲记录）。 */
export async function setAlbumMetadata(
  albumId: string,
  patch: MetadataPatch,
): Promise<LibrarySnapshot | null> {
  if (!isTauri()) return null;
  return invoke<LibrarySnapshot>("set_album_metadata", { albumId, patch });
}

/** 还原为文件里的原始信息。 */
export async function resetTrackMetadata(trackId: string): Promise<LibrarySnapshot | null> {
  if (!isTauri()) return null;
  return invoke<LibrarySnapshot>("reset_track_metadata", { trackId });
}

export async function resetAlbumMetadata(albumId: string): Promise<LibrarySnapshot | null> {
  if (!isTauri()) return null;
  return invoke<LibrarySnapshot>("reset_album_metadata", { albumId });
}

/** 订阅扫描进度。返回取消订阅函数。 */
export async function onScanProgress(
  handler: (p: ScanProgress) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  const unlisten = await listen<ScanProgress>(EVENT_SCAN_PROGRESS, (e) => handler(e.payload));
  return unlisten;
}

/* ============================================================
   播放引擎
   ============================================================ */

/** 播放事件名，与 `src-tauri/src/player.rs` 的 EVENT_PLAYER 一致。 */
const EVENT_PLAYER = "player://event";

/**
 * 播放引擎适配器。
 *
 * 两套实现的分工与本文件其余部分一致：Tauri 里走真引擎，浏览器预览走 Mock。
 * **Mock 不是摆设**——`pnpm dev` 的界面开发依赖它，没有它则进度条、切歌、
 * 循环模式在浏览器里全是死的，而这些恰恰是最需要反复看的交互。
 */
/** 一次装载的全部参数。字段含义见 [`EngineAdapter.load`]。 */
export interface LoadArgs {
  path: string;
  trackId: string;
  loadId: string;
  autoplay: boolean;
  initialVolume: number;
  initialPositionSec: number | null;
  loudness: boolean;
  /** 无缝接续的下一首；`null` = 放完就停。 */
  next: NextTrackArgs | null;
  /** 这份初始 next 的版本；`next = null` 时同样有效。 */
  queueRevision: number;
}

/** 「下一首」的指定。 */
export interface NextTrackArgs {
  path: string;
  trackId: string;
  /** 为下一首**预先**生成的装载 ID：越过边界后的事件都由它认领。 */
  loadId: string;
}

/** 一次后续的 next 更新。chain 只在显式 Load 时变化，revision 在链内递增。 */
export interface NextUpdateArgs {
  chainId: string;
  queueRevision: number;
  next: NextTrackArgs | null;
}

export interface EngineAdapter {
  /**
   * 这个引擎是否需要真实的本地文件。
   *
   * 判据必须问引擎，不能问曲目有没有 `path`：种子曲库**全都**没有路径，
   * 而假引擎根本不读文件。按曲目判会把浏览器预览的播放整个挡死——
   * 实测就是如此，点播放只得到「这是演示曲目」，进度纹丝不动，
   * 而这正是阶段 0 出口条件里「浏览器验证流程不回退」要防的。
   */
  readonly requiresPath: boolean;
  /**
   * 装载并（可选）立即播放。
   *
   * 参数收成一个对象而不是平铺，与 Rust 侧的 `LoadRequest` 一一对应，理由也是同一个：
   * 这些值**必须与装载原子生效**。拆成多条 IPC 就没有顺序保证——音量晚到让第一首以
   * 满音量炸出来，位置晚到先漏出一段曲首，下一首早到会被装载的 teardown 抹掉。
   *
   * `loudness` 传的是**用户的设置**而不是增益倍率：具体倍率取决于分析结果、目标响度
   * 与峰值上限，那是后端的策略，改了不该要求前端跟着改。
   */
  load(args: LoadArgs): Promise<void>;
  /**
   * 更新无缝接续的下一首；`null` = 当前这首放完就停。
   *
   * 「没有下一首」也必须明说：不说的话引擎会一直接着上次指定的那首，
   * 用户删掉队尾之后反而会绕回去。
   */
  setNext(update: NextUpdateArgs, loudness: boolean): Promise<void>;
  /**
   * 列出可用的输出端点。
   *
   * **每次调用都重新问系统**，不缓存：设备会插拔，缓存一份只会让菜单显示已经拔掉的耳机。
   */
  listDevices(): Promise<AudioDeviceInfo[]>;
  /**
   * 选定输出端点；`null` = 跟随系统默认。
   *
   * 选中的端点用不了时不会静默回落，而是回一条 `deviceRejected` 事件、继续在原端点上
   * 放——静默换一台意味着用户以为声音在 DAC 上、实际从笔记本喇叭里出来。
   */
  setDevice(deviceId: string | null, deviceRevision: number): Promise<void>;
  play(): Promise<void>;
  pause(): Promise<void>;
  seek(positionSec: number): Promise<void>;
  setVolume(volume: number): Promise<void>;
  stop(): Promise<void>;
  /**
   * 订阅播放事件，返回取消订阅函数。
   *
   * 事件都带 `trackId` 与 `loadId`：快速连点两首歌，乃至连续重载同一首时，
   * 调用方都能丢弃上一代事件（否则表现为进度条或状态跳一下）。
   */
  onEvent(handler: (e: PlayerEvent) => void): Promise<() => void>;
}

/** 摊成后端要的形状：曲目 ID 与装载 ID 合成一份不透明上下文。 */
function toNextPayload(next: NextTrackArgs | null) {
  return next === null
    ? null
    : {
        path: next.path,
        context: { trackId: next.trackId, loadId: next.loadId },
      };
}

/** 真引擎：命令走 IPC，事件走 Tauri event。 */
const tauriEngine: EngineAdapter = {
  requiresPath: true,
  load: ({
    path,
    trackId,
    loadId,
    autoplay,
    initialVolume,
    initialPositionSec,
    loudness,
    next,
    queueRevision,
  }) =>
    invoke<void>("player_load", {
      path,
      context: { trackId, loadId },
      autoplay,
      initialVolume,
      initialPositionSec,
      loudness,
      next: toNextPayload(next),
      queueRevision,
    }),
  setNext: ({ chainId, queueRevision, next }, loudness) =>
    invoke<void>("player_set_next", {
      next: toNextPayload(next),
      loudness,
      chainId,
      queueRevision,
    }),
  listDevices: () => invoke<AudioDeviceInfo[]>("player_list_devices"),
  setDevice: (deviceId, deviceRevision) =>
    invoke<void>("player_set_device", { deviceId, deviceRevision }),
  play: () => invoke<void>("player_play"),
  pause: () => invoke<void>("player_pause"),
  seek: (positionSec) => invoke<void>("player_seek", { positionSec }),
  setVolume: (volume) => invoke<void>("player_set_volume", { volume }),
  stop: () => invoke<void>("player_stop"),
  onEvent: async (handler) => listen<PlayerEvent>(EVENT_PLAYER, (e) => handler(e.payload)),
};

/**
 * 浏览器预览用的假引擎：不出声，但把状态机与时钟完整跑一遍。
 *
 * 时长从哪来是个真问题——它没有文件可读。约定由调用方经 `hintDuration` 按曲目 ID 告知
 * （前端队列里本来就有曲目时长）。拿不到就按 0 处理，于是「装载即结束」，
 * 这比让进度条跑一个编出来的时长要诚实。
 *
 * **无缝换曲也要模拟**：真引擎在边界处发 `trackChanged`，只有链子走到头才发 `ended`。
 * 假引擎若一律发 `ended`，浏览器预览走的就是另一条前端代码路径——而队列推进、
 * 当前曲目对账恰恰是最需要在浏览器里反复看的交互。
 */
/**
 * 浏览器预览的假端点表。第一台标为系统默认，与真后端一致。
 *
 * 两台是最少的有意义数量：一台的话「换设备」这条交互在浏览器里根本走不到。
 */
const MOCK_DEVICES: AudioDeviceInfo[] = [
  { id: "mock:builtin", label: "内建扬声器（预览）", isDefault: true },
  { id: "mock:usb-dac", label: "外接 DAC（预览）", isDefault: false },
];

function createMockEngine(): EngineAdapter & { setDuration(trackId: string, sec: number): void } {
  const handlers = new Set<(e: PlayerEvent) => void>();
  let timer: ReturnType<typeof setInterval> | null = null;
  let trackId: string | null = null;
  let loadId = "mock-idle";
  let position = 0;
  let duration = 0;
  let next: { track: NextTrackArgs; revision: number } | null = null;
  let chainId = "mock-idle";
  let queueRevision = 0;
  let deviceId = MOCK_DEVICES[0].id;
  let deviceName = MOCK_DEVICES[0].label;
  let preferredDeviceId: string | null = null;
  let preferredDeviceLabel: string | null = null;
  let acceptedDeviceRevision = 0;
  // 按曲目 ID 记时长。队列有多长它就有多少条，仅存在于 dev 构建。
  const durations = new Map<string, number>();

  const emit = (e: PlayerEvent) => handlers.forEach((h) => h(e));

  const mockFormat = (durationSec: number) => ({
    container: "mock",
    codec: "mock",
    sampleRate: 44100,
    channels: 2,
    layout: "stereo",
    durationSec,
    deviceName,
    deviceId,
    outputSampleRate: 44100,
    sampleFormat: "f32",
    resampled: false,
  });

  const stopTimer = () => {
    if (timer !== null) {
      clearInterval(timer);
      timer = null;
    }
  };

  // 与真引擎同频（约 5 Hz）：界面在事件之间自行插值，两边的插值手感因此一致。
  const startTimer = () => {
    stopTimer();
    timer = setInterval(() => {
      position = Math.min(position + 0.2, duration);
      emit({ type: "progress", trackId, loadId, positionSec: position, durationSec: duration, bufferedSec: duration });
      if (position < duration) return;
      if (next === null) {
        stopTimer();
        emit({ type: "status", trackId, loadId, status: "ended" });
        emit({ type: "ended", trackId, loadId });
        return;
      }
      // 无缝交接：换成下一首继续跑表，不停 timer。真引擎在这里是「消费端越过边界」。
      const from = trackId;
      const { trackId: to, loadId: toLoad } = next.track;
      const crossedRevision = next.revision;
      next = null;
      trackId = to;
      loadId = toLoad;
      duration = durations.get(to) ?? 0;
      position = 0;
      emit({
        type: "trackChanged",
        trackId,
        loadId,
        fromTrackId: from,
        queueRevision: crossedRevision,
        format: mockFormat(duration),
      });
    }, 200);
  };

  return {
    // 假引擎不读文件，因此种子曲库照样能跑完整个状态机与时钟。
    requiresPath: false,
    setDuration: (id, sec) => {
      durations.set(id, Math.max(0, sec));
    },
    // 假引擎不出声，响度增益对它没有可观测效果，但参数照收——两边签名一致，
    // 调用点才不会长出「浏览器里少传一个」的分支。
    load: async ({
      trackId: id,
      loadId: idForLoad,
      autoplay,
      initialPositionSec,
      next: after,
      queueRevision: initialRevision,
    }) => {
      trackId = id;
      loadId = idForLoad;
      chainId = idForLoad;
      queueRevision = initialRevision;
      duration = durations.get(id) ?? 0;
      next = after ? { track: after, revision: initialRevision } : null;
      position =
        initialPositionSec !== null && Number.isFinite(initialPositionSec)
          ? Math.max(0, Math.min(initialPositionSec, duration))
          : 0;
      emit({ type: "status", trackId, loadId, status: "loading" });
      emit({ type: "opened", trackId, loadId, format: mockFormat(duration) });
      emit({ type: "status", trackId, loadId, status: autoplay ? "playing" : "paused" });
      if (autoplay) startTimer();
    },
    setNext: async (update) => {
      if (update.chainId !== chainId || update.queueRevision <= queueRevision) return;
      queueRevision = update.queueRevision;
      next = update.next ? { track: update.next, revision: update.queueRevision } : null;
    },
    listDevices: async () => MOCK_DEVICES,
    setDevice: async (id, deviceRevision) => {
      // 假引擎不出声，但状态机要走全：换端点发 outputChanged，选一台不存在的发
      // deviceRejected。少了这一步，浏览器里就看不出「被拒之后菜单该退回哪一项」。
      if (deviceRevision <= acceptedDeviceRevision) return;
      acceptedDeviceRevision = deviceRevision;
      const picked =
        id === null
          ? MOCK_DEVICES.find((device) => device.isDefault)
          : MOCK_DEVICES.find((device) => device.id === id);
      if (!picked) {
        emit({
          type: "deviceRejected",
          trackId,
          loadId,
          deviceRevision,
          preferredDeviceId,
          preferredDeviceLabel,
          error: {
            stage: "output",
            kind: "noDevice",
            container: null,
            codec: null,
            message: `标识为「${id}」的输出设备已不可用`,
          },
        });
        return;
      }
      preferredDeviceId = id;
      preferredDeviceLabel = id === null ? null : picked.label;
      deviceId = picked.id;
      deviceName = picked.label;
      emit({
        type: "outputChanged",
        trackId,
        loadId,
        deviceRevision,
        format: mockFormat(duration),
      });
    },
    play: async () => {
      emit({ type: "status", trackId, loadId, status: "playing" });
      startTimer();
    },
    pause: async () => {
      stopTimer();
      emit({ type: "status", trackId, loadId, status: "paused" });
    },
    seek: async (positionSec) => {
      position = Math.max(0, Math.min(positionSec, duration));
      emit({ type: "progress", trackId, loadId, positionSec: position, durationSec: duration, bufferedSec: duration });
    },
    setVolume: async () => {},
    stop: async () => {
      stopTimer();
      position = 0;
      emit({ type: "status", trackId, loadId, status: "idle" });
    },
    onEvent: async (handler) => {
      handlers.add(handler);
      return () => handlers.delete(handler);
    },
  };
}

export const mockEngine = createMockEngine();

/** 当前环境该用的引擎。调用点不写环境判断，与本文件其余部分一致。 */
export const engine: EngineAdapter = isTauri() ? tauriEngine : mockEngine;

/**
 * 浏览器预览下把某首曲目的时长告知假引擎。Tauri 环境是 no-op——真引擎自己从文件读，
 * 前端记的时长只是标签里的值，未必与实际码流一致。
 *
 * 按曲目 ID 记而不是「当前这首」：无缝交接时假引擎要在边界处自己换到下一首，
 * 那一刻没有调用方可问。
 */
export function hintDuration(trackId: string, sec: number): void {
  if (!isTauri()) mockEngine.setDuration(trackId, sec);
}

/* ============================================================
   播放会话（队列 / 进度 / 循环与随机状态）
   ============================================================ */

/**
 * 保存播放会话。浏览器预览下是 no-op——dev 环境每次刷新都该是干净的起点，
 * 把种子曲库的队列存进 localStorage 只会让「我明明改了代码怎么还是旧的」更难查。
 */
export async function saveSession(json: string): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>("save_session", { json });
}

/** 读回播放会话；没有或读不出来都是 null。 */
export async function loadSession(): Promise<string | null> {
  if (!isTauri()) return null;
  return invoke<string | null>("load_session");
}

/**
 * 保存界面设置（主题 / 语言 / 视图 / 开关）。
 *
 * 浏览器预览下同样是 no-op，理由与会话一致：dev 环境每次刷新都该是干净的起点，
 * 否则「我明明改了默认值怎么还是旧的」会变成一个要查半天的问题。
 */
export async function saveSettings(json: string): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>("save_settings", { json });
}

/** 读回界面设置；没有或读不出来都是 null。 */
export async function loadSettings(): Promise<string | null> {
  if (!isTauri()) return null;
  return invoke<string | null>("load_settings");
}

/* ============================================================
   响度归一化（后台分析队列）
   ============================================================ */

/** 一件待分析曲目。顺序即优先级，由调用方按播放顺序给出。 */
export interface LoudnessQueueItem {
  trackId: string;
  path: string;
}

/**
 * 按播放顺序重排后台分析队列，返回还有多少首要分析。
 *
 * 传空数组表示停下（用户关掉了响度归一化）。浏览器预览没有后端，返回 0——
 * Mock 引擎不出声，也就没有可归一化的东西。
 */
export async function setLoudnessQueue(items: LoudnessQueueItem[]): Promise<number> {
  if (!isTauri()) return 0;
  return invoke<number>("loudness_set_queue", { items });
}

/* ── 收藏与歌单 ────────────────────────────────────────────────────────── */

/**
 * 读回收藏与歌单。
 *
 * 浏览器预览没有后端，返回 `null` 让 store 保留种子演示数据——与曲库同一套路：
 * 界面不该因为在浏览器里跑就变成空的，那样 UI 开发就没法继续了。
 */
export async function loadCollections(): Promise<[Favorites, Playlist[]] | null> {
  if (!isTauri()) return null;
  return invoke<[Favorites, Playlist[]]>("collections_load");
}

export async function favoriteTrack(trackId: string, on: boolean): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>("favorite_track", { trackId, on });
}

/**
 * 收藏 / 取消收藏一张专辑，传的是它**当前**的全部曲目 ID。
 *
 * 不传专辑 ID：那个 ID 由含目录的归组键哈希而来，改标签或挪文件就变（见
 * `core/src/id.rs`），拿它当持久化的键，用户整理一次音乐文件夹收藏就没了。
 */
export async function favoriteAlbum(trackIds: string[], on: boolean): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>("favorite_album", { trackIds, on });
}

export async function favoriteArtist(name: string, on: boolean): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>("favorite_artist", { name, on });
}

export async function favoritePlaylist(playlistId: string, on: boolean): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>("favorite_playlist", { playlistId, on });
}

/**
 * 新建歌单，返回后端发的 ID 与时间戳；浏览器预览返回 `null`（调用方本地发号）。
 *
 * ID 由后端生成：它要进数据库当主键、也要被收藏表引用，由拥有存储的那一侧发号
 * 才不会出现「前端以为叫这个、库里叫那个」。代价是新建这一步没法乐观更新——
 * 用户得等一次 IPC。这与红心不同：收藏是连点几十次的动作，新建歌单是一次。
 */
export async function playlistCreate(
  title: string,
  trackIds: string[],
): Promise<Playlist | null> {
  if (!isTauri()) return null;
  return invoke<Playlist>("playlist_create", { title, trackIds });
}

/** 整体保存一个歌单（改名、改简介、重排或增删曲目都走这条）。 */
export async function playlistSave(playlist: Playlist): Promise<Playlist | null> {
  if (!isTauri()) return null;
  return invoke<Playlist>("playlist_save", { playlist });
}

export async function playlistDelete(playlistId: string): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>("playlist_delete", { playlistId });
}

/** 歌单列表自身的顺序（用户在歌单页拖出来的「自定义顺序」）。 */
export async function playlistReorder(ids: string[]): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>("playlist_reorder", { ids });
}
