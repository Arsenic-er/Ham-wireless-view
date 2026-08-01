// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import i18n from "../i18n";

import type { CalculationResult } from "./types";

export const MAP_OVERLAY_FILTER_ENCODING = "u8-dbm-floor-v1" as const;
export const MIN_VISIBLE_DBM = -140;
export const MAX_VISIBLE_DBM = -60;

type FilterPayload = Pick<
  CalculationResult,
  "mapOverlayWidth" | "mapOverlayHeight" | "mapOverlayFilterBase64"
> & { mapOverlayFilterEncoding: string };

export function thresholdDbmToBin(thresholdDbm: number): number {
  const normalized = Math.round(
    Math.max(MIN_VISIBLE_DBM, Math.min(MAX_VISIBLE_DBM, thresholdDbm)),
  );
  return normalized + 141;
}

export function decodeMapOverlayFilter(payload: FilterPayload): Uint8Array {
  if (payload.mapOverlayFilterEncoding !== MAP_OVERLAY_FILTER_ENCODING) {
    throw new Error(i18n.t("errorFilterEncodingDetail", { encoding: payload.mapOverlayFilterEncoding }));
  }
  if (
    !Number.isSafeInteger(payload.mapOverlayWidth) ||
    !Number.isSafeInteger(payload.mapOverlayHeight) ||
    payload.mapOverlayWidth <= 0 ||
    payload.mapOverlayHeight <= 0
  ) {
    throw new Error(i18n.t("errorFilterDimensionsDetail"));
  }
  let binary: string;
  try {
    binary = atob(payload.mapOverlayFilterBase64);
  } catch {
    throw new Error(i18n.t("errorFilterBase64Detail"));
  }
  const expectedLength = payload.mapOverlayWidth * payload.mapOverlayHeight;
  if (binary.length !== expectedLength) {
    throw new Error(i18n.t("errorFilterLengthDetail", { actual: binary.length, expected: expectedLength }));
  }
  const bins = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  const invalidIndex = bins.findIndex((bin) => bin > 81);
  if (invalidIndex !== -1) {
    throw new Error(i18n.t("errorFilterBinDetail", { index: invalidIndex }));
  }
  return bins;
}

export function applyVisibleSignalThreshold(
  rgba: Uint8ClampedArray,
  originalAlpha: Uint8ClampedArray,
  filterBins: Uint8Array,
  thresholdDbm: number,
): void {
  if (rgba.length !== filterBins.length * 4 || originalAlpha.length !== filterBins.length) {
    throw new Error(i18n.t("errorFilterRgbaDetail"));
  }
  const minimumVisibleBin = thresholdDbmToBin(thresholdDbm);
  for (let index = 0; index < filterBins.length; index += 1) {
    const bin = filterBins[index];
    rgba[index * 4 + 3] =
      bin !== 0 && bin >= minimumVisibleBin ? originalAlpha[index] : 0;
  }
}
