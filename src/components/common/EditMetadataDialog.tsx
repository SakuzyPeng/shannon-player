import * as Dialog from "@radix-ui/react-dialog";
import { useEffect, useMemo, useState } from "react";
import { useT } from "@/i18n";
import type { MessageKey } from "@/i18n/messages";
import {
  isTauri,
  resetAlbumMetadata,
  resetTrackMetadata,
  setAlbumMetadata,
  setTrackMetadata,
  type MetadataPatch,
} from "@/lib/backend";
import { cn } from "@/lib/cn";
import { useLibraryStore } from "@/store/library";
import type { Album, FieldSource, Track } from "@/types/player";

/**
 * 编辑元数据。
 *
 * 存在的理由：扫描在标签缺失时会用文件夹名、文件名猜歌手与专辑，专辑艺人还要靠
 * 同专辑曲目多数决推断——只要有推断就会有猜错，必须给用户改正的手段。
 *
 * 两条界面规矩：
 * 1. **只提交用户真正动过的字段**。把界面上显示的推断值原样回写，等于把「猜的」
 *    固化成「用户指定的」，以后文件标签修好了也不会再更新（见 `dirtyPatch`）。
 * 2. **清空某栏 = 撤销这一栏的修改**，不是把它改成空字符串——后端的三态语义
 *    （见 `Overrides::merge`）正是为此。
 *
 * 修改只存在应用自己的数据目录里，不写回音频文件：写标签是破坏性操作，
 * 不该是「编辑信息」的副作用。
 */

type Target = { kind: "track"; track: Track } | { kind: "album"; album: Album; trackCount: number };

/**
 * 各页面接入编辑对话框的统一入口：`{dialog}` 挂进 JSX，菜单动作调 `editTrack` /
 * `editAlbum`。菜单里的「编辑信息」出现在专辑页、歌曲页、歌单页、收藏页、歌手页，
 * 逐页各写一份 open/target 状态既啰嗦又容易漏。
 */
export function useMetadataEditor() {
  const [target, setTarget] = useState<Target | null>(null);
  const [open, setOpen] = useState(false);
  return {
    dialog: <EditMetadataDialog open={open} onOpenChange={setOpen} target={target} />,
    editTrack: (track: Track) => {
      setTarget({ kind: "track", track });
      setOpen(true);
    },
    editAlbum: (album: Album, trackCount: number) => {
      setTarget({ kind: "album", album, trackCount });
      setOpen(true);
    },
  };
}

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  target: Target | null;
}

/** 字段来源 → 提示文案。标签来源无需提示（那是文件里就有的，天经地义）。 */
const SOURCE_LABEL: Partial<Record<FieldSource, MessageKey>> = {
  folder: "edit.srcFolder",
  fileName: "edit.srcFileName",
  majority: "edit.srcMajority",
  unknown: "edit.srcUnknown",
  userEdit: "edit.srcEdited",
};

const FIELD_CLS =
  "w-full rounded-[11px] border border-bd bg-bg px-3 py-2 text-[13.5px] text-tx outline-none focus:border-ac";
const BTN =
  "flex-none cursor-pointer whitespace-nowrap rounded-full px-[15px] py-[7px] text-[12.5px] font-semibold transition-colors";

/** 一行字段：标签 + 输入框 + 来源提示。 */
function Field({
  label,
  source,
  value,
  onChange,
  type = "text",
}: {
  label: string;
  source?: FieldSource;
  value: string;
  onChange: (v: string) => void;
  type?: "text" | "number";
}) {
  const { t } = useT();
  const hint = source && SOURCE_LABEL[source];
  return (
    <label className="block">
      <div className="mb-1 flex items-baseline justify-between gap-2">
        <span className="flex-none whitespace-nowrap text-[12px] text-tx2">{label}</span>
        {hint && (
          <span
            className={cn(
              "flex-none whitespace-nowrap text-[11px]",
              source === "userEdit" ? "text-ac" : "text-tx2 opacity-80",
            )}
          >
            {t(hint)}
          </span>
        )}
      </div>
      <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className={FIELD_CLS}
      />
    </label>
  );
}

