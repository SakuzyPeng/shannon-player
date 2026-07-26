import { convertFileSrc } from "@tauri-apps/api/core";
import { isTauri } from "@/lib/backend";
import type { Cover } from "@/types/player";

/**
 * 封面缩略图的档位选择。
 *
 * 后端按封面内容指纹生成三档正方形缩略图（见 `core/src/cover.rs`），这里按**实际
 * 显示尺寸**挑一档：拿 1024 去渲染 44px 的列表缩略图，等于让每一行都解码一张大图。
 *
 * 档位要乘以设备像素比——Retina 上 44px 的框实际是 88 物理像素，用 128 档才不糊。
 */
const SIZES = [128, 512, 1024] as const;

/** 选出不小于所需物理像素的最小档；超过最大档则用最大档。 */
export function pickSize(cssPx: number): number {
  const dpr = typeof window !== "undefined" ? Math.min(window.devicePixelRatio || 1, 3) : 1;
  const need = cssPx * dpr;
  return SIZES.find((s) => s >= need) ?? SIZES[SIZES.length - 1];
}

/**
 * 缩略图 URL。没有封面指纹（无内嵌封面）或不在原生窗口内时返回 null，
 * 调用方回落到占位渐变。
 *
 * 本地文件必须经 `convertFileSrc` 转成 asset 协议 URL 才能被 WebView 加载，
 * 直接用文件路径会被当成相对路径。
 */
export function coverSrc(cover: Cover, dir: string | null, cssPx: number): string | null {
  if (!dir || !cover.coverKey) return null;
  const file = `${dir}/${cover.coverKey}-${pickSize(cssPx)}.jpg`;
  // 非原生窗口（浏览器预览）没有 asset 协议，把目录当普通 URL 前缀用——
  // 这样把 coverDir 指向 Vite 的 /@fs/ 路径就能在浏览器里核对真实封面效果。
  return isTauri() ? convertFileSrc(file) : file;
}
