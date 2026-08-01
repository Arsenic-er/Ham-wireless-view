// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

export const APP_LOCALES = ["en", "zh-CN", "zh-TW", "ja-JP"] as const;

export type AppLocale = (typeof APP_LOCALES)[number];

export const LOCALE_STORAGE_KEY = "hamheatmap.locale.v1";

export const LOCALE_NATIVE_NAMES: Record<AppLocale, string> = {
  en: "English",
  "zh-CN": "简体中文",
  "zh-TW": "繁體中文",
  "ja-JP": "日本語",
};

export function normalizeLocale(value: string | null | undefined): AppLocale | null {
  if (!value) return null;
  const normalized = value.replaceAll("_", "-").toLowerCase();
  if (normalized === "en" || normalized.startsWith("en-")) return "en";
  if (normalized === "ja" || normalized.startsWith("ja-")) return "ja-JP";
  if (
    normalized === "zh-tw" ||
    normalized === "zh-hk" ||
    normalized === "zh-mo" ||
    normalized.startsWith("zh-hant")
  ) {
    return "zh-TW";
  }
  if (
    normalized === "zh" ||
    normalized === "zh-cn" ||
    normalized === "zh-sg" ||
    normalized.startsWith("zh-hans")
  ) {
    return "zh-CN";
  }
  return null;
}

export function detectLocale(
  storedLocale?: string | null,
  browserLanguages?: readonly string[],
): AppLocale {
  const stored = normalizeLocale(storedLocale);
  if (stored) return stored;
  for (const language of browserLanguages ?? []) {
    const resolved = normalizeLocale(language);
    if (resolved) return resolved;
  }
  return "en";
}

export function readInitialLocale(): AppLocale {
  const stored =
    typeof localStorage === "undefined" ? null : localStorage.getItem(LOCALE_STORAGE_KEY);
  const languages =
    typeof navigator === "undefined"
      ? []
      : navigator.languages?.length
        ? navigator.languages
        : [navigator.language];
  return detectLocale(stored, languages);
}

export function applyDocumentLocale(locale: AppLocale): void {
  if (typeof document !== "undefined") {
    document.documentElement.lang = locale;
  }
}

export function persistLocale(locale: AppLocale): void {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(LOCALE_STORAGE_KEY, locale);
  }
}
