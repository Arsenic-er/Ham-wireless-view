// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { BackendMode } from "./lib/backend";
import type {
  BasemapInfo,
  CalculationPreview,
  CalculationResult,
  LinkAnalysisResult,
  LinkParameters,
  OnlineBasemapInfo,
  RadioParameters,
  SessionCoverageResult,
} from "./lib/types";

const backendMocks = vi.hoisted(() => ({
  mode: "validation-server" as BackendMode,
  bootstrap: vi.fn(),
  inspectPoint: vi.fn(),
  calculate: vi.fn(),
  analyzeLink: vi.fn(),
  cancelCalculation: vi.fn(),
  cacheOverview: vi.fn(),
  estimateDownload: vi.fn(),
  configureOnlineBasemap: vi.fn(),
  probeOnlineBasemap: vi.fn(),
  clearOnlineBasemap: vi.fn(),
  exportReport: vi.fn(),
  isCancellationError: vi.fn(),
  localizedBackendError: vi.fn(),
  previewHandler: null as ((preview: CalculationPreview) => void) | null,
  deleteCacheRegion: vi.fn(),
}));

const mapPointMocks = vi.hoisted(() => ({
  extraPoint: { lat: 30.5, lon: 103.5 },
}));

vi.mock("./lib/backend", () => ({
  backendCapabilities: () => ({
    mode: backendMocks.mode,
    canDownload: backendMocks.mode !== "preview",
    canDeleteCache: backendMocks.mode !== "preview",
    canCalculate: backendMocks.mode !== "preview",
    canAnalyzeLink: backendMocks.mode !== "preview",
    canExport: backendMocks.mode !== "preview",
    canConfigureOnlineBasemap: backendMocks.mode === "tauri",
  }),
  bootstrap: backendMocks.bootstrap,
  inspectPoint: backendMocks.inspectPoint,
  calculate: backendMocks.calculate,
  analyzeLink: backendMocks.analyzeLink,
  cacheOverview: backendMocks.cacheOverview,
  cancelCalculation: backendMocks.cancelCalculation,
  cancelDownload: vi.fn().mockResolvedValue(undefined),
  deleteCacheRegion: backendMocks.deleteCacheRegion,
  downloadRegion: vi.fn(),
  estimateDownload: backendMocks.estimateDownload,
  exportReport: backendMocks.exportReport,
  isCancellationError: backendMocks.isCancellationError,
  localizedBackendError: backendMocks.localizedBackendError,
  configureOnlineBasemap: backendMocks.configureOnlineBasemap,
  probeOnlineBasemap: backendMocks.probeOnlineBasemap,
  clearOnlineBasemap: backendMocks.clearOnlineBasemap,
  listenCalculationPreview: vi.fn().mockImplementation(
    async (handler: (preview: CalculationPreview) => void) => {
      backendMocks.previewHandler = handler;
      return () => {
        if (backendMocks.previewHandler === handler) backendMocks.previewHandler = null;
      };
    },
  ),
  listenCalculationProgress: vi.fn().mockResolvedValue(() => undefined),
  listenDownloadProgress: vi.fn().mockResolvedValue(() => undefined),
}));

const exportMocks = vi.hoisted(() => ({
  createReport: vi.fn(),
  suggestedFileName: vi.fn(),
}));

vi.mock("./lib/export", () => ({
  createExportReportPngDataUrl: exportMocks.createReport,
  suggestedExportFileName: exportMocks.suggestedFileName,
}));

vi.mock("./components/MapView", () => ({
  MapView: ({
    point,
    heatmaps,
    preview,
    linkTx,
    linkRx,
    linkResult,
    onPointSelect,
  }: {
    point: { lat: number; lon: number } | null;
    heatmaps: SessionCoverageResult[];
    preview: CalculationPreview | null;
    linkTx?: { lat: number; lon: number } | null;
    linkRx?: { lat: number; lon: number } | null;
    linkResult?: LinkAnalysisResult | null;
    onPointSelect: (point: { lat: number; lon: number }) => void;
  }) => (
    <div>
      <button type="button" onClick={() => onPointSelect({ lat: 30.5, lon: 103.5 })}>
        select-point
      </button>
      <button type="button" onClick={() => onPointSelect({ lat: 31, lon: 104 })}>
        select-new-point
      </button>
      <button type="button" onClick={() => onPointSelect(mapPointMocks.extraPoint)}>
        select-extra-point
      </button>
      <span data-testid="selected-point">{point ? `${point.lat},${point.lon}` : "none"}</span>
      <span data-testid="heatmap">{heatmaps.length ? "present" : "none"}</span>
      <span data-testid="heatmap-count">{heatmaps.length}</span>
      <span data-testid="preview">{preview ? String(preview.sequence) : "none"}</span>
      <span data-testid="link-tx">{linkTx ? `${linkTx.lat},${linkTx.lon}` : "none"}</span>
      <span data-testid="link-rx">{linkRx ? `${linkRx.lat},${linkRx.lon}` : "none"}</span>
      <span data-testid="link-result">{linkResult ? linkResult.classification : "none"}</span>
    </div>
  ),
}));

vi.mock("./components/LinkProfileChart", () => ({
  LinkProfileChart: ({ result }: { result: LinkAnalysisResult }) => (
    <div data-testid="link-profile">{result.classification}</div>
  ),
}));

vi.mock("./components/LinkParameterPanel", () => ({
  LinkParameterPanel: ({
    parameters,
    onChange,
  }: {
    parameters: LinkParameters;
    onChange: (parameters: LinkParameters) => void;
  }) => (
    <div>
      <span data-testid="link-threshold">{parameters.receiverThresholdDbm}</span>
      <button
        type="button"
        onClick={() =>
          onChange({ ...parameters, receiverThresholdDbm: -110 })
        }
      >
        change-link-threshold
      </button>
    </div>
  ),
}));

