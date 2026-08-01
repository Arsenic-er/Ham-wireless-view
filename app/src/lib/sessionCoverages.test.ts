import { describe, expect, it } from "vitest";

import { DEFAULT_PARAMETERS } from "./parameters";
import {
  MAX_SESSION_COVERAGES,
  mergeSessionCoverage,
} from "./sessionCoverages";
import type { CalculationResult, SessionCoverageResult } from "./types";

function coverage(id: string, lat: number, lon: number): SessionCoverageResult {
  const result: CalculationResult = {
    schemaVersion: 4,
    modelName: "model",
    modelVersion: "version",
    center: { lat, lon },
    txGroundElevationM: 100,
    txGroundElevationSource: "dem",
    imageWidth: 1,
    imageHeight: 1,
    imageCorners: [[lon, lat]],
    heatmapPngDataUrl: "data:image/png;base64,iVBORw0KGgo=",
    mapOverlayProjection: "EPSG:3857",
    mapOverlayWidth: 1,
    mapOverlayHeight: 1,
    mapOverlayCorners: [[lon, lat]],
    mapOverlayPngDataUrl: "data:image/png;base64,iVBORw0KGgo=",
    mapOverlayFilterEncoding: "u8-dbm-floor-v1",
    mapOverlayFilterBase64: "AQ==",
    statistics: {
      validPixelCount: 1,
      belowThresholdPixelCount: 0,
      warningPixelCount: 0,
      minimumDbm: -100,
      maximumDbm: -80,
      meanDbm: -90,
      waterAffectedPixelCount: 0,
      meanPathWaterFraction: 0,
      propagationSeconds: 1,
      totalSeconds: 1,
    },
  };
  return {
    id,
    result,
    parameters: { ...DEFAULT_PARAMETERS },
    completedAt: Number(id.replace(/\D/g, "")) || 1,
  };
}

describe("session coverage retention", () => {
  it("keeps completed results from different transmitter locations", () => {
    const first = coverage("coverage-1", 30.5, 103.5);
    const second = coverage("coverage-2", 30.6, 103.6);

    expect(mergeSessionCoverage([first], second)).toEqual([first, second]);
  });

  it("replaces the previous result at the exact same location", () => {
    const first = coverage("coverage-1", 30.5, 103.5);
    const other = coverage("coverage-2", 30.6, 103.6);
    const replacement = coverage("coverage-3", 30.5, 103.5);

    expect(mergeSessionCoverage([first, other], replacement)).toEqual([
      other,
      replacement,
    ]);
  });

  it("evicts the oldest layer when the session reaches its limit", () => {
    let current: SessionCoverageResult[] = [];
    for (let index = 0; index <= MAX_SESSION_COVERAGES; index += 1) {
      current = mergeSessionCoverage(
        current,
        coverage(`coverage-${index + 1}`, 20 + index, 100 + index),
      );
    }

    expect(current).toHaveLength(MAX_SESSION_COVERAGES);
    expect(current[0]?.id).toBe("coverage-2");
    expect(current.at(-1)?.id).toBe("coverage-9");
  });
});
