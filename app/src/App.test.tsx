import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { BackendMode } from "./lib/backend";
import type { CalculationResult } from "./lib/types";

const backendMocks = vi.hoisted(() => ({
  mode: "validation-server" as BackendMode,
  bootstrap: vi.fn(),
  inspectPoint: vi.fn(),
  calculate: vi.fn(),
  cacheOverview: vi.fn(),
  estimateDownload: vi.fn(),
}));

vi.mock("./lib/backend", () => ({
  backendCapabilities: () => ({
    mode: backendMocks.mode,
    canDownload: backendMocks.mode !== "preview",
    canDeleteCache: backendMocks.mode !== "preview",
    canCalculate: backendMocks.mode !== "preview",
    canExport: backendMocks.mode === "tauri",
  }),
  bootstrap: backendMocks.bootstrap,
  inspectPoint: backendMocks.inspectPoint,
  calculate: backendMocks.calculate,
  cacheOverview: backendMocks.cacheOverview,
  cancelCalculation: vi.fn().mockResolvedValue(undefined),
  cancelDownload: vi.fn().mockResolvedValue(undefined),
  deleteCacheRegion: vi.fn(),
  downloadRegion: vi.fn(),
  estimateDownload: backendMocks.estimateDownload,
  exportReport: vi.fn(),
  listenCalculationProgress: vi.fn().mockResolvedValue(() => undefined),
  listenDownloadProgress: vi.fn().mockResolvedValue(() => undefined),
}));

vi.mock("./components/MapView", () => ({
  MapView: ({
    point,
    heatmap,
    onPointSelect,
  }: {
    point: { lat: number; lon: number } | null;
    heatmap: CalculationResult | null;
    onPointSelect: (point: { lat: number; lon: number }) => void;
  }) => (
    <div>
      <button type="button" onClick={() => onPointSelect({ lat: 30.5, lon: 103.5 })}>
        select-point
      </button>
      <span data-testid="selected-point">{point ? `${point.lat},${point.lon}` : "none"}</span>
      <span data-testid="heatmap">{heatmap ? "present" : "none"}</span>
    </div>
  ),
}));

vi.mock("./components/ParameterPanel", () => ({
  ParameterPanel: () => <div>parameter-panel</div>,
}));

import { App } from "./App";

const cacheUsage = {
  totalBytes: 120_000_000,
  demBytes: 100_000_000,
  waterBytes: 19_000_000,
  partialBytes: 0,
  metadataBytes: 1_000_000,
  remainingBytes: 2_380_000_000,
  capBytes: 2_500_000_000,
};

const result: CalculationResult = {
  schemaVersion: 2,
  modelName: "NTIA ITM Point-to-Point",
  modelVersion: "land-water-v1",
  center: { lat: 30.5, lon: 103.5 },
  imageWidth: 401,
  imageHeight: 401,
  imageCorners: [[0, 0], [1, 0], [1, 1], [0, 1]],
  heatmapPngDataUrl: "data:image/png;base64,AA==",
  mapOverlayProjection: "EPSG:3857",
  mapOverlayWidth: 401,
  mapOverlayHeight: 401,
  mapOverlayCorners: [[0, 0], [1, 0], [1, 1], [0, 1]],
  mapOverlayPngDataUrl: "data:image/png;base64,AA==",
  statistics: {
    validPixelCount: 125_628,
    belowThresholdPixelCount: 1,
    warningPixelCount: 0,
    minimumDbm: -150,
    maximumDbm: -40,
    meanDbm: -100,
    waterAffectedPixelCount: 0,
    meanPathWaterFraction: 0,
    propagationSeconds: 1,
    totalSeconds: 2,
  },
};

