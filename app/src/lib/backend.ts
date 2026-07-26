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
  OperationKind,
  OperationStatus,
  OperationTicket,
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

class ValidationRequestError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
    this.name = "ValidationRequestError";
  }
}

class CancellationTimeoutError extends Error {
  constructor() {
    super("Cancellation timed out before the operation became cancellable.");
    this.name = "CancellationTimeoutError";
  }
}

async function validationRequest<T>(
  path: string,
  body?: Record<string, unknown>,
  method = body ? "POST" : "GET",
  signal?: AbortSignal,
): Promise<T> {
  const sendsJson = body !== undefined || method.toUpperCase() === "POST";
  const response = await fetch(path, {
    method,
    signal,
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
    throw new ValidationRequestError(response.status, message);
  }
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

type OperationFamily = "download" | "calculation";

interface ValidationOperationHandle {
  kind: OperationKind;
  family: OperationFamily;
  generation: number;
  operationId: string | null;
  ticketPromise: Promise<string>;
  stopped: boolean;
  lastSequence: number;
  timerId: number | null;
  pollController: AbortController | null;
  inflightPoll: Promise<void> | null;
}

const OPERATION_POLL_INTERVAL_MS = 250;
const FINAL_OPERATION_REQUEST_TIMEOUT_MS = 1_500;
const CANCELLATION_RETRY_DELAY_MS = 100;
const CANCELLATION_TIMEOUT_MS = 3_000;
const OPERATION_ID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const OPERATION_STATES = new Set([
  "running",
  "cancellation-requested",
  "succeeded",
  "failed",
  "reserved",
  "cancelled",
]);
const CALCULATION_PHASES = new Set([
  "loading-data",
  "computing",
  "encoding",
  "complete",
]);

const calculationProgressHandlers = new Set<
  (progress: CalculationProgress) => void
>();
const downloadProgressHandlers = new Set<(progress: DownloadProgress) => void>();

let calculationGeneration = 0;
let downloadGeneration = 0;
let activeCalculationOperation: ValidationOperationHandle | null = null;
let activeDownloadOperation: ValidationOperationHandle | null = null;

function operationFamily(kind: OperationKind): OperationFamily {
  return kind === "calculation" ? "calculation" : "download";
}

function activeOperation(family: OperationFamily): ValidationOperationHandle | null {
  return family === "calculation"
    ? activeCalculationOperation
    : activeDownloadOperation;
}

function setActiveOperation(
  family: OperationFamily,
  handle: ValidationOperationHandle | null,
): void {
  if (family === "calculation") {
    activeCalculationOperation = handle;
  } else {
    activeDownloadOperation = handle;
  }
}

function isCurrentOperation(handle: ValidationOperationHandle): boolean {
  const current = activeOperation(handle.family);
  return current === handle && current.generation === handle.generation;
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function beginValidationOperation(kind: OperationKind): ValidationOperationHandle {
  const family = operationFamily(kind);
  if (activeOperation(family)) {
    throw new Error(`A ${family} operation is already active.`);
  }

  const generation =
    family === "calculation" ? ++calculationGeneration : ++downloadGeneration;
  const handle: ValidationOperationHandle = {
    kind,
    family,
    generation,
    operationId: null,
    ticketPromise: Promise.resolve(""),
    stopped: false,
    lastSequence: -1,
    timerId: null,
    pollController: null,
    inflightPoll: null,
  };
  setActiveOperation(family, handle);

  handle.ticketPromise = validationRequest<OperationTicket>("/api/operation-ticket", {
    kind,
  }).then((ticket) => {
    if (
      ticket.schemaVersion !== 1 ||
      ticket.kind !== kind ||
      ticket.state !== "reserved" ||
      !OPERATION_ID_PATTERN.test(ticket.operationId)
    ) {
      throw new Error("The validation server returned an invalid operation ticket.");
    }
    handle.operationId = ticket.operationId;
    return ticket.operationId;
  });

  return handle;
}

function statusMatchesHandle(
  status: OperationStatus,
  handle: ValidationOperationHandle,
): boolean {
  return (
    status.schemaVersion === 1 &&
    status.operationId === handle.operationId &&
    status.kind === handle.kind &&
    OPERATION_STATES.has(status.state) &&
    Number.isSafeInteger(status.sequence) &&
    status.sequence >= 0
  );
}

function notifyCalculationProgress(progress: CalculationProgress): void {
  for (const handler of [...calculationProgressHandlers]) {
    try {
      handler(progress);
    } catch {
      // A UI listener must not stop polling or the long-running request.
    }
  }
}

function notifyDownloadProgress(progress: DownloadProgress): void {
  for (const handler of [...downloadProgressHandlers]) {
    try {
      handler(progress);
    } catch {
      // A UI listener must not stop polling or the long-running request.
    }
  }
}

function publishOperationProgress(
  handle: ValidationOperationHandle,
  status: OperationStatus,
  allowStopped: boolean,
): void {
  if (
    !isCurrentOperation(handle) ||
    (handle.stopped && !allowStopped) ||
    !statusMatchesHandle(status, handle) ||
    status.sequence <= handle.lastSequence
  ) {
    return;
  }
  handle.lastSequence = status.sequence;

  const progress = status.progress;
  if (!progress) return;

  if (
    handle.kind === "calculation" &&
    progress.type === "calculation" &&
    CALCULATION_PHASES.has(progress.phase) &&
    isFiniteNumber(progress.percent) &&
    isFiniteNumber(progress.completedPixelCount) &&
    isFiniteNumber(progress.totalPixelCount)
  ) {
    notifyCalculationProgress({
      phase: progress.phase,
      percent: progress.percent,
      completedPixelCount: progress.completedPixelCount,
      totalPixelCount: progress.totalPixelCount,
    });
    return;
  }

  if (
    handle.kind === "download" &&
    progress.type === "download" &&
    isFiniteNumber(progress.assetIndex) &&
    isFiniteNumber(progress.assetCount) &&
    isFiniteNumber(progress.assetDownloadedBytes) &&
    isFiniteNumber(progress.assetExpectedBytes) &&
    isFiniteNumber(progress.totalDownloadedBytes) &&
    isFiniteNumber(progress.totalExpectedBytes) &&
    isFiniteNumber(progress.percent)
  ) {
    notifyDownloadProgress({
      assetIndex: progress.assetIndex,
      assetCount: progress.assetCount,
      assetKey: "",
      assetDownloadedBytes: progress.assetDownloadedBytes,
      assetExpectedBytes: progress.assetExpectedBytes,
      totalDownloadedBytes: progress.totalDownloadedBytes,
      totalExpectedBytes: progress.totalExpectedBytes,
      percent: progress.percent,
    });
  }
}

async function requestOperationStatus(
  handle: ValidationOperationHandle,
  signal: AbortSignal | undefined,
  allowStopped: boolean,
): Promise<OperationStatus | null> {
  if (!handle.operationId) return null;
  const status = await validationRequest<OperationStatus>(
    "/api/operation-status",
    { operationId: handle.operationId },
    "POST",
    signal,
  );
  publishOperationProgress(handle, status, allowStopped);
  return status;
}

async function validationRequestWithTimeout<T>(
  request: (signal: AbortSignal) => Promise<T>,
  timeoutMs = FINAL_OPERATION_REQUEST_TIMEOUT_MS,
): Promise<T> {
  const controller = new AbortController();
  const timerId = window.setTimeout(
    () => controller.abort(),
    timeoutMs,
  );
  try {
    return await request(controller.signal);
  } finally {
    window.clearTimeout(timerId);
  }
}

function scheduleOperationPoll(handle: ValidationOperationHandle): void {
  if (handle.stopped || !isCurrentOperation(handle) || handle.timerId !== null) {
    return;
  }
  handle.timerId = window.setTimeout(() => {
    handle.timerId = null;
    if (handle.stopped || !isCurrentOperation(handle)) return;

    const controller = new AbortController();
    handle.pollController = controller;
    const poll = requestOperationStatus(handle, controller.signal, false)
      .then(() => undefined)
      .catch(() => {
        // A transient polling error does not fail the primary operation.
      })
      .finally(() => {
        if (handle.pollController === controller) handle.pollController = null;
        if (handle.inflightPoll === poll) handle.inflightPoll = null;
        scheduleOperationPoll(handle);
      });
    handle.inflightPoll = poll;
  }, OPERATION_POLL_INTERVAL_MS);
}

async function stopOperationPoll(handle: ValidationOperationHandle): Promise<void> {
  handle.stopped = true;
  if (handle.timerId !== null) {
    window.clearTimeout(handle.timerId);
    handle.timerId = null;
  }
  handle.pollController?.abort();
  const inflight = handle.inflightPoll;
  if (inflight) await inflight;
}

async function finishValidationOperation(handle: ValidationOperationHandle): Promise<void> {
  const operationId = handle.operationId;
  try {
    try {
      await stopOperationPoll(handle);
    } catch {
      // Cleanup must continue even if an unusual fetch implementation rejects.
    }
    if (operationId) {
      try {
        await validationRequestWithTimeout((signal) =>
          requestOperationStatus(handle, signal, true),
        );
      } catch {
        // The primary result remains authoritative when the final poll fails.
      }
    }
  } finally {
    if (isCurrentOperation(handle)) setActiveOperation(handle.family, null);
  }

  if (operationId) {
    try {
      await validationRequestWithTimeout((signal) =>
        validationRequest<{ acknowledged: boolean }>(
          "/api/operation-ack",
          { operationId },
          "POST",
          signal,
        ),
      );
    } catch {
      // Bounded server retention cleans up if the best-effort ack is lost.
    }
  }
}

async function runValidationOperation<T>(
  kind: OperationKind,
  path: string,
  body: Record<string, unknown>,
): Promise<T> {
  const handle = beginValidationOperation(kind);
  try {
    const operationId = await handle.ticketPromise;
    const primary = validationRequest<T>(path, { operationId, ...body });
    scheduleOperationPoll(handle);
    return await primary;
  } finally {
    await finishValidationOperation(handle);
  }
}

function remainingCancellationTime(deadline: number): number {
  const remaining = deadline - performance.now();
  if (remaining <= 0) throw new CancellationTimeoutError();
  return remaining;
}

async function awaitBeforeCancellationDeadline<T>(
  promise: Promise<T>,
  deadline: number,
): Promise<T> {
  const timeoutMs = remainingCancellationTime(deadline);
  let timerId = 0;
  try {
    return await Promise.race([
      promise,
      new Promise<T>((_resolve, reject) => {
        timerId = window.setTimeout(
          () => reject(new CancellationTimeoutError()),
          timeoutMs,
        );
      }),
    ]);
  } finally {
    window.clearTimeout(timerId);
  }
}

async function waitForCancellationRetry(deadline: number): Promise<void> {
  const delayMs = Math.min(
    CANCELLATION_RETRY_DELAY_MS,
    remainingCancellationTime(deadline),
  );
  await new Promise<void>((resolve) => window.setTimeout(resolve, delayMs));
  remainingCancellationTime(deadline);
}

function rethrowCancellationRequestError(error: unknown, deadline: number): never {
  if (
    performance.now() >= deadline ||
    (error instanceof DOMException && error.name === "AbortError")
  ) {
    throw new CancellationTimeoutError();
  }
  throw error;
}

async function cancelValidationOperation(
  family: OperationFamily,
  path: string,
): Promise<void> {
  const handle = activeOperation(family);
  if (!handle) return;
  const deadline = performance.now() + CANCELLATION_TIMEOUT_MS;

  let operationId: string;
  try {
    operationId = await awaitBeforeCancellationDeadline(
      handle.ticketPromise,
      deadline,
    );
  } catch (error) {
    if (error instanceof CancellationTimeoutError) throw error;
    return;
  }

  while (isCurrentOperation(handle) && !handle.stopped) {
    let cancellation: { cancelled: boolean };
    try {
      cancellation = await validationRequestWithTimeout(
        (signal) =>
          validationRequest<{ cancelled: boolean }>(
            path,
            { operationId },
            "POST",
            signal,
          ),
        remainingCancellationTime(deadline),
      );
    } catch (error) {
      if (error instanceof ValidationRequestError && error.status === 404) return;
      rethrowCancellationRequestError(error, deadline);
    }

    if (typeof cancellation.cancelled !== "boolean") {
      throw new Error("The validation server returned an invalid cancellation response.");
    }
    if (cancellation.cancelled) return;
    if (!isCurrentOperation(handle) || handle.stopped) return;

    let status: OperationStatus | null;
    try {
      status = await validationRequestWithTimeout(
        (signal) => requestOperationStatus(handle, signal, false),
        remainingCancellationTime(deadline),
      );
    } catch (error) {
      if (error instanceof ValidationRequestError && error.status === 404) return;
      rethrowCancellationRequestError(error, deadline);
    }

    if (!isCurrentOperation(handle) || handle.stopped) return;
    if (!status || !statusMatchesHandle(status, handle)) {
      throw new Error("The validation server returned an invalid operation status.");
    }
    if (
      status.state === "cancellation-requested" ||
      status.state === "cancelled" ||
      status.state === "succeeded" ||
      status.state === "failed"
    ) {
      return;
    }

    await waitForCancellationRetry(deadline);
  }
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
    return runValidationOperation<DownloadEstimate>(
      "estimate-download",
      "/api/estimate-download",
      { point },
    );
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
    return runValidationOperation<DownloadResult>(
      "download",
      "/api/download-region",
      { point },
    );
  }
  throw new Error("浏览器仅展示下载确认界面；真实数据只能由 Tauri 桌面后端下载。");
}

export async function cancelDownload(): Promise<void> {
  if (desktopBackendAvailable()) {
    await invoke("cancel_download");
  } else if (backendMode() === "validation-server") {
    await cancelValidationOperation("download", "/api/cancel-download");
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
    return runValidationOperation<CalculationResult>(
      "calculation",
      "/api/calculate",
      { request },
    );
  }
  throw new Error("浏览器仅用于界面检查；真实传播计算必须在 Tauri 桌面后端中运行。");
}

export async function cancelCalculation(): Promise<void> {
  if (desktopBackendAvailable()) {
    await invoke("cancel_calculation");
  } else if (backendMode() === "validation-server") {
    await cancelValidationOperation("calculation", "/api/cancel-calculation");
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
  if (desktopBackendAvailable()) {
    return listen<CalculationProgress>("calculation-progress", (event) => {
      handler(event.payload);
    });
  }
  if (backendMode() === "validation-server") {
    calculationProgressHandlers.add(handler);
    return () => {
      calculationProgressHandlers.delete(handler);
    };
  }
  return () => undefined;
}

export async function listenDownloadProgress(
  handler: (progress: DownloadProgress) => void,
): Promise<UnlistenFn> {
  if (desktopBackendAvailable()) {
    return listen<DownloadProgress>("download-progress", (event) => {
      handler(event.payload);
    });
  }
  if (backendMode() === "validation-server") {
    downloadProgressHandlers.add(handler);
    return () => {
      downloadProgressHandlers.delete(handler);
    };
  }
  return () => undefined;
}
