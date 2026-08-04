// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import i18n, { currentAppLocale } from "../i18n";
import type { AppLocale } from "../i18n/locale";
import { exportReportInBrowser } from "./browserExport";
import {
  decodeMapOverlayFilter,
  MAP_OVERLAY_FILTER_ENCODING,
} from "./coverageVisibility";
import type {
  BasemapInfo,
  BootstrapInfo,
  CacheDeleteResult,
  CacheOverview,
  CalculationPreview,
  CalculationProgress,
  CalculationRequest,
  ExportRequest,
  ExportResult,
  CalculationResult,
  DownloadEstimate,
  DownloadProgress,
  DownloadResult,
  LinkAnalysisRequest,
  LinkAnalysisResult,
  MapPoint,
  OperationKind,
  OperationStatus,
  OperationTicket,
  OnlineBasemapInfo,
  OnlineBasemapProbeResult,
  PointInspection,
} from "./types";

const PREVIEW_CACHE_CAP = 2_500_000_000;

export type BackendMode = "tauri" | "validation-server" | "preview";

export interface BackendCapabilities {
  mode: BackendMode;
  canDownload: boolean;
  canDeleteCache: boolean;
  canCalculate: boolean;
  canAnalyzeLink: boolean;
  canExport: boolean;
  canConfigureOnlineBasemap: boolean;
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
    canAnalyzeLink: mode !== "preview",
    canExport: mode !== "preview",
    canConfigureOnlineBasemap: mode === "tauri",
  };
}

export function desktopBackendAvailable(): boolean {
  return backendMode() === "tauri";
}

export function isCancellationError(error: unknown): boolean {
  if (error && typeof error === "object" && "code" in error) {
    const code = (error as { code?: unknown }).code;
    if (code === "operation.cancelled" || code === "cancelled") return true;
  }
  const message = error instanceof Error ? error.message : String(error);
  const normalized = message.trim().toLowerCase();
  return normalized === "operation cancelled" ||
    normalized === "operation canceled" ||
    normalized === "coverage calculation cancelled" ||
    normalized.includes("操作已取消") ||
    normalized.includes("計算をキャンセル") ||
    normalized.includes("運算已取消");
}

function messageMatchesLocale(message: string, locale: AppLocale): boolean {
  const hasHan = /\p{Script=Han}/u.test(message);
  const hasKana = /[\u3040-\u30ff]/u.test(message);
  if (locale === "en") return !hasHan && !hasKana;
  if (locale === "ja-JP") return hasKana;
  if (locale === "zh-CN") return hasHan && !hasKana;
  return (
    hasHan &&
    !hasKana &&
    /[體傳檔錯誤請與為後線區數據緩載網應]/u.test(message)
  );
}

