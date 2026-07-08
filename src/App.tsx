import { AlbumDetailScreen } from "@/components/album/AlbumDetailScreen";
import { ArtistDetailScreen } from "@/components/artist/ArtistDetailScreen";
import { IconRail } from "@/components/layout/IconRail";
import { LibraryScreen } from "@/components/library/LibraryScreen";
import { LyricsScreen } from "@/components/lyrics/LyricsScreen";
import { PlayBar } from "@/components/player/PlayBar";
import { useApplyTheme } from "@/hooks/useApplyTheme";
import { usePlaybackTicker } from "@/hooks/usePlaybackTicker";
import { useUiStore } from "@/store/ui";

export default function App() {
  useApplyTheme();
  usePlaybackTicker();
  const openAlbumId = useUiStore((s) => s.openAlbumId);
  const openArtistName = useUiStore((s) => s.openArtistName);
  const lyricsOpen = useUiStore((s) => s.lyricsOpen);

  return (
    <div className="flex h-screen overflow-hidden bg-bg text-tx transition-colors">
      <IconRail />
      <main className="relative flex min-w-0 flex-1 flex-col">
        {openArtistName ? (
          <ArtistDetailScreen artistName={openArtistName} />
        ) : openAlbumId ? (
          <AlbumDetailScreen albumId={openAlbumId} />
        ) : (
          <LibraryScreen />
        )}
        <PlayBar />
      </main>
      {lyricsOpen && <LyricsScreen />}
    </div>
  );
}