vi.mock("./components/ParameterPanel", () => ({
  ParameterPanel: ({
    parameters,
    onChange,
  }: {
    parameters: RadioParameters;
    onChange: (parameters: RadioParameters) => void;
  }) => (
    <div>
      <span data-testid="ground-elevation-override">
        {parameters.txGroundElevationOverrideM === null
          ? "automatic"
          : String(parameters.txGroundElevationOverrideM)}
      </span>
      <button
        type="button"
        onClick={() => onChange({ ...parameters, txGroundElevationOverrideM: 800 })}
      >
        set-manual-override
      </button>
      <button
        type="button"
        onClick={() => onChange({ ...parameters, txGroundElevationOverrideM: 900 })}
      >
        change-manual-override
      </button>
    </div>
  ),
}));

import i18n from "./i18n";
import { App } from "./App";
import { directWgs84 } from "./lib/geodesy";

const cacheUsage = {
  totalBytes: 120_000_000,
  demBytes: 100_000_000,
  waterBytes: 19_000_000,
  partialBytes: 0,
  metadataBytes: 1_000_000,
  remainingBytes: 2_380_000_000,
  capBytes: 2_500_000_000,
};

const cacheRegion = {
  regionId: "region",
  center: { lat: 30.5, lon: 103.5 },
  assetCount: 50,
  readyAssetCount: 50,
  partialAssetCount: 0,
  referencedBytes: 120_000_000,
  reclaimableBytes: 120_000_000,
  createdUnix: 1_700_000_000,
};

function missingInspection(point: { lat: number; lon: number }) {
  return {
    point,
    regionId: "region",
    tileCount: 25,
    readyDemCount: 0,
    readyWaterCount: 0,
    missingAssetCount: 50,
    dataReady: false,
    elevationM: null,
    cacheUsage: { ...cacheUsage, totalBytes: 0 },
  };
}

const tiandituBasemap: BasemapInfo = {
  enabled: true,
  providerId: "tianditu",
  displayName: "天地图",
  attribution: "天地图",
  mode: "same-origin-proxy",
  maxZoom: 18,
  layers: [
    { id: "vec", displayName: "矢量底图" },
    { id: "cva", displayName: "中文注记" },
  ],
  tilePathTemplate: "/api/basemap/tianditu/{layer}/{z}/{x}/{y}",
};


const onlineBasemap: OnlineBasemapInfo = {
  configured: true,
  provider: "Tianditu",
  protocolScheme: "tianditu",
  vectorTemplate: "tianditu://localhost/vec/{z}/{x}/{y}",
  vectorLabelTemplate: "tianditu://localhost/cva/{z}/{x}/{y}",
  imageryTemplate: "tianditu://localhost/img/{z}/{x}/{y}",
  imageryLabelTemplate: "tianditu://localhost/cia/{z}/{x}/{y}",
  attribution: "天地图",
  minZoom: 1,
  maxZoom: 18,
};

