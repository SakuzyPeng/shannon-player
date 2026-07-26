import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { LibrarySnapshot, ScanProgress } from "@/types/generated/library";
import type { TrackOverride } from "@/types/generated/overrides";

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
 * 字段缺席 = 没动；空字符串 = 撤销该字段的修改；有值 = 改成这个值。
 *
 * 只提交用户真正动过的字段很重要——把界面上显示的推断值原样回写，
 * 等于把「猜的」固化成「用户指定的」，以后文件标签修好了也不会再更新。
 */
export type MetadataPatch = TrackOverride;

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
