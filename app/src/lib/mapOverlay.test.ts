// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it, vi } from "vitest";

import {
  MapOverlayBlobUrlLease,
  buildMapOverlayImageSpec,
  createMapOverlayBlobUrl,
} from "./mapOverlay";

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

  it("converts a backend PNG data URL into an image/png blob URL without fetching data:", () => {
    let capturedBlob: Blob | null = null;
    const objectUrl = createMapOverlayBlobUrl(
      "data:image/png;base64,iVBORw0KGgo=",
      (blob) => {
        capturedBlob = blob;
        return "blob:hamheatmap-overlay";
      },
    );

    expect(objectUrl).toBe("blob:hamheatmap-overlay");
    expect(capturedBlob).not.toBeNull();
    expect(capturedBlob!.type).toBe("image/png");
    expect(capturedBlob!.size).toBe(8);
  });

  it("rejects non-PNG, malformed base64, and invalid PNG signatures", () => {
    expect(() => createMapOverlayBlobUrl("data:text/plain;base64,aGk=")).toThrow(
      "base64 PNG",
    );
    expect(() => createMapOverlayBlobUrl("data:image/png;base64,%%%"))
      .toThrow("invalid base64");
    expect(() => createMapOverlayBlobUrl("data:image/png;base64,bm90LXBuZw=="))
      .toThrow("PNG signature");
  });

  it("reuses the active blob URL and revokes it on replacement, clear, and only once", () => {
    const create = vi.fn((dataUrl: string) => `blob:${dataUrl}`);
    const revoke = vi.fn();
    const lease = new MapOverlayBlobUrlLease(create, revoke);

    expect(lease.acquire("first")).toBe("blob:first");
    expect(lease.acquire("first")).toBe("blob:first");
    expect(create).toHaveBeenCalledTimes(1);
    expect(revoke).not.toHaveBeenCalled();

    expect(lease.acquire("second")).toBe("blob:second");
    expect(revoke).toHaveBeenNthCalledWith(1, "blob:first");
    lease.clear();
    expect(revoke).toHaveBeenNthCalledWith(2, "blob:second");
    lease.clear();
    expect(revoke).toHaveBeenCalledTimes(2);
  });

});
