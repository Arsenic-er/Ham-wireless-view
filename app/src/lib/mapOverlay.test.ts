import { describe, expect, it } from "vitest";

import { buildMapOverlayImageSpec } from "./mapOverlay";

const overlayCorners: [number, number][] = [
  [101.39, 32.31],
  [105.61, 32.31],
  [105.61, 28.69],
  [101.39, 28.69],
];

const originalCorners: [number, number][] = [
  [101.4, 32.3],
  [105.6, 32.3],
  [105.6, 28.7],
  [101.4, 28.7],
];

describe("MapLibre map overlay image spec", () => {
  it("uses the reprojected overlay URL and corners instead of the original report heatmap", () => {
    const result = {
      mapOverlayProjection: "EPSG:3857" as const,
      mapOverlayCorners: overlayCorners,
      mapOverlayPngDataUrl: "data:image/png;base64,overlay",
      imageCorners: originalCorners,
      heatmapPngDataUrl: "data:image/png;base64,native-report",
    };

    const image = buildMapOverlayImageSpec(result);

    expect(image).toEqual({
      url: "data:image/png;base64,overlay",
      coordinates: overlayCorners,
    });
    expect(image.url).not.toBe(result.heatmapPngDataUrl);
    expect(image.coordinates).not.toEqual(result.imageCorners);
  });

  it("rejects an incomplete corner contract before MapLibre receives it", () => {
    expect(() =>
      buildMapOverlayImageSpec({
        mapOverlayProjection: "EPSG:3857",
        mapOverlayCorners: overlayCorners.slice(0, 3),
        mapOverlayPngDataUrl: "data:image/png;base64,overlay",
      }),
    ).toThrow("exactly four corners");
  });
});
