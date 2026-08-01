// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import {
  applyVisibleSignalThreshold,
  decodeMapOverlayFilter,
} from "./coverageVisibility";
import {
  MapOverlayBlobUrlLease,
  buildMapOverlayImageSpec,
  type MapLibreImageCoordinates,
} from "./mapOverlay";
import type { CalculationResult } from "./types";

const MAX_LOAD_ATTEMPTS = 2;

export class MapOverlayCanvasLease {
  readonly canvas: HTMLCanvasElement;

  private readonly blobUrls = new MapOverlayBlobUrlLease();
  private generation = 0;
  private image: HTMLImageElement | null = null;
  private pngDataUrl: string | null = null;
  private filterBase64: string | null = null;
  private imageData: ImageData | null = null;
  private originalAlpha: Uint8ClampedArray | null = null;
  private filterBins: Uint8Array | null = null;
  private thresholdDbm: number | null = null;
  private coordinatesValue: MapLibreImageCoordinates | null = null;
  private readyValue = false;
  private disposed = false;
  private dirtyValue = false;
  private loadAttempts = 0;
  private retryScheduled = false;
  private lastOnReady: (() => void) | null = null;

  constructor() {
    this.canvas = document.createElement("canvas");
    this.canvas.width = 1;
    this.canvas.height = 1;
  }

  get ready(): boolean {
    return this.readyValue;
  }

  get dirty(): boolean {
    return this.dirtyValue;
  }

  get coordinates(): MapLibreImageCoordinates | null {
    return this.coordinatesValue;
  }

  update(
    result: CalculationResult,
    thresholdDbm: number,
    onReady: () => void,
  ): boolean {
    if (this.disposed) return false;
    this.lastOnReady = onReady;
    const samePayload =
      this.pngDataUrl === result.mapOverlayPngDataUrl &&
      this.filterBase64 === result.mapOverlayFilterBase64;
    if (samePayload) {
      this.thresholdDbm = thresholdDbm;
      if (this.readyValue) this.applyThreshold(thresholdDbm);
      else if (
        !this.image &&
        !this.retryScheduled &&
        this.loadAttempts < MAX_LOAD_ATTEMPTS
      ) {
        this.startLoadAttempt(result, onReady);
      }
      return this.readyValue;
    }

    this.generation += 1;
    this.readyValue = false;
    this.dirtyValue = false;
    this.thresholdDbm = thresholdDbm;
    this.pngDataUrl = result.mapOverlayPngDataUrl;
    this.filterBase64 = result.mapOverlayFilterBase64;
    this.loadAttempts = 0;
    this.retryScheduled = false;
    this.imageData = null;
    this.originalAlpha = null;
    this.filterBins = null;
    this.image?.removeAttribute("src");
    this.image = null;
    this.blobUrls.clear();
    return this.startLoadAttempt(result, onReady);
  }

  markUploaded(): void {
    if (this.readyValue) this.dirtyValue = false;
  }

  dispose(): void {
    this.disposed = true;
    this.generation += 1;
    this.retryScheduled = false;
    this.lastOnReady = null;
    this.image?.removeAttribute("src");
    this.image = null;
    this.blobUrls.clear();
    this.failClosed();
  }

  private startLoadAttempt(
    result: CalculationResult,
    onReady: () => void,
  ): boolean {
    this.loadAttempts += 1;
    this.generation += 1;
    const generation = this.generation;
    this.readyValue = false;
    this.dirtyValue = false;
    let filterBins: Uint8Array;
    let coordinates: MapLibreImageCoordinates;
    let objectUrl: string;
    try {
      filterBins = decodeMapOverlayFilter(result);
      coordinates = buildMapOverlayImageSpec(result).coordinates;
      objectUrl = this.blobUrls.acquire(result.mapOverlayPngDataUrl);
    } catch {
      this.loadAttempts = MAX_LOAD_ATTEMPTS;
      this.failClosed();
      this.scheduleSynchronization(generation, onReady);
      return false;
    }

    this.canvas.width = result.mapOverlayWidth;
    this.canvas.height = result.mapOverlayHeight;
    this.coordinatesValue = coordinates;
    this.filterBins = filterBins;

    const image = new Image();
    this.image = image;
    image.decoding = "async";
    image.onload = () => {
      if (this.disposed || generation !== this.generation) return;
      this.image = null;
      try {
        if (
          image.naturalWidth !== result.mapOverlayWidth ||
          image.naturalHeight !== result.mapOverlayHeight
        ) {
          throw new Error("map overlay PNG dimensions do not match the filter");
        }
        const context = this.canvas.getContext("2d", {
          alpha: true,
          willReadFrequently: true,
        });
        if (!context) throw new Error("2D canvas is unavailable");
        context.clearRect(0, 0, this.canvas.width, this.canvas.height);
        context.drawImage(image, 0, 0, this.canvas.width, this.canvas.height);
        const imageData = context.getImageData(
          0,
          0,
          this.canvas.width,
          this.canvas.height,
        );
        const originalAlpha = new Uint8ClampedArray(filterBins.length);
        for (let index = 0; index < filterBins.length; index += 1) {
          originalAlpha[index] = imageData.data[index * 4 + 3];
        }
        this.imageData = imageData;
        this.originalAlpha = originalAlpha;
        this.readyValue = true;
        if (!this.applyThreshold(this.thresholdDbm ?? -140, true)) {
          if (!this.readyValue) {
            this.scheduleSynchronization(generation, onReady);
          }
          return;
        }
        onReady();
      } catch {
        this.failClosed();
        this.scheduleSynchronization(generation, onReady);
      }
    };
    image.onerror = () => {
      if (this.disposed || generation !== this.generation) return;
      this.image = null;
      this.failClosed();
      this.scheduleSynchronization(generation, onReady);
    };
    image.src = objectUrl;
    return false;
  }

  applyThreshold(thresholdDbm: number, force = false): boolean {
    if (
      !this.readyValue ||
      !this.imageData ||
      !this.originalAlpha ||
      !this.filterBins ||
      (!force && this.thresholdDbm === thresholdDbm)
    ) {
      this.thresholdDbm = thresholdDbm;
      return false;
    }
    this.thresholdDbm = thresholdDbm;
    try {
      applyVisibleSignalThreshold(
        this.imageData.data,
        this.originalAlpha,
        this.filterBins,
        thresholdDbm,
      );
      const context = this.canvas.getContext("2d");
      if (!context) throw new Error("2D canvas is unavailable");
      context.putImageData(this.imageData, 0, 0);
      this.dirtyValue = true;
      return true;
    } catch {
      const generation = this.generation;
      this.failClosed();
      this.scheduleSynchronization(generation, this.lastOnReady);
      return false;
    }
  }

  private scheduleSynchronization(
    generation: number,
    onReady: (() => void) | null,
  ): void {
    if (!onReady || this.retryScheduled || this.disposed) return;
    this.retryScheduled = true;
    queueMicrotask(() => {
      if (this.disposed || generation !== this.generation) return;
      this.retryScheduled = false;
      onReady();
    });
  }

  private failClosed(): void {
    this.readyValue = false;
    this.dirtyValue = false;
    this.imageData = null;
    this.originalAlpha = null;
    this.filterBins = null;
    this.coordinatesValue = null;
    this.image = null;
    this.blobUrls.clear();
    this.canvas.width = 1;
    this.canvas.height = 1;
  }
}
