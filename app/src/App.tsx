// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { LinkParameterPanel } from "./components/LinkParameterPanel";
import { LinkProfileDialog } from "./components/LinkProfileDialog";
import { MapView } from "./components/MapView";
import { ParameterPanel } from "./components/ParameterPanel";
import {
  analyzeLink,
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
  isCancellationError,
  localizedBackendError,
  listenCalculationPreview,
  listenCalculationProgress,
  listenDownloadProgress,
  probeOnlineBasemap,
} from "./lib/backend";
import i18n, { currentAppLocale, setAppLocale } from "./i18n";
import { APP_LOCALES, LOCALE_NATIVE_NAMES, type AppLocale } from "./i18n/locale";
import {
  isTrustedOnlineBasemap,
  isTrustedCartoBasemap,
  isTrustedTiandituBasemap,
} from "./lib/basemap";
import { MAX_VISIBLE_DBM, MIN_VISIBLE_DBM } from "./lib/coverageVisibility";
import {
  createExportReportPngDataUrl,
  suggestedExportFileName,
} from "./lib/export";
import {
  buildLinkAnalysisRequest,
  DEFAULT_LINK_PARAMETERS,
  linkParameterValidationMessage,
} from "./lib/linkParameters";
import { inverseWgs84DistanceM } from "./lib/geodesy";
import { DEFAULT_PARAMETERS, parameterValidationMessage } from "./lib/parameters";
import {
  MAX_SESSION_COVERAGES,
  mergeSessionCoverage,
} from "./lib/sessionCoverages";
import type {
  AnalysisMode,
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
  LinkAnalysisResult,
  LinkParameters,
  LinkSelectionStage,
  MapPoint,
  OnlineBasemapProbeResult,
  PointInspection,
  RadioParameters,
  ResolvedTheme,
  SessionCoverageResult,
  ThemePreference,
  WorkflowState,
} from "./lib/types";

type LinkWorkflow =
  | "selecting"
  | "ready"
  | "inspecting"
  | "estimating-download"
  | "download-required"
  | "downloading"
  | "missing-data"
  | "download-cancelled"
  | "calculating"
  | "completed"
  | "cancelled"
  | "error";

