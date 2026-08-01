// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from "vitest";

import i18n, { resources, setAppLocale } from ".";
import {
  APP_LOCALES,
  detectLocale,
  LOCALE_STORAGE_KEY,
  normalizeLocale,
} from "./locale";

describe("application locales", () => {
  it.each([
    ["en-US", "en"],
    ["ja", "ja-JP"],
    ["ja-JP", "ja-JP"],
    ["zh", "zh-CN"],
    ["zh-Hans-CN", "zh-CN"],
    ["zh-SG", "zh-CN"],
    ["zh-Hant", "zh-TW"],
    ["zh-TW", "zh-TW"],
    ["zh-HK", "zh-TW"],
    ["fr-FR", null],
  ] as const)("normalizes %s to %s", (input, expected) => {
    expect(normalizeLocale(input)).toBe(expected);
  });

  it("prefers persisted locale, then system locale, then English", () => {
    expect(detectLocale("ja-JP", ["zh-CN"])).toBe("ja-JP");
    expect(detectLocale("invalid", ["zh-Hant-HK", "en-US"])).toBe("zh-TW");
    expect(detectLocale(null, ["fr-FR"])).toBe("en");
  });

  it("keeps every locale in exact key parity with English", () => {
    const reference = Object.keys(resources.en.translation).sort();
    expect(reference).toHaveLength(288);
    expect(Object.keys(resources)).toEqual(APP_LOCALES);
    for (const locale of APP_LOCALES) {
      expect(Object.keys(resources[locale].translation).sort()).toEqual(reference);
      expect(Object.values(resources[locale].translation).every(Boolean)).toBe(true);
    }
  });

  it("does not persist the initial or test-selected system locale", () => {
    expect(localStorage.getItem(LOCALE_STORAGE_KEY)).toBeNull();
    expect(document.documentElement.lang).toBe("zh-CN");
  });

  it("persists only an explicit user selection and updates the document language", async () => {
    try {
      await setAppLocale("ja-JP");
      expect(localStorage.getItem(LOCALE_STORAGE_KEY)).toBe("ja-JP");
      expect(document.documentElement.lang).toBe("ja-JP");
    } finally {
      localStorage.removeItem(LOCALE_STORAGE_KEY);
      await i18n.changeLanguage("zh-CN");
    }
  });
});
