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

export function desktopBackendAvailable(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export async function bootstrap(): Promise<BootstrapInfo> {
  if (desktopBackendAvailable()) {
    return invoke<BootstrapInfo>("bootstrap");
  }
  return {
    schemaVersion: 1,
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
  if (!desktopBackendAvailable()) {
    throw new Error("浏览器仅展示下载确认界面；真实数据只能由 Tauri 桌面后端下载。");
  }
  return invoke<DownloadResult>("download_region", { point });
}

export async function cancelDownload(): Promise<void> {
  if (desktopBackendAvailable()) {
    await invoke("cancel_download");
  }
}

export async function cacheOverview(): Promise<CacheOverview> {
  if (desktopBackendAvailable()) {
    return invoke<CacheOverview>("cache_overview");
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
  if (!desktopBackendAvailable()) {
    throw new Error("缓存删除只在 Tauri 桌面后端中可用。");
  }
  return invoke<CacheDeleteResult>("delete_cache_region", { regionId });
}

export async function calculate(
  request: CalculationRequest,
): Promise<CalculationResult> {
  if (!desktopBackendAvailable()) {
    throw new Error("浏览器仅用于界面检查；真实传播计算必须在 Tauri 桌面后端中运行。");
  }
  return invoke<CalculationResult>("calculate", { request });
}

export async function cancelCalculation(): Promise<void> {
  if (desktopBackendAvailable()) {
    await invoke("cancel_calculation");
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
