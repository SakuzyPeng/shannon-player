import { LANGUAGES } from "@/data/library";
import type { Language, LibraryView, ThemeMode } from "@/types/player";
import type { SettingKey } from "@/store/ui";

/**
 * 界面设置的持久化格式。
 *
 * 与播放会话（`src/lib/session.ts`）同一套路：后端只负责原子存取一段文本，结构与版本
 * 由前端拥有——主题深浅、界面语言、开关状态，后端在其中没有任何领域判断可做。
 *
 * ## 未知值一律回落默认，不照单全收
 *
 * 这份文件在用户的应用数据目录里，可以被手改，也可能来自**别的版本**。因此每个字段
 * 都按当前支持范围校验：
 *
 * - 语言只认 `LANGUAGES` 里那三项。`Language` 类型本身还包含繁體中文与日本語（词条
 *   备好了但没进 UI），若照单全收，一份旧文件或手改就能把界面切到一个我们对外并未
 *   承诺的语言上——那是承诺范围的问题，不是校验松紧的问题。
 * - 开关按**键**逐个取，缺的用默认补。将来加一个新开关时，旧文件里没有它，
 *   整份照搬会得到 `undefined`，那个开关就会以「既不是开也不是关」的样子出现在界面上。
 *
 * 被拒绝的值**不会**被立刻改写回文件：启动时不写盘是刻意的（没有用户动作就不落盘），
 * 它只是被忽略。所以在文件里看到一个界面上并未生效的语言，不是校验没起作用。
 */

/** 当前 schema 版本。结构不兼容地变化时 +1，旧版本一律当作没有设置。 */
const SETTINGS_VERSION = 1;

const THEMES: ThemeMode[] = ["light", "dark", "system"];
const VIEWS: LibraryView[] = ["grid", "list"];

export interface PersistedSettings {
  version: number;
  theme: ThemeMode;
  view: LibraryView;
  language: Language;
  /** 设置页开关。按键存，读时与默认值合并。 */
  settings: Partial<Record<SettingKey, boolean>>;
}

/** 从当前 store 状态构造要落盘的设置。 */
export function toSettings(state: {
  theme: ThemeMode;
  view: LibraryView;
  language: Language;
  settings: Record<SettingKey, boolean>;
}): PersistedSettings {
  return {
    version: SETTINGS_VERSION,
    theme: state.theme,
    view: state.view,
    language: state.language,
    settings: { ...state.settings },
  };
}

/** 恢复结果：只包含**认得出**的字段，其余交给 store 的默认值。 */
export interface RestoredSettings {
  theme?: ThemeMode;
  view?: LibraryView;
  language?: Language;
  settings: Partial<Record<SettingKey, boolean>>;
}

/**
 * 解析设置。
 *
 * 任何读不懂的内容都返回 `null` 而不是抛错：设置是随手能重设的数据，读不懂就当没有，
 * 不该让一份坏掉的文件挡住启动——而这份恰好是在首帧之前读的。
 */
export function fromSettings(json: string): RestoredSettings | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(json);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const raw = parsed as Record<string, unknown>;
  if (raw.version !== SETTINGS_VERSION) return null;

  const settings: Partial<Record<SettingKey, boolean>> = {};
  if (typeof raw.settings === "object" && raw.settings !== null) {
    for (const [key, value] of Object.entries(raw.settings)) {
      // 只收布尔值：一个 "true" 字符串会让开关看起来是开的，点一下才发现状态不对。
      if (typeof value === "boolean") settings[key as SettingKey] = value;
    }
  }

  return {
    theme: THEMES.find((t) => t === raw.theme),
    view: VIEWS.find((v) => v === raw.view),
    // 只认对外承诺的那几种语言，见文件头。
    language: LANGUAGES.find((l) => l === raw.language),
    settings,
  };
}
