import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  BootstrapInfo,
  CacheDeleteResult,
  CacheOverview,
  CalculationProgress,
  CalculationRequest,
  ExportRequest,
  ExportResult,
  CalculationResult,
  DownloadEstimate,
  DownloadProgress,
  DownloadResult,
  MapPoint,
  PointInspection,
} from "./types";

const PREVIEW_CACHE_CAP = 2_500_000_000;

export type BackendMode = "tauri" | "validation-server" | "preview";

export interface BackendCapabilities {
  mode: BackendMode;
  canDownload: boolean;
  canDeleteCache: boolean;
  canCalculate: boolean;
  canExport: boolean;
}

export function backendMode(): BackendMode {
  if ("__TAURI_INTERNALS__" in window) return "tauri";
  return import.meta.env.VITE_VALIDATION_SERVER === "1"
    ? "validation-server"
    : "preview";
}

export function backendCapabilities(): BackendCapabilities {
  const mode = backendMode();
  return {
    mode,
    canDownload: mode !== "preview",
    canDeleteCache: mode !== "preview",
    canCalculate: mode !== "preview",
    canExport: mode === "tauri",
  };
}

export function desktopBackendAvailable(): boolean {
  return backendMode() === "tauri";
}

async function validationRequest<T>(
  path: string,
  body?: Record<string, unknown>,
  method = body ? "POST" : "GET",
): Promise<T> {
  const sendsJson = body !== undefined || method.toUpperCase() === "POST";
  const response = await fetch(path, {
    method,
    headers: sendsJson
      ? { Accept: "application/json", "Content-Type": "application/json" }
      : { Accept: "application/json" },
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!response.ok) {
    const contentType = response.headers.get("content-type") ?? "";
    let message = `${response.status} ${response.statusText}`.trim();
    if (contentType.includes("application/json")) {
      const value = (await response.json()) as { error?: unknown; message?: unknown };
      const detail = value.message ?? value.error;
      if (typeof detail === "string" && detail.trim()) message = detail;
    } else {
      const detail = (await response.text()).trim();
      if (detail) message = detail;
    }
    throw new Error(message);
  }
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

export async function bootstrap(): Promise<BootstrapInfo> {
  if (desktopBackendAvailable()) {
    return invoke<BootstrapInfo>("bootstrap");
  }
  if (backendMode() === "validation-server") {
    return validationRequest<BootstrapInfo>("/api/bootstrap");
  }
  return {
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
      remainingBytes: PREVIEW_CACHE_CAP,
      capBytes: PREVIEW_CACHE_CAP,
    },
    internalBuildWarning: "内部测试底图，不得公开发布",
  };
}

export async function inspectPoint(point: MapPoint): Promise<PointInspection> {
  if (desktopBackendAvailable()) {
    return invoke<PointInspection>("inspect_point", { point });
  }
  if (backendMode() === "validation-server") {
    return validationRequest<PointInspection>("/api/inspect-point", { point });
  }
  return {
    point,
    regionId: "browser-interface-preview",
    tileCount: 21,
    readyDemCount: 0,
    readyWaterCount: 0,
    missingAssetCount: 42,
    dataReady: false,
    elevationM: null,
    cacheUsage: {
      totalBytes: 0,
      demBytes: 0,
      waterBytes: 0,
      partialBytes: 0,
      metadataBytes: 0,
      remainingBytes: PREVIEW_CACHE_CAP,
      capBytes: PREVIEW_CACHE_CAP,
    },
  };
}

export async function estimateDownload(point: MapPoint): Promise<DownloadEstimate> {
  if (desktopBackendAvailable()) {
    return invoke<DownloadEstimate>("estimate_download", { point });
  }
  if (backendMode() === "validation-server") {
    return validationRequest<DownloadEstimate>("/api/estimate-download", { point });
  }
  return {
    point,
    regionId: "browser-interface-preview",
    tileCount: 21,
    readyAssetCount: 0,
    requiredAssetCount: 42,
    generatedAssetCount: 4,
    additionalDownloadBytes: 118_400_000,
    resumableBytes: 0,
    projectedTotalBytes: 118_400_000,
    projectedRemainingBytes: PREVIEW_CACHE_CAP - 118_400_000,
    cacheUsage: {
      totalBytes: 0,
      demBytes: 0,
      waterBytes: 0,
      partialBytes: 0,
      metadataBytes: 0,
      remainingBytes: PREVIEW_CACHE_CAP,
      capBytes: PREVIEW_CACHE_CAP,
    },
  };
}

export async function downloadRegion(point: MapPoint): Promise<DownloadResult> {
  if (desktopBackendAvailable()) {
    return invoke<DownloadResult>("download_region", { point });
  }
  if (backendMode() === "validation-server") {
    return validationRequest<DownloadResult>("/api/download-region", { point });
  }
  throw new Error("浏览器仅展示下载确认界面；真实数据只能由 Tauri 桌面后端下载。");
}

export async function cancelDownload(): Promise<void> {
  if (desktopBackendAvailable()) {
    await invoke("cancel_download");
  } else if (backendMode() === "validation-server") {
    await validationRequest<void>("/api/cancel-download", undefined, "POST");
  }
}

export async function cacheOverview(): Promise<CacheOverview> {
  if (desktopBackendAvailable()) {
    return invoke<CacheOverview>("cache_overview");
  }
  if (backendMode() === "validation-server") {
    return validationRequest<CacheOverview>("/api/cache-overview");
  }
  return {
    usage: {
      totalBytes: 0,
      demBytes: 0,
      waterBytes: 0,
      partialBytes: 0,
      metadataBytes: 0,
      remainingBytes: PREVIEW_CACHE_CAP,
      capBytes: PREVIEW_CACHE_CAP,
    },
    regions: [],
  };
}

export async function deleteCacheRegion(regionId: string): Promise<CacheDeleteResult> {
  if (desktopBackendAvailable()) {
    return invoke<CacheDeleteResult>("delete_cache_region", { regionId });
  }
  if (backendMode() === "validation-server") {
    return validationRequest<CacheDeleteResult>("/api/delete-cache-region", { regionId });
  }
  throw new Error("缓存删除只在 Tauri 桌面后端中可用。");
}

export async function calculate(
  request: CalculationRequest,
): Promise<CalculationResult> {
  if (desktopBackendAvailable()) {
    return invoke<CalculationResult>("calculate", { request });
  }
  if (backendMode() === "validation-server") {
    return validationRequest<CalculationResult>("/api/calculate", { request });
  }
  throw new Error("浏览器仅用于界面检查；真实传播计算必须在 Tauri 桌面后端中运行。");
}

export async function cancelCalculation(): Promise<void> {
  if (desktopBackendAvailable()) {
    await invoke("cancel_calculation");
  } else if (backendMode() === "validation-server") {
    await validationRequest<void>("/api/cancel-calculation", undefined, "POST");
  }
}

export async function exportReport(request: ExportRequest): Promise<ExportResult> {
  if (!desktopBackendAvailable()) {
    throw new Error("文件导出只在 Tauri Windows 桌面应用中可用。");
  }
  return invoke<ExportResult>("export_result", { request });
}

export async function listenCalculationProgress(
  handler: (progress: CalculationProgress) => void,
): Promise<UnlistenFn> {
  if (!desktopBackendAvailable()) {
    return () => undefined;
  }
  return listen<CalculationProgress>("calculation-progress", (event) => {
    handler(event.payload);
  });
}

export async function listenDownloadProgress(
  handler: (progress: DownloadProgress) => void,
): Promise<UnlistenFn> {
  if (!desktopBackendAvailable()) {
    return () => undefined;
  }
  return listen<DownloadProgress>("download-progress", (event) => {
    handler(event.payload);
  });
}
