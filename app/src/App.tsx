import { useEffect, useMemo, useRef, useState } from "react";

import { MapView } from "./components/MapView";
import { ParameterPanel } from "./components/ParameterPanel";
import {
  backendCapabilities,
  bootstrap,
  clearOnlineBasemap,
  cacheOverview as loadCacheOverview,
  calculate,
  cancelCalculation,
  cancelDownload,
  configureOnlineBasemap,
  deleteCacheRegion,
  exportReport,
  downloadRegion,
  estimateDownload,
  inspectPoint,
  listenCalculationPreview,
  listenCalculationProgress,
  listenDownloadProgress,
  probeOnlineBasemap,
} from "./lib/backend";
import {
  isTrustedOnlineBasemap,
  isTrustedTiandituBasemap,
} from "./lib/basemap";
import { MAX_VISIBLE_DBM, MIN_VISIBLE_DBM } from "./lib/coverageVisibility";
import {
  createExportReportPngDataUrl,
  suggestedExportFileName,
} from "./lib/export";
import { DEFAULT_PARAMETERS, parameterValidationMessage } from "./lib/parameters";
import {
  MAX_SESSION_COVERAGES,
  mergeSessionCoverage,
} from "./lib/sessionCoverages";
import type {
  BootstrapInfo,
  CacheOverview,
  CacheRegion,
  CalculationPreview,
  CalculationProgress,
  CalculationRequest,
  CalculationResult,
  DownloadEstimate,
  DownloadProgress,
  ExportFormat,
  MapPoint,
  OnlineBasemapProbeResult,
  PointInspection,
  RadioParameters,
  ResolvedTheme,
  SessionCoverageResult,
  ThemePreference,
  WorkflowState,
} from "./lib/types";

