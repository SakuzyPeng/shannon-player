import { lazy, Suspense, useEffect } from "react";
import { PageTransition } from "@/components/common/PageTransition";
import { IconRail } from "@/components/layout/IconRail";
import { LibraryScreen } from "@/components/library/LibraryScreen";
import { PlayBar } from "@/components/player/PlayBar";
import { useApplyTheme } from "@/hooks/useApplyTheme";
import { usePlaybackTicker } from "@/hooks/usePlaybackTicker";
import { useUiStore } from "@/store/ui";

const AlbumDetailScreen = lazy(() =>
  import("@/components/album/AlbumDetailScreen").then((module) => ({
    default: module.AlbumDetailScreen,
  })),
);

const ArtistDetailScreen = lazy(() =>
  import("@/components/artist/ArtistDetailScreen").then((module) => ({
    default: module.ArtistDetailScreen,
  })),
);

const ArtistsScreen = lazy(() =>
  import("@/components/artist/ArtistsScreen").then((module) => ({
    default: module.ArtistsScreen,
  })),
);

const SongsScreen = lazy(() =>
  import("@/components/songs/SongsScreen").then((module) => ({
    default: module.SongsScreen,
  })),
);

const LyricsScreen = lazy(() =>
  import("@/components/lyrics/LyricsScreen").then((module) => ({
    default: module.LyricsScreen,
  })),
);

const PlaylistDetailScreen = lazy(() =>
  import("@/components/playlist/PlaylistDetailScreen").then((module) => ({
    default: module.PlaylistDetailScreen,
  })),
);

const FavoritesScreen = lazy(() =>
  import("@/components/favorites/FavoritesScreen").then((module) => ({
    default: module.FavoritesScreen,
  })),
);

const SearchScreen = lazy(() =>
  import("@/components/search/SearchScreen").then((module) => ({
    default: module.SearchScreen,
  })),
);

const SettingsScreen = lazy(() =>
  import("@/components/settings/SettingsScreen").then((module) => ({
    default: module.SettingsScreen,
  })),
);

const FirstRunScreen = lazy(() =>
  import("@/components/onboarding/FirstRunScreen").then((module) => ({
    default: module.FirstRunScreen,
  })),
);

export default function App() {
  useApplyTheme();
  usePlaybackTicker();

  // ⌘F / Ctrl+F 全局唤起搜索页。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && (e.key === "f" || e.key === "F")) {
        e.preventDefault();
        useUiStore.getState().setNav("search");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const openAlbumId = useUiStore((s) => s.openAlbumId);
  const openArtistName = useUiStore((s) => s.openArtistName);
  const openPlaylistId = useUiStore((s) => s.openPlaylistId);
  const lyricsOpen = useUiStore((s) => s.lyricsOpen);
  const onboardingOpen = useUiStore((s) => s.onboardingOpen);
  const nav = useUiStore((s) => s.nav);
  const screen = onboardingOpen
    ? { key: "onboarding", content: <FirstRunScreen /> }
    : openPlaylistId
      ? { key: `playlist-${openPlaylistId}`, content: <PlaylistDetailScreen playlistId={openPlaylistId} /> }
      : openArtistName
        ? { key: `artist-${openArtistName}`, content: <ArtistDetailScreen artistName={openArtistName} /> }
        : openAlbumId
          ? { key: `album-${openAlbumId}`, content: <AlbumDetailScreen albumId={openAlbumId} /> }
          : nav === "songs"
            ? { key: "songs", content: <SongsScreen /> }
            : nav === "artists"
              ? { key: "artists", content: <ArtistsScreen /> }
              : nav === "search"
                ? { key: "search", content: <SearchScreen /> }
                : nav === "favorites"
                  ? { key: "favorites", content: <FavoritesScreen /> }
                  : nav === "settings"
                    ? { key: "settings", content: <SettingsScreen /> }
                    : { key: "albums", content: <LibraryScreen /> };

  return (
    <div className="flex h-screen overflow-hidden bg-bg text-tx transition-colors">
      <IconRail />
      <main className="relative flex min-w-0 flex-1 flex-col overflow-hidden">
        <PageTransition pageKey={screen.key}>
          <Suspense fallback={null}>{screen.content}</Suspense>
        </PageTransition>
        {/* 首次启动引导期间隐藏播放条（空曲库无播放） */}
        {!onboardingOpen && <PlayBar />}
      </main>
      <PageTransition pageKey={lyricsOpen ? "lyrics" : null} className="fixed inset-0 z-40">
        {lyricsOpen && (
          <Suspense fallback={null}>
            <LyricsScreen />
          </Suspense>
        )}
      </PageTransition>
    </div>
  );
}
