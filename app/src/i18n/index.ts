// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import i18n from "i18next";
import { initReactI18next } from "react-i18next";

import { en } from "./locales/en";
import { jaJP } from "./locales/ja-JP";
import { zhCN } from "./locales/zh-CN";
import { zhTW } from "./locales/zh-TW";
import {
  applyDocumentLocale,
  normalizeLocale,
  persistLocale,
  readInitialLocale,
  type AppLocale,
} from "./locale";

export const resources = {
  en: { translation: en },
  "zh-CN": { translation: zhCN },
  "zh-TW": { translation: zhTW },
  "ja-JP": { translation: jaJP },
} as const;

const initialLocale = readInitialLocale();

void i18n.use(initReactI18next).init({
  resources,
  lng: initialLocale,
  fallbackLng: "en",
  supportedLngs: ["en", "zh-CN", "zh-TW", "ja-JP"],
  load: "currentOnly",
  interpolation: { escapeValue: false },
  returnNull: false,
});

applyDocumentLocale(initialLocale);
i18n.on("languageChanged", (language) => {
  applyDocumentLocale(normalizeLocale(language) ?? "en");
});

export async function setAppLocale(locale: AppLocale): Promise<void> {
  await i18n.changeLanguage(locale);
  persistLocale(locale);
}

export function currentAppLocale(): AppLocale {
  return normalizeLocale(i18n.resolvedLanguage ?? i18n.language) ?? "en";
}

export default i18n;