const result: CalculationResult = {
  schemaVersion: 4,
  modelName: "NTIA ITM Point-to-Point",
  modelVersion: "land-water-v1",
  center: { lat: 30.5, lon: 103.5 },
  txGroundElevationM: 512,
  txGroundElevationSource: "dem",
  imageWidth: 401,
  imageHeight: 401,
  imageCorners: [[0, 0], [1, 0], [1, 1], [0, 1]],
  heatmapPngDataUrl: "data:image/png;base64,AA==",
  mapOverlayProjection: "EPSG:3857",
  mapOverlayWidth: 401,
  mapOverlayHeight: 401,
  mapOverlayCorners: [[0, 0], [1, 0], [1, 1], [0, 1]],
  mapOverlayPngDataUrl: "data:image/png;base64,AA==",
  mapOverlayFilterEncoding: "u8-dbm-floor-v1",
  mapOverlayFilterBase64: btoa("\x01".repeat(401 * 401)),
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

const linkResult: LinkAnalysisResult = {
  schemaVersion: 1,
  classification: "direct-los",
  classificationReason: "clear",
  distanceM: 75_000,
  initialBearingDeg: 45,
  finalBearingDeg: 225,
  frequencyMhz: 145,
  wavelengthM: 2.0675,
  sampleSpacingM: 90,
  sampleCount: 3,
  effectiveEarthRadiusM: 8_494_678.4,
  kFactor: 4 / 3,
  txGroundElevationM: 500,
  rxGroundElevationM: 420,
  txAntennaElevationM: 520,
  rxAntennaElevationM: 421.5,
  geometricLos: true,
  fresnelClearance60: true,
  minimumLosClearanceM: 20,
  minimumFresnelClearanceRatio: 0.8,
  criticalSampleIndex: 1,
  itmMode: "line-of-sight",
  itmBasicTransmissionLossDb: 120,
  itmWarnings: 0,
  waterFraction: 0,
  coPolarizedReferencePowerDbm: -85,
  polarizationMismatchLossDb: 0,
  predictedRxPowerDbm: -85,
  receiverThresholdDbm: -120,
  linkMarginDb: 35,
  critical: false,
  profile: [
    { distanceM: 0, lat: 30.5, lon: 103.5, terrainElevationM: 500, earthBulgeM: 0, adjustedTerrainM: 500, losHeightM: 520, fresnelRadiusM: 0 },
    { distanceM: 37_500, lat: 30.75, lon: 103.75, terrainElevationM: 430, earthBulgeM: 82, adjustedTerrainM: 512, losHeightM: 470.75, fresnelRadiusM: 197 },
    { distanceM: 75_000, lat: 31, lon: 104, terrainElevationM: 420, earthBulgeM: 0, adjustedTerrainM: 420, losHeightM: 421.5, fresnelRadiusM: 0 },
  ],
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

beforeEach(() => {
  backendMocks.mode = "validation-server";
  backendMocks.previewHandler = null;
  mapPointMocks.extraPoint = { lat: 30.5, lon: 103.5 };
  backendMocks.configureOnlineBasemap.mockResolvedValue(onlineBasemap);
  backendMocks.probeOnlineBasemap.mockResolvedValue({
    schemaVersion: 1,
    status: "reachable",
  });
  backendMocks.clearOnlineBasemap.mockResolvedValue({ ...onlineBasemap, configured: false });
  backendMocks.exportReport.mockResolvedValue({
    cancelled: false,
    path: null,
    bytesWritten: 8,
  });
  backendMocks.isCancellationError.mockImplementation((error: unknown) => {
    if (error && typeof error === "object" && "code" in error) {
      return ["operation.cancelled", "cancelled"].includes(
        String((error as { code?: unknown }).code),
      );
    }
    const message = error instanceof Error ? error.message : String(error);
    return message.toLowerCase().includes("cancel") || message.includes("取消");
  });
  backendMocks.localizedBackendError.mockImplementation((error: unknown) =>
    error instanceof Error ? error.message : String(error),
  );
  exportMocks.createReport.mockResolvedValue("data:image/png;base64,iVBORw0KGgo=");
  exportMocks.suggestedFileName.mockImplementation(
    (_result: CalculationResult, _parameters: RadioParameters, format: string) =>
      `coverage.${format}`,
  );
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
  backendMocks.analyzeLink.mockResolvedValue(linkResult);
  backendMocks.cancelCalculation.mockResolvedValue(undefined);
  backendMocks.cacheOverview.mockResolvedValue({ usage: cacheUsage, regions: [] });
  backendMocks.deleteCacheRegion.mockResolvedValue({
    deletedAssetCount: 50,
    freedBytes: 120_000_000,
    overview: {
      usage: { ...cacheUsage, totalBytes: 0 },
      regions: [],
    },
  });
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

afterEach(async () => {
  cleanup();
  vi.clearAllMocks();
  localStorage.removeItem("hamheatmap.locale.v1");
  await i18n.changeLanguage("zh-CN");
});

describe("desktop online map settings", () => {
  function useDesktopBasemap(configured: boolean) {
    backendMocks.mode = "tauri";
    backendMocks.bootstrap.mockResolvedValue({
      schemaVersion: 2,
      modelName: "NTIA ITM Point-to-Point",
      modelVersion: "land-water-v1",
      coverageRadiusKm: 200,
      gridSize: 401,
      cacheUsage,
      internalBuildWarning: "internal",
      onlineBasemap: { ...onlineBasemap, configured },
    });
  }

  async function openDesktopMapSettings() {
    const settingsButton = await screen.findByRole("button", { name: /在线地图/ });
    await waitFor(() => {
      expect((settingsButton as HTMLButtonElement).disabled).toBe(false);
    });
    fireEvent.click(settingsButton);
    return screen.getByRole("dialog", { name: "配置天地图" });
  }

  it("keeps tk temporary, saves then probes, and clears configuration", async () => {
    useDesktopBasemap(false);
    const storageSpy = vi.spyOn(Storage.prototype, "setItem");
    render(<App />);

    await openDesktopMapSettings();
    const input = screen.getByLabelText("天地图 tk") as HTMLInputElement;
    expect(input.type).toBe("password");
    expect(input.value).toBe("");
    expect(screen.getByText("尚未测试")).toBeTruthy();

    const secret = "temporary-secret-token";
    fireEvent.change(input, { target: { value: secret } });
    fireEvent.click(screen.getByRole("button", { name: "保存并测试" }));

    await waitFor(() => {
      expect(backendMocks.configureOnlineBasemap).toHaveBeenCalledWith(secret);
      expect(backendMocks.probeOnlineBasemap).toHaveBeenCalledOnce();
      expect(input.value).toBe("");
    });
    expect(screen.queryByDisplayValue(secret)).toBeNull();
    expect(storageSpy.mock.calls.every(([, value]) => value !== secret)).toBe(true);
    expect(screen.getByText("配置已保存。")).toBeTruthy();
    expect(screen.getByText("连接测试通过")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "清除配置" }));
    await waitFor(() => expect(backendMocks.clearOnlineBasemap).toHaveBeenCalledOnce());
    expect(screen.getByText("已清除在线地图配置，当前使用 WGS84 坐标网格。")).toBeTruthy();
    expect(screen.getByText("尚未测试")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /测试连接/ })).toBeNull();
  });

  it("retains a saved configuration when the immediate probe fails", async () => {
    useDesktopBasemap(false);
    backendMocks.probeOnlineBasemap.mockResolvedValueOnce({
      schemaVersion: 1,
      status: "upstream-or-credential",
    });
    render(<App />);
    await openDesktopMapSettings();

    fireEvent.change(screen.getByLabelText("天地图 tk"), {
      target: { value: "temporary-secret-token" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存并测试" }));

    expect(await screen.findByText("配置已保存。")).toBeTruthy();
    expect(screen.getByText("服务或配置暂不可用")).toBeTruthy();
    expect(screen.getByText("已保存")).toBeTruthy();
    expect(screen.getByRole("button", { name: "重新测试连接" })).toBeTruthy();
  });

  it("does not probe when saving the configuration fails", async () => {
    useDesktopBasemap(false);
    backendMocks.configureOnlineBasemap.mockRejectedValueOnce(
      new Error("sensitive backend detail"),
    );
    render(<App />);
    await openDesktopMapSettings();

    fireEvent.change(screen.getByLabelText("天地图 tk"), {
      target: { value: "temporary-secret-token" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存并测试" }));

    expect(
      await screen.findByText("在线地图配置未保存。请确认 tk 格式；若缓存接近 2.5 GB，请先清理缓存；若 Windows 本地安全存储（DPAPI）暂不可用，请稍后重试或检查系统状态。"),
    ).toBeTruthy();
    expect(screen.queryByText("sensitive backend detail")).toBeNull();
    expect(backendMocks.probeOnlineBasemap).not.toHaveBeenCalled();
  });

  it.each([
    ["not-configured", "尚未保存配置"],
    ["network", "网络连接失败"],
    ["timeout", "连接测试超时"],
    ["upstream-or-credential", "服务或配置暂不可用"],
    ["invalid-content", "地图响应内容无效"],
  ] as const)("maps %s probe failures to actionable local copy", async (status, title) => {
    useDesktopBasemap(true);
    backendMocks.probeOnlineBasemap.mockResolvedValueOnce({
      schemaVersion: 1,
      status,
    });
    render(<App />);
    await openDesktopMapSettings();

    fireEvent.click(screen.getByRole("button", { name: "测试连接" }));

    expect(await screen.findByText(title)).toBeTruthy();
    expect(backendMocks.configureOnlineBasemap).not.toHaveBeenCalled();
    expect(backendMocks.probeOnlineBasemap).toHaveBeenCalledOnce();
    expect(screen.getByRole("button", { name: "重新测试连接" })).toBeTruthy();
  });

  it("retries an explicit failed probe and reaches success", async () => {
    useDesktopBasemap(true);
    backendMocks.probeOnlineBasemap
      .mockResolvedValueOnce({ schemaVersion: 1, status: "network" })
      .mockResolvedValueOnce({ schemaVersion: 1, status: "reachable" });
    render(<App />);
    await openDesktopMapSettings();

    fireEvent.click(screen.getByRole("button", { name: "测试连接" }));
    expect(await screen.findByText("网络连接失败")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "重新测试连接" }));

    expect(await screen.findByText("连接测试通过")).toBeTruthy();
    expect(backendMocks.probeOnlineBasemap).toHaveBeenCalledTimes(2);
  });

  it("locks the modal while an explicit probe is pending", async () => {
    useDesktopBasemap(true);
    const pending = deferred<{ schemaVersion: 1; status: "reachable" }>();
    backendMocks.probeOnlineBasemap.mockReturnValueOnce(pending.promise);
    render(<App />);
    await openDesktopMapSettings();

    const input = screen.getByLabelText("天地图 tk") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "replacement-token" } });
    fireEvent.click(screen.getByRole("button", { name: "测试连接" }));

    expect(await screen.findByText("正在测试连接…")).toBeTruthy();
    expect(input.disabled).toBe(true);
    expect(
      (screen.getByRole("button", { name: "关闭在线地图设置" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    expect(
      (screen.getByRole("button", { name: "正在测试…" }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(
      (screen.getByRole("button", { name: "清除配置" }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(
      (screen.getByRole("button", { name: "保存并测试" }) as HTMLButtonElement).disabled,
    ).toBe(true);

    await act(async () => {
      pending.resolve({ schemaVersion: 1, status: "reachable" });
      await pending.promise;
    });
    expect(await screen.findByText("连接测试通过")).toBeTruthy();
  });

  it.each(["validation-server", "preview"] as const)(
    "does not expose or invoke desktop map probing in %s mode",
    async (mode) => {
      backendMocks.mode = mode;
      render(<App />);

      await screen.findByText("等待选择发射点");
      expect(screen.queryByRole("button", { name: /在线地图/ })).toBeNull();
      expect(backendMocks.probeOnlineBasemap).not.toHaveBeenCalled();
    },
  );
});
describe("validation server UI", () => {
  it("discloses remote processing and enables calculation plus browser export", async () => {
    render(<App />);

    expect(await screen.findByText("\u5185\u90e8\u670d\u52a1\u5668\u9a8c\u8bc1")).toBeTruthy();
    expect(screen.getByText(/\u5750\u6807.*\u53d1\u9001\u5230\u672c\u670d\u52a1\u5668/)).toBeTruthy();
    expect(
      screen.getByText("未配置受信任的真实底图；当前只显示 WGS84 坐标网格。"),
    ).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("\u6570\u636e\u5df2\u5c31\u7eea");

    const calculateButton = screen.getByRole("button", { name: /\u5f00\u59cb\u8ba1\u7b97/ });
    expect((calculateButton as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(calculateButton);

    await screen.findByText("\u8986\u76d6\u8ba1\u7b97\u5b8c\u6210");
    expect(backendMocks.calculate).toHaveBeenCalledTimes(1);
    await waitFor(() => {
      const exportButton = screen.getByRole("button", { name: /\u5bfc\u51fa/ });
      expect((exportButton as HTMLButtonElement).disabled).toBe(false);
    });
  });

  it("freezes report and completion copy to the locale active when export starts", async () => {
    const pendingReport = deferred<string>();
    exportMocks.createReport.mockReturnValueOnce(pendingReport.promise);
    render(<App />);

    await screen.findByText("等待选择发射点");
    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("数据已就绪");
    fireEvent.click(screen.getByRole("button", { name: /开始计算/ }));
    await screen.findByText("覆盖计算完成");
    fireEvent.click(screen.getByRole("button", { name: /导出/ }));
    fireEvent.click(screen.getByRole("button", { name: /PNG 图像/ }));
    await waitFor(() => expect(exportMocks.createReport).toHaveBeenCalledOnce());

    await act(async () => {
      await i18n.changeLanguage("ja-JP");
      pendingReport.resolve("data:image/png;base64,iVBORw0KGgo=");
      await pendingReport.promise;
    });

    expect(await screen.findByText("已触发 PNG 下载 · 0.0 KB")).toBeTruthy();
    expect(exportMocks.createReport).toHaveBeenCalledWith(
      result,
      expect.objectContaining({ txGroundElevationOverrideM: null }),
      expect.any(Date),
      "zh-CN",
    );
    expect(backendMocks.exportReport).toHaveBeenCalledWith(
      expect.objectContaining({ format: "png", suggestedFileName: "coverage.png" }),
      "zh-CN",
    );
  });

  it("shows an incompatible calculation-result error without entering success", async () => {
    backendMocks.calculate.mockRejectedValueOnce(
      new Error("\u8ba1\u7b97\u7ed3\u679c\u534f\u8bae\u4e0d\u517c\u5bb9\uff1a\u9700\u8981 schemaVersion 4\u3002"),
    );
    render(<App />);

    await screen.findByText("\u7b49\u5f85\u9009\u62e9\u53d1\u5c04\u70b9");
    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("\u6570\u636e\u5df2\u5c31\u7eea");
    fireEvent.click(screen.getByRole("button", { name: /\u5f00\u59cb\u8ba1\u7b97/ }));

    expect(await screen.findByText("\u64cd\u4f5c\u672a\u5b8c\u6210")).toBeTruthy();
    expect(screen.getByText(/schemaVersion 4/)).toBeTruthy();
    expect(screen.getByTestId("heatmap").textContent).toBe("none");
    expect(screen.queryByText("\u8986\u76d6\u8ba1\u7b97\u5b8c\u6210")).toBeNull();
  });

  it("clears only the heatmap and keeps the selected ready point reusable", async () => {
    render(<App />);

    await screen.findByText("\u7b49\u5f85\u9009\u62e9\u53d1\u5c04\u70b9");
    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("\u6570\u636e\u5df2\u5c31\u7eea");
    expect(screen.getByTestId("selected-point").textContent).toBe("30.5,103.5");

    fireEvent.click(screen.getByRole("button", { name: "set-manual-override" }));
    expect(screen.getByTestId("ground-elevation-override").textContent).toBe("800");

    fireEvent.click(screen.getByRole("button", { name: /\u5f00\u59cb\u8ba1\u7b97/ }));
    await screen.findByText("\u8986\u76d6\u8ba1\u7b97\u5b8c\u6210");
    expect(screen.getByTestId("heatmap").textContent).toBe("present");
    const visibilitySlider = screen.getByRole("slider", { name: "最弱可见场强" });
    expect((visibilitySlider as HTMLInputElement).disabled).toBe(false);
    fireEvent.change(visibilitySlider, { target: { value: "-120" } });
    expect(screen.getByText("显示 ≥ -120 dBm")).toBeTruthy();


    fireEvent.click(screen.getByRole("button", { name: "\u6e05\u7a7a" }));

    expect(screen.getByTestId("heatmap").textContent).toBe("none");
    expect(screen.getByText("显示 ≥ -120 dBm")).toBeTruthy();
    expect(screen.getByTestId("selected-point").textContent).toBe("30.5,103.5");
    expect(screen.getByTestId("ground-elevation-override").textContent).toBe("800");
    expect(screen.getByText("\u6570\u636e\u5df2\u5c31\u7eea")).toBeTruthy();
    expect(backendMocks.inspectPoint).toHaveBeenCalledTimes(1);
    expect(backendMocks.calculate).toHaveBeenCalledWith(
      expect.objectContaining({ txGroundElevationOverrideM: 800 }),
    );

    const calculateButton = screen.getByRole("button", { name: /\u5f00\u59cb\u8ba1\u7b97/ });
    expect((calculateButton as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(calculateButton);
    await waitFor(() => expect(backendMocks.calculate).toHaveBeenCalledTimes(2));
  });

  it("preserves transmitter, manual parameters, completed heatmaps, and threshold across language changes", async () => {
    render(<App />);

    await screen.findByText("等待选择发射点");
    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("数据已就绪");
    fireEvent.click(screen.getByRole("button", { name: "set-manual-override" }));
    fireEvent.click(screen.getByRole("button", { name: /开始计算/ }));
    await screen.findByText("覆盖计算完成");

    const slider = screen.getByRole("slider", { name: "最弱可见场强" });
    fireEvent.change(slider, { target: { value: "-120" } });
    fireEvent.change(screen.getByRole("combobox", { name: "语言" }), {
      target: { value: "ja-JP" },
    });
    await screen.findByRole("combobox", { name: "言語" });

    expect(screen.getByTestId("selected-point").textContent).toBe("30.5,103.5");
    expect(screen.getByTestId("ground-elevation-override").textContent).toBe("800");
    expect(screen.getByTestId("heatmap-count").textContent).toBe("1");
    expect((screen.getByRole("slider", { name: "表示する最低受信電力" }) as HTMLInputElement).value).toBe("-120");
  });

  it("keeps different transmitter results until clear is clicked", async () => {
    backendMocks.calculate
      .mockResolvedValueOnce(result)
      .mockResolvedValueOnce({ ...result, center: { lat: 31, lon: 104 } });
    render(<App />);

    await screen.findByText("等待选择发射点");
    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("数据已就绪");
    fireEvent.click(screen.getByRole("button", { name: /开始计算/ }));
    await screen.findByText("覆盖计算完成");
    expect(screen.getByTestId("heatmap-count").textContent).toBe("1");

    fireEvent.click(screen.getByRole("button", { name: "select-new-point" }));
    await waitFor(() => expect(backendMocks.inspectPoint).toHaveBeenCalledTimes(2));
    expect(screen.getByTestId("heatmap-count").textContent).toBe("1");
    await screen.findByText("数据已就绪");
    fireEvent.click(screen.getByRole("button", { name: /开始计算/ }));
    await waitFor(() =>
      expect(screen.getByTestId("heatmap-count").textContent).toBe("2"),
    );

    fireEvent.click(screen.getByRole("button", { name: "清空" }));
    expect(screen.getByTestId("heatmap-count").textContent).toBe("0");
    expect(screen.getByTestId("selected-point").textContent).toBe("31,104");
  });

  it("marks a result stale after an override change and resets the override for a new point", async () => {
    render(<App />);

    await screen.findByText("\u7b49\u5f85\u9009\u62e9\u53d1\u5c04\u70b9");
    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("\u6570\u636e\u5df2\u5c31\u7eea");
    expect(screen.getByTestId("ground-elevation-override").textContent).toBe("automatic");

    fireEvent.click(screen.getByRole("button", { name: "set-manual-override" }));
    fireEvent.click(screen.getByRole("button", { name: /\u5f00\u59cb\u8ba1\u7b97/ }));
    await screen.findByText("\u8986\u76d6\u8ba1\u7b97\u5b8c\u6210");
    expect(backendMocks.calculate).toHaveBeenCalledWith(
      expect.objectContaining({ txGroundElevationOverrideM: 800 }),
    );

    fireEvent.click(screen.getByRole("button", { name: "change-manual-override" }));
    expect(screen.getByText("\u53c2\u6570\u5df2\u53d8\u5316\uff0c\u7ed3\u679c\u5df2\u8fc7\u671f")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "select-new-point" }));
    await waitFor(() => expect(backendMocks.inspectPoint).toHaveBeenCalledTimes(2));
    await screen.findByText("\u6570\u636e\u5df2\u5c31\u7eea");
    expect(screen.getByTestId("ground-elevation-override").textContent).toBe("automatic");
    expect(screen.getByTestId("heatmap").textContent).toBe("present");
  });

  it("keeps an older map result when a recalculation is cancelled and allows a clean retry", async () => {
    backendMocks.mode = "tauri";
    let rejectCalculation: (reason?: unknown) => void = () => undefined;
    backendMocks.calculate
      .mockResolvedValueOnce(result)
      .mockImplementationOnce(
        () =>
          new Promise<CalculationResult>((_resolve, reject) => {
            rejectCalculation = reject;
          }),
      );
    backendMocks.cancelCalculation.mockImplementationOnce(async () => {
      rejectCalculation({ code: "operation.cancelled" });
    });

    render(<App />);

    await screen.findByText("\u7b49\u5f85\u9009\u62e9\u53d1\u5c04\u70b9");
    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("\u6570\u636e\u5df2\u5c31\u7eea");

    fireEvent.click(screen.getByRole("button", { name: /\u5f00\u59cb\u8ba1\u7b97/ }));
    await screen.findByText("\u8986\u76d6\u8ba1\u7b97\u5b8c\u6210");
    expect(backendMocks.calculate).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("heatmap").textContent).toBe("present");
    expect(
      (screen.getByRole("button", { name: /\u5bfc\u51fa/ }) as HTMLButtonElement).disabled,
    ).toBe(false);

    fireEvent.click(screen.getByRole("button", { name: /\u5f00\u59cb\u8ba1\u7b97/ }));
    const cancelButton = await screen.findByRole("button", {
      name: "\u53d6\u6d88\u8ba1\u7b97",
    });
    fireEvent.click(cancelButton);

    await screen.findByText("\u8ba1\u7b97\u5df2\u53d6\u6d88");
    expect(backendMocks.cancelCalculation).toHaveBeenCalledTimes(1);
    expect(backendMocks.calculate).toHaveBeenCalledTimes(2);
    expect(screen.getByTestId("heatmap").textContent).toBe("present");
    expect(
      (screen.getByRole("button", { name: /\u5bfc\u51fa/ }) as HTMLButtonElement).disabled,
    ).toBe(true);

    const retryButton = screen.getByRole("button", { name: /\u5f00\u59cb\u8ba1\u7b97/ });
    expect((retryButton as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(retryButton);

    await screen.findByText("\u8986\u76d6\u8ba1\u7b97\u5b8c\u6210");
    expect(backendMocks.calculate).toHaveBeenCalledTimes(3);
    expect(screen.getByTestId("heatmap").textContent).toBe("present");
    expect(
      (screen.getByRole("button", { name: /\u5bfc\u51fa/ }) as HTMLButtonElement).disabled,
    ).toBe(false);
  });

  it("blocks a new calculation until a delayed cancellation request settles", async () => {
    backendMocks.mode = "tauri";
    let finishCalculation: (value: CalculationResult) => void = () => undefined;
    let finishCancellation: () => void = () => undefined;
    backendMocks.calculate.mockImplementationOnce(
      () =>
        new Promise<CalculationResult>((resolve) => {
          finishCalculation = resolve;
        }),
    );
    backendMocks.cancelCalculation.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          finishCancellation = resolve;
        }),
    );

    render(<App />);

    await screen.findByText("\u7b49\u5f85\u9009\u62e9\u53d1\u5c04\u70b9");
    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("\u6570\u636e\u5df2\u5c31\u7eea");

    fireEvent.click(screen.getByRole("button", { name: /\u5f00\u59cb\u8ba1\u7b97/ }));
    fireEvent.click(await screen.findByRole("button", { name: "\u53d6\u6d88\u8ba1\u7b97" }));
    expect(backendMocks.cancelCalculation).toHaveBeenCalledTimes(1);

    finishCalculation(result);
    await screen.findByText("\u8986\u76d6\u8ba1\u7b97\u5b8c\u6210");

    const blockedButton = screen.getByRole("button", { name: /\u5f00\u59cb\u8ba1\u7b97/ });
    expect((blockedButton as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(blockedButton);
    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    expect(backendMocks.calculate).toHaveBeenCalledTimes(1);
    expect(backendMocks.inspectPoint).toHaveBeenCalledTimes(1);
    expect(
      (screen.getByRole("button", { name: /\u5bfc\u51fa/ }) as HTMLButtonElement).disabled,
    ).toBe(true);

    finishCancellation();
    await waitFor(() => expect((blockedButton as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(blockedButton);
    await waitFor(() => expect(backendMocks.calculate).toHaveBeenCalledTimes(2));
  });

  it("identifies the trusted same-origin TianDiTu basemap", async () => {
    backendMocks.bootstrap.mockResolvedValueOnce({
      schemaVersion: 2,
      modelName: "NTIA ITM Point-to-Point",
      modelVersion: "land-water-v1",
      coverageRadiusKm: 200,
      gridSize: 401,
      cacheUsage,
      internalBuildWarning: "internal",
      basemap: tiandituBasemap,
    });

    render(<App />);

    expect(
      await screen.findByText(
        "已接入天地图在线矢量、中文地名及卫星影像；网络不可用时自动回退 WGS84 网格。",
      ),
    ).toBeTruthy();
  });

  it("keeps DEM and WBM cache accounting without an offline basemap row", async () => {
    backendMocks.bootstrap.mockResolvedValueOnce({
      schemaVersion: 2,
      modelName: "NTIA ITM Point-to-Point",
      modelVersion: "land-water-v1",
      coverageRadiusKm: 200,
      gridSize: 401,
      cacheUsage,
      internalBuildWarning: "internal",
      basemap: tiandituBasemap,
    });
    backendMocks.cacheOverview.mockResolvedValueOnce({
      usage: cacheUsage,
      regions: [],
    });

    render(<App />);
    await screen.findByText("等待选择发射点");
    fireEvent.click(screen.getByRole("button", { name: /缓存/ }));
    await screen.findByText("缓存概览");

    expect(screen.queryByText("离线底图")).toBeNull();
    expect(screen.getByText("高程 DEM")).toBeTruthy();
    expect(screen.getByText("水体 WBM")).toBeTruthy();
    const metadataRow = screen.getByText("索引与元数据").parentElement;
    expect(metadataRow?.querySelector("strong")?.textContent).toBe("1.0 MB");
    expect(screen.getByText("120.0 MB")).toBeTruthy();
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

const preview: CalculationPreview = {
  schemaVersion: 1,
  sequence: 1,
  completedPixelCount: 12_563,
  totalPixelCount: 125_628,
  mapOverlayProjection: "EPSG:3857",
  mapOverlayWidth: 401,
  mapOverlayHeight: 401,
  mapOverlayCorners: [[0, 0], [1, 0], [1, 1], [0, 1]],
  mapOverlayPngDataUrl: "data:image/png;base64,iVBORw0KGgo=",
};

describe("progressive calculation preview UI", () => {
  it("shows a preview, keeps it non-exportable, then atomically replaces it with the final result", async () => {
    backendMocks.mode = "validation-server";
    let resolveCalculation: (value: CalculationResult) => void = () => undefined;
    backendMocks.calculate.mockImplementationOnce(
      () =>
        new Promise<CalculationResult>((resolve) => {
          resolveCalculation = resolve;
        }),
    );

    render(<App />);
    await screen.findByText("\u7b49\u5f85\u9009\u62e9\u53d1\u5c04\u70b9");
    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("\u6570\u636e\u5df2\u5c31\u7eea");
    fireEvent.click(screen.getByRole("button", { name: /\u5f00\u59cb\u8ba1\u7b97/ }));

    act(() => backendMocks.previewHandler?.(preview));
    expect((await screen.findByTestId("preview")).textContent).toBe("1");
    expect(screen.getByTestId("heatmap").textContent).toBe("none");
    expect(
      (screen.getByRole("button", { name: /\u5bfc\u51fa/ }) as HTMLButtonElement).disabled,
    ).toBe(true);

    act(() => resolveCalculation(result));
    await screen.findByText("\u8986\u76d6\u8ba1\u7b97\u5b8c\u6210");
    expect(screen.getByTestId("preview").textContent).toBe("none");
    expect(screen.getByTestId("heatmap").textContent).toBe("present");
    expect(
      (screen.getByRole("button", { name: /\u5bfc\u51fa/ }) as HTMLButtonElement).disabled,
    ).toBe(false);

    fireEvent.click(screen.getByRole("button", { name: "\u6e05\u7a7a" }));
    expect(screen.getByTestId("preview").textContent).toBe("none");
    expect(screen.getByTestId("heatmap").textContent).toBe("none");

    act(() =>
      backendMocks.previewHandler?.({
        ...preview,
        sequence: 2,
        completedPixelCount: 25_126,
      }),
    );
    expect(screen.getByTestId("preview").textContent).toBe("none");
    expect(screen.getByTestId("heatmap").textContent).toBe("none");
  });

  it("suppresses a late preview after cancellation is requested", async () => {
    backendMocks.mode = "validation-server";
    let rejectCalculation: (reason?: unknown) => void = () => undefined;
    let resolveCancellation: () => void = () => undefined;
    backendMocks.calculate.mockImplementationOnce(
      () =>
        new Promise<CalculationResult>((_resolve, reject) => {
          rejectCalculation = reject;
        }),
    );
    backendMocks.cancelCalculation.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          resolveCancellation = resolve;
        }),
    );

    render(<App />);
    await screen.findByText("\u7b49\u5f85\u9009\u62e9\u53d1\u5c04\u70b9");
    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("\u6570\u636e\u5df2\u5c31\u7eea");
    fireEvent.click(screen.getByRole("button", { name: /\u5f00\u59cb\u8ba1\u7b97/ }));

    act(() => backendMocks.previewHandler?.(preview));
    expect((await screen.findByTestId("preview")).textContent).toBe("1");
    fireEvent.click(await screen.findByRole("button", { name: "\u53d6\u6d88\u8ba1\u7b97" }));
    expect(screen.getByTestId("preview").textContent).toBe("none");

    act(() =>
      backendMocks.previewHandler?.({
        ...preview,
        sequence: 2,
        completedPixelCount: 25_126,
      }),
    );
    expect(screen.getByTestId("preview").textContent).toBe("none");

    await act(async () => {
      rejectCalculation(new Error("calculation cancelled"));
      resolveCancellation();
      await Promise.resolve();
    });
    await screen.findByText("\u8ba1\u7b97\u5df2\u53d6\u6d88");
    expect(screen.getByTestId("preview").textContent).toBe("none");
  });
});


describe("cache deletion inspection refresh", () => {
  async function startDeletingOnlyRegion() {
    backendMocks.cacheOverview.mockResolvedValueOnce({
      usage: cacheUsage,
      regions: [cacheRegion],
    });
    fireEvent.click(screen.getByRole("button", { name: /缓存/ }));
    await screen.findByText("缓存概览");
    fireEvent.click(screen.getByRole("button", { name: "删除" }));
    await screen.findByText("删除该离线区域？");
    fireEvent.click(screen.getByRole("button", { name: "确认删除" }));
    await waitFor(() =>
      expect(backendMocks.deleteCacheRegion).toHaveBeenCalledWith("region"),
    );
  }

  it("reuses one post-delete inspection when coverage and link TX are the same point", async () => {
    render(<App />);
    await screen.findByText("等待选择发射点");

    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("数据已就绪");
    fireEvent.click(screen.getByRole("tab", { name: "链路通视分析" }));
    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("选择接收台位置");
    fireEvent.click(screen.getByRole("button", { name: "select-new-point" }));
    await screen.findByText("链路端点已就绪");
    await waitFor(() => expect(backendMocks.inspectPoint).toHaveBeenCalledTimes(2));

    backendMocks.inspectPoint.mockResolvedValueOnce(
      missingInspection({ lat: 30.5, lon: 103.5 }),
    );
    await startDeletingOnlyRegion();

    await screen.findByText("当前区域缺少离线计算数据");
    expect(backendMocks.inspectPoint).toHaveBeenCalledTimes(3);
    expect(backendMocks.inspectPoint).toHaveBeenLastCalledWith({
      lat: 30.5,
      lon: 103.5,
    });
    expect(
      screen.getByRole("button", { name: /准备离线数据/ }),
    ).toBeTruthy();

    fireEvent.click(
      screen.getByRole("button", { name: "关闭缓存概览" }),
    );
    fireEvent.click(screen.getByRole("tab", { name: "覆盖范围分析" }));
    expect(screen.getByText("当前区域缺少离线计算数据")).toBeTruthy();
    expect(
      screen.getByRole("button", { name: /准备离线数据/ }),
    ).toBeTruthy();
  });

  it("serially reinspects different coverage and link TX points after deletion", async () => {
    render(<App />);
    await screen.findByText("等待选择发射点");

    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("数据已就绪");
    fireEvent.click(screen.getByRole("tab", { name: "链路通视分析" }));
    fireEvent.click(screen.getByRole("button", { name: "select-new-point" }));
    await screen.findByText("选择接收台位置");
    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("链路端点已就绪");
    await waitFor(() => expect(backendMocks.inspectPoint).toHaveBeenCalledTimes(2));

    const coverageRefresh = deferred<ReturnType<typeof missingInspection>>();
    backendMocks.inspectPoint
      .mockImplementationOnce(() => coverageRefresh.promise)
      .mockResolvedValueOnce(missingInspection({ lat: 31, lon: 104 }));

    await startDeletingOnlyRegion();
    await waitFor(() => expect(backendMocks.inspectPoint).toHaveBeenCalledTimes(3));
    expect(backendMocks.inspectPoint).toHaveBeenNthCalledWith(3, {
      lat: 30.5,
      lon: 103.5,
    });
    expect(backendMocks.inspectPoint).not.toHaveBeenNthCalledWith(4, {
      lat: 31,
      lon: 104,
    });

    await act(async () => {
      coverageRefresh.resolve(
        missingInspection({ lat: 30.5, lon: 103.5 }),
      );
      await coverageRefresh.promise;
    });
    await waitFor(() => expect(backendMocks.inspectPoint).toHaveBeenCalledTimes(4));
    expect(backendMocks.inspectPoint).toHaveBeenNthCalledWith(4, {
      lat: 31,
      lon: 104,
    });
    await screen.findByText("当前区域缺少离线计算数据");

    fireEvent.click(
      screen.getByRole("button", { name: "关闭缓存概览" }),
    );
    fireEvent.click(screen.getByRole("tab", { name: "覆盖范围分析" }));
    expect(screen.getByText("当前区域缺少离线计算数据")).toBeTruthy();
  });
});

describe("link analysis workspace", () => {
  it("accepts exact 1 km and 200 km WGS84 endpoint boundaries", async () => {
    render(<App />);
    await screen.findByText("等待选择发射点");
    fireEvent.click(screen.getByRole("tab", { name: "链路通视分析" }));
    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("选择接收台位置");

    mapPointMocks.extraPoint = directWgs84(
      { lat: 30.5, lon: 103.5 },
      73,
      1_000,
    );
    fireEvent.click(screen.getByRole("button", { name: "select-extra-point" }));
    const analyzeButton = screen.getByRole("button", { name: /分析链路/ });
    await waitFor(() =>
      expect((analyzeButton as HTMLButtonElement).disabled).toBe(false),
    );

    fireEvent.click(screen.getByRole("button", { name: "重选接收台" }));
    mapPointMocks.extraPoint = directWgs84(
      { lat: 30.5, lon: 103.5 },
      73,
      200_000,
    );
    fireEvent.click(screen.getByRole("button", { name: "select-extra-point" }));
    await waitFor(() =>
      expect((analyzeButton as HTMLButtonElement).disabled).toBe(false),
    );
    expect(screen.queryByText(/链路分析仅支持 1–200 km/)).toBeNull();
  });

  it("selects TX then RX, preserves heatmaps across modes, and clears only the link", async () => {
    render(<App />);
    await screen.findByText("等待选择发射点");

    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    await screen.findByText("数据已就绪");
    fireEvent.click(screen.getByRole("button", { name: /开始计算/ }));
    await screen.findByText("覆盖计算完成");
    expect(screen.getByTestId("heatmap-count").textContent).toBe("1");

    fireEvent.click(screen.getByRole("tab", { name: "链路通视分析" }));
    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    expect(screen.getByTestId("link-tx").textContent).toBe("30.5,103.5");
    expect(screen.getByTestId("link-rx").textContent).toBe("none");
    fireEvent.click(screen.getByRole("button", { name: "select-new-point" }));
    expect(screen.getByTestId("link-rx").textContent).toBe("31,104");

    const analyzeButton = screen.getByRole("button", { name: /分析链路/ });
    await waitFor(() => expect((analyzeButton as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(analyzeButton);
    await screen.findByText("视距良好 · 模型预测可用");
    expect(backendMocks.analyzeLink).toHaveBeenCalledWith(
      expect.objectContaining({
        tx: expect.objectContaining({ point: { lat: 30.5, lon: 103.5 } }),
        rx: expect.objectContaining({ point: { lat: 31, lon: 104 } }),
        receiverThresholdDbm: -120,
      }),
    );
    expect(screen.getByTestId("link-profile").textContent).toBe("direct-los");

    fireEvent.click(screen.getByRole("button", { name: "清空链路" }));
    expect(screen.getByTestId("link-tx").textContent).toBe("none");
    expect(screen.getByTestId("link-rx").textContent).toBe("none");
    expect(screen.getByTestId("heatmap-count").textContent).toBe("1");

    fireEvent.click(screen.getByRole("tab", { name: "覆盖范围分析" }));
    expect(screen.getByTestId("heatmap-count").textContent).toBe("1");
    expect(screen.getByTestId("selected-point").textContent).toBe("30.5,103.5");
  });

  it("preserves both endpoints, parameters, and result across language changes", async () => {
    render(<App />);
    await screen.findByText("等待选择发射点");
    fireEvent.click(screen.getByRole("tab", { name: "链路通视分析" }));
    fireEvent.click(screen.getByRole("button", { name: "select-point" }));
    fireEvent.click(screen.getByRole("button", { name: "select-new-point" }));
    fireEvent.click(screen.getByRole("button", { name: "change-link-threshold" }));
    const analyzeButton = screen.getByRole("button", { name: /分析链路/ });
    await waitFor(() => expect((analyzeButton as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(analyzeButton);
    await screen.findByText("视距良好 · 模型预测可用");

    fireEvent.change(screen.getByRole("combobox", { name: "语言" }), {
      target: { value: "ja-JP" },
    });
    await screen.findByRole("tab", { name: "見通しリンク解析" });
    expect(screen.getByTestId("link-tx").textContent).toBe("30.5,103.5");
    expect(screen.getByTestId("link-rx").textContent).toBe("31,104");
    expect(screen.getByTestId("link-threshold").textContent).toBe("-110");
    expect(screen.getByTestId("link-result").textContent).toBe("direct-los");
  });
});
