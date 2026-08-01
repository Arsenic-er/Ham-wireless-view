// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import type { CalculationPreview, CalculationResult } from "./types";

type MapOverlayResult = Pick<
  CalculationResult | CalculationPreview,
  "mapOverlayProjection" | "mapOverlayCorners" | "mapOverlayPngDataUrl"
>;

export type MapLibreImageCoordinates = [
  [number, number],
  [number, number],
  [number, number],
  [number, number],
];

export interface MapLibreImageSpec {
  url: string;
  coordinates: MapLibreImageCoordinates;
}

export function buildMapOverlayImageSpec(result: MapOverlayResult): MapLibreImageSpec {
  if (result.mapOverlayProjection !== "EPSG:3857") {
    throw new Error(`unsupported map overlay projection: ${result.mapOverlayProjection}`);
  }
  if (result.mapOverlayCorners.length !== 4) {
    throw new Error("map overlay must contain exactly four corners");
  }
  const [topLeft, topRight, bottomRight, bottomLeft] = result.mapOverlayCorners;
  return {
    url: result.mapOverlayPngDataUrl,
    coordinates: [topLeft, topRight, bottomRight, bottomLeft] as MapLibreImageCoordinates,
  };
}

const PNG_DATA_URL_PREFIX = "data:image/png;base64,";
const PNG_SIGNATURE = [137, 80, 78, 71, 13, 10, 26, 10] as const;

export function createMapOverlayBlobUrl(
  dataUrl: string,
  createObjectUrl: (blob: Blob) => string = (blob) => URL.createObjectURL(blob),
): string {
  if (!dataUrl.startsWith(PNG_DATA_URL_PREFIX)) {
    throw new Error("map overlay must be a base64 PNG data URL");
  }
  let binary: string;
  try {
    binary = atob(dataUrl.slice(PNG_DATA_URL_PREFIX.length));
  } catch {
    throw new Error("map overlay contains invalid base64");
  }
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  if (
    bytes.length < PNG_SIGNATURE.length ||
    PNG_SIGNATURE.some((expected, index) => bytes[index] !== expected)
  ) {
    throw new Error("map overlay does not contain a PNG signature");
  }
  return createObjectUrl(new Blob([bytes], { type: "image/png" }));
}

export class MapOverlayBlobUrlLease {
  private dataUrl: string | null = null;
  private objectUrl: string | null = null;

  constructor(
    private readonly create: (dataUrl: string) => string = createMapOverlayBlobUrl,
    private readonly revoke: (objectUrl: string) => void = (objectUrl) =>
      URL.revokeObjectURL(objectUrl),
  ) {}

  acquire(dataUrl: string): string {
    if (this.dataUrl === dataUrl && this.objectUrl) return this.objectUrl;
    const nextObjectUrl = this.create(dataUrl);
    const previousObjectUrl = this.objectUrl;
    this.dataUrl = dataUrl;
    this.objectUrl = nextObjectUrl;
    if (previousObjectUrl) this.revoke(previousObjectUrl);
    return nextObjectUrl;
  }

  clear(): void {
    if (this.objectUrl) this.revoke(this.objectUrl);
    this.dataUrl = null;
    this.objectUrl = null;
  }
}
