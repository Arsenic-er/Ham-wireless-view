import type { CalculationResult } from "./types";

type MapOverlayResult = Pick<
  CalculationResult,
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
