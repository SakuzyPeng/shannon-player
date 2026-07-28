import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { LibrarySnapshot, ScanProgress } from "@/types/generated/library";
import type { TrackMetadataPatch } from "@/types/generated/overrides";
import type { PlayerEvent } from "@/types/generated/player";

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
export interface EngineAdapter {
  /** 装载并（可选）立即播放。`trackId` 用于给事件盖章，见下方 onEvent。 */
  load(path: string, trackId: string, autoplay: boolean): Promise<void>;
  play(): Promise<void>;
  pause(): Promise<void>;
  seek(positionSec: number): Promise<void>;
  setVolume(volume: number): Promise<void>;
  stop(): Promise<void>;
  /**
   * 订阅播放事件，返回取消订阅函数。
   *
   * 事件都带 `trackId`：快速连点两首歌时，前一首的进度事件会晚于后一首的装载到达，
   * 调用方据此丢弃过期事件（否则表现为进度条跳一下）。
   */
  onEvent(handler: (e: PlayerEvent) => void): Promise<() => void>;
}

/** 真引擎：命令走 IPC，事件走 Tauri event。 */
const tauriEngine: EngineAdapter = {
  load: (path, trackId, autoplay) =>
    invoke<void>("player_load", { path, trackId, autoplay }),
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
 * 时长从哪来是个真问题——它没有文件可读。约定由调用方在 `load` 前经
 * `mockEngine.setDuration` 告知（前端队列里本来就有曲目时长）。拿不到就按 0 处理，
 * 于是「装载即结束」，这比让进度条跑一个编出来的时长要诚实。
 */
function createMockEngine(): EngineAdapter & { setDuration(sec: number): void } {
  const handlers = new Set<(e: PlayerEvent) => void>();
  let timer: ReturnType<typeof setInterval> | null = null;
  let trackId: string | null = null;
  let position = 0;
  let duration = 0;

  const emit = (e: PlayerEvent) => handlers.forEach((h) => h(e));

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
      emit({ type: "progress", trackId, positionSec: position, durationSec: duration, bufferedSec: duration });
      if (position >= duration) {
        stopTimer();
        emit({ type: "status", trackId, status: "ended" });
        emit({ type: "ended", trackId });
      }
    }, 200);
  };

  return {
    setDuration: (sec) => {
      duration = Math.max(0, sec);
    },
    load: async (_path, id, autoplay) => {
      trackId = id;
      position = 0;
      emit({ type: "status", trackId, status: "loading" });
      emit({
        type: "opened",
        trackId,
        format: {
          container: "mock",
          codec: "mock",
          sampleRate: 44100,
          channels: 2,
          layout: "stereo",
          durationSec: duration,
          deviceName: "浏览器预览（无声）",
          outputSampleRate: 44100,
          sampleFormat: "f32",
          resampled: false,
        },
      });
      emit({ type: "status", trackId, status: autoplay ? "playing" : "paused" });
      if (autoplay) startTimer();
    },
    play: async () => {
      emit({ type: "status", trackId, status: "playing" });
      startTimer();
    },
    pause: async () => {
      stopTimer();
      emit({ type: "status", trackId, status: "paused" });
    },
    seek: async (positionSec) => {
      position = Math.max(0, Math.min(positionSec, duration));
      emit({ type: "progress", trackId, positionSec: position, durationSec: duration, bufferedSec: duration });
    },
    setVolume: async () => {},
    stop: async () => {
      stopTimer();
      position = 0;
      emit({ type: "status", trackId, status: "idle" });
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
 * 浏览器预览下把时长告知假引擎。Tauri 环境是 no-op——真引擎自己从文件读，
 * 前端记的时长只是标签里的值，未必与实际码流一致。
 */
export function hintDuration(sec: number): void {
  if (!isTauri()) mockEngine.setDuration(sec);
}
