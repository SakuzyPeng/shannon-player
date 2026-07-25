import { useMemo } from "react";
import type { Language } from "@/types/player";
import { useUiStore } from "@/store/ui";
import { CATALOG, type Locale, type MessageKey } from "@/i18n/messages";

export type { Locale, MessageKey } from "@/i18n/messages";

/**
 * 从系统偏好推断 locale（供「跟随系统」使用）。
 * 当前仅承诺简体中文与 English，因此只解析到这两者，
 * 不会因系统语言而弹出尚未承诺的繁體 / 日本語。
 */
export function detectSystemLocale(): Locale {
  const langs = typeof navigator !== "undefined" ? navigator.languages ?? [navigator.language] : [];
  for (const raw of langs) {
    const l = raw.toLowerCase();
    if (l.startsWith("zh")) return "zh-Hans";
    if (l.startsWith("en")) return "en";
  }
  return "zh-Hans";
}

/** 把语言选择（含「跟随系统」）解析为具体 locale。 */
export function resolveLocale(language: Language): Locale {
  switch (language) {
    case "简体中文":
      return "zh-Hans";
    case "繁體中文":
      return "zh-Hant";
    case "English":
      return "en";
    case "日本語":
      return "ja";
    case "跟随系统":
    default:
      return detectSystemLocale();
  }
}

export type TParams = Record<string, string | number>;
export type TFunction = (key: MessageKey, params?: TParams) => string;

/** {var|单数形|复数形}：按 params[var] 是否为 1 选词。 */
const PLURAL_RE = /\{(\w+)\|([^|{}]*)\|([^|{}]*)\}/g;

/**
 * 生成绑定到某 locale 的翻译函数。
 *
 * 支持两种标记：
 * - `{var}`      —— 普通插值。
 * - `{var|one|other}` —— 单复数选词，按 `params[var] === 1` 取 one，否则取 other。
 *   中日文没有单复数变化，其字典直接写死名词即可，无需使用该标记；
 *   英文写成 `"{n} {n|song|songs}"`，避免出现「1 songs」这类文案。
 */
export function createT(locale: Locale): TFunction {
  const dict = CATALOG[locale];
  return (key, params) => {
    let s = dict[key];
    // 先选词再插值：复数标记内含变量名，普通 {var} 正则不会误伤。
    s = s.replace(PLURAL_RE, (_, name: string, one: string, other: string) =>
      Number(params?.[name]) === 1 ? one : other,
    );
    if (params) {
      for (const [k, v] of Object.entries(params)) {
        s = s.replace(new RegExp(`\\{${k}\\}`, "g"), String(v));
      }
    }
    return s;
  };
}

/** React 组件内使用：随 UI store 的语言实时切换。 */
export function useT(): { t: TFunction; locale: Locale } {
  const language = useUiStore((s) => s.language);
  const locale = resolveLocale(language);
  const t = useMemo(() => createT(locale), [locale]);
  return { t, locale };
}
