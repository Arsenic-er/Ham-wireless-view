import { afterEach, describe, expect, it, vi } from "vitest";
import { Channel } from "@tauri-apps/api/core";
import { mockIPC } from "@tauri-apps/api/mocks";

import {
  backendCapabilities,
  backendMode,
  bootstrap,
  calculate,
  cancelCalculation,
  cancelDownload,
  clearOnlineBasemap,
  configureOnlineBasemap,
  getOnlineBasemap,
  downloadRegion,
  estimateDownload,
  exportReport,
  inspectPoint,
  listenCalculationPreview,
  listenCalculationProgress,
  listenDownloadProgress,
  probeOnlineBasemap,
} from "./backend";
import type {
  CalculationPreview,
  CalculationRequest,
  CalculationResult,
  DownloadResult,
} from "./types";

function removeTauriInternals(): void {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
}

afterEach(() => {
  removeTauriInternals();
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("backend mode", () => {
  it("keeps an ordinary browser in interface-only preview mode", () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "0");

    expect(backendMode()).toBe("preview");
    expect(backendCapabilities()).toEqual({
      mode: "preview",
      canDownload: false,
      canDeleteCache: false,
      canCalculate: false,
      canExport: false,
      canConfigureOnlineBasemap: false,
    });
  });

  it("enables server operations and browser-local report export", () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");

    expect(backendCapabilities()).toEqual({
      mode: "validation-server",
      canDownload: true,
      canDeleteCache: true,
      canCalculate: true,
      canExport: true,
      canConfigureOnlineBasemap: false,
    });
  });

  it("gives Tauri precedence over the validation build flag", () => {
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });

    expect(backendMode()).toBe("tauri");
    expect(backendCapabilities().canExport).toBe(true);
    expect(backendCapabilities().canConfigureOnlineBasemap).toBe(true);
  });

  it("merges desktop online metadata and uses dedicated configure and clear commands", async () => {
    const onlineBasemap = {
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
    const desktopBootstrap = {
      schemaVersion: 2,
      modelName: "model",
      modelVersion: "version",
      coverageRadiusKm: 200,
      gridSize: 401,
      cacheUsage: {
        totalBytes: 0,
        demBytes: 0,
        waterBytes: 0,
        partialBytes: 0,
        metadataBytes: 0,
        remainingBytes: 2_500_000_000,
        capBytes: 2_500_000_000,
      },
      internalBuildWarning: "internal",
    };
    const invokeMock = vi.fn((command: string) => {
      if (command === "bootstrap") return Promise.resolve(desktopBootstrap);
      if (command === "get_online_basemap") return Promise.resolve(onlineBasemap);
      if (command === "configure_online_basemap") return Promise.resolve(onlineBasemap);
      if (command === "clear_online_basemap") {
        return Promise.resolve({ ...onlineBasemap, configured: false });
      }
      if (command === "probe_online_basemap") {
        return Promise.resolve({ schemaVersion: 1, status: "reachable" });
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: { invoke: invokeMock },
    });

    await expect(bootstrap()).resolves.toEqual({
      ...desktopBootstrap,
      onlineBasemap,
    });
    await expect(getOnlineBasemap()).resolves.toEqual(onlineBasemap);
    await expect(configureOnlineBasemap("  secret-token  ")).resolves.toEqual(
      onlineBasemap,
    );
    await expect(clearOnlineBasemap()).resolves.toEqual({
      ...onlineBasemap,
      configured: false,
    });
    await expect(probeOnlineBasemap()).resolves.toEqual({
      schemaVersion: 1,
      status: "reachable",
    });

    expect(invokeMock).toHaveBeenCalledWith(
      "configure_online_basemap",
      { token: "secret-token" },
      undefined,
    );
    expect(invokeMock.mock.calls.flatMap(([command]) => [command])).toEqual([
      "bootstrap",
      "get_online_basemap",
      "get_online_basemap",
      "configure_online_basemap",
      "clear_online_basemap",
      "probe_online_basemap",
    ]);
  });

  it("rejects probing outside Tauri with a stable localized error", async () => {
    removeTauriInternals();

    await expect(probeOnlineBasemap()).rejects.toThrow(
      "在线地图连接测试只在 Tauri Windows 桌面应用中可用。",
    );
  });

  it("rejects an incompatible probe result without exposing backend text", async () => {
    const invokeMock = vi.fn().mockResolvedValue({
      schemaVersion: 1,
      status: "unexpected",
      message: "sensitive backend detail",
    });
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: { invoke: invokeMock },
    });

    await expect(probeOnlineBasemap()).rejects.toThrow(
      "在线地图连接测试返回了不兼容的结果。",
    );
  });

  it("falls back to grid metadata when desktop online metadata loading fails", async () => {
    const invokeMock = vi.fn((command: string) => {
      if (command === "bootstrap") {
        return Promise.resolve({
          schemaVersion: 2,
          modelName: "model",
          modelVersion: "version",
          coverageRadiusKm: 200,
          gridSize: 401,
          cacheUsage: {
            totalBytes: 0,
            demBytes: 0,
            waterBytes: 0,
            partialBytes: 0,
            metadataBytes: 0,
            remainingBytes: 2_500_000_000,
            capBytes: 2_500_000_000,
          },
          internalBuildWarning: "internal",
          basemap: { enabled: true },
        });
      }
      return Promise.reject(new Error("metadata unavailable"));
    });
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: { invoke: invokeMock },
    });

    await expect(bootstrap()).resolves.toEqual(
      expect.objectContaining({ basemap: undefined, onlineBasemap: undefined }),
    );
  });
});