export function localizedBackendError(
  error: unknown,
  locale: AppLocale = currentAppLocale(),
): string {
  const t = i18n.getFixedT(locale);
  if (isCancellationError(error)) return t("statusCancelled");
  const message = error instanceof Error ? error.message : String(error);
  const normalized = message.toLowerCase();

  if (
    normalized.includes("already active") ||
    normalized.includes("operation is already") ||
    message.includes("正在进行，请稍候或先取消")
  ) {
    return t("errorOperationActive");
  }
  if (
    message.includes("缓存空间不足") ||
    message.includes("持久数据空间不足") ||
    normalized.includes("cache quota") ||
    normalized.includes("cache space")
  ) {
    return t("errorBackendCacheFull");
  }
  if (
    message.includes("正在使用中") ||
    normalized.includes("currently in use")
  ) {
    return t("errorBackendRegionInUse");
  }
  if (
    message.includes("数据仍不完整") ||
    message.includes("仍缺少") ||
    message.includes("先联网缓存") ||
    normalized.includes("calculation data") && normalized.includes("missing") ||
    normalized.includes("region data") && normalized.includes("incomplete")
  ) {
    return t("errorBackendDataMissing");
  }
  if (
    message.includes("完整性检查失败") ||
    normalized.includes("integrity check")
  ) {
    return t("errorBackendIntegrity");
  }
  if (
    normalized.includes("failed to fetch") ||
    normalized.includes("networkerror") ||
    normalized.includes("network request failed")
  ) {
    return t("errorBackendNetwork");
  }

  const status =
    error && typeof error === "object" && "status" in error
      ? (error as { status?: unknown }).status
      : undefined;
  if (typeof status === "number" && Number.isSafeInteger(status)) {
    return t("errorBackendHttp", { status });
  }
  return messageMatchesLocale(message, locale) ? message : t("errorBackendUnknown");
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
    super(i18n.t("errorCancellationTimeout"));
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
  lastPreviewSequence: number;
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
const calculationPreviewHandlers = new Set<
  (preview: CalculationPreview) => void
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

const LINK_CLASSIFICATIONS = new Set([
  "direct-los",
  "obstructed-usable",
  "predicted-unavailable",
]);

function validateLinkAnalysisResult(value: unknown): LinkAnalysisResult {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(i18n.t("errorLinkResultObject"));
  }
  const result = value as Partial<LinkAnalysisResult>;
  if (result.schemaVersion !== 1) {
    throw new Error(i18n.t("errorLinkResultSchema"));
  }
  if (
    typeof result.classification !== "string" ||
    !LINK_CLASSIFICATIONS.has(result.classification) ||
    typeof result.classificationReason !== "string" ||
    typeof result.geometricLos !== "boolean" ||
    typeof result.fresnelClearance60 !== "boolean" ||
    typeof result.itmMode !== "string" ||
    typeof result.critical !== "boolean" ||
    !Array.isArray(result.profile) ||
    result.profile.length < 2 ||
    result.sampleCount !== result.profile.length
  ) {
    throw new Error(i18n.t("errorLinkResultProtocol"));
  }

  const numericFields = [
    result.distanceM,
    result.initialBearingDeg,
    result.finalBearingDeg,
    result.frequencyMhz,
    result.wavelengthM,
    result.sampleSpacingM,
    result.sampleCount,
    result.effectiveEarthRadiusM,
    result.kFactor,
    result.txGroundElevationM,
    result.rxGroundElevationM,
    result.txAntennaElevationM,
    result.rxAntennaElevationM,
    result.minimumLosClearanceM,
    result.minimumFresnelClearanceRatio,
    result.criticalSampleIndex,
    result.itmBasicTransmissionLossDb,
    result.itmWarnings,
    result.waterFraction,
    result.coPolarizedReferencePowerDbm,
    result.polarizationMismatchLossDb,
    result.predictedRxPowerDbm,
    result.receiverThresholdDbm,
    result.linkMarginDb,
  ];
  if (numericFields.some((item) => !isFiniteNumber(item))) {
    throw new Error(i18n.t("errorLinkResultProtocol"));
  }
  if (
    !Number.isSafeInteger(result.sampleCount) ||
    !Number.isSafeInteger(result.criticalSampleIndex) ||
    (result.criticalSampleIndex as number) < 0 ||
    (result.criticalSampleIndex as number) >= result.profile.length ||
    (result.distanceM as number) <= 0 ||
    (result.sampleSpacingM as number) <= 0
  ) {
    throw new Error(i18n.t("errorLinkResultProtocol"));
  }

  let previousDistance = -1;
  for (const sample of result.profile) {
    if (
      !sample ||
      typeof sample !== "object" ||
      [
        sample.distanceM,
        sample.lat,
        sample.lon,
        sample.terrainElevationM,
        sample.earthBulgeM,
        sample.adjustedTerrainM,
        sample.losHeightM,
        sample.fresnelRadiusM,
      ].some((item) => !isFiniteNumber(item)) ||
      sample.distanceM < previousDistance ||
      sample.fresnelRadiusM < 0
    ) {
      throw new Error(i18n.t("errorLinkResultProfile"));
    }
    previousDistance = sample.distanceM;
  }
  const first = result.profile[0];
  const last = result.profile[result.profile.length - 1];
  if (
    Math.abs(first.distanceM) > 1e-6 ||
    Math.abs(last.distanceM - (result.distanceM as number)) > 1
  ) {
    throw new Error(i18n.t("errorLinkResultProfile"));
  }
  return result as LinkAnalysisResult;
}

function validateCalculationResult(value: unknown): CalculationResult {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(i18n.t("errorResultObject"));
  }
  const result = value as Partial<CalculationResult>;
  if (result.schemaVersion !== 4) {
    throw new Error(i18n.t("errorResultSchema"));
  }
  if (result.mapOverlayFilterEncoding !== MAP_OVERLAY_FILTER_ENCODING) {
    throw new Error(i18n.t("errorResultEncoding", { encoding: MAP_OVERLAY_FILTER_ENCODING }));
  }
  const width = result.mapOverlayWidth;
  const height = result.mapOverlayHeight;
  const filterBase64 = result.mapOverlayFilterBase64;
  if (
    width !== 401 ||
    height !== 401 ||
    typeof filterBase64 !== "string" ||
    typeof result.mapOverlayPngDataUrl !== "string" ||
    result.mapOverlayPngDataUrl.length === 0
  ) {
    throw new Error(i18n.t("errorResultImage"));
  }
  try {
    decodeMapOverlayFilter({
      mapOverlayWidth: width as number,
      mapOverlayHeight: height as number,
      mapOverlayFilterEncoding: result.mapOverlayFilterEncoding,
      mapOverlayFilterBase64: filterBase64,
    });
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(i18n.t("errorResultFilter", { detail }));
  }
  return result as CalculationResult;
}