const THEME_STORAGE_KEY = "hamheatmap-theme";
const LINK_MIN_DISTANCE_M = 1_000;
const LINK_MAX_DISTANCE_M = 200_000;
const LINK_DISTANCE_PREFLIGHT_TOLERANCE_M = 0.01;
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
  if (!progress) return i18n.t("phasePreparing");
  switch (progress.phase) {
    case "loading-data":
      return i18n.t("phaseLoading");
    case "computing":
      return i18n.t("phaseComputing", { completed: progress.completedPixelCount.toLocaleString(currentAppLocale()), total: progress.totalPixelCount.toLocaleString(currentAppLocale()) });
    case "encoding":
      return i18n.t("phaseEncoding");
    case "complete":
      return i18n.t("phaseComplete");
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
        title: i18n.t("probeReachableTitle"),
        detail: i18n.t("probeReachableDetail"),
      };
    case "not-configured":
      return {
        tone: "warning",
        title: i18n.t("probeNotConfiguredTitle"),
        detail: i18n.t("probeNotConfiguredDetail"),
      };
    case "network":
      return {
        tone: "error",
        title: i18n.t("probeNetworkTitle"),
        detail: i18n.t("probeNetworkDetail"),
      };
    case "timeout":
      return {
        tone: "warning",
        title: i18n.t("probeTimeoutTitle"),
        detail: i18n.t("probeTimeoutDetail"),
      };
    case "upstream-or-credential":
      return {
        tone: "warning",
        title: i18n.t("probeUpstreamTitle"),
        detail:
          i18n.t("probeUpstreamDetail"),
      };
    case "invalid-content":
      return {
        tone: "error",
        title: i18n.t("probeInvalidTitle"),
        detail: i18n.t("probeInvalidDetail"),
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
  const { t } = useTranslation();
  const locale = currentAppLocale();
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
  const [analysisMode, setAnalysisMode] = useState<AnalysisMode>("coverage");
  const [linkTx, setLinkTx] = useState<MapPoint | null>(null);
  const [linkRx, setLinkRx] = useState<MapPoint | null>(null);
  const [linkSelectionStage, setLinkSelectionStage] =
    useState<LinkSelectionStage>("tx");
  const [linkParameters, setLinkParameters] =
    useState<LinkParameters>(DEFAULT_LINK_PARAMETERS);
  const [linkWorkflow, setLinkWorkflow] = useState<LinkWorkflow>("selecting");
  const [linkResult, setLinkResult] = useState<LinkAnalysisResult | null>(null);
  const [linkResultStale, setLinkResultStale] = useState(false);
  const [linkProfileOpen, setLinkProfileOpen] = useState(false);
  const [linkProfileDimmed, setLinkProfileDimmed] = useState(false);
  const [linkError, setLinkError] = useState<string | null>(null);
  const [linkInspection, setLinkInspection] = useState<PointInspection | null>(null);
  const [linkInspectionLoading, setLinkInspectionLoading] = useState(false);
  const [downloadTargetMode, setDownloadTargetMode] =
    useState<AnalysisMode | null>(null);

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
        setErrorMessage(localizedBackendError(error));
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
        setErrorMessage(localizedBackendError(error));
        setWorkflow("error");
      });
    return () => {
      active = false;
    };
  }, [point]);

  useEffect(() => {
    let active = true;
    if (!linkTx) {
      setLinkInspection(null);
      setLinkInspectionLoading(false);
      return () => {
        active = false;
      };
    }
    setLinkInspection(null);
    setLinkInspectionLoading(true);
    setLinkError(null);
    setLinkWorkflow("inspecting");
    inspectPoint(linkTx)
      .then((value) => {
        if (!active) return;
        setLinkInspection(value);
        setBootstrapInfo((current) =>
          current ? { ...current, cacheUsage: value.cacheUsage } : current,
        );
        setLinkWorkflow(value.dataReady ? "ready" : "missing-data");
      })
      .catch((error: unknown) => {
        if (!active) return;
        setLinkError(localizedBackendError(error));
        setLinkWorkflow("error");
      })
      .finally(() => {
        if (active) setLinkInspectionLoading(false);
      });
    return () => {
      active = false;
    };
  }, [linkTx]);

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
  const linkValidationMessage = linkParameterValidationMessage(linkParameters);
  const linkDistanceM = useMemo(
    () =>
      linkTx && linkRx ? inverseWgs84DistanceM(linkTx, linkRx) : null,
    [linkRx, linkTx],
  );
  const linkDistanceInRange =
    linkDistanceM !== null &&
    linkDistanceM >=
      LINK_MIN_DISTANCE_M - LINK_DISTANCE_PREFLIGHT_TOLERANCE_M &&
    linkDistanceM <=
      LINK_MAX_DISTANCE_M + LINK_DISTANCE_PREFLIGHT_TOLERANCE_M;
  const linkDistanceValidationMessage =
    linkDistanceM !== null && !linkDistanceInRange
      ? t("linkDistanceRangeError", {
          distance: (linkDistanceM / 1000).toFixed(1),
        })
      : null;
  const isCalculating = workflow === "calculating";
  const isLinkCalculating = linkWorkflow === "calculating";
  const isDownloading = workflow === "downloading";
  const isEstimatingDownload = workflow === "estimating-download";
  const isLinkDownloading = linkWorkflow === "downloading";
  const isLinkEstimatingDownload = linkWorkflow === "estimating-download";
  const pointSelectionLocked =
    bootstrapLoading ||
    cacheLoading ||
    workflow === "inspecting" ||
    isEstimatingDownload ||
    isLinkEstimatingDownload ||
    isDownloading ||
    isLinkDownloading ||
    isCalculating ||
    isLinkCalculating ||
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
  const canPrepareLinkData = Boolean(
    linkTx &&
      linkRx &&
      linkDistanceInRange &&
      linkInspection &&
      !linkInspection.dataReady &&
      !isBusy &&
      linkWorkflow !== "download-required",
  );
  const canAnalyzeLink = Boolean(
    capabilities.canAnalyzeLink &&
      linkTx &&
      linkRx &&
      linkDistanceInRange &&
      linkInspection?.dataReady &&
      !linkValidationMessage &&
      !isBusy,
  );

  const status = useMemo(() => {
    if (bootstrapLoading) {
      return {
        tone: "working",
        title: t("statusInitializing"),
        detail: t("statusInitializingDetail"),
      };
    }
    switch (workflow) {
      case "idle":
        return { tone: "neutral", title: t("statusIdle"), detail: t("statusIdleDetail") };
      case "inspecting":
        return { tone: "working", title: t("statusInspecting"), detail: t("statusInspectingDetail") };
      case "estimating-download":
        return {
          tone: "working",
          title: t("statusEstimating"),
          detail: t("statusEstimatingDetail"),
        };
      case "download-required":
        return {
          tone: "warning",
          title: t("statusDownloadRequired"),
          detail: downloadEstimate
            ? t("statusDownloadBytes", { bytes: formatBytes(downloadEstimate.additionalDownloadBytes) })
            : t("statusDownloadDetail"),
        };
      case "downloading":
        return {
          tone: "working",
          title: t("statusDownloading"),
          detail: downloadProgress
            ? t("statusDownloadingProgress", { downloaded: formatBytes(downloadProgress.totalDownloadedBytes), expected: formatBytes(downloadProgress.totalExpectedBytes), index: downloadProgress.assetIndex, count: downloadProgress.assetCount })
            : t("statusDownloadingDetail"),
        };
      case "ready":
        return { tone: "ready", title: t("statusReady"), detail: t("statusReadyDetail") };
      case "missing-data":
        return capabilities.mode !== "preview"
          ? {
              tone: "warning",
              title: t("statusMissing"),
              detail: t("statusMissingDetail", { count: inspection?.missingAssetCount ?? 0 }),
            }
          : {
              tone: "warning",
              title: t("statusPreview"),
              detail: t("statusPreviewDetail"),
            };
      case "calculating":
        return { tone: "working", title: t("statusCalculating"), detail: phaseLabel(progress) };
      case "completed":
        return {
          tone: "ready",
          title: resultStale ? t("statusStale") : t("statusCompleted"),
          detail: result
            ? t("statusCompletedStats", { pixels: result.statistics.validPixelCount.toLocaleString(locale), seconds: result.statistics.totalSeconds.toFixed(1) })
            : t("statusCompletedDetail"),
        };
      case "cancelled":
        return { tone: "neutral", title: t("statusCancelled"), detail: t("statusCancelledDetail") };
      case "download-cancelled":
        return {
          tone: "neutral",
          title: t("statusDownloadCancelled"),
          detail: t("statusDownloadCancelledDetail"),
        };
      case "error":
        return { tone: "error", title: t("statusError"), detail: errorMessage ?? t("unknownError") };
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
    locale,
    t,
  ]);

  const linkStatus = useMemo(() => {
    if (linkWorkflow === "inspecting" || linkInspectionLoading) {
      return {
        tone: "working",
        title: t("statusInspecting"),
        detail: t("statusInspectingDetail"),
      };
    }
    if (linkDistanceValidationMessage) {
      return {
        tone: "error",
        title: t("linkDistanceInvalidTitle"),
        detail: linkDistanceValidationMessage,
      };
    }
    if (linkWorkflow === "estimating-download") {
      return {
        tone: "working",
        title: t("statusEstimating"),
        detail: t("statusEstimatingDetail"),
      };
    }
    if (linkWorkflow === "download-required") {
      return {
        tone: "warning",
        title: t("statusDownloadRequired"),
        detail: downloadEstimate
          ? t("statusDownloadBytes", {
              bytes: formatBytes(downloadEstimate.additionalDownloadBytes),
            })
          : t("statusDownloadDetail"),
      };
    }
    if (linkWorkflow === "downloading") {
      return {
        tone: "working",
        title: t("statusDownloading"),
        detail: downloadProgress
          ? t("statusDownloadingProgress", {
              downloaded: formatBytes(downloadProgress.totalDownloadedBytes),
              expected: formatBytes(downloadProgress.totalExpectedBytes),
              index: downloadProgress.assetIndex,
              count: downloadProgress.assetCount,
            })
          : t("statusDownloadingDetail"),
      };
    }
    if (linkWorkflow === "missing-data") {
      return capabilities.mode !== "preview"
        ? {
            tone: "warning",
            title: t("statusMissing"),
            detail: t("statusMissingDetail", {
              count: linkInspection?.missingAssetCount ?? 0,
            }),
          }
        : {
            tone: "warning",
            title: t("statusPreview"),
            detail: t("statusPreviewDetail"),
          };
    }
    if (linkWorkflow === "download-cancelled") {
      return {
        tone: "neutral",
        title: t("statusDownloadCancelled"),
        detail: t("statusDownloadCancelledDetail"),
      };
    }
    if (linkWorkflow === "calculating") {
      return {
        tone: "working",
        title: t("statusLinkCalculating"),
        detail: t("statusLinkCalculatingDetail"),
      };
    }
    if (linkWorkflow === "completed" && linkResult) {
      const title =
        linkResult.classification === "direct-los"
          ? t("linkClassDirect")
          : linkResult.classification === "obstructed-usable"
            ? t("linkClassObstructed")
            : t("linkClassUnavailable");
      return {
        tone:
          linkResult.classification === "direct-los"
            ? "ready"
            : linkResult.classification === "obstructed-usable"
              ? "warning"
              : "error",
        title,
        detail: t("statusLinkCompletedDetail", {
          power: linkResult.predictedRxPowerDbm.toFixed(1),
          margin: linkResult.linkMarginDb.toFixed(1),
        }),
      };
    }
    if (linkWorkflow === "error") {
      return {
        tone: "error",
        title: t("statusError"),
        detail: linkError ?? t("unknownError"),
      };
    }
    if (linkWorkflow === "cancelled") {
      return {
        tone: "neutral",
        title: t("statusCancelled"),
        detail: t("statusLinkCancelledDetail"),
      };
    }
    if (!linkTx || linkSelectionStage === "tx") {
      return {
        tone: "neutral",
        title: t("selectLinkTx"),
        detail: t("selectLinkTxDetail"),
      };
    }
    if (!linkRx || linkSelectionStage === "rx") {
      return {
        tone: "working",
        title: t("selectLinkRx"),
        detail: t("selectLinkRxDetail"),
      };
    }
    return {
      tone: linkResultStale ? "warning" : "ready",
      title: linkResultStale ? t("statusStale") : t("statusLinkReady"),
      detail: linkResultStale
        ? t("statusLinkStaleDetail")
        : t("statusLinkReadyDetail"),
    };
  }, [
    capabilities.mode,
    downloadEstimate,
    downloadProgress,
    linkError,
    linkInspection,
    linkInspectionLoading,
    linkDistanceValidationMessage,
    linkResult,
    linkResultStale,
    linkRx,
    linkSelectionStage,
    linkTx,
    linkWorkflow,
    t,
  ]);
  const activeStatus = analysisMode === "coverage" ? status : linkStatus;
  const activeValidationMessage =
    analysisMode === "coverage"
      ? validationMessage
      : linkValidationMessage ?? linkDistanceValidationMessage;

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
      setCacheError(localizedBackendError(error));
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

  async function handlePrepareData(targetMode: AnalysisMode = analysisMode) {
    const targetPoint = targetMode === "coverage" ? point : linkTx;
    const targetInspection =
      targetMode === "coverage" ? inspection : linkInspection;
    const allowed =
      targetMode === "coverage" ? canPrepareData : canPrepareLinkData;
    if (!targetPoint || !targetInspection || !allowed) return;

    setDownloadTargetMode(targetMode);
    if (targetMode === "coverage") {
      setWorkflow("estimating-download");
      setErrorMessage(null);
    } else {
      setLinkWorkflow("estimating-download");
      setLinkError(null);
    }
    setDownloadProgress(null);
    try {
      const value = await estimateDownload(targetPoint);
      setDownloadEstimate(value);
      setBootstrapInfo((current) =>
        current ? { ...current, cacheUsage: value.cacheUsage } : current,
      );
      if (targetMode === "coverage") setWorkflow("download-required");
      else setLinkWorkflow("download-required");
    } catch (error) {
      setDownloadEstimate(null);
      setDownloadProgress(null);
      setDownloadTargetMode(null);
      if (isCancellationError(error)) {
        if (targetMode === "coverage") {
          setErrorMessage(null);
          setWorkflow(
            targetInspection.dataReady ? "ready" : "download-cancelled",
          );
        } else {
          setLinkError(null);
          setLinkWorkflow(
            targetInspection.dataReady ? "ready" : "download-cancelled",
          );
        }
      } else if (targetMode === "coverage") {
        setErrorMessage(localizedBackendError(error));
        setWorkflow("error");
      } else {
        setLinkError(localizedBackendError(error));
        setLinkWorkflow("error");
      }
    }
  }

  function dismissDownloadEstimate() {
    const targetMode = downloadTargetMode ?? analysisMode;
    setDownloadEstimate(null);
    setDownloadTargetMode(null);
    if (targetMode === "coverage") {
      setWorkflow(inspection?.dataReady ? "ready" : "missing-data");
    } else {
      setLinkWorkflow(linkInspection?.dataReady ? "ready" : "missing-data");
    }
  }

  async function handleConfirmDownload() {
    if (!downloadEstimate || !capabilities.canDownload || isBusy) return;
    const estimate = downloadEstimate;
    const targetMode = downloadTargetMode ?? analysisMode;
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
    if (targetMode === "coverage") {
      setWorkflow("downloading");
      setErrorMessage(null);
    } else {
      setLinkWorkflow("downloading");
      setLinkError(null);
    }
    try {
      const value = await downloadRegion(estimate.point);
      if (targetMode === "coverage") {
        setInspection(value.inspection);
        setWorkflow("ready");
      } else {
        setLinkInspection(value.inspection);
        setLinkWorkflow("ready");
      }
      setBootstrapInfo((current) =>
        current
          ? { ...current, cacheUsage: value.inspection.cacheUsage }
          : current,
      );
      setDownloadProgress(null);
      setDownloadTargetMode(null);
      if (cacheOpen) void refreshCacheOverview();
    } catch (error) {
      if (isCancellationError(error)) {
        try {
          const checked = await inspectPoint(estimate.point);
          if (targetMode === "coverage") {
            setInspection(checked);
            setWorkflow(checked.dataReady ? "ready" : "download-cancelled");
          } else {
            setLinkInspection(checked);
            setLinkWorkflow(
              checked.dataReady ? "ready" : "download-cancelled",
            );
          }
          setBootstrapInfo((current) =>
            current ? { ...current, cacheUsage: checked.cacheUsage } : current,
          );
        } catch {
          if (targetMode === "coverage") setWorkflow("download-cancelled");
          else setLinkWorkflow("download-cancelled");
        }
      } else if (targetMode === "coverage") {
        setErrorMessage(localizedBackendError(error));
        setWorkflow("error");
      } else {
        setLinkError(localizedBackendError(error));
        setLinkWorkflow("error");
      }
      setDownloadTargetMode(null);
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

      const refreshedInspections = new Map<string, PointInspection>();
      const refreshInspection = async (target: MapPoint) => {
        const key = target.lat + "," + target.lon;
        const existing = refreshedInspections.get(key);
        if (existing) return existing;
        const checked = await inspectPoint(target);
        refreshedInspections.set(key, checked);
        setBootstrapInfo((current) =>
          current ? { ...current, cacheUsage: checked.cacheUsage } : current,
        );
        return checked;
      };

      if (point) {
        setWorkflow("inspecting");
        try {
          const checked = await refreshInspection(point);
          setInspection(checked);
          setWorkflow(
            checked.dataReady
              ? result
                ? "completed"
                : "ready"
              : "missing-data",
          );
        } catch (error) {
          const message = localizedBackendError(error);
          setInspection(null);
          setErrorMessage(message);
          setWorkflow("error");
          setCacheError(message);
        }
      }

      if (linkTx) {
        setLinkInspectionLoading(true);
        setLinkWorkflow("inspecting");
        try {
          const checked = await refreshInspection(linkTx);
          setLinkInspection(checked);
          setLinkWorkflow(
            checked.dataReady
              ? linkResult
                ? "completed"
                : linkRx
                  ? "ready"
                  : "selecting"
              : "missing-data",
          );
        } catch (error) {
          const message = localizedBackendError(error);
          setLinkInspection(null);
          setLinkError(message);
          setLinkWorkflow("error");
          setCacheError(message);
        } finally {
          setLinkInspectionLoading(false);
        }
      }
    } catch (error) {
      setCacheError(localizedBackendError(error));
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
      const message = localizedBackendError(error, locale);
      setErrorMessage(t("cancellationFailed", { action: actionLabel, message }));
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
      if (isCancellationError(error)) {
        setResult(null);
        setResultParameters(null);
        setActiveResultId(null);
        setPreview(null);
        setWorkflow("cancelled");
      } else {
        setPreview(null);
        setErrorMessage(localizedBackendError(error));
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
    const exportLocale = locale;
    const exportT = i18n.getFixedT(exportLocale);
    exportingRef.current = true;
    setExportingFormat(format);
    setExportMessage(null);
    setExportError(null);
    try {
      const reportPngDataUrl = await createExportReportPngDataUrl(resultSnapshot, parameterSnapshot, generatedAt, exportLocale);
      const exported = await exportReport({
        format,
        suggestedFileName: suggestedExportFileName(resultSnapshot, parameterSnapshot, format, generatedAt),
        reportPngDataUrl,
      }, exportLocale);
      if (exported.cancelled) {
        setExportMessage(exportT("exportCancelled"));
      } else {
        setExportMessage(
          exported.path
            ? exportT("exportSaved", { format: format.toUpperCase(), bytes: formatBytes(exported.bytesWritten), path: exported.path })
            : exportT("exportDownloaded", { format: format.toUpperCase(), bytes: formatBytes(exported.bytesWritten) }),
        );
      }
    } catch (error) {
      setExportError(localizedBackendError(error, exportLocale));
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
  function handleLinkPointSelect(value: MapPoint) {
    if (isBusy) return;
    if (linkSelectionStage === "tx") {
      setLinkTx(value);
      setLinkRx(null);
      setLinkSelectionStage("rx");
      setLinkWorkflow("selecting");
    } else if (linkSelectionStage === "rx") {
      if (linkTx && linkTx.lat === value.lat && linkTx.lon === value.lon) {
        setLinkError(t("linkSamePointError"));
        return;
      }
      setLinkRx(value);
      setLinkSelectionStage("ready");
      setLinkWorkflow("ready");
    } else {
      return;
    }
    setLinkResult(null);
    setLinkResultStale(false);
    setLinkProfileOpen(false);
    setLinkProfileDimmed(false);
    setLinkError(null);
  }

  function handleMapPointSelect(value: MapPoint) {
    if (analysisMode === "coverage") handlePointSelect(value);
    else handleLinkPointSelect(value);
  }

  function handleLinkParameterChange(value: LinkParameters) {
    setLinkParameters(value);
    if (linkResult) setLinkResultStale(true);
  }

  function selectLinkTransmitter() {
    if (isBusy) return;
    setLinkTx(null);
    setLinkRx(null);
    setLinkResult(null);
    setLinkResultStale(false);
    setLinkProfileOpen(false);
    setLinkProfileDimmed(false);
    setLinkError(null);
    setLinkSelectionStage("tx");
    setLinkWorkflow("selecting");
  }

  function selectLinkReceiver() {
    if (isBusy || !linkTx) return;
    setLinkRx(null);
    setLinkResult(null);
    setLinkResultStale(false);
    setLinkProfileOpen(false);
    setLinkProfileDimmed(false);
    setLinkError(null);
    setLinkSelectionStage("rx");
    setLinkWorkflow("selecting");
  }

  function handleClearLink() {
    if (isBusy) return;
    selectLinkTransmitter();
  }

  async function handleAnalyzeLink() {
    if (!linkTx || !linkRx || !canAnalyzeLink) return;
    setLinkWorkflow("calculating");
    setProgress(null);
    setLinkResult(null);
    setLinkResultStale(false);
    setLinkProfileOpen(false);
    setLinkProfileDimmed(false);
    setLinkError(null);
    try {
      const value = await analyzeLink(
        buildLinkAnalysisRequest(linkTx, linkRx, linkParameters),
      );
      setLinkResult(value);
      setProgress(null);
      setLinkProfileOpen(true);
      setLinkProfileDimmed(false);
      setLinkWorkflow("completed");
    } catch (error) {
      setProgress(null);
      if (isCancellationError(error)) {
        setLinkWorkflow("cancelled");
      } else {
        setLinkError(localizedBackendError(error));
        setLinkWorkflow("error");
      }
    }
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
      setMapSettingsError(t("errorEnterToken"));
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
          ? { ...current, onlineBasemap }
          : current,
      );
      setMapToken("");
      setMapSettingsMessage(t("configurationSaved"));
      try {
        setMapProbeResult(await probeOnlineBasemap());
      } catch {
        setMapProbeUnexpectedError(true);
      }
    } catch {
      setMapSettingsError(
        t("saveConfigurationError"),
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
          ? { ...current, onlineBasemap }
          : current,
      );
      setMapSettingsMessage(t("configurationCleared"));
    } catch {
      setMapSettingsError(t("clearConfigurationError"));
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
  const basemapStatus =
    trustedOnlineBasemap || isTrustedTiandituBasemap(bootstrapInfo?.basemap)
      ? t("basemapConnected")
      : isTrustedCartoBasemap(bootstrapInfo?.basemap)
        ? t("basemapPublicConnected")
        : t("basemapUnconfigured");
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
            <p>{t("appTagline")}</p>
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
              {t("onlineMap")}
            </button>
          )}
          <button
            className="header-button"
            type="button"
            disabled={isBusy}
            onClick={openCacheModal}
          >
            <span className="button-icon">▤</span>
            {t("cache")}
          </button>
          <button
            className="header-button"
            type="button"
            disabled={
              analysisMode !== "coverage" ||
              !capabilities.canExport ||
              !result ||
              resultStale ||
              isBusy
            }
            onClick={openExportModal}
          >
            <span className="button-icon">⇩</span>
            {t("export")}
          </button>
          <label className="language-select">
            <span className="sr-only">{t("languageLabel")}</span>
            <select
              aria-label={t("languageLabel")}
              value={locale}
              disabled={isBusy}
              onChange={(event) => void setAppLocale(event.target.value as AppLocale)}
            >
              {APP_LOCALES.map((item) => (
                <option key={item} value={item}>{LOCALE_NATIVE_NAMES[item]}</option>
              ))}
            </select>
          </label>
          <div className="theme-switch" aria-label={t("themeGroup")} >
            {(["system", "light", "dark"] as const).map((theme) => (
              <button
                type="button"
                key={theme}
                className={themePreference === theme ? "active" : ""}
                aria-pressed={themePreference === theme}
                title={theme === "system" ? t("themeSystem") : theme === "light" ? t("themeLight") : t("themeDark")}
                onClick={() => setThemePreference(theme)}
              >
                {theme === "system" ? "◐" : theme === "light" ? "☼" : "☾"}
              </button>
            ))}
          </div>
        </div>
      </header>

      <div className="compliance-banner">
        <strong>{t("internalWarning")}</strong>
        <span>{basemapStatus}</span>
      </div>

      {validationServerMode && (
        <div className="validation-server-banner" role="status">
          <strong>{t("validationServerTitle")}</strong>
          <span>{t("validationServerDetail")}</span>
        </div>
      )}

      <div className="analysis-mode-switch" role="tablist" aria-label={t("analysisMode")}>
        <button
          type="button"
          role="tab"
          aria-selected={analysisMode === "coverage"}
          className={analysisMode === "coverage" ? "active" : ""}
          disabled={isBusy}
          onClick={() => setAnalysisMode("coverage")}
        >
          {t("coverageAnalysis")}
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={analysisMode === "link"}
          className={analysisMode === "link" ? "active" : ""}
          disabled={isBusy}
          onClick={() => setAnalysisMode("link")}
        >
          {t("linkAnalysis")}
        </button>
      </div>

      <main className={`workspace ${analysisMode === "link" ? "link-mode" : "coverage-mode"}`}>
        <div className="map-column">
          <MapView
            theme={resolvedTheme}
            point={point}
            analysisMode={analysisMode}
            linkTx={linkTx}
            linkRx={linkRx}
            linkResult={linkResult}
            heatmaps={sessionResults}
            activeHeatmapId={activeResultId}
            preview={preview}
            heatmapStale={resultStale}
            visibleSignalThresholdDbm={visibleSignalThresholdDbm}
            onPointSelect={handleMapPointSelect}
            basemap={bootstrapInfo?.basemap ?? null}
            onlineBasemap={desktopMode ? (bootstrapInfo?.onlineBasemap ?? null) : null}
          />
          {analysisMode === "coverage" ? (
          <div className="legend-bar" aria-label={t("legendAria")}>
            <div className="legend-title">
              <span>{t("predictedPower")}</span>
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
                  aria-label={t("weakestVisible")}
                  aria-valuetext={t("weakestValue", { value: visibleSignalThresholdDbm })}
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
              <strong>{t("showThreshold", { value: visibleSignalThresholdDbm })}</strong>
              <span>
                {sessionResults.length === 0 ? t("adjustAfterHeatmap") : t("hideWeaker")}
              </span>
            </div>
            <div className="legend-note-stack">
              <div className="legend-note">{t("displayOnly")}</div>
              <div className="legend-note">
                {sessionResults.length > 0
                  ? t("sessionResults", { count: sessionResults.length, max: MAX_SESSION_COVERAGES })
                   : t("transparentBelow")}
              </div>
              <div className="legend-note">{t("colorDisclaimer")}</div>
            </div>
          </div>
          ) : null}
        </div>

        <aside className="sidebar">
          <div className="sidebar-scroll">
            {analysisMode === "coverage" ? (
              <ParameterPanel
                parameters={parameters}
                disabled={isCalculating || cancellationPending || exportingFormat !== null}
                elevationM={inspection?.elevationM ?? null}
                onChange={handleParameterChange}
              />
            ) : (
              <>
                <div className="link-selection-actions">
                  <button
                    type="button"
                    disabled={isBusy}
                    onClick={selectLinkTransmitter}
                  >
                    {t("reselectLinkTx")}
                  </button>
                  <button
                    type="button"
                    disabled={isBusy || !linkTx}
                    onClick={selectLinkReceiver}
                  >
                    {t("reselectLinkRx")}
                  </button>
                  {linkResult && (
                    <button
                      type="button"
                      className="link-profile-open-action"
                      onClick={() => {
                        setLinkProfileOpen(true);
                        setLinkProfileDimmed(false);
                      }}
                    >
                      {t("openLinkProfile")}
                    </button>
                  )}
                </div>
                <LinkParameterPanel
                  parameters={linkParameters}
                  disabled={isLinkCalculating || cancellationPending}
                  onChange={handleLinkParameterChange}
                />
              </>
            )}
          </div>
          <div className="action-dock">
            <div className={`workflow-status ${activeStatus.tone}`}>
              <i />
              <div>
                <strong>{activeStatus.title}</strong>
                <span>{activeStatus.detail}</span>
              </div>
            </div>
            {(isCalculating ||
              isLinkCalculating ||
              isDownloading ||
              isLinkDownloading) && (
              <div
                className={"progress-track" + (isLinkCalculating ? " indeterminate" : "")}
                aria-label={
                  isDownloading || isLinkDownloading
                    ? t("downloadProgress")
                    : t("calculationProgress")
                }
              >
                <span
                  style={{
                    width: `${Math.max(
                      0,
                      Math.min(
                        100,
                        isDownloading || isLinkDownloading
                          ? (downloadProgress?.percent ?? 0)
                          : (progress?.percent ?? 0),
                      ),
                    )}%`,
                  }}
                />
              </div>
            )}
            {activeValidationMessage && (
              <p className="dock-validation">{activeValidationMessage}</p>
            )}
            <div className="action-row">
              {isLinkCalculating ? (
                <button
                  type="button"
                  className="secondary-action danger"
                  disabled={cancellationPending}
                  onClick={() =>
                    void handleCancellation(cancelCalculation, t("cancelLinkRequest"))
                  }
                >
                  {cancellationPending ? t("cancelling") : t("cancelLinkAnalysis")}
                </button>
              ) : isCalculating ? (
                <button
                  type="button"
                  className="secondary-action danger"
                  disabled={cancellationPending}
                  onClick={() =>
                    void handleCancellation(cancelCalculation, t("cancelCalculationRequest"), true)
                  }
                >
                  {cancellationPending ? t("cancelling") : t("cancelCalculation")}
                </button>
              ) : isDownloading ||
                isEstimatingDownload ||
                isLinkDownloading ||
                isLinkEstimatingDownload ? (
                <button
                  type="button"
                  className="secondary-action danger"
                  disabled={cancellationPending}
                  onClick={() =>
                    void handleCancellation(cancelDownload, t("cancelDownloadRequest"))
                  }
                >
                  {cancellationPending
                    ? t("cancelling")
                    : isEstimatingDownload || isLinkEstimatingDownload
                      ? t("cancelDataCheck")
                      : t("cancelDownload")}
                </button>
              ) : (
                <button
                  type="button"
                  className="secondary-action"
                  disabled={isBusy}
                  onClick={analysisMode === "coverage" ? handleClear : handleClearLink}
                >
                  {analysisMode === "coverage" ? t("clear") : t("clearLink")}
                </button>
              )}
              <button
                type="button"
                className="primary-action"
                disabled={
                  analysisMode === "link"
                    ? linkInspection?.dataReady
                      ? !canAnalyzeLink
                      : !canPrepareLinkData
                    : inspection?.dataReady
                      ? !canCalculate
                      : !canPrepareData
                }
                onClick={() =>
                  void (
                    analysisMode === "link"
                      ? linkInspection?.dataReady
                        ? handleAnalyzeLink()
                        : handlePrepareData("link")
                      : inspection?.dataReady
                        ? handleCalculate()
                        : handlePrepareData("coverage")
                  )
                }
              >
                <span>⌁</span>
                {analysisMode === "link"
                  ? linkInspection && !linkInspection.dataReady
                    ? capabilities.canDownload
                      ? t("prepareOfflineData")
                      : t("previewDownload")
                    : linkResultStale
                      ? t("reanalyzeLink")
                      : t("startLinkAnalysis")
                  : inspection && !inspection.dataReady
                    ? capabilities.canDownload
                      ? t("prepareOfflineData")
                      : t("previewDownload")
                    : resultStale
                      ? t("recalculate")
                      : t("startCalculation")}
              </button>
            </div>
          </div>
        </aside>
      </main>

      {analysisMode === "link" && linkResult && linkProfileOpen && (
        <LinkProfileDialog
          result={linkResult}
          dimmed={linkProfileDimmed}
          onActivate={() => setLinkProfileDimmed(false)}
          onInteractOutside={() => setLinkProfileDimmed(true)}
          onClose={() => {
            setLinkProfileOpen(false);
            setLinkProfileDimmed(false);
          }}
        />
      )}

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
                <span className="eyebrow">{t("mapSettingsEyebrow")}</span>
                <h2 id="online-map-title">{t("configureTianditu")}</h2>
              </div>
              <button
                type="button"
                aria-label={t("closeMapSettings")}
                disabled={mapSettingsBusy}
                onClick={closeMapSettings}
              >
                ×
              </button>
            </div>
            <div className="online-map-status">
              <span>{t("configurationStatus")}</span>
              <strong>{savedOnlineBasemap ? t("saved") : t("notSaved")}</strong>
            </div>
            <form
              className="online-map-form"
              onSubmit={(event) => {
                event.preventDefault();
                void handleConfigureOnlineBasemap();
              }}
            >
              <label htmlFor="online-map-token">{t("tiandituKey")}</label>
              <input
                id="online-map-token"
                type="password"
                value={mapToken}
                autoComplete="new-password"
                spellCheck={false}
                disabled={mapSettingsBusy}
                placeholder={
                  savedOnlineBasemap
                    ? t("tokenReplace")
                    : t("tokenPlaceholder")
                }
                onChange={(event) => setMapToken(event.target.value)}
              />
              <p>
                {t("tokenPrivacy")}
              </p>
              {mapSettingsMessage && <p className="online-map-message">{mapSettingsMessage}</p>}
              {mapSettingsError && <p className="online-map-error">{mapSettingsError}</p>}
              <div
                className={`online-map-probe ${mapProbePresentation?.tone ?? (mapProbeUnexpectedError ? "error" : "idle")}`}
                aria-live="polite"
              >
                <span>{t("connectionTest")}</span>
                {mapSettingsAction === "testing" ||
                mapSettingsAction === "saving-and-testing" ? (
                  <>
                    <strong>{t("testingConnection")}</strong>
                    <p>{t("testingConnectionDetail")}</p>
                  </>
                ) : mapProbePresentation ? (
                  <>
                    <strong>{mapProbePresentation.title}</strong>
                    <p>{mapProbePresentation.detail}</p>
                  </>
                ) : mapProbeUnexpectedError ? (
                  <>
                    <strong>{t("probeIncomplete")}</strong>
                    <p>{t("probeIncompleteDetail")}</p>
                  </>
                ) : (
                  <>
                    <strong>{t("notTested")}</strong>
                    <p>{t("notTestedDetail")}</p>
                  </>
                )}
              </div>
              <div className="modal-actions">
                <button
                  type="button"
                  disabled={mapSettingsBusy || !savedOnlineBasemap}
                  onClick={() => void handleClearOnlineBasemap()}
                >
                  {mapSettingsAction === "clearing" ? t("clearing") : t("clearConfiguration")}
                </button>
                {savedOnlineBasemap && (
                  <button
                    type="button"
                    disabled={mapSettingsBusy}
                    onClick={() => void handleProbeOnlineBasemap()}
                  >
                    {mapSettingsAction === "testing"
                      ? t("testingAgain")
                      : mapProbeResult || mapProbeUnexpectedError
                        ? t("testAgain")
                        : t("testConnection")}
                  </button>
                )}
                <button
                  type="submit"
                  className="confirm-download"
                  disabled={mapSettingsBusy || !mapToken.trim()}
                >
                  {mapSettingsAction === "saving-and-testing"
                    ? t("savingAndTesting")
                    : t("saveAndTest")}
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
                <span className="eyebrow">{t("exportEyebrow")}</span>
                <h2 id="export-title">{t("exportTitle")}</h2>
              </div>
              <button
                type="button"
                aria-label={t("closeExport")}
                disabled={exportingFormat !== null}
                onClick={closeExportModal}
              >
                ×
              </button>
            </div>
            <p className="export-description">
              {t("exportDescription")}
            </p>
            <div className="export-options">
              <button
                type="button"
                disabled={exportingFormat !== null}
                onClick={() => void handleExport("png")}
              >
                <strong>{t("pngImage")}</strong>
                <span>{t("pngDetail")}</span>
                <small>{exportingFormat === "png" ? t("generatingSave") : t("pngHint")}</small>
              </button>
              <button
                type="button"
                disabled={exportingFormat !== null}
                onClick={() => void handleExport("pdf")}
              >
                <strong>{t("pdfReport")}</strong>
                <span>{t("pdfDetail")}</span>
                <small>{exportingFormat === "pdf" ? t("generatingSave") : t("pdfHint")}</small>
              </button>
            </div>
            {exportMessage && <p className="export-message">{exportMessage}</p>}
            {exportError && <p className="export-error">{exportError}</p>}
            <p className="export-note">
              {validationServerMode
                ? t("browserExportPrivacy")
                : t("desktopExportPrivacy")}
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
                <span className="eyebrow">{t("cacheEyebrow")}</span>
                <h2 id="cache-title">{t("cacheOverview")}</h2>
              </div>
              <button type="button" aria-label={t("closeCache")} onClick={closeCacheModal}>
                ×
              </button>
            </div>
            <div className="quota-number">
              <strong>{cacheUsage ? formatBytes(cacheUsage.totalBytes)  : t("loading")}</strong>
              <span>/ 2.50 GB</span>
            </div>
            <div className="quota-track">
              <span style={{ width: `${cachePercent}%` }} />
            </div>
            <div className="cache-grid">
              <div><span>{t("elevationDem")}</span><strong>{formatBytes(cacheUsage?.demBytes ?? 0)}</strong></div>
              <div><span>{t("waterWbm")}</span><strong>{formatBytes(cacheUsage?.waterBytes ?? 0)}</strong></div>
              <div><span>{t("temporaryDownloads")}</span><strong>{formatBytes(cacheUsage?.partialBytes ?? 0)}</strong></div>
              <div><span>{t("indexesMetadata")}</span><strong>{formatBytes(displayedMetadataBytes)}</strong></div>
            </div>
            <div className="cache-region-heading">
              <div>
                <strong>{t("registeredRegions")}</strong>
                <span>{t("deleteShared")}</span>
              </div>
              <button type="button" disabled={isBusy} onClick={() => void refreshCacheOverview()}>
                {t("refresh")}
              </button>
            </div>
            <div className="cache-region-list">
              {cacheLoading && !cacheOverview && <p className="cache-empty">{t("readingCache")}</p>}
              {!cacheLoading && (cacheOverview?.regions.length ?? 0) === 0 && (
                <p className="cache-empty">{t("noRegions")}</p>
              )}
              {cacheOverview?.regions.map((region) => (
                <article className="cache-region" key={region.regionId}>
                  <div>
                    <strong>{region.center.lat.toFixed(4)}°, {region.center.lon.toFixed(4)}°</strong>
                    <span>
                      {t("assetsReady", { ready: region.readyAssetCount, total: region.assetCount })}
                      {region.partialAssetCount > 0 ? t("resumableCount", { count: region.partialAssetCount }) : ""}
                    </span>
                    <small>
                      {t("referencedBytes", { referenced: formatBytes(region.referencedBytes), reclaimable: formatBytes(region.reclaimableBytes) })}
                    </small>
                  </div>
                  <button
                    type="button"
                    disabled={!capabilities.canDeleteCache || isBusy}
                    onClick={() => setDeleteCandidate(region)}
                  >
                    {t("delete")}
                  </button>
                </article>
              ))}
            </div>
            {deleteCandidate && (
              <div className="delete-confirm" role="alert">
                <strong>{t("deleteRegionTitle")}</strong>
                <span>
                  {t("deleteRegionDetail", { bytes: formatBytes(deleteCandidate.reclaimableBytes) })}
                </span>
                <div>
                  <button type="button" disabled={deletingRegion} onClick={() => setDeleteCandidate(null)}>
                    {t("back")}
                  </button>
                  <button type="button" disabled={deletingRegion} onClick={() => void handleDeleteRegion()}>
                    {deletingRegion ? t("deleting") : t("confirmDelete")}
                  </button>
                </div>
              </div>
            )}
            {cacheError && <p className="cache-error">{cacheError}</p>}
            <p className="cache-note">
              {t("cacheCap")}
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
                <span className="eyebrow">{t("downloadEyebrow")}</span>
                <h2 id="download-title">{t("downloadTitle")}</h2>
              </div>
              <button type="button" aria-label={t("closeDownload")} onClick={dismissDownloadEstimate}>
                ×
              </button>
            </div>
            <div className="download-size">
              <span>{t("additionalRequired")}</span>
              <strong>{formatBytes(downloadEstimate.additionalDownloadBytes)}</strong>
            </div>
            <div className="download-facts">
              <div><span>{t("geographicTiles")}</span><strong>{downloadEstimate.tileCount}</strong></div>
              <div><span>{t("assetsToPrepare")}</span><strong>{downloadEstimate.requiredAssetCount}</strong></div>
              <div><span>{t("resumableData")}</span><strong>{formatBytes(downloadEstimate.resumableBytes)}</strong></div>
              <div><span>{t("projectedUsage")}</span><strong>{formatBytes(downloadEstimate.projectedTotalBytes)}</strong></div>
            </div>
            {downloadEstimate.generatedAssetCount > 0 && (
              <p className="download-ocean-note">
                {t("generatedOcean", { count: downloadEstimate.generatedAssetCount })}
              </p>
            )}
            <p className="download-note">
              {t("downloadSource")}
            </p>
            {capabilities.mode === "preview" && (
              <p className="preview-callout">{t("browserDownloadPreview")}</p>
            )}
            <div className="modal-actions">
              <button type="button" onClick={dismissDownloadEstimate}>{t("later")}</button>
              <button
                type="button"
                className="confirm-download"
                disabled={!capabilities.canDownload || isBusy}
                onClick={() => void handleConfirmDownload()}
              >
                {t("downloadPrepare")}
              </button>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}