describe("validation server adapter", () => {
  it("uses same-origin JSON endpoints and wrapped Tauri-shaped request bodies", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            schemaVersion: 2,
            modelName: "NTIA ITM Point-to-Point",
            modelVersion: "land-water-v1",
            coverageRadiusKm: 200,
            gridSize: 401,
            cacheUsage: {
              totalBytes: 0,
              demBytes: 0,
              waterBytes: 0,
              partialBytes: 0,
              metadataBytes: 0,
              remainingBytes: 2_500_000_000,
              capBytes: 2_500_000_000,
            },
            internalBuildWarning: "internal",
          }),
          { headers: { "content-type": "application/json" } },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            point: { lat: 30.5, lon: 103.5 },
            regionId: "region",
            tileCount: 25,
            readyDemCount: 25,
            readyWaterCount: 25,
            missingAssetCount: 0,
            dataReady: true,
            elevationM: 512,
            cacheUsage: {
              totalBytes: 1,
              demBytes: 1,
              waterBytes: 0,
              partialBytes: 0,
              metadataBytes: 0,
              remainingBytes: 2_499_999_999,
              capBytes: 2_500_000_000,
            },
          }),
          { headers: { "content-type": "application/json" } },
        ),
      );
    vi.stubGlobal("fetch", fetchMock);

    await bootstrap();
    await inspectPoint({ lat: 30.5, lon: 103.5 });
    await cancelCalculation();

    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      "/api/bootstrap",
      expect.objectContaining({ method: "GET" }),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "/api/inspect-point",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ point: { lat: 30.5, lon: 103.5 } }),
      }),
    );
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("surfaces JSON API errors and validates browser export input", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(
      new Response(JSON.stringify({ message: "busy" }), {
        status: 409,
        headers: { "content-type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    await expect(bootstrap()).rejects.toThrow("busy");
    await expect(
      exportReport({ format: "png", suggestedFileName: "x.png", reportPngDataUrl: "data:" }),
    ).rejects.toThrow("图像格式");
    expect(fetchMock).toHaveBeenCalledOnce();
  });
});

const OPERATION_ID_1 = "11111111-1111-4111-8111-111111111111";
const OPERATION_ID_2 = "22222222-2222-4222-8222-222222222222";

const CALCULATION_REQUEST: CalculationRequest = {
  center: { lat: 30.5, lon: 103.5 },
  band: "vhf-144",
  frequencyMhz: 145.5,
  powerValue: 5,
  powerUnit: "watt",
  txGainValue: 2,
  txGainUnit: "dbi",
  txHeightM: 10,
  txGroundElevationOverrideM: null,
  rxGainValue: 2,
  rxGainUnit: "dbi",
  rxHeightM: 1.5,
  polarization: "vertical",
};

const CALCULATION_RESULT: CalculationResult = {
  schemaVersion: 4,
  modelName: "model",
  modelVersion: "version",
  center: CALCULATION_REQUEST.center,
  txGroundElevationM: 512,
  txGroundElevationSource: "dem",
  imageWidth: 1,
  imageHeight: 1,
  imageCorners: [
    [103, 31],
    [104, 31],
    [104, 30],
    [103, 30],
  ],
  heatmapPngDataUrl: "data:image/png;base64,AA==",
  mapOverlayProjection: "EPSG:3857",
  mapOverlayWidth: 401,
  mapOverlayHeight: 401,
  mapOverlayCorners: [
    [103, 31],
    [104, 31],
    [104, 30],
    [103, 30],
  ],
  mapOverlayPngDataUrl: "data:image/png;base64,AA==",
  mapOverlayFilterEncoding: "u8-dbm-floor-v1",
  mapOverlayFilterBase64: btoa("\x51".repeat(401 * 401)),
  statistics: {
    validPixelCount: 1,
    belowThresholdPixelCount: 0,
    warningPixelCount: 0,
    minimumDbm: -80,
    maximumDbm: -80,
    meanDbm: -80,
    waterAffectedPixelCount: 0,
    meanPathWaterFraction: 0,
    propagationSeconds: 1,
    totalSeconds: 1,
  },
};

const DOWNLOAD_RESULT: DownloadResult = {
  inspection: {
    point: { lat: 30.5, lon: 103.5 },
    regionId: "region",
    tileCount: 1,
    readyDemCount: 1,
    readyWaterCount: 1,
    missingAssetCount: 0,
    dataReady: true,
    elevationM: 500,
    cacheUsage: {
      totalBytes: 2,
      demBytes: 1,
      waterBytes: 1,
      partialBytes: 0,
      metadataBytes: 0,
      remainingBytes: 2_499_999_998,
      capBytes: 2_500_000_000,
    },
  },
  preparedAssetCount: 2,
  downloadedBytes: 2,
};

function jsonResponse(value: unknown, status = 200): Response {
  return new Response(JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason?: unknown) => void;
} {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

async function flushMicrotasks(): Promise<void> {
  for (let index = 0; index < 8; index += 1) await Promise.resolve();
}

function callBody(fetchMock: ReturnType<typeof vi.fn<typeof fetch>>, path: string): unknown {
  const call = fetchMock.mock.calls.find(([input]) => String(input) === path);
  if (!call || typeof call[1]?.body !== "string") return null;
  return JSON.parse(call[1].body) as unknown;
}

describe("validation operation protocol", () => {
  it("uses ticket, primary request, polling progress, final status, and ack", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    vi.useFakeTimers();
    const primary = deferred<Response>();
    let statusCalls = 0;
    const fetchMock = vi.fn<typeof fetch>().mockImplementation((input) => {
      const path = String(input);
      if (path === "/api/operation-ticket") {
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: "reserved",
          }),
        );
      }
      if (path === "/api/calculate") return primary.promise;
      if (path === "/api/operation-status") {
        statusCalls += 1;
        return Promise.resolve(
          jsonResponse(
            statusCalls === 1
              ? {
                  schemaVersion: 1,
                  operationId: OPERATION_ID_1,
                  kind: "calculation",
                  state: "running",
                  sequence: 1,
                  progress: {
                    type: "calculation",
                    phase: "computing",
                    percent: 40,
                    completedPixelCount: 40,
                    totalPixelCount: 100,
                  },
                }
              : {
                  schemaVersion: 1,
                  operationId: OPERATION_ID_1,
                  kind: "calculation",
                  state: "succeeded",
                  sequence: 2,
                  progress: {
                    type: "calculation",
                    phase: "complete",
                    percent: 100,
                    completedPixelCount: 100,
                    totalPixelCount: 100,
                  },
                },
          ),
        );
      }
      if (path === "/api/operation-ack") {
        return Promise.resolve(jsonResponse({ acknowledged: true }));
      }
      return Promise.reject(new Error(`unexpected request: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);
    const progressHandler = vi.fn();
    const unlisten = await listenCalculationProgress(progressHandler);

    const resultPromise = calculate(CALCULATION_REQUEST);
    await flushMicrotasks();
    expect(callBody(fetchMock, "/api/operation-ticket")).toEqual({
      kind: "calculation",
    });
    expect(callBody(fetchMock, "/api/calculate")).toEqual({
      operationId: OPERATION_ID_1,
      request: CALCULATION_REQUEST,
    });

    await vi.advanceTimersByTimeAsync(250);
    expect(progressHandler).toHaveBeenCalledWith({
      phase: "computing",
      percent: 40,
      completedPixelCount: 40,
      totalPixelCount: 100,
    });

    primary.resolve(jsonResponse(CALCULATION_RESULT));
    await expect(resultPromise).resolves.toEqual(CALCULATION_RESULT);
    expect(statusCalls).toBe(2);
    expect(callBody(fetchMock, "/api/operation-ack")).toEqual({
      operationId: OPERATION_ID_1,
    });
    expect(progressHandler).toHaveBeenLastCalledWith(
      expect.objectContaining({ phase: "complete", percent: 100 }),
    );
    expect(vi.getTimerCount()).toBe(0);
    unlisten();
  });

  it("rejects an incompatible validation-server result and still acknowledges cleanup", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    const fetchMock = vi.fn<typeof fetch>().mockImplementation((input) => {
      const path = String(input);
      if (path === "/api/operation-ticket") {
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: "reserved",
          }),
        );
      }
      if (path === "/api/calculate") {
        return Promise.resolve(
          jsonResponse({ ...CALCULATION_RESULT, schemaVersion: 3 }),
        );
      }
      if (path === "/api/operation-status") {
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: "succeeded",
            sequence: 1,
            progress: null,
          }),
        );
      }
      if (path === "/api/operation-ack") {
        return Promise.resolve(jsonResponse({ acknowledged: true }));
      }
      return Promise.reject(new Error(`unexpected request: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    await expect(calculate(CALCULATION_REQUEST)).rejects.toThrow("schemaVersion 4");
    expect(callBody(fetchMock, "/api/operation-ack")).toEqual({
      operationId: OPERATION_ID_1,
    });
  });

  it("accepts reserved snapshots and ignores repeated, wrong-id, and out-of-order progress", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    vi.useFakeTimers();
    const primary = deferred<Response>();
    const snapshots = [
      {
        schemaVersion: 1,
        operationId: OPERATION_ID_1,
        kind: "calculation",
        state: "reserved",
        sequence: 0,
        progress: null,
      },
      {
        schemaVersion: 1,
        operationId: OPERATION_ID_1,
        kind: "calculation",
        state: "running",
        sequence: 0,
        progress: {
          type: "calculation",
          phase: "computing",
          percent: 99,
          completedPixelCount: 99,
          totalPixelCount: 100,
        },
      },
      {
        schemaVersion: 1,
        operationId: OPERATION_ID_1,
        kind: "calculation",
        state: "running",
        sequence: 2,
        progress: {
          type: "calculation",
          phase: "computing",
          percent: 20,
          completedPixelCount: 20,
          totalPixelCount: 100,
        },
      },
      {
        schemaVersion: 1,
        operationId: OPERATION_ID_2,
        kind: "calculation",
        state: "running",
        sequence: 3,
        progress: {
          type: "calculation",
          phase: "computing",
          percent: 90,
          completedPixelCount: 90,
          totalPixelCount: 100,
        },
      },
      {
        schemaVersion: 1,
        operationId: OPERATION_ID_1,
        kind: "calculation",
        state: "running",
        sequence: 1,
        progress: {
          type: "calculation",
          phase: "computing",
          percent: 10,
          completedPixelCount: 10,
          totalPixelCount: 100,
        },
      },
    ];
    let statusIndex = 0;
    const fetchMock = vi.fn<typeof fetch>().mockImplementation((input) => {
      const path = String(input);
      if (path === "/api/operation-ticket") {
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: "reserved",
          }),
        );
      }
      if (path === "/api/calculate") return primary.promise;
      if (path === "/api/operation-status") {
        const snapshot =
          snapshots[statusIndex++] ?? {
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: "succeeded",
            sequence: 3,
            progress: null,
          };
        return Promise.resolve(jsonResponse(snapshot));
      }
      if (path === "/api/operation-ack") {
        return Promise.resolve(jsonResponse({ acknowledged: true }));
      }
      return Promise.reject(new Error(`unexpected request: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);
    const progressHandler = vi.fn();
    const unlisten = await listenCalculationProgress(progressHandler);

    const resultPromise = calculate(CALCULATION_REQUEST);
    await flushMicrotasks();
    for (let index = 0; index < snapshots.length; index += 1) {
      await vi.advanceTimersByTimeAsync(250);
    }
    expect(progressHandler).toHaveBeenCalledTimes(1);
    expect(progressHandler).toHaveBeenCalledWith(
      expect.objectContaining({ percent: 20 }),
    );

    primary.resolve(jsonResponse(CALCULATION_RESULT));
    await resultPromise;
    expect(vi.getTimerCount()).toBe(0);
    unlisten();
  });

  it("never overlaps polling requests", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    vi.useFakeTimers();
    const primary = deferred<Response>();
    const firstStatus = deferred<Response>();
    let statusCalls = 0;
    const fetchMock = vi.fn<typeof fetch>().mockImplementation((input) => {
      const path = String(input);
      if (path === "/api/operation-ticket") {
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: "reserved",
          }),
        );
      }
      if (path === "/api/calculate") return primary.promise;
      if (path === "/api/operation-status") {
        statusCalls += 1;
        if (statusCalls === 1) return firstStatus.promise;
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: "succeeded",
            sequence: 2,
            progress: null,
          }),
        );
      }
      if (path === "/api/operation-ack") {
        return Promise.resolve(jsonResponse({ acknowledged: true }));
      }
      return Promise.reject(new Error(`unexpected request: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    const resultPromise = calculate(CALCULATION_REQUEST);
    await flushMicrotasks();
    await vi.advanceTimersByTimeAsync(250);
    expect(statusCalls).toBe(1);
    await vi.advanceTimersByTimeAsync(1_000);
    expect(statusCalls).toBe(1);

    firstStatus.resolve(
      jsonResponse({
        schemaVersion: 1,
        operationId: OPERATION_ID_1,
        kind: "calculation",
        state: "running",
        sequence: 1,
        progress: null,
      }),
    );
    await flushMicrotasks();
    await vi.advanceTimersByTimeAsync(249);
    expect(statusCalls).toBe(1);

    primary.resolve(jsonResponse(CALCULATION_RESULT));
    await resultPromise;
    expect(statusCalls).toBe(2);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("stops polling after primary failure while preserving the primary error", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    vi.useFakeTimers();
    let statusCalls = 0;
    let ackCalls = 0;
    const fetchMock = vi.fn<typeof fetch>().mockImplementation((input) => {
      const path = String(input);
      if (path === "/api/operation-ticket") {
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: "reserved",
          }),
        );
      }
      if (path === "/api/calculate") {
        return Promise.resolve(jsonResponse({ message: "primary boom" }, 500));
      }
      if (path === "/api/operation-status") {
        statusCalls += 1;
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: "failed",
            sequence: 1,
            progress: null,
          }),
        );
      }
      if (path === "/api/operation-ack") {
        ackCalls += 1;
        return Promise.resolve(jsonResponse({ acknowledged: true }));
      }
      return Promise.reject(new Error(`unexpected request: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    await expect(calculate(CALCULATION_REQUEST)).rejects.toThrow("primary boom");
    await vi.advanceTimersByTimeAsync(1_000);
    expect(statusCalls).toBe(1);
    expect(ackCalls).toBe(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("maps download progress without exposing an asset key", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    vi.useFakeTimers();
    const primary = deferred<Response>();
    let statusCalls = 0;
    const fetchMock = vi.fn<typeof fetch>().mockImplementation((input) => {
      const path = String(input);
      if (path === "/api/operation-ticket") {
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "download",
            state: "reserved",
          }),
        );
      }
      if (path === "/api/download-region") return primary.promise;
      if (path === "/api/operation-status") {
        statusCalls += 1;
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "download",
            state: statusCalls === 1 ? "running" : "succeeded",
            sequence: statusCalls,
            progress:
              statusCalls === 1
                ? {
                    type: "download",
                    assetIndex: 1,
                    assetCount: 2,
                    assetDownloadedBytes: 10,
                    assetExpectedBytes: 20,
                    totalDownloadedBytes: 10,
                    totalExpectedBytes: 40,
                    percent: 25,
                  }
                : null,
          }),
        );
      }
      if (path === "/api/operation-ack") {
        return Promise.resolve(jsonResponse({ acknowledged: true }));
      }
      return Promise.reject(new Error(`unexpected request: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);
    const progressHandler = vi.fn();
    const unlisten = await listenDownloadProgress(progressHandler);

    const resultPromise = downloadRegion({ lat: 30.5, lon: 103.5 });
    await flushMicrotasks();
    await vi.advanceTimersByTimeAsync(250);
    expect(progressHandler).toHaveBeenCalledWith({
      assetIndex: 1,
      assetCount: 2,
      assetKey: "",
      assetDownloadedBytes: 10,
      assetExpectedBytes: 20,
      totalDownloadedBytes: 10,
      totalExpectedBytes: 40,
      percent: 25,
    });

    primary.resolve(jsonResponse(DOWNLOAD_RESULT));
    await expect(resultPromise).resolves.toEqual(DOWNLOAD_RESULT);
    expect(callBody(fetchMock, "/api/download-region")).toEqual({
      operationId: OPERATION_ID_1,
      point: { lat: 30.5, lon: 103.5 },
    });
    unlisten();
  });

  it("uses a separate estimate ticket and does not let ack failure replace the result", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    const estimate = { point: { lat: 30.5, lon: 103.5 }, regionId: "region" };
    const fetchMock = vi.fn<typeof fetch>().mockImplementation((input) => {
      const path = String(input);
      if (path === "/api/operation-ticket") {
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "estimate-download",
            state: "reserved",
          }),
        );
      }
      if (path === "/api/estimate-download") return Promise.resolve(jsonResponse(estimate));
      if (path === "/api/operation-status") {
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "estimate-download",
            state: "succeeded",
            sequence: 1,
            progress: { type: "estimate-download", stage: "estimating" },
          }),
        );
      }
      if (path === "/api/operation-ack") {
        return Promise.resolve(jsonResponse({ message: "ack lost" }, 503));
      }
      return Promise.reject(new Error(`unexpected request: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    await expect(estimateDownload({ lat: 30.5, lon: 103.5 })).resolves.toEqual(estimate);
    expect(callBody(fetchMock, "/api/operation-ticket")).toEqual({
      kind: "estimate-download",
    });
    expect(callBody(fetchMock, "/api/estimate-download")).toEqual({
      operationId: OPERATION_ID_1,
      point: { lat: 30.5, lon: 103.5 },
    });
  });

  it("keeps a delayed old cancellation bound to the captured operation id", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    vi.useFakeTimers();
    const primary1 = deferred<Response>();
    const primary2 = deferred<Response>();
    const delayedCancel = deferred<Response>();
    let ticketCalls = 0;
    const fetchMock = vi.fn<typeof fetch>().mockImplementation((input, init) => {
      const path = String(input);
      if (path === "/api/operation-ticket") {
        ticketCalls += 1;
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: ticketCalls === 1 ? OPERATION_ID_1 : OPERATION_ID_2,
            kind: "calculation",
            state: "reserved",
          }),
        );
      }
      if (path === "/api/calculate") {
        const body = JSON.parse(String(init?.body)) as { operationId: string };
        return body.operationId === OPERATION_ID_1 ? primary1.promise : primary2.promise;
      }
      if (path === "/api/cancel-calculation") return delayedCancel.promise;
      if (path === "/api/operation-status") {
        const body = JSON.parse(String(init?.body)) as { operationId: string };
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: body.operationId,
            kind: "calculation",
            state: "succeeded",
            sequence: 1,
            progress: null,
          }),
        );
      }
      if (path === "/api/operation-ack") {
        return Promise.resolve(jsonResponse({ acknowledged: true }));
      }
      return Promise.reject(new Error(`unexpected request: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    const first = calculate(CALCULATION_REQUEST);
    await flushMicrotasks();
    const cancellation = cancelCalculation();
    await flushMicrotasks();

    primary1.resolve(jsonResponse(CALCULATION_RESULT));
    await first;
    const second = calculate(CALCULATION_REQUEST);
    await flushMicrotasks();

    const cancelCall = fetchMock.mock.calls.find(
      ([input]) => String(input) === "/api/cancel-calculation",
    );
    expect(JSON.parse(String(cancelCall?.[1]?.body))).toEqual({
      operationId: OPERATION_ID_1,
    });
    expect(callBody(fetchMock, "/api/calculate")).toEqual({
      operationId: OPERATION_ID_1,
      request: CALCULATION_REQUEST,
    });

    delayedCancel.resolve(jsonResponse({ cancelled: false }));
    await cancellation;
    primary2.resolve(jsonResponse(CALCULATION_RESULT));
    await second;
    const calculationBodies = fetchMock.mock.calls
      .filter(([input]) => String(input) === "/api/calculate")
      .map(([, init]) => JSON.parse(String(init?.body)) as { operationId: string });
    expect(calculationBodies.map(({ operationId }) => operationId)).toEqual([
      OPERATION_ID_1,
      OPERATION_ID_2,
    ]);
  });

  it("rejects non-canonical uppercase UUID tickets and clears the active handle", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(
      jsonResponse({
        schemaVersion: 1,
        operationId: "AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA",
        kind: "calculation",
        state: "reserved",
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    await expect(calculate(CALCULATION_REQUEST)).rejects.toThrow("invalid operation ticket");
    await cancelCalculation();
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("does not issue HTTP requests in Tauri mode", async () => {
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    const invokeMock = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: { invoke: invokeMock },
    });
    const fetchMock = vi.fn<typeof fetch>();
    vi.stubGlobal("fetch", fetchMock);

    await cancelCalculation();
    await cancelDownload();
    expect(invokeMock).toHaveBeenNthCalledWith(1, "cancel_calculation", {}, undefined);
    expect(invokeMock).toHaveBeenNthCalledWith(2, "cancel_download", {}, undefined);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("treats cancellation without an active validation operation as a no-op", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    const fetchMock = vi.fn<typeof fetch>();
    vi.stubGlobal("fetch", fetchMock);

    await cancelCalculation();
    await cancelDownload();
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

describe("validation operation recovery edges", () => {
  it("continues polling after a transient status failure", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    vi.useFakeTimers();
    const primary = deferred<Response>();
    let statusCalls = 0;
    const fetchMock = vi.fn<typeof fetch>().mockImplementation((input) => {
      const path = String(input);
      if (path === "/api/operation-ticket") {
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: "reserved",
          }),
        );
      }
      if (path === "/api/calculate") return primary.promise;
      if (path === "/api/operation-status") {
        statusCalls += 1;
        if (statusCalls === 1) {
          return Promise.resolve(jsonResponse({ message: "temporary" }, 503));
        }
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: statusCalls === 2 ? "running" : "succeeded",
            sequence: statusCalls,
            progress:
              statusCalls === 2
                ? {
                    type: "calculation",
                    phase: "computing",
                    percent: 50,
                    completedPixelCount: 50,
                    totalPixelCount: 100,
                  }
                : null,
          }),
        );
      }
      if (path === "/api/operation-ack") {
        return Promise.resolve(jsonResponse({ acknowledged: true }));
      }
      return Promise.reject(new Error(`unexpected request: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);
    const progressHandler = vi.fn();
    const unlisten = await listenCalculationProgress(progressHandler);

    const resultPromise = calculate(CALCULATION_REQUEST);
    await flushMicrotasks();
    await vi.advanceTimersByTimeAsync(250);
    expect(progressHandler).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(250);
    expect(progressHandler).toHaveBeenCalledWith(expect.objectContaining({ percent: 50 }));

    primary.resolve(jsonResponse(CALCULATION_RESULT));
    await expect(resultPromise).resolves.toEqual(CALCULATION_RESULT);
    expect(statusCalls).toBe(3);
    unlisten();
  });

  it("waits for the captured ticket before sending cancellation", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    vi.useFakeTimers();
    const ticketResponse = deferred<Response>();
    const primary = deferred<Response>();
    const fetchMock = vi.fn<typeof fetch>().mockImplementation((input, init) => {
      const path = String(input);
      if (path === "/api/operation-ticket") return ticketResponse.promise;
      if (path === "/api/calculate") return primary.promise;
      if (path === "/api/cancel-calculation") {
        return Promise.resolve(jsonResponse({ cancelled: true }));
      }
      if (path === "/api/operation-status") {
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: "cancelled",
            sequence: 1,
            progress: null,
          }),
        );
      }
      if (path === "/api/operation-ack") {
        return Promise.resolve(jsonResponse({ acknowledged: true }));
      }
      return Promise.reject(new Error(`unexpected request: ${path} ${String(init?.body)}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    const calculation = calculate(CALCULATION_REQUEST);
    const cancellation = cancelCalculation();
    await flushMicrotasks();
    expect(fetchMock).toHaveBeenCalledTimes(1);

    ticketResponse.resolve(
      jsonResponse({
        schemaVersion: 1,
        operationId: OPERATION_ID_1,
        kind: "calculation",
        state: "reserved",
      }),
    );
    await flushMicrotasks();
    expect(callBody(fetchMock, "/api/calculate")).toEqual({
      operationId: OPERATION_ID_1,
      request: CALCULATION_REQUEST,
    });
    expect(callBody(fetchMock, "/api/cancel-calculation")).toEqual({
      operationId: OPERATION_ID_1,
    });
    await cancellation;

    primary.resolve(jsonResponse(CALCULATION_RESULT));
    await calculation;
    expect(vi.getTimerCount()).toBe(0);
  });
});

function abortableNever(signal: AbortSignal | null | undefined): Promise<Response> {
  return new Promise<Response>((_resolve, reject) => {
    const rejectAbort = () => reject(new DOMException("Aborted", "AbortError"));
    if (signal?.aborted) {
      rejectAbort();
    } else {
      signal?.addEventListener("abort", rejectAbort, { once: true });
    }
  });
}

describe("validation operation bounded cleanup", () => {
  it("bounds final status and ack while allowing the next generation to start", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    vi.useFakeTimers();
    let ticketCalls = 0;
    const fetchMock = vi.fn<typeof fetch>().mockImplementation((input, init) => {
      const path = String(input);
      if (path === "/api/operation-ticket") {
        ticketCalls += 1;
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: ticketCalls === 1 ? OPERATION_ID_1 : OPERATION_ID_2,
            kind: "calculation",
            state: "reserved",
          }),
        );
      }
      if (path === "/api/calculate") {
        return Promise.resolve(jsonResponse(CALCULATION_RESULT));
      }
      if (path === "/api/operation-status") {
        const body = JSON.parse(String(init?.body)) as { operationId: string };
        if (body.operationId === OPERATION_ID_1) return abortableNever(init?.signal);
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_2,
            kind: "calculation",
            state: "succeeded",
            sequence: 1,
            progress: null,
          }),
        );
      }
      if (path === "/api/operation-ack") {
        const body = JSON.parse(String(init?.body)) as { operationId: string };
        if (body.operationId === OPERATION_ID_1) return abortableNever(init?.signal);
        return Promise.resolve(jsonResponse({ acknowledged: true }));
      }
      return Promise.reject(new Error(`unexpected request: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    let firstSettled = false;
    const first = calculate(CALCULATION_REQUEST).then((result) => {
      firstSettled = true;
      return result;
    });
    await flushMicrotasks();
    expect(firstSettled).toBe(false);

    await vi.advanceTimersByTimeAsync(1_499);
    expect(firstSettled).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await flushMicrotasks();
    expect(firstSettled).toBe(false);
    expect(callBody(fetchMock, "/api/operation-ack")).toEqual({
      operationId: OPERATION_ID_1,
    });

    const second = calculate(CALCULATION_REQUEST);
    await expect(second).resolves.toEqual(CALCULATION_RESULT);
    expect(ticketCalls).toBe(2);
    expect(firstSettled).toBe(false);

    await vi.advanceTimersByTimeAsync(1_500);
    await expect(first).resolves.toEqual(CALCULATION_RESULT);
    expect(firstSettled).toBe(true);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("retries the exact cancellation after a reserved false response", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    vi.useFakeTimers();
    const primary = deferred<Response>();
    let cancelCalls = 0;
    let statusCalls = 0;
    const fetchMock = vi.fn<typeof fetch>().mockImplementation((input) => {
      const path = String(input);
      if (path === "/api/operation-ticket") {
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: "reserved",
          }),
        );
      }
      if (path === "/api/calculate") return primary.promise;
      if (path === "/api/cancel-calculation") {
        cancelCalls += 1;
        return Promise.resolve(jsonResponse({ cancelled: cancelCalls > 1 }));
      }
      if (path === "/api/operation-status") {
        statusCalls += 1;
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: statusCalls === 1 ? "reserved" : "cancelled",
            sequence: statusCalls,
            progress: null,
          }),
        );
      }
      if (path === "/api/operation-ack") {
        return Promise.resolve(jsonResponse({ acknowledged: true }));
      }
      return Promise.reject(new Error(`unexpected request: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    const calculation = calculate(CALCULATION_REQUEST);
    await flushMicrotasks();
    const cancellation = cancelCalculation();
    await flushMicrotasks();
    expect(cancelCalls).toBe(1);
    expect(statusCalls).toBe(1);

    await vi.advanceTimersByTimeAsync(100);
    await expect(cancellation).resolves.toBeUndefined();
    expect(cancelCalls).toBe(2);
    const cancelBodies = fetchMock.mock.calls
      .filter(([input]) => String(input) === "/api/cancel-calculation")
      .map(([, init]) => JSON.parse(String(init?.body)) as { operationId: string });
    expect(cancelBodies).toEqual([
      { operationId: OPERATION_ID_1 },
      { operationId: OPERATION_ID_1 },
    ]);

    primary.resolve(jsonResponse(CALCULATION_RESULT));
    await calculation;
    expect(vi.getTimerCount()).toBe(0);
  });

  it("reports a bounded cancellation timeout while retaining exact-id isolation", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    vi.useFakeTimers();
    const primary = deferred<Response>();
    const cancelBodies: { operationId: string }[] = [];
    const fetchMock = vi.fn<typeof fetch>().mockImplementation((input, init) => {
      const path = String(input);
      if (path === "/api/operation-ticket") {
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: "reserved",
          }),
        );
      }
      if (path === "/api/calculate") return primary.promise;
      if (path === "/api/cancel-calculation") {
        cancelBodies.push(JSON.parse(String(init?.body)) as { operationId: string });
        return Promise.resolve(jsonResponse({ cancelled: false }));
      }
      if (path === "/api/operation-status") {
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: "reserved",
            sequence: 0,
            progress: null,
          }),
        );
      }
      if (path === "/api/operation-ack") {
        return Promise.resolve(jsonResponse({ acknowledged: true }));
      }
      return Promise.reject(new Error(`unexpected request: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    const calculation = calculate(CALCULATION_REQUEST);
    await flushMicrotasks();
    const cancellationExpectation = expect(cancelCalculation()).rejects.toThrow(
      "Cancellation timed out",
    );
    await flushMicrotasks();
    await vi.advanceTimersByTimeAsync(3_000);
    await cancellationExpectation;
    expect(cancelBodies.length).toBeGreaterThan(1);
    expect(cancelBodies.every(({ operationId }) => operationId === OPERATION_ID_1)).toBe(true);

    primary.resolve(jsonResponse(CALCULATION_RESULT));
    await calculation;
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe("validation calculation preview protocol", () => {
  const PREVIEW = {
    schemaVersion: 1 as const,
    sequence: 1,
    completedPixelCount: 12_563,
    totalPixelCount: 125_628,
    mapOverlayProjection: "EPSG:3857" as const,
    mapOverlayWidth: 401,
    mapOverlayHeight: 401,
    mapOverlayCorners: [
      [101, 32],
      [106, 32],
      [106, 29],
      [101, 29],
    ] as [number, number][],
    mapOverlayPngDataUrl: "data:image/png;base64,iVBORw0KGgo=",
  };

  it("polls status then preview without overlap, accepts 200, and handles 204", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    vi.useFakeTimers();
    const primary = deferred<Response>();
    const firstPreview = deferred<Response>();
    let statusCalls = 0;
    let previewCalls = 0;
    const fetchMock = vi.fn<typeof fetch>().mockImplementation((input) => {
      const path = String(input);
      if (path === "/api/operation-ticket") {
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: "reserved",
          }),
        );
      }
      if (path === "/api/calculate") return primary.promise;
      if (path === "/api/operation-status") {
        statusCalls += 1;
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: "running",
            sequence: statusCalls,
            progress: null,
          }),
        );
      }
      if (path === "/api/operation-preview") {
        previewCalls += 1;
        return previewCalls === 1
          ? firstPreview.promise
          : Promise.resolve(new Response(null, { status: 204 }));
      }
      if (path === "/api/operation-ack") {
        return Promise.resolve(jsonResponse({ acknowledged: true }));
      }
      return Promise.reject(new Error(`unexpected request: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);
    const previewHandler = vi.fn();
    const unlisten = await listenCalculationPreview(previewHandler);

    const resultPromise = calculate(CALCULATION_REQUEST);
    await flushMicrotasks();
    await vi.advanceTimersByTimeAsync(250);
    expect(statusCalls).toBe(1);
    expect(previewCalls).toBe(1);
    const firstBody = fetchMock.mock.calls
      .filter(([input]) => String(input) === "/api/operation-preview")
      .map(([, init]) => JSON.parse(String(init?.body)))[0];
    expect(firstBody).toEqual({
      operationId: OPERATION_ID_1,
      afterSequence: 0,
    });

    await vi.advanceTimersByTimeAsync(1_000);
    expect(statusCalls).toBe(1);
    expect(previewCalls).toBe(1);

    firstPreview.resolve(jsonResponse(PREVIEW));
    await flushMicrotasks();
    expect(previewHandler).toHaveBeenCalledTimes(1);
    expect(previewHandler).toHaveBeenCalledWith(PREVIEW);

    await vi.advanceTimersByTimeAsync(250);
    expect(statusCalls).toBe(2);
    expect(previewCalls).toBe(2);
    const previewBodies = fetchMock.mock.calls
      .filter(([input]) => String(input) === "/api/operation-preview")
      .map(([, init]) => JSON.parse(String(init?.body)));
    expect(previewBodies[1]).toEqual({
      operationId: OPERATION_ID_1,
      afterSequence: 1,
    });
    expect(previewHandler).toHaveBeenCalledTimes(1);

    primary.resolve(jsonResponse(CALCULATION_RESULT));
    await expect(resultPromise).resolves.toEqual(CALCULATION_RESULT);
    expect(statusCalls).toBe(3);
    expect(vi.getTimerCount()).toBe(0);
    unlisten();
  });

  it("drops an in-flight preview after the captured operation is stopped", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    vi.useFakeTimers();
    const primary = deferred<Response>();
    const latePreview = deferred<Response>();
    let statusCalls = 0;
    const fetchMock = vi.fn<typeof fetch>().mockImplementation((input) => {
      const path = String(input);
      if (path === "/api/operation-ticket") {
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: "reserved",
          }),
        );
      }
      if (path === "/api/calculate") return primary.promise;
      if (path === "/api/operation-status") {
        statusCalls += 1;
        return Promise.resolve(
          jsonResponse({
            schemaVersion: 1,
            operationId: OPERATION_ID_1,
            kind: "calculation",
            state: statusCalls === 1 ? "running" : "succeeded",
            sequence: statusCalls,
            progress: null,
          }),
        );
      }
      if (path === "/api/operation-preview") return latePreview.promise;
      if (path === "/api/operation-ack") {
        return Promise.resolve(jsonResponse({ acknowledged: true }));
      }
      return Promise.reject(new Error(`unexpected request: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);
    const previewHandler = vi.fn();
    const unlisten = await listenCalculationPreview(previewHandler);

    const resultPromise = calculate(CALCULATION_REQUEST);
    await flushMicrotasks();
    await vi.advanceTimersByTimeAsync(250);
    primary.resolve(jsonResponse(CALCULATION_RESULT));
    await flushMicrotasks();
    latePreview.resolve(jsonResponse(PREVIEW));

    await expect(resultPromise).resolves.toEqual(CALCULATION_RESULT);
    expect(previewHandler).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
    unlisten();
  });
});

describe("Tauri calculation preview channel", () => {
  const TAURI_PREVIEW: CalculationPreview = {
    schemaVersion: 1,
    sequence: 1,
    completedPixelCount: 12_563,
    totalPixelCount: 125_628,
    mapOverlayProjection: "EPSG:3857",
    mapOverlayWidth: 401,
    mapOverlayHeight: 401,
    mapOverlayCorners: [
      [101, 32],
      [106, 32],
      [106, 29],
      [101, 29],
    ],
    mapOverlayPngDataUrl: "data:image/png;base64,iVBORw0KGgo=",
  };

  it.each([
    [
      "an old schema",
      { ...CALCULATION_RESULT, schemaVersion: 3 },
      "schemaVersion 4",
    ],
    [
      "an unsupported filter encoding",
      { ...CALCULATION_RESULT, mapOverlayFilterEncoding: "u8-other" },
      "u8-dbm-floor-v1",
    ],
    [
      "a non-401 overlay",
      { ...CALCULATION_RESULT, mapOverlayWidth: 400 },
      "401 x 401",
    ],
    [
      "a missing map overlay PNG",
      { ...CALCULATION_RESULT, mapOverlayPngDataUrl: "" },
      "401 x 401",
    ],
    [
      "a decoded filter with the wrong length",
      { ...CALCULATION_RESULT, mapOverlayFilterBase64: "UQ==" },
      "does not match",
    ],
    [
      "a filter bin above 81",
      {
        ...CALCULATION_RESULT,
        mapOverlayFilterBase64: btoa(
          "\x52" + "\x51".repeat(401 * 401 - 1),
        ),
      },
      "0..81",
    ],
  ])("rejects %s before returning success", async (_label, payload, message) => {
    mockIPC((command) => {
      expect(command).toBe("calculate");
      return Promise.resolve(payload);
    });

    await expect(calculate(CALCULATION_REQUEST)).rejects.toThrow(message);
  });

  it("passes a per-invocation Channel, validates messages, and suppresses late delivery", async () => {
    const completion = deferred<CalculationResult>();
    let capturedChannel: Channel<CalculationPreview> | null = null;
    mockIPC((command, args) => {
      expect(command).toBe("calculate");
      const payload = args as {
        request?: unknown;
        previewChannel?: unknown;
      };
      expect(payload.request).toEqual(CALCULATION_REQUEST);
      expect(payload.previewChannel).toBeInstanceOf(Channel);
      capturedChannel = payload.previewChannel as Channel<CalculationPreview>;
      return completion.promise;
    });
    const previewHandler = vi.fn();
    const unlisten = await listenCalculationPreview(previewHandler);

    const resultPromise = calculate(CALCULATION_REQUEST);
    await flushMicrotasks();
    const activeChannel = capturedChannel as Channel<CalculationPreview> | null;
    if (!activeChannel) throw new Error("calculate did not pass its preview Channel");

    activeChannel.onmessage({
      ...TAURI_PREVIEW,
      mapOverlayWidth: 400,
    });
    activeChannel.onmessage(TAURI_PREVIEW);
    activeChannel.onmessage(TAURI_PREVIEW);
    expect(previewHandler).toHaveBeenCalledTimes(1);
    expect(previewHandler).toHaveBeenCalledWith(TAURI_PREVIEW);

    completion.resolve(CALCULATION_RESULT);
    await expect(resultPromise).resolves.toEqual(CALCULATION_RESULT);

    activeChannel.onmessage({
      ...TAURI_PREVIEW,
      sequence: 2,
      completedPixelCount: 25_126,
    });
    expect(previewHandler).toHaveBeenCalledTimes(1);
    unlisten();
  });
});
