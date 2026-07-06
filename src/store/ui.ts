import { create } from "zustand";
import type { Language, LibraryView, NavKey, ThemeMode } from "@/types/player";

interface UiState {
  theme: ThemeMode;
  view: LibraryView;
  nav: NavKey;
  language: Language;

  /** 外观三态循环：浅色 → 深色 → 跟随系统 → 浅色。 */
  cycleTheme: () => void;
  setView: (v: LibraryView) => void;
  setNav: (n: NavKey) => void;
  setLanguage: (l: Language) => void;
}

const THEME_CYCLE: ThemeMode[] = ["light", "dark", "system"];

export const useUiStore = create<UiState>((set) => ({
  theme: "light",
  view: "grid",
  nav: "albums",
  language: "跟随系统",

  cycleTheme: () =>
    set((s) => ({ theme: THEME_CYCLE[(THEME_CYCLE.indexOf(s.theme) + 1) % 3] })),
  setView: (view) => set({ view }),
  setNav: (nav) => set({ nav }),
  setLanguage: (language) => set({ language }),
}));