function beginValidationOperation(kind: OperationKind): ValidationOperationHandle {
  const family = operationFamily(kind);
  if (activeOperation(family)) {
    throw new Error(i18n.t("errorOperationActive"));
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
    lastPreviewSequence: 0,
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
      throw new Error(i18n.t("errorInvalidTicket"));
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

function notifyCalculationPreview(preview: CalculationPreview): void {
  for (const handler of [...calculationPreviewHandlers]) {
    try {
      handler(preview);
    } catch {
      // A UI listener must not stop polling or the long-running request.
    }
  }
}

function isCoordinatePair(value: unknown): value is [number, number] {
  return (
    Array.isArray(value) &&
    value.length === 2 &&
    isFiniteNumber(value[0]) &&
    isFiniteNumber(value[1])
  );
}

function isCalculationPreview(value: unknown): value is CalculationPreview {
  if (!value || typeof value !== "object") return false;
  const preview = value as Record<string, unknown>;
  return (
    preview.schemaVersion === 1 &&
    Number.isSafeInteger(preview.sequence) &&
    (preview.sequence as number) >= 0 &&
    Number.isSafeInteger(preview.completedPixelCount) &&
    (preview.completedPixelCount as number) >= 0 &&
    Number.isSafeInteger(preview.totalPixelCount) &&
    (preview.totalPixelCount as number) > 0 &&
    (preview.completedPixelCount as number) <= (preview.totalPixelCount as number) &&
    preview.mapOverlayProjection === "EPSG:3857" &&
    preview.mapOverlayWidth === 401 &&
    preview.mapOverlayHeight === 401 &&
    Array.isArray(preview.mapOverlayCorners) &&
    preview.mapOverlayCorners.length === 4 &&
    preview.mapOverlayCorners.every(isCoordinatePair) &&
    typeof preview.mapOverlayPngDataUrl === "string" &&
    preview.mapOverlayPngDataUrl.startsWith("data:image/png;base64,")
  );
}

function publishCalculationPreview(
  handle: ValidationOperationHandle,
  value: unknown,
): void {
  if (
    !isCurrentOperation(handle) ||
    handle.stopped ||
    handle.kind !== "calculation" ||
    !isCalculationPreview(value) ||
    value.sequence <= handle.lastPreviewSequence
  ) {
    return;
  }
  handle.lastPreviewSequence = value.sequence;
  notifyCalculationPreview(value);
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

async function requestCalculationPreview(
  handle: ValidationOperationHandle,
  signal: AbortSignal,
): Promise<void> {
  if (
    handle.kind !== "calculation" ||
    !handle.operationId ||
    handle.stopped ||
    !isCurrentOperation(handle)
  ) {
    return;
  }
  const preview = await validationRequest<CalculationPreview | undefined>(
    "/api/operation-preview",
    {
      operationId: handle.operationId,
      afterSequence: handle.lastPreviewSequence,
    },
    "POST",
    signal,
  );
  if (preview !== undefined) publishCalculationPreview(handle, preview);
}

async function requestOperationPollCycle(
  handle: ValidationOperationHandle,
  signal: AbortSignal,
): Promise<void> {
  await requestOperationStatus(handle, signal, false);
  await requestCalculationPreview(handle, signal);
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
    const poll = requestOperationPollCycle(handle, controller.signal)
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
      throw new Error(i18n.t("errorInvalidCancellationResponse"));
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
      throw new Error(i18n.t("errorInvalidOperationStatus"));
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

export async function getPublicBasemap(): Promise<BasemapInfo | null> {
  if (!desktopBackendAvailable()) return null;
  return invoke<BasemapInfo | null>("get_public_basemap");
}

export async function getOnlineBasemap(): Promise<OnlineBasemapInfo | null> {
  if (!desktopBackendAvailable()) return null;
  return invoke<OnlineBasemapInfo | null>("get_online_basemap");
}

export async function configureOnlineBasemap(token: string): Promise<OnlineBasemapInfo> {
  if (!desktopBackendAvailable()) {
    throw new Error(i18n.t("errorMapDesktopOnly"));
  }
  const value = token.trim();
  if (!value) throw new Error(i18n.t("errorEnterToken"));
  return invoke<OnlineBasemapInfo>("configure_online_basemap", { token: value });
}

export async function clearOnlineBasemap(): Promise<OnlineBasemapInfo> {
  if (!desktopBackendAvailable()) {
    throw new Error(i18n.t("errorMapDesktopOnly"));
  }
  return invoke<OnlineBasemapInfo>("clear_online_basemap");
}

const ONLINE_BASEMAP_PROBE_STATUSES = new Set([
  "reachable",
  "not-configured",
  "network",
  "timeout",
  "upstream-or-credential",
  "invalid-content",
]);

export async function probeOnlineBasemap(): Promise<OnlineBasemapProbeResult> {
  if (!desktopBackendAvailable()) {
    throw new Error(i18n.t("errorMapProbeDesktopOnly"));
  }
  const result = await invoke<OnlineBasemapProbeResult>("probe_online_basemap");
  if (
    !result ||
    result.schemaVersion !== 1 ||
    !ONLINE_BASEMAP_PROBE_STATUSES.has(result.status)
  ) {
    throw new Error(i18n.t("errorMapProbeProtocol"));
  }
  return result;
}

export async function bootstrap(): Promise<BootstrapInfo> {
  if (desktopBackendAvailable()) {
    const value = await invoke<BootstrapInfo>("bootstrap");
    const [basemap, onlineBasemap] = await Promise.all([
      getPublicBasemap().catch(() => null),
      getOnlineBasemap().catch(() => null),
    ]);
    return {
      ...value,
      basemap: basemap ?? undefined,
      onlineBasemap: onlineBasemap ?? undefined,
    };
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
    internalBuildWarning: i18n.t("internalWarning"),
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
  throw new Error(i18n.t("errorDownloadPreviewOnly"));
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
  throw new Error(i18n.t("errorCacheDesktopOnly"));
}

export async function calculate(
  request: CalculationRequest,
): Promise<CalculationResult> {
  if (desktopBackendAvailable()) {
    let acceptingPreviews = true;
    let lastPreviewSequence = -1;
    const previewChannel = new Channel<CalculationPreview>();
    previewChannel.onmessage = (value) => {
      if (
        !acceptingPreviews ||
        !isCalculationPreview(value) ||
        value.sequence <= lastPreviewSequence
      ) {
        return;
      }
      lastPreviewSequence = value.sequence;
      notifyCalculationPreview(value);
    };
    try {
      const result = await invoke<unknown>("calculate", {
        request,
        previewChannel,
      });
      return validateCalculationResult(result);
    } finally {
      acceptingPreviews = false;
      previewChannel.onmessage = () => undefined;
    }
  }
  if (backendMode() === "validation-server") {
    const result = await runValidationOperation<unknown>(
      "calculation",
      "/api/calculate",
      { request },
    );
    return validateCalculationResult(result);
  }
  throw new Error(i18n.t("errorCalculationPreviewOnly"));
}

export async function analyzeLink(
  request: LinkAnalysisRequest,
): Promise<LinkAnalysisResult> {
  if (desktopBackendAvailable()) {
    const result = await invoke<unknown>("analyze_link", { request });
    return validateLinkAnalysisResult(result);
  }
  if (backendMode() === "validation-server") {
    const result = await runValidationOperation<unknown>(
      "calculation",
      "/api/link-analysis",
      { request },
    );
    return validateLinkAnalysisResult(result);
  }
  throw new Error(i18n.t("errorLinkAnalysisPreviewOnly"));
}

export async function cancelCalculation(): Promise<void> {
  if (desktopBackendAvailable()) {
    await invoke("cancel_calculation");
  } else if (backendMode() === "validation-server") {
    await cancelValidationOperation("calculation", "/api/cancel-calculation");
  }
}

export async function exportReport(
  request: ExportRequest,
  locale: AppLocale = currentAppLocale(),
): Promise<ExportResult> {
  if (desktopBackendAvailable())
    return invoke<ExportResult>("export_result", { request });
  if (backendMode() === "validation-server")
    return exportReportInBrowser(request, locale);
  throw new Error(i18n.getFixedT(locale)("errorExportUnavailable"));
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

export async function listenCalculationPreview(
  handler: (preview: CalculationPreview) => void,
): Promise<UnlistenFn> {
  const mode = backendMode();
  if (mode === "tauri" || mode === "validation-server") {
    calculationPreviewHandlers.add(handler);
    return () => {
      calculationPreviewHandlers.delete(handler);
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
