// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { CalculationResult } from "./types";
import { MapOverlayCanvasLease } from "./mapOverlayCanvas";

interface ControlledImage {
  naturalWidth: number;
  naturalHeight: number;
  decoding: string;
  onload: ((event: Event) => void) | null;
  onerror: ((event: Event) => void) | null;
  src: string;
  removeAttribute: ReturnType<typeof vi.fn>;
}

const PNG_SIGNATURE = [137, 80, 78, 71, 13, 10, 26, 10] as const;

function bytesBase64(bytes: readonly number[]): string {
  return btoa(String.fromCharCode(...bytes));
}

function result(
  suffix: readonly number[] = [],
): CalculationResult {
  return {
    mapOverlayProjection: "EPSG:3857",
    mapOverlayWidth: 2,
    mapOverlayHeight: 1,
    mapOverlayCorners: [
      [101, 31],
      [105, 31],
      [105, 29],
      [101, 29],
    ],
    mapOverlayPngDataUrl: `data:image/png;base64,${bytesBase64([
      ...PNG_SIGNATURE,
      ...suffix,
    ])}`,
    mapOverlayFilterEncoding: "u8-dbm-floor-v1",
    mapOverlayFilterBase64: bytesBase64([21, 81]),
  } as CalculationResult;
}

describe("MapOverlayCanvasLease", () => {
  const images: ControlledImage[] = [];
  const createObjectUrl = vi.fn(
    () => `blob:coverage-${createObjectUrl.mock.calls.length}`,
  );
  const revokeObjectUrl = vi.fn();
  const imageData = {
    data: new Uint8ClampedArray([
      255, 0, 0, 220,
      0, 0, 255, 180,
    ]),
    width: 2,
    height: 1,
    colorSpace: "srgb",
  } as ImageData;
  const context = {
    clearRect: vi.fn(),
    drawImage: vi.fn(),
    getImageData: vi.fn(() => imageData),
    putImageData: vi.fn(),
  };
  let contextAvailable = true;

  beforeEach(() => {
    images.length = 0;
    contextAvailable = true;
    vi.clearAllMocks();
    imageData.data.set([
      255, 0, 0, 220,
      0, 0, 255, 180,
    ]);
    Object.defineProperty(URL, "createObjectURL", {
      configurable: true,
      value: createObjectUrl,
    });
    Object.defineProperty(URL, "revokeObjectURL", {
      configurable: true,
      value: revokeObjectUrl,
    });
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(
      () => (contextAvailable ? context : null) as never,
    );
    vi.stubGlobal(
      "Image",
      class {
        naturalWidth = 2;
        naturalHeight = 1;
        decoding = "";
        onload: ((event: Event) => void) | null = null;
        onerror: ((event: Event) => void) | null = null;
        src = "";
        removeAttribute = vi.fn();

        constructor() {
          images.push(this);
        }
      },
    );
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("marks successful pixels dirty until their source upload is acknowledged", () => {
    const lease = new MapOverlayCanvasLease();
    const onReady = vi.fn();

    expect(lease.update(result(), -120, onReady)).toBe(false);
    images[0].onload?.(new Event("load"));

    expect(lease.ready).toBe(true);
    expect(lease.dirty).toBe(true);
    expect(imageData.data[3]).toBe(220);
    expect(imageData.data[7]).toBe(180);
    lease.markUploaded();
    expect(lease.dirty).toBe(false);
    expect(onReady).toHaveBeenCalledOnce();
  });

  it("retries the same payload once after a transient image failure", async () => {
    const lease = new MapOverlayCanvasLease();
    const payload = result();
    const synchronize = vi.fn(() => lease.update(payload, -120, synchronize));

    lease.update(payload, -120, synchronize);
    images[0].onerror?.(new Event("error"));
    await Promise.resolve();
    expect(images).toHaveLength(2);

    images[1].onload?.(new Event("load"));
    expect(lease.ready).toBe(true);
    expect(lease.dirty).toBe(true);
    expect(synchronize).toHaveBeenCalledTimes(2);
  });

  it("retries a transient canvas failure once and then renders", async () => {
    const lease = new MapOverlayCanvasLease();
    const payload = result();
    const synchronize = vi.fn(() => lease.update(payload, -120, synchronize));

    lease.update(payload, -120, synchronize);
    contextAvailable = false;
    images[0].onload?.(new Event("load"));
    contextAvailable = true;
    await Promise.resolve();
    expect(images).toHaveLength(2);

    images[1].onload?.(new Event("load"));
    expect(lease.ready).toBe(true);
    expect(lease.dirty).toBe(true);
  });

  it("stops after one retry and remains fail-closed for a bad image", async () => {
    const lease = new MapOverlayCanvasLease();
    const payload = result();
    const synchronize = vi.fn(() => lease.update(payload, -120, synchronize));

    lease.update(payload, -120, synchronize);
    images[0].onerror?.(new Event("error"));
    await Promise.resolve();
    images[1].onerror?.(new Event("error"));
    await Promise.resolve();

    expect(images).toHaveLength(2);
    expect(lease.ready).toBe(false);
    expect(lease.dirty).toBe(false);
    expect(lease.coordinates).toBeNull();
    expect(lease.canvas.width).toBe(1);
    expect(lease.canvas.height).toBe(1);
  });

  it("ignores stale image completion after a new payload generation starts", () => {
    const lease = new MapOverlayCanvasLease();
    const onReady = vi.fn();

    lease.update(result([1]), -120, onReady);
    lease.update(result([2]), -120, onReady);
    images[0].onload?.(new Event("load"));
    expect(lease.ready).toBe(false);

    images[1].onload?.(new Event("load"));
    expect(lease.ready).toBe(true);
    expect(onReady).toHaveBeenCalledOnce();
  });
});
