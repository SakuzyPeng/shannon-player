/**
 * 运行平台判断。
 *
 * 用 UA 而不是 `@tauri-apps/plugin-os`：只为分辨一个平台，不值得多装一个插件
 * 与一条权限；而且浏览器预览下也要能判，插件在那儿本来就不可用。
 *
 * 目前只有窗口外观需要它——最大化该不该抹掉圆角、绿灯该进全屏还是最大化，
 * macOS 与 Windows 的答案相反。
 */
export const isMacPlatform = (): boolean =>
  typeof navigator !== "undefined" && /Macintosh|Mac OS X/i.test(navigator.userAgent);
