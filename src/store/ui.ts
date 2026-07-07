import { create } from "zustand";
import type { Id, Language, LibraryView, NavKey, ThemeMode } from "@/types/player";

interface UiState {
  theme: ThemeMode;
  view: LibraryView;
  nav: NavKey;
  language: Language;
  /** 当前打开的专辑详情页（null = 列表页）。 */
  openAlbumId: Id | null;
  /** 当前打开的歌手详情页（以歌手名为键；与专辑详情互斥）。 */
  openArtistName: string | null;

  /** 外观三态循环：浅色 → 深色 → 跟随系统 → 浅色。 */
  cycleTheme: () => void;
  setView: (v: LibraryView) => void;
  setNav: (n: NavKey) => void;
  setLanguage: (l: Language) => void;
  openAlbum: (id: Id) => void;
  closeAlbum: () => void;
  openArtist: (name: string) => void;
  closeArtist: () => void;
}

const THEME_CYCLE: ThemeMode[] = ["light", "dark", "system"];

export const useUiStore = create<UiState>((set) => ({
  theme: "light",
  view: "grid",
  nav: "albums",
  language: "跟随系统",
  openAlbumId: null,
  openArtistName: null,

  cycleTheme: () =>
    set((s) => ({ theme: THEME_CYCLE[(THEME_CYCLE.indexOf(s.theme) + 1) % 3] })),
  setView: (view) => set({ view }),
  // 切换主导航时关闭详情页。
  setNav: (nav) => set({ nav, openAlbumId: null, openArtistName: null }),
  setLanguage: (language) => set({ language }),
  openAlbum: (openAlbumId) => set({ openAlbumId, openArtistName: null }),
  closeAlbum: () => set({ openAlbumId: null }),
  openArtist: (openArtistName) => set({ openArtistName, openAlbumId: null }),
  closeArtist: () => set({ openArtistName: null }),
}));