export function EditMetadataDialog({ open, onOpenChange, target }: Props) {
  const { t } = useT();
  const setLibrary = useLibraryStore((s) => s.setLibrary);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // 初始值：界面显示什么就编辑什么（含推断值），但提交时只发改动过的字段。
  const initial = useMemo(() => {
    if (!target) return null;
    if (target.kind === "track") {
      const tk = target.track;
      return {
        title: tk.title,
        artist: tk.artist,
        album: tk.album,
        albumArtist: "",
        discNo: tk.discNo != null ? String(tk.discNo) : "",
        trackNo: tk.trackNo != null ? String(tk.trackNo) : "",
      };
    }
    return {
      title: target.album.title,
      artist: "",
      album: target.album.title,
      albumArtist: target.album.artist,
      discNo: "",
      trackNo: "",
    };
  }, [target]);

  const [form, setForm] = useState(initial);
  useEffect(() => {
    // 每次打开都以当前值为起点，上次编辑的残留不该带入。
    if (open) {
      setForm(initial);
      setError(null);
    }
  }, [open, initial]);

  if (!target || !initial || !form) return null;
  const isTrack = target.kind === "track";
  const sources = isTrack ? target.track.sources : undefined;
  const set = (k: keyof typeof form) => (v: string) => setForm({ ...form, [k]: v });

  /** 只收集改动过的字段；清空的字段发空串（后端据此撤销该字段的修改）。 */
  const dirtyPatch = (): MetadataPatch => {
    const patch: MetadataPatch = {};
    const text = (k: "title" | "artist" | "album" | "albumArtist") => {
      if (form[k] !== initial[k]) patch[k] = form[k].trim();
    };
    text("title");
    text("artist");
    text("album");
    text("albumArtist");
    const num = (k: "discNo" | "trackNo") => {
      if (form[k] === initial[k]) return;
      const n = Number.parseInt(form[k], 10);
      patch[k] = Number.isFinite(n) && n > 0 ? n : undefined;
    };
    if (isTrack) {
      num("discNo");
      num("trackNo");
    }
    return patch;
  };

  const run = async (fn: () => ReturnType<typeof setTrackMetadata>) => {
    if (!isTauri()) {
      setError(t("edit.previewOnly"));
      return;
    }
    setBusy(true);
    try {
      const snapshot = await fn();
      if (snapshot) setLibrary(snapshot);
      onOpenChange(false);
    } catch (e) {
      setError(t("edit.failed", { error: String(e) }));
    } finally {
      setBusy(false);
    }
  };

  const submit = () => {
    const patch = dirtyPatch();
    if (Object.keys(patch).length === 0) {
      onOpenChange(false);
      return;
    }
    void run(() =>
      isTrack
        ? setTrackMetadata(target.track.id, patch)
        : setAlbumMetadata(target.album.id, patch),
    );
  };

  const reset = () =>
    void run(() =>
      isTrack ? resetTrackMetadata(target.track.id) : resetAlbumMetadata(target.album.id),
    );

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="animate-dialog-overlay fixed inset-0 z-50 bg-[var(--cover-overlay)]" />
        <Dialog.Content className="surface-corners animate-dialog menu-shadow fixed left-1/2 top-1/2 z-50 max-h-[min(560px,calc(100vh-80px))] w-[min(440px,calc(100vw-64px))] -translate-x-1/2 -translate-y-1/2 overflow-y-auto rounded-[18px] border border-bd bg-srf p-5">
          <Dialog.Title className="m-0 font-serif text-[17px] font-semibold text-tx">
            {t(isTrack ? "edit.trackTitle" : "edit.albumTitle")}
          </Dialog.Title>
          <Dialog.Description className="mb-0 mt-1.5 text-[12.5px] leading-relaxed text-tx2">
            {isTrack
              ? target.track.album
              : t("edit.albumScope", { count: target.trackCount })}
          </Dialog.Description>

          <form
            className="mt-3.5 flex flex-col gap-3"
            onSubmit={(e) => {
              e.preventDefault();
              submit();
            }}
          >
            {isTrack && (
              <Field
                label={t("edit.fieldTitle")}
                source={sources?.title}
                value={form.title}
                onChange={set("title")}
              />
            )}
            {isTrack && (
              <Field
                label={t("edit.fieldArtist")}
                source={sources?.artist}
                value={form.artist}
                onChange={set("artist")}
              />
            )}
            <Field
              label={t("edit.fieldAlbum")}
              source={isTrack ? sources?.album : undefined}
              value={form.album}
              onChange={set("album")}
            />
            <div>
              <Field
                label={t("edit.fieldAlbumArtist")}
                source={isTrack ? sources?.albumArtist : target.album.artistSource}
                value={form.albumArtist}
                onChange={set("albumArtist")}
              />
              <p className="mt-1.5 text-[11.5px] leading-relaxed text-tx2 opacity-85">
                {t("edit.mergeHint")}
              </p>
            </div>
            {isTrack && (
              <div className="flex gap-3">
                <div className="min-w-0 flex-1">
                  <Field
                    label={t("edit.fieldDisc")}
                    value={form.discNo}
                    onChange={set("discNo")}
                    type="number"
                  />
                </div>
                <div className="min-w-0 flex-1">
                  <Field
                    label={t("edit.fieldTrack")}
                    value={form.trackNo}
                    onChange={set("trackNo")}
                    type="number"
                  />
                </div>
              </div>
            )}

            {error && <p className="text-[12px] leading-relaxed text-danger">{error}</p>}

            <div className="mt-1 flex items-center justify-between gap-2">
              <button
                type="button"
                onClick={reset}
                disabled={busy}
                className={cn(
                  BTN,
                  "border border-bd bg-srf text-tx2 hover:bg-hv disabled:cursor-not-allowed disabled:opacity-45",
                )}
              >
                {t("edit.reset")}
              </button>
              <div className="flex flex-none items-center gap-2">
                <button
                  type="button"
                  onClick={() => onOpenChange(false)}
                  className={cn(BTN, "border border-bd bg-srf text-tx hover:bg-hv")}
                >
                  {t("dialog.cancel")}
                </button>
                <button
                  type="submit"
                  disabled={busy}
                  className={cn(
                    BTN,
                    "bg-ac text-on-ac transition-[filter] hover:brightness-[1.08] disabled:cursor-not-allowed disabled:opacity-45",
                  )}
                >
                  {t("dialog.save")}
                </button>
              </div>
            </div>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
