import { AlbumDetailScreen } from "@/components/album/AlbumDetailScreen";
import { ArtistDetailScreen } from "@/components/artist/ArtistDetailScreen";
import { IconRail } from "@/components/layout/IconRail";
import { LibraryScreen } from "@/components/library/LibraryScreen";
import { PlayBar } from "@/components/player/PlayBar";
import { useApplyTheme } from "@/hooks/useApplyTheme";
import { useUiStore } from "@/store/ui";

export default function App() {
  useApplyTheme();
  const openAlbumId = useUiStore((s) => s.openAlbumId);
  const openArtistName = useUiStore((s) => s.openArtistName);

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
    </div>
  );
}
