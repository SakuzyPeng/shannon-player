import { lazy, Suspense, useEffect } from "react";
import { PageTransition } from "@/components/common/PageTransition";
import { IconRail } from "@/components/layout/IconRail";
import { LibraryScreen } from "@/components/library/LibraryScreen";
import { PlayBar } from "@/components/player/PlayBar";
import { useApplyTheme } from "@/hooks/useApplyTheme";
import { useGlobalHotkeys } from "@/hooks/useGlobalHotkeys";
import { usePlaybackTicker } from "@/hooks/usePlaybackTicker";
import { useWindowChrome } from "@/hooks/useWindowChrome";
import { getCoverDir, getLibrary, getMusicFolders } from "@/lib/backend";
import { useLibraryStore } from "@/store/library";
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

const PlaylistsScreen = lazy(() =>
  import("@/components/playlist/PlaylistsScreen").then((module) => ({
    default: module.PlaylistsScreen,
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

/**
 * 启动时从后端恢复曲库。
 *
 * 后端把上次扫描的原始结果缓存在应用数据目录，重启后套用用户的元数据修改重新
 * 聚合即可——不必重扫，也不该退回演示曲库。浏览器预览没有后端，返回 null，
 * 保留种子数据。
 */
function useRestoreLibrary() {
  const setLibrary = useLibraryStore((s) => s.setLibrary);
  useEffect(() => {
    void (async () => {
      // 封面目录要先拿到：晚于曲库到位的话，首屏封面会先空一拍再补上。
      const [snapshot, roots, coverDir] = await Promise.all([
        getLibrary(),
        getMusicFolders(),
        getCoverDir(),
      ]);
      useLibraryStore.getState().setCoverDir(coverDir);
      if (!snapshot) return;
      setLibrary(snapshot);
      // 设置页显示真实扫描目录；曲目数按路径前缀统计，文件监听尚未实现，一律标为已扫描。
      useUiStore.getState().setMusicFolders(
        roots.map((path) => ({
          path,
          tracks: snapshot.tracks.filter((tk) => tk.path?.startsWith(path)).length,
          watching: false,
        })),
      );
    })();
  }, [setLibrary]);
}

export default function App() {
  useApplyTheme();
  useWindowChrome();
  usePlaybackTicker();
  useRestoreLibrary();
  useGlobalHotkeys();

  const openAlbumId = useUiStore((s) => s.openAlbumId);
  const openArtistName = useUiStore((s) => s.openArtistName);
  const openPlaylistId = useUiStore((s) => s.openPlaylistId);
  const lyricsOpen = useUiStore((s) => s.lyricsOpen);
  const onboardingOpen = useUiStore((s) => s.onboardingOpen);
  const nav = useUiStore((s) => s.nav);
  // 整库替换（扫描完成）时递增：并入页面 key 强制重挂载，
  // 免得各页 useMemo 还缓存着旧曲库的派生结果。
  const libraryVersion = useLibraryStore((s) => s.version);
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
              : nav === "playlists"
              ? { key: "playlists", content: <PlaylistsScreen /> }
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
        <PageTransition pageKey={`${screen.key}@${libraryVersion}`}>
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
