// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from "vitest";

import {
  applyVisibleSignalThreshold,
  decodeMapOverlayFilter,
  thresholdDbmToBin,
} from "./coverageVisibility";

function base64(bytes: readonly number[]): string {
  return btoa(String.fromCharCode(...bytes));
}

describe("coverage visibility filter", () => {
  it("maps the full integer display range to the backend byte contract", () => {
    expect(thresholdDbmToBin(-140)).toBe(1);
    expect(thresholdDbmToBin(-120)).toBe(21);
    expect(thresholdDbmToBin(-60)).toBe(81);
    expect(thresholdDbmToBin(-999)).toBe(1);
    expect(thresholdDbmToBin(999)).toBe(81);
  });

  it("decodes the exact width by height byte payload", () => {
    const decoded = decodeMapOverlayFilter({
      mapOverlayWidth: 2,
      mapOverlayHeight: 2,
      mapOverlayFilterEncoding: "u8-dbm-floor-v1",
      mapOverlayFilterBase64: base64([0, 1, 21, 81]),
    });
    expect([...decoded]).toEqual([0, 1, 21, 81]);
  });

  it("rejects unknown encodings and mismatched payload lengths", () => {
    expect(() =>
      decodeMapOverlayFilter({
        mapOverlayWidth: 1,
        mapOverlayHeight: 1,
        mapOverlayFilterEncoding: "unknown",
        mapOverlayFilterBase64: base64([1]),
      }),
    ).toThrow(/不支持/);
    expect(() =>
      decodeMapOverlayFilter({
        mapOverlayWidth: 2,
        mapOverlayHeight: 1,
        mapOverlayFilterEncoding: "u8-dbm-floor-v1",
        mapOverlayFilterBase64: base64([1]),
      }),
    ).toThrow(/长度/);
    expect(() =>
      decodeMapOverlayFilter({
        mapOverlayWidth: 1,
        mapOverlayHeight: 1,
        mapOverlayFilterEncoding: "u8-dbm-floor-v1",
        mapOverlayFilterBase64: base64([82]),
      }),
    ).toThrow(/0–81/);
  });

  it("hides weaker pixels and restores their original alpha when relaxed", () => {
    const rgba = new Uint8ClampedArray([
      1, 2, 3, 255,
      4, 5, 6, 220,
      7, 8, 9, 180,
      10, 11, 12, 140,
    ]);
    const originalAlpha = new Uint8ClampedArray([255, 220, 180, 140]);
    const bins = new Uint8Array([0, 20, 21, 81]);

    applyVisibleSignalThreshold(rgba, originalAlpha, bins, -120);
    expect([rgba[3], rgba[7], rgba[11], rgba[15]]).toEqual([0, 0, 180, 140]);

    applyVisibleSignalThreshold(rgba, originalAlpha, bins, -140);
    expect([rgba[3], rgba[7], rgba[11], rgba[15]]).toEqual([0, 220, 180, 140]);
  });

  it("filters eight 401 by 401 overlays within a generous interaction budget", () => {
    const pixelCount = 401 * 401;
    const rgba = Array.from(
      { length: 8 },
      () => new Uint8ClampedArray(pixelCount * 4).fill(255),
    );
    const alpha = new Uint8ClampedArray(pixelCount).fill(214);
    const bins = new Uint8Array(pixelCount);
    for (let index = 0; index < bins.length; index += 1) {
      bins[index] = (index % 81) + 1;
    }

    const startedAt = performance.now();
    for (const pixels of rgba) {
      applyVisibleSignalThreshold(pixels, alpha, bins, -120);
    }
    const elapsedMs = performance.now() - startedAt;

    expect(elapsedMs).toBeLessThan(100);
    expect(rgba[7][3]).toBe(0);
    expect(rgba[7][80 * 4 + 3]).toBe(214);
  });
});
