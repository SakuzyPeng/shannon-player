import { AlbumDetailScreen } from "@/components/album/AlbumDetailScreen";
import { IconRail } from "@/components/layout/IconRail";
import { LibraryScreen } from "@/components/library/LibraryScreen";
import { PlayBar } from "@/components/player/PlayBar";
import { useApplyTheme } from "@/hooks/useApplyTheme";
import { useUiStore } from "@/store/ui";

export default function App() {
  useApplyTheme();
  const openAlbumId = useUiStore((s) => s.openAlbumId);

  return (
    <div className="flex h-screen overflow-hidden bg-bg text-tx transition-colors">
      <IconRail />
      <main className="relative flex min-w-0 flex-1 flex-col">
        {openAlbumId ? <AlbumDetailScreen albumId={openAlbumId} /> : <LibraryScreen />}
        <PlayBar />
      </main>
    </div>
  );
}