const THEME_STORAGE_KEY = "hamheatmap-theme";
function currentSystemTheme(): ResolvedTheme {
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function loadThemePreference(): ThemePreference {
  const stored = localStorage.getItem(THEME_STORAGE_KEY);
  return stored === "light" || stored === "dark" || stored === "system" ? stored : "system";
}

function formatBytes(bytes: number): string {
  if (bytes < 1_000_000) return `${(bytes / 1000).toFixed(1)} KB`;
  if (bytes < 1_000_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  return `${(bytes / 1_000_000_000).toFixed(2)} GB`;
}

function phaseLabel(progress: CalculationProgress | null): string {
  if (!progress) return "准备计算";
  switch (progress.phase) {
    case "loading-data":
      return "正在校验并载入高程与水体数据";
    case "computing":
      return `正在计算传播路径 ${progress.completedPixelCount.toLocaleString()} / ${progress.totalPixelCount.toLocaleString()}`;
    case "encoding":
      return "正在生成热力图";
    case "complete":
      return "计算完成";
  }
}

type OnlineBasemapProbePresentation = {
  tone: "success" | "warning" | "error";
  title: string;
  detail: string;
};

function describeOnlineBasemapProbe(
  status: OnlineBasemapProbeResult["status"],
): OnlineBasemapProbePresentation {
  switch (status) {
    case "reachable":
      return {
        tone: "success",
        title: "连接测试通过",
        detail: "天地图瓦片服务当前可访问。实际显示仍取决于各缩放级别的服务状态。",
      };
    case "not-configured":
      return {
        tone: "warning",
        title: "尚未保存配置",
        detail: "请先输入天地图 tk，然后点击“保存并测试”。",
      };
    case "network":
      return {
        tone: "error",
        title: "网络连接失败",
        detail: "请检查网络、代理或防火墙设置，然后重新测试连接。",
      };
    case "timeout":
      return {
        tone: "warning",
        title: "连接测试超时",
        detail: "请稍后重试，并检查当前网络是否稳定。",
      };
    case "upstream-or-credential":
      return {
        tone: "warning",
        title: "服务或配置暂不可用",
        detail:
          "可能与 tk、账号权限、调用配额或天地图服务状态有关；自检无法精确区分，请在天地图控制台检查后重试。",
      };
    case "invalid-content":
      return {
        tone: "error",
        title: "地图响应内容无效",
        detail: "请稍后重试；如果持续出现，请检查代理或网络内容拦截设置。",
      };
  }
}

function buildRequest(point: MapPoint, parameters: RadioParameters): CalculationRequest {
  return {
    center: point,
    band: parameters.band === "vhf144" ? "vhf-144" : "uhf-430",
    frequencyMhz: parameters.frequencyMhz,
    powerValue: parameters.powerValue,
    powerUnit: parameters.powerUnit,
    txGainValue: parameters.txGainValue,
    txGainUnit: parameters.txGainUnit,
    txHeightM: parameters.txHeightM,
    txGroundElevationOverrideM: parameters.txGroundElevationOverrideM,
    rxGainValue: parameters.rxGainValue,
    rxGainUnit: parameters.rxGainUnit,
    rxHeightM: parameters.rxHeightM,
    polarization: parameters.polarization,
  };
}

export function App() {
  const [themePreference, setThemePreference] = useState<ThemePreference>(loadThemePreference);
  const [systemTheme, setSystemTheme] = useState<ResolvedTheme>(currentSystemTheme);
  const resolvedTheme = themePreference === "system" ? systemTheme : themePreference;
  const bootstrapStartedRef = useRef(false);
  const cacheLoadingRef = useRef(false);
  const deletingRegionRef = useRef(false);
  const exportingRef = useRef(false);
  const coverageSequenceRef = useRef(0);
  const cancellationPendingRef = useRef(false);
  const previewSuppressedRef = useRef(true);
  const [bootstrapLoading, setBootstrapLoading] = useState(true);
  const [bootstrapInfo, setBootstrapInfo] = useState<BootstrapInfo | null>(null);
  const [point, setPoint] = useState<MapPoint | null>(null);
  const [inspection, setInspection] = useState<PointInspection | null>(null);
  const [parameters, setParameters] = useState<RadioParameters>(DEFAULT_PARAMETERS);
  const [workflow, setWorkflow] = useState<WorkflowState>("idle");
  const [progress, setProgress] = useState<CalculationProgress | null>(null);
  const [preview, setPreview] = useState<CalculationPreview | null>(null);
  const [downloadProgress, setDownloadProgress] = useState<DownloadProgress | null>(null);
  const [downloadEstimate, setDownloadEstimate] = useState<DownloadEstimate | null>(null);
  const [result, setResult] = useState<CalculationResult | null>(null);
  const [resultParameters, setResultParameters] = useState<RadioParameters | null>(null);
  const [sessionResults, setSessionResults] = useState<SessionCoverageResult[]>([]);
  const [activeResultId, setActiveResultId] = useState<string | null>(null);
  const [resultStale, setResultStale] = useState(false);
  const [visibleSignalThresholdDbm, setVisibleSignalThresholdDbm] = useState(MIN_VISIBLE_DBM);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [cancellationPending, setCancellationPending] = useState(false);
  const [cacheOpen, setCacheOpen] = useState(false);
  const [cacheOverview, setCacheOverview] = useState<CacheOverview | null>(null);
  const [cacheLoading, setCacheLoading] = useState(false);
  const [cacheError, setCacheError] = useState<string | null>(null);
  const [deleteCandidate, setDeleteCandidate] = useState<CacheRegion | null>(null);
  const [deletingRegion, setDeletingRegion] = useState(false);
  const capabilities = backendCapabilities();
  const validationServerMode = capabilities.mode === "validation-server";
  const desktopMode = capabilities.mode === "tauri";
  const [exportOpen, setExportOpen] = useState(false);
  const [exportingFormat, setExportingFormat] = useState<ExportFormat | null>(null);
  const [exportMessage, setExportMessage] = useState<string | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);
  const [mapSettingsOpen, setMapSettingsOpen] = useState(false);
  const [mapToken, setMapToken] = useState("");
  const [mapSettingsAction, setMapSettingsAction] = useState<
    "saving-and-testing" | "testing" | "clearing" | null
  >(null);
  const mapSettingsBusy = mapSettingsAction !== null;
  const [mapProbeResult, setMapProbeResult] =
    useState<OnlineBasemapProbeResult | null>(null);
  const [mapProbeUnexpectedError, setMapProbeUnexpectedError] = useState(false);
  const [mapSettingsMessage, setMapSettingsMessage] = useState<string | null>(null);
  const [mapSettingsError, setMapSettingsError] = useState<string | null>(null);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const listener = () => setSystemTheme(media.matches ? "dark" : "light");
    media.addEventListener("change", listener);
    return () => media.removeEventListener("change", listener);
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = resolvedTheme;
    document.documentElement.style.colorScheme = resolvedTheme;
    localStorage.setItem(THEME_STORAGE_KEY, themePreference);
  }, [resolvedTheme, themePreference]);

  useEffect(() => {
    if (bootstrapStartedRef.current) return;
    bootstrapStartedRef.current = true;
    bootstrap()
      .then(setBootstrapInfo)
      .catch((error: unknown) => {
        setErrorMessage(error instanceof Error ? error.message : String(error));
        setWorkflow("error");
      })
      .finally(() => setBootstrapLoading(false));
  }, []);

  useEffect(() => {
    let active = true;
    if (!point) {
      setInspection(null);
      setWorkflow("idle");
      return () => {
        active = false;
      };
    }
    setWorkflow("inspecting");
    setInspection(null);
    setErrorMessage(null);
    inspectPoint(point)
      .then((value) => {
        if (!active) return;
        setInspection(value);
        setBootstrapInfo((current) =>
          current ? { ...current, cacheUsage: value.cacheUsage } : current,
        );
        setWorkflow(value.dataReady ? "ready" : "missing-data");
      })
      .catch((error: unknown) => {
        if (!active) return;
        setErrorMessage(error instanceof Error ? error.message : String(error));
        setWorkflow("error");
      });
    return () => {
      active = false;
    };
  }, [point]);

  useEffect(() => {
    let active = true;
    let unlistenCalculation: (() => void) | undefined;
    let unlistenPreview: (() => void) | undefined;
    let unlistenDownload: (() => void) | undefined;
    listenCalculationProgress(setProgress).then((dispose) => {
      if (active) unlistenCalculation = dispose;
      else dispose();
    });
    listenCalculationPreview((value) => {
      if (!previewSuppressedRef.current) setPreview(value);
    }).then((dispose) => {
      if (active) unlistenPreview = dispose;
      else dispose();
    });
    listenDownloadProgress(setDownloadProgress).then((dispose) => {
      if (active) unlistenDownload = dispose;
      else dispose();
    });
    return () => {
      active = false;
      unlistenCalculation?.();
      unlistenPreview?.();
      unlistenDownload?.();
    };
  }, []);

  const validationMessage = parameterValidationMessage(parameters);
  const isCalculating = workflow === "calculating";
  const isDownloading = workflow === "downloading";
  const isEstimatingDownload = workflow === "estimating-download";
  const pointSelectionLocked =
    bootstrapLoading ||
    cacheLoading ||
    workflow === "inspecting" ||
    isEstimatingDownload ||
    isDownloading ||
    isCalculating ||
    cancellationPending;
  const isBusy =
    pointSelectionLocked || deletingRegion || exportingFormat !== null || mapSettingsBusy;
  const canCalculate = Boolean(
    capabilities.canCalculate &&
      point &&
      inspection?.dataReady &&
      !validationMessage &&
      !isBusy,
  );
  const canPrepareData = Boolean(
    point && inspection && !inspection.dataReady && !isBusy && workflow !== "download-required",
  );

  const status = useMemo(() => {
    if (bootstrapLoading) {
      return {
        tone: "working",
        title: "正在初始化本地数据",
        detail: "校验缓存目录、索引和 2.5 GB 硬上限",
      };
    }
    switch (workflow) {
      case "idle":
        return { tone: "neutral", title: "等待选择发射点", detail: "在地图上单击一个位置开始" };
      case "inspecting":
        return { tone: "working", title: "正在检查区域数据", detail: "校验 DEM、WBM 与缓存完整性" };
      case "estimating-download":
        return {
          tone: "working",
          title: "正在核对固定数据源",
          detail: "读取 DEM/WBM 大小并检查 2.5 GB 配额",
        };
      case "download-required":
        return {
          tone: "warning",
          title: "等待下载确认",
          detail: downloadEstimate
            ? `需要新增 ${formatBytes(downloadEstimate.additionalDownloadBytes)}`
            : "请确认当前区域数据",
        };
      case "downloading":
        return {
          tone: "working",
          title: "正在准备离线区域",
          detail: downloadProgress
            ? `${formatBytes(downloadProgress.totalDownloadedBytes)} / ${formatBytes(downloadProgress.totalExpectedBytes)} · 资产 ${downloadProgress.assetIndex}/${downloadProgress.assetCount}`
            : "正在建立安全下载任务",
        };
      case "ready":
        return { tone: "ready", title: "数据已就绪", detail: "可以开始 200 km 覆盖计算" };
      case "missing-data":
        return capabilities.mode !== "preview"
          ? {
              tone: "warning",
              title: "当前区域缺少离线数据",
              detail: `还需准备 ${inspection?.missingAssetCount ?? 0} 个 DEM/WBM 资产`,
            }
          : {
              tone: "warning",
              title: "浏览器界面预览",
              detail: "真实缓存检查和传播计算只在 Tauri 桌面后端运行",
            };
      case "calculating":
        return { tone: "working", title: "传播计算进行中", detail: phaseLabel(progress) };
      case "completed":
        return {
          tone: "ready",
          title: resultStale ? "参数已变化，结果已过期" : "覆盖计算完成",
          detail: result
            ? `${result.statistics.validPixelCount.toLocaleString()} 个像素 · ${result.statistics.totalSeconds.toFixed(1)} 秒`
            : "热力图已生成",
        };
      case "cancelled":
        return { tone: "neutral", title: "计算已取消", detail: "未保留可导出的半成品" };
      case "download-cancelled":
        return {
          tone: "neutral",
          title: "下载已取消",
          detail: "已完成资产和可续传临时文件仍保留在缓存中",
        };
      case "error":
        return { tone: "error", title: "操作未完成", detail: errorMessage ?? "发生未知错误" };
    }
  }, [
    capabilities.mode,
    bootstrapLoading,
    downloadEstimate,
    downloadProgress,
    errorMessage,
    inspection,
    progress,
    result,
    resultStale,
    workflow,
  ]);

  async function refreshCacheOverview() {
    if (cacheLoadingRef.current || deletingRegionRef.current) return;
    cacheLoadingRef.current = true;
    setCacheLoading(true);
    setCacheError(null);
    try {
      const value = await loadCacheOverview();
      setCacheOverview(value);
      setBootstrapInfo((current) =>
        current ? { ...current, cacheUsage: value.usage } : current,
      );
    } catch (error) {
      setCacheError(error instanceof Error ? error.message : String(error));
    } finally {
      cacheLoadingRef.current = false;
      setCacheLoading(false);
    }
  }

  function openCacheModal() {
    if (isBusy) return;
    setCacheOpen(true);
    void refreshCacheOverview();
  }

  async function handlePrepareData() {
    if (!point || !canPrepareData) return;
    setWorkflow("estimating-download");
    setDownloadProgress(null);
    setErrorMessage(null);
    try {
      const value = await estimateDownload(point);
      setDownloadEstimate(value);
      setBootstrapInfo((current) =>
        current ? { ...current, cacheUsage: value.cacheUsage } : current,
      );
      setWorkflow("download-required");
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (message.toLowerCase().includes("cancel") || message.includes("取消")) {
        setDownloadEstimate(null);
        setDownloadProgress(null);
        setErrorMessage(null);
        setWorkflow(inspection?.dataReady ? "ready" : "download-cancelled");
      } else {
        setErrorMessage(message);
        setWorkflow("error");
      }
    }
  }

  function dismissDownloadEstimate() {
    setDownloadEstimate(null);
    setWorkflow(inspection?.dataReady ? "ready" : "missing-data");
  }

  async function handleConfirmDownload() {
    if (!point || !downloadEstimate || !capabilities.canDownload || isBusy) return;
    const estimate = downloadEstimate;
    setDownloadEstimate(null);
    setDownloadProgress({
      assetIndex: 0,
      assetCount: estimate.requiredAssetCount,
      assetKey: "",
      assetDownloadedBytes: 0,
      assetExpectedBytes: 0,
      totalDownloadedBytes: 0,
      totalExpectedBytes: estimate.additionalDownloadBytes,
      percent: 0,
    });
    setWorkflow("downloading");
    setErrorMessage(null);
    try {
      const value = await downloadRegion(point);
      setInspection(value.inspection);
      setBootstrapInfo((current) =>
        current ? { ...current, cacheUsage: value.inspection.cacheUsage } : current,
      );
      setWorkflow("ready");
      setDownloadProgress(null);
      if (cacheOpen) void refreshCacheOverview();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (message.toLowerCase().includes("cancel") || message.includes("取消")) {
        try {
          const checked = await inspectPoint(point);
          setInspection(checked);
          setBootstrapInfo((current) =>
            current ? { ...current, cacheUsage: checked.cacheUsage } : current,
          );
          setWorkflow(checked.dataReady ? "ready" : "download-cancelled");
        } catch {
          setWorkflow("download-cancelled");
        }
      } else {
        setErrorMessage(message);
        setWorkflow("error");
      }
    }
  }

  async function handleDeleteRegion() {
    if (!deleteCandidate || !capabilities.canDeleteCache || isBusy || deletingRegionRef.current) return;
    deletingRegionRef.current = true;
    setDeletingRegion(true);
    setCacheError(null);
    try {
      const deleted = await deleteCacheRegion(deleteCandidate.regionId);
      setCacheOverview(deleted.overview);
      setBootstrapInfo((current) =>
        current ? { ...current, cacheUsage: deleted.overview.usage } : current,
      );
      setDeleteCandidate(null);
      if (point) {
        const checked = await inspectPoint(point);
        setInspection(checked);
        setBootstrapInfo((current) =>
          current ? { ...current, cacheUsage: checked.cacheUsage } : current,
        );
        setWorkflow(result ? "completed" : checked.dataReady ? "ready" : "missing-data");
      }
    } catch (error) {
      setCacheError(error instanceof Error ? error.message : String(error));
    } finally {
      deletingRegionRef.current = false;
      setDeletingRegion(false);
    }
  }

  function closeCacheModal() {
    if (deletingRegion) return;
    setDeleteCandidate(null);
    setCacheOpen(false);
  }

  async function handleCancellation(
    cancel: () => Promise<void>,
    actionLabel: string,
    clearCalculationPreview = false,
  ) {
    if (cancellationPendingRef.current) return;
    cancellationPendingRef.current = true;
    setCancellationPending(true);
    setErrorMessage(null);
    if (clearCalculationPreview) {
      previewSuppressedRef.current = true;
      setPreview(null);
    }
    try {
      await cancel();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setErrorMessage(`${actionLabel}\u5931\u8d25\uff1a${message}`);
    } finally {
      cancellationPendingRef.current = false;
      setCancellationPending(false);
    }
  }

  async function handleCalculate() {
    if (!point || !canCalculate) return;
    setWorkflow("calculating");
    setResult(null);
    setResultParameters(null);
    setActiveResultId(null);
    setResultStale(false);
    previewSuppressedRef.current = false;
    setPreview(null);
    setProgress({
      phase: "loading-data",
      percent: 0,
      completedPixelCount: 0,
      totalPixelCount: 0,
    });
    setErrorMessage(null);
    try {
      const value = await calculate(buildRequest(point, parameters));
      const parameterSnapshot = { ...parameters };
      const coverageId = `coverage-${++coverageSequenceRef.current}`;
      previewSuppressedRef.current = true;
      setResult(value);
      setResultParameters(parameterSnapshot);
      setSessionResults((current) => {
        return mergeSessionCoverage(current, {
          id: coverageId,
          result: value,
          parameters: parameterSnapshot,
          completedAt: Date.now(),
        });
      });
      setActiveResultId(coverageId);
      setPreview(null);
      setResultStale(false);
      setWorkflow("completed");
    } catch (error) {
      previewSuppressedRef.current = true;
      const message = error instanceof Error ? error.message : String(error);
      if (message.toLowerCase().includes("cancel")) {
        setResult(null);
        setResultParameters(null);
        setActiveResultId(null);
        setPreview(null);
        setWorkflow("cancelled");
      } else {
        setPreview(null);
        setErrorMessage(message);
        setWorkflow("error");
      }
    }
  }

  function openExportModal() {
    if (!result || resultStale || isBusy || !capabilities.canExport) return;
    setExportMessage(null);
    setExportError(null);
    setExportOpen(true);
  }

  function closeExportModal() {
    if (exportingRef.current) return;
    setExportOpen(false);
    setExportMessage(null);
    setExportError(null);
  }

  async function handleExport(format: ExportFormat) {
    if (!result || !resultParameters || resultStale || exportingRef.current || !capabilities.canExport) return;
    const resultSnapshot = result;
    const parameterSnapshot = { ...resultParameters };
    const generatedAt = new Date();
    exportingRef.current = true;
    setExportingFormat(format);
    setExportMessage(null);
    setExportError(null);
    try {
      const reportPngDataUrl = await createExportReportPngDataUrl(resultSnapshot, parameterSnapshot, generatedAt);
      const exported = await exportReport({
        format,
        suggestedFileName: suggestedExportFileName(resultSnapshot, parameterSnapshot, format, generatedAt),
        reportPngDataUrl,
      });
      if (exported.cancelled) {
        setExportMessage("已取消保存，没有写入文件。");
      } else {
        setExportMessage(
          exported.path
            ? `已保存 ${format.toUpperCase()} · ${formatBytes(exported.bytesWritten)}\n${exported.path}`
            : `已触发 ${format.toUpperCase()} 下载 · ${formatBytes(exported.bytesWritten)}`,
        );
      }
    } catch (error) {
      setExportError(error instanceof Error ? error.message : String(error));
    } finally {
      exportingRef.current = false;
      setExportingFormat(null);
    }
  }

  function handlePointSelect(value: MapPoint) {
    if (isBusy) return;
    const selectedNewPoint =
      point === null || point.lat !== value.lat || point.lon !== value.lon;
    previewSuppressedRef.current = true;
    setInspection(null);
    setWorkflow("inspecting");
    setPoint(value);
    if (selectedNewPoint) {
      setParameters((current) =>
        current.txGroundElevationOverrideM === null
          ? current
          : { ...current, txGroundElevationOverrideM: null },
      );
    }
    setResult(null);
    setResultParameters(null);
    setActiveResultId(null);
    setPreview(null);
    setResultStale(false);
    setProgress(null);
    setDownloadEstimate(null);
    setDownloadProgress(null);
    setExportOpen(false);
    setExportMessage(null);
    setExportError(null);
  }

  function handleParameterChange(value: RadioParameters) {
    previewSuppressedRef.current = true;
    setParameters(value);
    setPreview(null);
    if (result) setResultStale(true);
  }
  function resetMapProbeState() {
    setMapProbeResult(null);
    setMapProbeUnexpectedError(false);
  }

  function openMapSettings() {
    if (!desktopMode || isBusy) return;
    setMapToken("");
    setMapSettingsMessage(null);
    setMapSettingsError(null);
    resetMapProbeState();
    setMapSettingsOpen(true);
  }

  function closeMapSettings() {
    if (mapSettingsBusy) return;
    setMapToken("");
    setMapSettingsMessage(null);
    setMapSettingsError(null);
    resetMapProbeState();
    setMapSettingsOpen(false);
  }

  async function handleProbeOnlineBasemap() {
    if (
      mapSettingsBusy ||
      bootstrapInfo?.onlineBasemap?.configured !== true
    ) {
      return;
    }
    setMapSettingsAction("testing");
    setMapSettingsError(null);
    resetMapProbeState();
    try {
      setMapProbeResult(await probeOnlineBasemap());
    } catch {
      setMapProbeUnexpectedError(true);
    } finally {
      setMapSettingsAction(null);
    }
  }

  async function handleConfigureOnlineBasemap() {
    if (mapSettingsBusy) return;
    if (!mapToken.trim()) {
      setMapSettingsError("请输入天地图 tk。");
      return;
    }
    setMapSettingsAction("saving-and-testing");
    setMapSettingsMessage(null);
    setMapSettingsError(null);
    resetMapProbeState();
    try {
      const onlineBasemap = await configureOnlineBasemap(mapToken);
      setBootstrapInfo((current) =>
        current
          ? { ...current, basemap: undefined, onlineBasemap }
          : current,
      );
      setMapToken("");
      setMapSettingsMessage("配置已保存。");
      try {
        setMapProbeResult(await probeOnlineBasemap());
      } catch {
        setMapProbeUnexpectedError(true);
      }
    } catch {
      setMapSettingsError(
        "在线地图配置未保存。请确认 tk 格式；若缓存接近 2.5 GB，请先清理缓存；若 Windows 本地安全存储（DPAPI）暂不可用，请稍后重试或检查系统状态。",
      );
    } finally {
      setMapToken("");
      setMapSettingsAction(null);
    }
  }

  async function handleClearOnlineBasemap() {
    if (mapSettingsBusy) return;
    setMapSettingsAction("clearing");
    setMapToken("");
    setMapSettingsMessage(null);
    setMapSettingsError(null);
    resetMapProbeState();
    try {
      const onlineBasemap = await clearOnlineBasemap();
      setBootstrapInfo((current) =>
        current
          ? { ...current, basemap: undefined, onlineBasemap }
          : current,
      );
      setMapSettingsMessage("已清除在线地图配置，当前使用 WGS84 坐标网格。");
    } catch {
      setMapSettingsError("无法清除在线地图配置，请稍后重试。");
    } finally {
      setMapToken("");
      setMapSettingsAction(null);
    }
  }

  function handleClear() {
    if (isBusy) return;
    previewSuppressedRef.current = true;
    setResult(null);
    setResultParameters(null);
    setSessionResults([]);
    setActiveResultId(null);
    setPreview(null);
    setResultStale(false);
    setProgress(null);
    setErrorMessage(null);
    setWorkflow(inspection ? (inspection.dataReady ? "ready" : "missing-data") : "idle");
    setExportOpen(false);
    setExportMessage(null);
    setExportError(null);
  }

  const cacheUsage = inspection?.cacheUsage ?? bootstrapInfo?.cacheUsage;
  const displayedMetadataBytes = cacheUsage?.metadataBytes ?? 0;
  const trustedOnlineBasemap =
    desktopMode && isTrustedOnlineBasemap(bootstrapInfo?.onlineBasemap);
  const savedOnlineBasemap = bootstrapInfo?.onlineBasemap?.configured === true;
  const mapProbePresentation = mapProbeResult
    ? describeOnlineBasemapProbe(mapProbeResult.status)
    : null;
  const basemapStatus = trustedOnlineBasemap
    ? "已接入天地图在线矢量、中文地名及卫星影像；网络不可用时自动回退 WGS84 网格。"
    : isTrustedTiandituBasemap(bootstrapInfo?.basemap)
      ? "已接入天地图在线矢量、中文地名及卫星影像；网络不可用时自动回退 WGS84 网格。"
      : "未配置受信任的真实底图；当前只显示 WGS84 坐标网格。";
  const cachePercent = cacheUsage
    ? Math.min(100, (cacheUsage.totalBytes / cacheUsage.capBytes) * 100)
    : 0;

  return (
    <div className={`app-shell${validationServerMode ? " validation-server-mode" : ""}`}>
      <header className="app-header">
        <div className="brand-block">
          <div className="brand-mark" aria-hidden="true">
            <i />
            <i />
            <span />
          </div>
          <div>
            <h1>HamHeatmap</h1>
            <p>业余无线电传播范围预测</p>
          </div>
          <span className="version-chip">ALPHA 0.1</span>
        </div>
        <div className="header-meta">
          <div className="model-meta">
            <span>{bootstrapInfo?.modelName ?? "NTIA ITM Point-to-Point"}</span>
            <strong>200 km · 1 km/px</strong>
          </div>
          {desktopMode && (
            <button
              className="header-button"
              type="button"
              disabled={!capabilities.canConfigureOnlineBasemap || isBusy}
              onClick={openMapSettings}
            >
              <span className="button-icon">⌘</span>
              在线地图
            </button>
          )}
          <button
            className="header-button"
            type="button"
            disabled={isBusy}
            onClick={openCacheModal}
          >
            <span className="button-icon">▤</span>
            缓存
          </button>
          <button
            className="header-button"
            type="button"
            disabled={!capabilities.canExport || !result || resultStale || isBusy}
            onClick={openExportModal}
          >
            <span className="button-icon">⇩</span>
            导出
          </button>
          <div className="theme-switch" aria-label="界面主题">
            {(["system", "light", "dark"] as const).map((theme) => (
              <button
                type="button"
                key={theme}
                className={themePreference === theme ? "active" : ""}
                aria-pressed={themePreference === theme}
                title={theme === "system" ? "跟随系统" : theme === "light" ? "浅色" : "深色"}
                onClick={() => setThemePreference(theme)}
              >
                {theme === "system" ? "◐" : theme === "light" ? "☼" : "☾"}
              </button>
            ))}
          </div>
        </div>
      </header>

      <div className="compliance-banner">
        <strong>{bootstrapInfo?.internalBuildWarning ?? "内部测试底图，不得公开发布"}</strong>
        <span>{basemapStatus}</span>
      </div>

      {validationServerMode && (
        <div className="validation-server-banner" role="status">
          <strong>内部服务器验证</strong>
          <span>坐标、无线电参数和计算请求会发送到本服务器；诊断 PNG/PDF 由当前浏览器直接下载。</span>
        </div>
      )}

      <main className="workspace">
        <div className="map-column">
          <MapView
            theme={resolvedTheme}
            point={point}
            heatmaps={sessionResults}
            activeHeatmapId={activeResultId}
            preview={preview}
            heatmapStale={resultStale}
            visibleSignalThresholdDbm={visibleSignalThresholdDbm}
            onPointSelect={handlePointSelect}
            basemap={desktopMode ? null : (bootstrapInfo?.basemap ?? null)}
            onlineBasemap={desktopMode ? (bootstrapInfo?.onlineBasemap ?? null) : null}
          />
          <div className="legend-bar" aria-label="接收功率色标">
            <div className="legend-title">
              <span>预测接收功率</span>
              <strong>dBm</strong>
            </div>
            <div className="legend-scale">
              <div className="legend-track">
                <div className="legend-gradient" />
                <input
                  className="legend-threshold-slider"
                  type="range"
                  dir="rtl"
                  min={MIN_VISIBLE_DBM}
                  max={MAX_VISIBLE_DBM}
                  step={1}
                  value={visibleSignalThresholdDbm}
                  disabled={sessionResults.length === 0}
                  aria-label="最弱可见场强"
                  aria-valuetext={`${visibleSignalThresholdDbm} dBm 及以上`}
                  onChange={(event) =>
                    setVisibleSignalThresholdDbm(Number(event.currentTarget.value))
                  }
                />
              </div>
              <div className="legend-labels">
                <span>≥ -60</span>
                <span>-75</span>
                <span>-90</span>
                <span>-105</span>
                <span>-120</span>
                <span>-140</span>
              </div>
            </div>
            <div className="legend-filter-status" aria-live="polite">
              <strong>显示 ≥ {visibleSignalThresholdDbm} dBm</strong>
              <span>
                {sessionResults.length === 0 ? "生成热力图后可调" : "拖动时动态隐藏较弱信号"}
              </span>
            </div>
            <div className="legend-note-stack">
              <div className="legend-note">仅筛选地图显示，不改变计算与导出</div>
              <div className="legend-note">
                {sessionResults.length > 0
                  ? `会话结果 ${sessionResults.length} / ${MAX_SESSION_COVERAGES}`
                  : "< -140 dBm 透明"}
              </div>
              <div className="legend-note">颜色仅表示预测值，不保证实际通联</div>
            </div>
          </div>
        </div>

        <aside className="sidebar">
          <div className="sidebar-scroll">
            <ParameterPanel
              parameters={parameters}
              disabled={isCalculating || cancellationPending || exportingFormat !== null}
              elevationM={inspection?.elevationM ?? null}
              onChange={handleParameterChange}
            />
          </div>
          <div className="action-dock">
            <div className={`workflow-status ${status.tone}`}>
              <i />
              <div>
                <strong>{status.title}</strong>
                <span>{status.detail}</span>
              </div>
            </div>
            {(isCalculating || isDownloading) && (
              <div className="progress-track" aria-label={isDownloading ? "下载进度" : "计算进度"}>
                <span
                  style={{
                    width: `${Math.max(
                      0,
                      Math.min(
                        100,
                        isDownloading ? (downloadProgress?.percent ?? 0) : (progress?.percent ?? 0),
                      ),
                    )}%`,
                  }}
                />
              </div>
            )}
            {validationMessage && <p className="dock-validation">{validationMessage}</p>}
            <div className="action-row">
              {isCalculating ? (
                <button
                  type="button"
                  className="secondary-action danger"
                  disabled={cancellationPending}
                  onClick={() =>
                    void handleCancellation(cancelCalculation, "取消计算请求", true)
                  }
                >
                  {cancellationPending ? "正在取消…" : "取消计算"}
                </button>
              ) : isDownloading || isEstimatingDownload ? (
                <button
                  type="button"
                  className="secondary-action danger"
                  disabled={cancellationPending}
                  onClick={() =>
                    void handleCancellation(cancelDownload, "取消下载请求")
                  }
                >
                  {cancellationPending
                    ? "正在取消…"
                    : isEstimatingDownload
                      ? "取消数据检查"
                      : "取消下载"}
                </button>
              ) : (
                <button
                  type="button"
                  className="secondary-action"
                  disabled={isBusy}
                  onClick={handleClear}
                >
                  清空
                </button>
              )}
              <button
                type="button"
                className="primary-action"
                disabled={inspection?.dataReady ? !canCalculate : !canPrepareData}
                onClick={() =>
                  void (inspection?.dataReady ? handleCalculate() : handlePrepareData())
                }
              >
                <span>⌁</span>
                {inspection && !inspection.dataReady
                  ? capabilities.canDownload
                    ? "准备离线数据"
                    : "预览下载确认"
                  : resultStale
                    ? "重新计算"
                    : "开始计算"}
              </button>
            </div>
          </div>
        </aside>
      </main>

      {mapSettingsOpen && (
        <div className="modal-backdrop" role="presentation" onMouseDown={closeMapSettings}>
          <section
            className="online-map-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="online-map-title"
            onMouseDown={(event) => event.stopPropagation()}
          >
            <div className="modal-heading">
              <div>
                <span className="eyebrow">Windows 在线地图</span>
                <h2 id="online-map-title">配置天地图</h2>
              </div>
              <button
                type="button"
                aria-label="关闭在线地图设置"
                disabled={mapSettingsBusy}
                onClick={closeMapSettings}
              >
                ×
              </button>
            </div>
            <div className="online-map-status">
              <span>配置状态</span>
              <strong>{savedOnlineBasemap ? "已保存" : "未保存"}</strong>
            </div>
            <form
              className="online-map-form"
              onSubmit={(event) => {
                event.preventDefault();
                void handleConfigureOnlineBasemap();
              }}
            >
              <label htmlFor="online-map-token">天地图 tk</label>
              <input
                id="online-map-token"
                type="password"
                value={mapToken}
                autoComplete="new-password"
                spellCheck={false}
                disabled={mapSettingsBusy}
                placeholder={
                  savedOnlineBasemap
                    ? "输入新的 tk 以替换现有配置"
                    : "输入天地图控制台提供的 tk"
                }
                onChange={(event) => setMapToken(event.target.value)}
              />
              <p>
                已保存的 tk 不会回显，也不会写入浏览器存储。保存或关闭后，输入框会立即清空。
              </p>
              {mapSettingsMessage && <p className="online-map-message">{mapSettingsMessage}</p>}
              {mapSettingsError && <p className="online-map-error">{mapSettingsError}</p>}
              <div
                className={`online-map-probe ${mapProbePresentation?.tone ?? (mapProbeUnexpectedError ? "error" : "idle")}`}
                aria-live="polite"
              >
                <span>连接自检</span>
                {mapSettingsAction === "testing" ||
                mapSettingsAction === "saving-and-testing" ? (
                  <>
                    <strong>正在测试连接…</strong>
                    <p>正在请求一个小型地图瓦片，不会批量下载或写入地图缓存。</p>
                  </>
                ) : mapProbePresentation ? (
                  <>
                    <strong>{mapProbePresentation.title}</strong>
                    <p>{mapProbePresentation.detail}</p>
                  </>
                ) : mapProbeUnexpectedError ? (
                  <>
                    <strong>连接自检未完成</strong>
                    <p>请稍后重新测试；已保存的配置不会因此被删除。</p>
                  </>
                ) : (
                  <>
                    <strong>尚未测试</strong>
                    <p>只有点击“保存并测试”或“测试连接”时才会访问天地图服务。</p>
                  </>
                )}
              </div>
              <div className="modal-actions">
                <button
                  type="button"
                  disabled={mapSettingsBusy || !savedOnlineBasemap}
                  onClick={() => void handleClearOnlineBasemap()}
                >
                  {mapSettingsAction === "clearing" ? "正在清除…" : "清除配置"}
                </button>
                {savedOnlineBasemap && (
                  <button
                    type="button"
                    disabled={mapSettingsBusy}
                    onClick={() => void handleProbeOnlineBasemap()}
                  >
                    {mapSettingsAction === "testing"
                      ? "正在测试…"
                      : mapProbeResult || mapProbeUnexpectedError
                        ? "重新测试连接"
                        : "测试连接"}
                  </button>
                )}
                <button
                  type="submit"
                  className="confirm-download"
                  disabled={mapSettingsBusy || !mapToken.trim()}
                >
                  {mapSettingsAction === "saving-and-testing"
                    ? "正在保存并测试…"
                    : "保存并测试"}
                </button>
              </div>
            </form>
          </section>
        </div>
      )}
      {exportOpen && (
        <div className="modal-backdrop" role="presentation" onMouseDown={closeExportModal}>
          <section
            className="export-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="export-title"
            onMouseDown={(event) => event.stopPropagation()}
          >
            <div className="modal-heading">
              <div>
                <span className="eyebrow">本地文件</span>
                <h2 id="export-title">导出传播预测报告</h2>
              </div>
              <button
                type="button"
                aria-label="关闭导出"
                disabled={exportingFormat !== null}
                onClick={closeExportModal}
              >
                ×
              </button>
            </div>
            <p className="export-description">
              报告包含热力图、发射点、当前无线电参数、计算统计、固定 dBm 色标和免责声明，不包含行政边界或未授权底图。
            </p>
            <div className="export-options">
              <button
                type="button"
                disabled={exportingFormat !== null}
                onClick={() => void handleExport("png")}
              >
                <strong>PNG 图像</strong>
                <span>1600 × 1100 无损报告画布</span>
                <small>{exportingFormat === "png" ? "正在生成并等待保存…" : "适合分享与插入文档"}</small>
              </button>
              <button
                type="button"
                disabled={exportingFormat !== null}
                onClick={() => void handleExport("pdf")}
              >
                <strong>PDF 报告</strong>
                <span>A4 横向，内嵌同一报告画布</span>
                <small>{exportingFormat === "pdf" ? "正在生成并等待保存…" : "适合归档与打印"}</small>
              </button>
            </div>
            {exportMessage && <p className="export-message">{exportMessage}</p>}
            {exportError && <p className="export-error">{exportError}</p>}
            <p className="export-note">
              {validationServerMode
                ? "文件由当前浏览器直接下载；报告只使用本次已完成结果，不新增服务器文件。"
                : "文件只写入你在 Windows 原生保存对话框中选择的位置。坐标与结果不会上传。"}
            </p>
          </section>
        </div>
      )}

      {cacheOpen && (
        <div className="modal-backdrop" role="presentation" onMouseDown={closeCacheModal}>
          <section
            className="cache-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="cache-title"
            onMouseDown={(event) => event.stopPropagation()}
          >
            <div className="modal-heading">
              <div>
                <span className="eyebrow">离线数据</span>
                <h2 id="cache-title">缓存概览</h2>
              </div>
              <button type="button" aria-label="关闭缓存概览" onClick={closeCacheModal}>
                ×
              </button>
            </div>
            <div className="quota-number">
              <strong>{cacheUsage ? formatBytes(cacheUsage.totalBytes) : "读取中"}</strong>
              <span>/ 2.50 GB</span>
            </div>
            <div className="quota-track">
              <span style={{ width: `${cachePercent}%` }} />
            </div>
            <div className="cache-grid">
              <div><span>高程 DEM</span><strong>{formatBytes(cacheUsage?.demBytes ?? 0)}</strong></div>
              <div><span>水体 WBM</span><strong>{formatBytes(cacheUsage?.waterBytes ?? 0)}</strong></div>
              <div><span>临时下载</span><strong>{formatBytes(cacheUsage?.partialBytes ?? 0)}</strong></div>
              <div><span>索引与元数据</span><strong>{formatBytes(displayedMetadataBytes)}</strong></div>
            </div>
            <div className="cache-region-heading">
              <div>
                <strong>已登记离线区域</strong>
                <span>删除只回收未被其他区域共享的资产</span>
              </div>
              <button type="button" disabled={isBusy} onClick={() => void refreshCacheOverview()}>
                刷新
              </button>
            </div>
            <div className="cache-region-list">
              {cacheLoading && !cacheOverview && <p className="cache-empty">正在读取缓存索引…</p>}
              {!cacheLoading && (cacheOverview?.regions.length ?? 0) === 0 && (
                <p className="cache-empty">尚未登记离线区域。</p>
              )}
              {cacheOverview?.regions.map((region) => (
                <article className="cache-region" key={region.regionId}>
                  <div>
                    <strong>{region.center.lat.toFixed(4)}°, {region.center.lon.toFixed(4)}°</strong>
                    <span>
                      {region.readyAssetCount}/{region.assetCount} 个资产就绪
                      {region.partialAssetCount > 0 ? ` · ${region.partialAssetCount} 个可续传` : ""}
                    </span>
                    <small>
                      已引用 {formatBytes(region.referencedBytes)} · 删除可释放 {formatBytes(region.reclaimableBytes)}
                    </small>
                  </div>
                  <button
                    type="button"
                    disabled={!capabilities.canDeleteCache || isBusy}
                    onClick={() => setDeleteCandidate(region)}
                  >
                    删除
                  </button>
                </article>
              ))}
            </div>
            {deleteCandidate && (
              <div className="delete-confirm" role="alert">
                <strong>删除该离线区域？</strong>
                <span>
                  预计释放 {formatBytes(deleteCandidate.reclaimableBytes)}。共享数据会保留，操作不会删除当前内存中的热力图。
                </span>
                <div>
                  <button type="button" disabled={deletingRegion} onClick={() => setDeleteCandidate(null)}>
                    返回
                  </button>
                  <button type="button" disabled={deletingRegion} onClick={() => void handleDeleteRegion()}>
                    {deletingRegion ? "正在删除…" : "确认删除"}
                  </button>
                </div>
              </div>
            )}
            {cacheError && <p className="cache-error">{cacheError}</p>}
            <p className="cache-note">
              所有持久数据共享不可调整的十进制 2,500,000,000 字节上限。不会自动淘汰已准备区域；配额不足时请在此手动删除。
            </p>
          </section>
        </div>
      )}

      {downloadEstimate && (
        <div className="modal-backdrop" role="presentation" onMouseDown={dismissDownloadEstimate}>
          <section
            className="download-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="download-title"
            onMouseDown={(event) => event.stopPropagation()}
          >
            <div className="modal-heading">
              <div>
                <span className="eyebrow">首次区域准备</span>
                <h2 id="download-title">确认下载 DEM 与 WBM</h2>
              </div>
              <button type="button" aria-label="关闭下载确认" onClick={dismissDownloadEstimate}>
                ×
              </button>
            </div>
            <div className="download-size">
              <span>需要新增</span>
              <strong>{formatBytes(downloadEstimate.additionalDownloadBytes)}</strong>
            </div>
            <div className="download-facts">
              <div><span>地理单元</span><strong>{downloadEstimate.tileCount}</strong></div>
              <div><span>待准备资产</span><strong>{downloadEstimate.requiredAssetCount}</strong></div>
              <div><span>可续传数据</span><strong>{formatBytes(downloadEstimate.resumableBytes)}</strong></div>
              <div><span>预计总占用</span><strong>{formatBytes(downloadEstimate.projectedTotalBytes)}</strong></div>
            </div>
            {downloadEstimate.generatedAssetCount > 0 && (
              <p className="download-ocean-note">
                其中 {downloadEstimate.generatedAssetCount} 个资产属于官方对象成对缺失的纯海洋单元，将在本机确定性生成。
              </p>
            )}
            <p className="download-note">
              数据仅从固定的 Copernicus GLO-90 HTTPS 地址获取。取消后保留已校验资产和可续传临时文件；开始前会再次执行硬配额与磁盘检查。
            </p>
            {capabilities.mode === "preview" && (
              <p className="preview-callout">浏览器当前只展示确认流程，真实下载按钮仅在 Windows/Tauri 桌面版启用。</p>
            )}
            <div className="modal-actions">
              <button type="button" onClick={dismissDownloadEstimate}>稍后</button>
              <button
                type="button"
                className="confirm-download"
                disabled={!capabilities.canDownload || isBusy}
                onClick={() => void handleConfirmDownload()}
              >
                下载并准备
              </button>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}
