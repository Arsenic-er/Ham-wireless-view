import { describe, expect, it } from "vitest";

import {
  DBM_COLOR_ANCHORS,
  buildExportReportModel,
  suggestedExportFileName,
} from "./export";
import type { CalculationResult, RadioParameters } from "./types";

const result: CalculationResult = {
  schemaVersion: 2,
  modelName: "NTIA ITM Point-to-Point",
  modelVersion: "land-water-v1",
  center: { lat: 30.5, lon: 103.5 },
  imageWidth: 401,
  imageHeight: 401,
  imageCorners: [
    [101.4, 32.3],
    [105.6, 32.3],
    [105.6, 28.7],
    [101.4, 28.7],
  ],
  heatmapPngDataUrl: "data:image/png;base64,iVBORw0KGgo=",
  mapOverlayProjection: "EPSG:3857",
  mapOverlayWidth: 401,
  mapOverlayHeight: 401,
  mapOverlayCorners: [
    [101.39, 32.31],
    [105.61, 32.31],
    [105.61, 28.69],
    [101.39, 28.69],
  ],
  mapOverlayPngDataUrl: "data:image/png;base64,iVBORw0KGgo=",
  statistics: {
    validPixelCount: 125_628,
    belowThresholdPixelCount: 400,
    warningPixelCount: 0,
    minimumDbm: -139.9,
    maximumDbm: -42.3,
    meanDbm: -96.2,
    waterAffectedPixelCount: 12_563,
    meanPathWaterFraction: 0.07,
    propagationSeconds: 8.5,
    totalSeconds: 9.75,
  },
};

const parameters: RadioParameters = {
  preset: "base-to-handheld",
  band: "vhf144",
  frequencyMhz: 145.25,
  powerValue: 25,
  powerUnit: "watt",
  txGainValue: 6,
  txGainUnit: "dbi",
  txHeightM: 20,
  rxGainValue: -3,
  rxGainUnit: "dbi",
  rxHeightM: 1.5,
  polarization: "vertical",
};

describe("export report model", () => {
  it("records the selected parameters and deterministic report facts", () => {
    const model = buildExportReportModel(result, parameters, new Date(2026, 6, 16, 12, 34, 56));
    expect(model.generatedAt).toMatch(/^2026-07-16 12:34:56 UTC[+-]\d{2}:\d{2}$/);
    expect(model.center).toBe("30.50000°, 103.50000°");
    expect(model.parameterRows).toContainEqual(["频段 / 频率", "144 MHz / 145.25 MHz"]);
    expect(model.parameterRows).toContainEqual(["极化", "垂直"]);
    expect(model.statisticRows).toContainEqual(["受水体影响路径", "10.0%"]);
    expect(model.warning).toContain("内部测试，不得公开发布");
    expect(model.subtitle).toContain("NTIA ITM v1.4 (668e4ab)");
    expect(model.subtitle).toContain("Copernicus DEM GLO-90 DEM/WBM");
  });

  it("keeps the fixed dBm anchors aligned with the Rust color scale", () => {
    expect(DBM_COLOR_ANCHORS.map(({ dbm }) => dbm)).toEqual([-60, -75, -90, -105, -120, -140]);
    expect(DBM_COLOR_ANCHORS.map(({ position }) => position)).toEqual([0, 0.1875, 0.375, 0.5625, 0.75, 1]);
  });

  it("creates a Windows-safe default file name for each format", () => {
    const at = new Date(2026, 6, 16, 12, 34, 56);
    expect(suggestedExportFileName(result, parameters, "png", at)).toBe(
      "HamHeatmap_145p25MHz_30.5000_103.5000_20260716-123456.png",
    );
    expect(suggestedExportFileName(result, parameters, "pdf", at)).toMatch(/\.pdf$/);
  });
});
