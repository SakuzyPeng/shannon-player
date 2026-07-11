import { create } from "zustand";
import type { Id, Language, LibraryView, NavKey, ThemeMode } from "@/types/player";

/** 设置页开关键（后期由后端持久化）。 */
export type SettingKey = "watch" | "cloud" | "loudness" | "ttml" | "karaoke";

/** 音乐文件夹（占位数据，后期由 Rust 后端扫描替换）。 */
export interface MusicFolder {
  path: string;
  tracks: number;
  /** true = 监听中，false = 已扫描。 */
  watching: boolean;
}

interface UiState {
  theme: ThemeMode;
  view: LibraryView;
  nav: NavKey;
  language: Language;
  /** 当前打开的专辑详情页（null = 列表页）。 */
  openAlbumId: Id | null;
  /** 当前打开的歌手详情页（以歌手名为键；与专辑详情互斥）。 */
  openArtistName: string | null;
  /** 当前打开的歌单详情页（以歌单 ID 为键；与其他详情互斥）。 */
  openPlaylistId: Id | null;
  /** 歌词页（沉浸式，覆盖整个窗口）。 */
  lyricsOpen: boolean;
  /** 首次启动引导（空曲库时展示；后期由后端根据是否已配置曲库自动触发）。 */
  onboardingOpen: boolean;
  /** 最近搜索词（会话内保存，后期由后端持久化）。 */
  searchRecents: string[];
  /** 设置页开关状态。 */
  settings: Record<SettingKey, boolean>;
  /** 音乐文件夹列表。 */
  musicFolders: MusicFolder[];

  /** 外观三态循环：浅色 → 深色 → 跟随系统 → 浅色。 */
  cycleTheme: () => void;
  /** 直接设定外观（设置页分段用）。 */
  setTheme: (mode: ThemeMode) => void;
  setView: (v: LibraryView) => void;
  setNav: (n: NavKey) => void;
  setLanguage: (l: Language) => void;
  toggleSetting: (key: SettingKey) => void;
  removeMusicFolder: (path: string) => void;
  openAlbum: (id: Id) => void;
  closeAlbum: () => void;
  openArtist: (name: string) => void;
  closeArtist: () => void;
  openPlaylist: (id: Id) => void;
  closePlaylist: () => void;
  openLyrics: () => void;
  closeLyrics: () => void;
  openOnboarding: () => void;
  closeOnboarding: () => void;
  /** 记录一次搜索（去重前插，上限 8 条）。 */
  pushSearchRecent: (q: string) => void;
}

const THEME_CYCLE: ThemeMode[] = ["light", "dark", "system"];

export const useUiStore = create<UiState>((set) => ({
  theme: "light",
  view: "grid",
  nav: "albums",
  language: "跟随系统",
  openAlbumId: null,
  openArtistName: null,
  openPlaylistId: null,
  lyricsOpen: false,
  onboardingOpen: false,
  searchRecents: ["万能青年旅店", "In Rainbows", "陈绮贞"],
  settings: { watch: true, cloud: true, loudness: false, ttml: true, karaoke: true },
  musicFolders: [
    { path: "/Users/shannon/Music/曲库", tracks: 1532, watching: false },
    { path: "/Volumes/NAS/无损音乐", tracks: 281, watching: false },
    { path: "~/Downloads/待整理", tracks: 34, watching: true },
  ],

  cycleTheme: () =>
    set((s) => ({ theme: THEME_CYCLE[(THEME_CYCLE.indexOf(s.theme) + 1) % 3] })),
  setTheme: (theme) => set({ theme }),
  setView: (view) => set({ view }),
  // 切换主导航时关闭所有详情页。
  setNav: (nav) => set({ nav, openAlbumId: null, openArtistName: null, openPlaylistId: null }),
  setLanguage: (language) => set({ language }),
  // 详情页互斥：打开一个即关闭其余。
  openAlbum: (openAlbumId) => set({ openAlbumId, openArtistName: null, openPlaylistId: null }),
  closeAlbum: () => set({ openAlbumId: null }),
  openArtist: (openArtistName) => set({ openArtistName, openAlbumId: null, openPlaylistId: null }),
  closeArtist: () => set({ openArtistName: null }),
  openPlaylist: (openPlaylistId) =>
    set({ openPlaylistId, openAlbumId: null, openArtistName: null }),
  closePlaylist: () => set({ openPlaylistId: null }),
  openLyrics: () => set({ lyricsOpen: true }),
  closeLyrics: () => set({ lyricsOpen: false }),
  openOnboarding: () => set({ onboardingOpen: true }),
  closeOnboarding: () => set({ onboardingOpen: false }),
  pushSearchRecent: (q) =>
    set((s) => {
      const term = q.trim();
      if (!term) return {};
      const next = [term, ...s.searchRecents.filter((r) => r.toLowerCase() !== term.toLowerCase())];
      return { searchRecents: next.slice(0, 8) };
    }),
  toggleSetting: (key) =>
    set((s) => ({ settings: { ...s.settings, [key]: !s.settings[key] } })),
  removeMusicFolder: (path) =>
    set((s) => ({ musicFolders: s.musicFolders.filter((f) => f.path !== path) })),
}));