beforeEach(() => {
  backendMocks.mode = "validation-server";
  backendMocks.bootstrap.mockResolvedValue({
    schemaVersion: 2,
    modelName: "NTIA ITM Point-to-Point",
    modelVersion: "land-water-v1",
    coverageRadiusKm: 200,
    gridSize: 401,
    cacheUsage,
    internalBuildWarning: "internal",
  });
  backendMocks.inspectPoint.mockResolvedValue({
    point: { lat: 30.5, lon: 103.5 },
    regionId: "region",
    tileCount: 25,
    readyDemCount: 25,
    readyWaterCount: 25,
    missingAssetCount: 0,
    dataReady: true,
    elevationM: 512,
    cacheUsage,
  });
  backendMocks.calculate.mockResolvedValue(result);
  backendMocks.cacheOverview.mockResolvedValue({ usage: cacheUsage, regions: [] });
  backendMocks.estimateDownload.mockResolvedValue({
    point: { lat: 30.5, lon: 103.5 },
    regionId: "region",
    tileCount: 25,
    readyAssetCount: 0,
    requiredAssetCount: 50,
    generatedAssetCount: 0,
    additionalDownloadBytes: 120_000_000,
    resumableBytes: 0,
    projectedTotalBytes: 120_000_000,
    projectedRemainingBytes: 2_380_000_000,
    cacheUsage,
  });
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: vi.fn().mockReturnValue({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    }),
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("validation server UI", () => {
  it("discloses remote processing, enables calculation, and keeps export disabled", async () => {
    render(<App />);

    expect(await screen.findByText("\u5185\u90e8\u670d\u52a1\u5668\u9a8c\u8bc1")).toBeTruthy();
    expect(screen.getByText(/\u5750\u6807.*\u53d1\u9001\u5230\u672c\u670d\u52a1\u5668/)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("\u6570\u636e\u5df2\u5c31\u7eea");

    const calculateButton = screen.getByRole("button", { name: /\u5f00\u59cb\u8ba1\u7b97/ });
    expect((calculateButton as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(calculateButton);

    await screen.findByText("\u8986\u76d6\u8ba1\u7b97\u5b8c\u6210");
    expect(backendMocks.calculate).toHaveBeenCalledTimes(1);
    await waitFor(() => {
      const exportButton = screen.getByRole("button", { name: /\u5bfc\u51fa/ });
      expect((exportButton as HTMLButtonElement).disabled).toBe(true);
    });
  });

  it("clears only the heatmap and keeps the selected ready point reusable", async () => {
    render(<App />);

    await screen.findByText("\u7b49\u5f85\u9009\u62e9\u53d1\u5c04\u70b9");
    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("\u6570\u636e\u5df2\u5c31\u7eea");
    expect(screen.getByTestId("selected-point").textContent).toBe("30.5,103.5");

    fireEvent.click(screen.getByRole("button", { name: /\u5f00\u59cb\u8ba1\u7b97/ }));
    await screen.findByText("\u8986\u76d6\u8ba1\u7b97\u5b8c\u6210");
    expect(screen.getByTestId("heatmap").textContent).toBe("present");

    fireEvent.click(screen.getByRole("button", { name: "\u6e05\u7a7a" }));

    expect(screen.getByTestId("heatmap").textContent).toBe("none");
    expect(screen.getByTestId("selected-point").textContent).toBe("30.5,103.5");
    expect(screen.getByText("\u6570\u636e\u5df2\u5c31\u7eea")).toBeTruthy();
    expect(backendMocks.inspectPoint).toHaveBeenCalledTimes(1);

    const calculateButton = screen.getByRole("button", { name: /\u5f00\u59cb\u8ba1\u7b97/ });
    expect((calculateButton as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(calculateButton);
    await waitFor(() => expect(backendMocks.calculate).toHaveBeenCalledTimes(2));
  });

  it("keeps ordinary preview interface-only and disables confirmed download", async () => {
    backendMocks.mode = "preview";
    backendMocks.inspectPoint.mockResolvedValueOnce({
      point: { lat: 30.5, lon: 103.5 },
      regionId: "preview",
      tileCount: 25,
      readyDemCount: 0,
      readyWaterCount: 0,
      missingAssetCount: 50,
      dataReady: false,
      elevationM: null,
      cacheUsage: { ...cacheUsage, totalBytes: 0 },
    });
    render(<App />);

    expect(screen.queryByText("\u5185\u90e8\u670d\u52a1\u5668\u9a8c\u8bc1")).toBeNull();
    await screen.findByText("\u7b49\u5f85\u9009\u62e9\u53d1\u5c04\u70b9");
    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("\u6d4f\u89c8\u5668\u754c\u9762\u9884\u89c8");

    fireEvent.click(screen.getByRole("button", { name: /\u9884\u89c8\u4e0b\u8f7d\u786e\u8ba4/ }));
    await screen.findByText("\u786e\u8ba4\u4e0b\u8f7d DEM \u4e0e WBM");
    const confirm = screen.getByRole("button", { name: "\u4e0b\u8f7d\u5e76\u51c6\u5907" });
    expect((confirm as HTMLButtonElement).disabled).toBe(true);
  });
});
