import { create } from "zustand";
import { SEED_ALBUMS, seedTracksOf } from "@/data/library";
import type { Album, Track } from "@/types/player";
import type { LibrarySnapshot, StorageStatus } from "@/types/generated/library";

/** 曲库来源。 */
export type LibrarySource = "seed" | "scan";

interface LibraryState {
  albums: Album[];
  tracks: Track[];
  /**
   * `seed` = 内置演示曲库（浏览器 dev 环境、或原生窗口里还没扫描过）；
   * `scan` = Rust 后端扫出的真实曲库。
   *
   * 保留 seed 回落是刻意的：没有音乐文件夹时界面全空会让 UI 开发无法进行。
   */
  source: LibrarySource;
  /** 遍历到但解析失败的文件数（仅 scan 来源有意义）。 */
  failed: number;
  /** 同一张专辑内被折叠掉的重复曲目数（同一首歌的多份拷贝）。 */
  duplicates: number;
  /** 每次整库替换 +1。App 以它为 key 强制重挂载，避免各页缓存旧曲库的派生结果。 */
  version: number;
  /**
   * 封面缩略图目录（原生窗口才有）。放 store 是因为几乎每个封面都要用它拼 URL，
   * 逐处异步取一次既啰嗦又会让首屏闪。
   */
  coverDir: string | null;
  /**
   * 曲库存储的健康状况，启动时问一次后端。
   *
   * 放曲库 store 而不是界面 store：它是曲库这份数据的属性（能不能存下、是不是刚
   * 损坏过），只是恰好要被界面读。`null` = 还没问到，此时什么都不显示——启动那一瞬
   * 先弹一句「一切正常」或先弹一句故障，都是在还不知道的时候下结论。
   */
  storage: StorageStatus | null;
  /** 用户读过存储提示后按下的关闭。只影响本次运行，下次启动仍会提醒。 */
  storageDismissed: boolean;
  setStorage: (status: StorageStatus) => void;
  dismissStorage: () => void;
  setCoverDir: (dir: string | null) => void;
  setLibrary: (snapshot: LibrarySnapshot) => void;
  resetToSeed: () => void;
}

const seedTracks = (): Track[] => SEED_ALBUMS.flatMap(seedTracksOf);

export const useLibraryStore = create<LibraryState>((set) => ({
  albums: SEED_ALBUMS,
  tracks: seedTracks(),
  source: "seed",
  failed: 0,
  duplicates: 0,
  version: 0,
  coverDir: null,
  storage: null,
  storageDismissed: false,
  setStorage: (storage) => set({ storage }),
  dismissStorage: () => set({ storageDismissed: true }),

  setCoverDir: (coverDir) => set({ coverDir }),

  setLibrary: (snapshot) =>
    set((s) => ({
      // 后端类型经 ts-rs 生成，字段与前端领域模型同名同形，可直接采用。
      albums: snapshot.albums as unknown as Album[],
      tracks: snapshot.tracks as unknown as Track[],
      source: "scan",
      failed: snapshot.failed,
      duplicates: snapshot.duplicates,
      version: s.version + 1,
    })),

  resetToSeed: () =>
    set((s) => ({
      albums: SEED_ALBUMS,
      tracks: seedTracks(),
      source: "seed",
      failed: 0,
      duplicates: 0,
      version: s.version + 1,
    })),
}));
