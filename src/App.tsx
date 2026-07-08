import { lazy, Suspense } from "react";
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

export default function App() {
  useApplyTheme();
  usePlaybackTicker();
  const openAlbumId = useUiStore((s) => s.openAlbumId);
  const openArtistName = useUiStore((s) => s.openArtistName);
  const lyricsOpen = useUiStore((s) => s.lyricsOpen);
  const nav = useUiStore((s) => s.nav);
  const content = openArtistName ? (
    <ArtistDetailScreen artistName={openArtistName} />
  ) : openAlbumId ? (
    <AlbumDetailScreen albumId={openAlbumId} />
  ) : nav === "songs" ? (
    <SongsScreen />
  ) : (
    <LibraryScreen />
  );

  return (
    <div className="flex h-screen overflow-hidden bg-bg text-tx transition-colors">
      <IconRail />
      <main className="relative flex min-w-0 flex-1 flex-col">
        <Suspense fallback={null}>{content}</Suspense>
        <PlayBar />
      </main>
      {lyricsOpen && (
        <Suspense fallback={null}>
          <LyricsScreen />
        </Suspense>
      )}
    </div>
  );
}
