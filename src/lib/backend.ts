import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { LibrarySnapshot, ScanProgress } from "@/types/generated/library";

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

/** 订阅扫描进度。返回取消订阅函数。 */
export async function onScanProgress(
  handler: (p: ScanProgress) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  const unlisten = await listen<ScanProgress>(EVENT_SCAN_PROGRESS, (e) => handler(e.payload));
  return unlisten;
}
