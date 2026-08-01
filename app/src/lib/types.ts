export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";
export type Band = "vhf144" | "uhf430";
export type ScenarioPreset = "base-to-handheld" | "handheld-to-base";
export type PowerUnit = "watt" | "dbm";
export type GainUnit = "dbi" | "dbd";
export type Polarization = "horizontal" | "vertical";

export interface MapPoint {
  lat: number;
  lon: number;
}

export interface SatelliteBasemapInfo {
  enabled: boolean;
  providerId: string;
  displayName: string;
  attribution: string;
  mode: string;
  maxZoom: number;
  tilePathTemplate: string;
}

export interface BasemapLayer {
  id: "vec" | "cva";
  displayName: string;
}

export interface BasemapInfo {
  enabled: boolean;
  providerId: string;
  displayName: string;
  attribution: string;
  mode: string;
  maxZoom: number;
  layers: BasemapLayer[];
  tilePathTemplate?: string;
  satellite?: SatelliteBasemapInfo;
}
export interface OnlineBasemapInfo {
  configured: boolean;
  provider: "Tianditu";
  protocolScheme: "tianditu";
  vectorTemplate: string;
  vectorLabelTemplate: string;
  imageryTemplate: string;
  imageryLabelTemplate: string;
  attribution: string;
  minZoom: number;
  maxZoom: number;
}

export type OnlineBasemapProbeStatus =
  | "reachable"
  | "not-configured"
  | "network"
  | "timeout"
  | "upstream-or-credential"
  | "invalid-content";

export interface OnlineBasemapProbeResult {
  schemaVersion: 1;
  status: OnlineBasemapProbeStatus;
}
export interface CacheUsage {
  totalBytes: number;
  demBytes: number;
  waterBytes: number;
  partialBytes: number;
  metadataBytes: number;
  remainingBytes: number;
  capBytes: number;
}

export interface BootstrapInfo {
  schemaVersion: number;
  modelName: string;
  modelVersion: string;
  coverageRadiusKm: number;
  gridSize: number;
  cacheUsage: CacheUsage;
  internalBuildWarning: string;
  basemap?: BasemapInfo;
  onlineBasemap?: OnlineBasemapInfo;
}

export interface PointInspection {
  point: MapPoint;
  regionId: string;
  tileCount: number;
  readyDemCount: number;
  readyWaterCount: number;
  missingAssetCount: number;
  dataReady: boolean;
  elevationM: number | null;
  cacheUsage: CacheUsage;
}

export interface DownloadEstimate {
  point: MapPoint;
  regionId: string;
  tileCount: number;
  readyAssetCount: number;
  requiredAssetCount: number;
  generatedAssetCount: number;
  additionalDownloadBytes: number;
  resumableBytes: number;
  projectedTotalBytes: number;
  projectedRemainingBytes: number;
  cacheUsage: CacheUsage;
}

export interface DownloadProgress {
  assetIndex: number;
  assetCount: number;
  assetKey: string;
  assetDownloadedBytes: number;
  assetExpectedBytes: number;
  totalDownloadedBytes: number;
  totalExpectedBytes: number;
  percent: number;
}

export type OperationKind = "estimate-download" | "download" | "calculation";

export type OperationState =
  | "reserved"
  | "running"
  | "cancellation-requested"
  | "succeeded"
  | "failed"
  | "cancelled";

export interface OperationTicket {
  schemaVersion: 1;
  operationId: string;
  kind: OperationKind;
  state: "reserved";
}

export interface EstimateDownloadOperationProgress {
  type: "estimate-download";
  stage: "estimating";
}

export interface DownloadOperationProgress {
  type: "download";
  assetIndex: number;
  assetCount: number;
  assetDownloadedBytes: number;
  assetExpectedBytes: number;
  totalDownloadedBytes: number;
  totalExpectedBytes: number;
  percent: number;
}

export interface CalculationOperationProgress extends CalculationProgress {
  type: "calculation";
}

export type OperationProgress =
  | EstimateDownloadOperationProgress
  | DownloadOperationProgress
  | CalculationOperationProgress;

export interface OperationStatus {
  schemaVersion: 1;
  operationId: string;
  kind: OperationKind;
  state: OperationState;
  sequence: number;
  progress: OperationProgress | null;
}

export interface DownloadResult {
  inspection: PointInspection;
  preparedAssetCount: number;
  downloadedBytes: number;
}

export interface CacheRegion {
  regionId: string;
  center: MapPoint;
  assetCount: number;
  readyAssetCount: number;
  partialAssetCount: number;
  referencedBytes: number;
  reclaimableBytes: number;
  createdUnix: number;
}

export interface CacheOverview {
  usage: CacheUsage;
  regions: CacheRegion[];
}

export interface CacheDeleteResult {
  deletedAssetCount: number;
  freedBytes: number;
  overview: CacheOverview;
}

export interface RadioParameters {
  preset: ScenarioPreset;
  band: Band;
  frequencyMhz: number;
  powerValue: number;
  powerUnit: PowerUnit;
  txGainValue: number;
  txGainUnit: GainUnit;
  txHeightM: number;
  txGroundElevationOverrideM: number | null;
  rxGainValue: number;
  rxGainUnit: GainUnit;
  rxHeightM: number;
  polarization: Polarization;
}

export interface CalculationRequest {
  center: MapPoint;
  band: "vhf-144" | "uhf-430";
  frequencyMhz: number;
  powerValue: number;
  powerUnit: PowerUnit;
  txGainValue: number;
  txGainUnit: GainUnit;
  txHeightM: number;
  txGroundElevationOverrideM: number | null;
  rxGainValue: number;
  rxGainUnit: GainUnit;
  rxHeightM: number;
  polarization: Polarization;
}

export type CalculationPhase =
  | "loading-data"
  | "computing"
  | "encoding"
  | "complete";

export interface CalculationProgress {
  phase: CalculationPhase;
  percent: number;
  completedPixelCount: number;
  totalPixelCount: number;
}

export interface CalculationPreview {
  schemaVersion: 1;
  sequence: number;
  completedPixelCount: number;
  totalPixelCount: number;
  mapOverlayProjection: "EPSG:3857";
  mapOverlayWidth: number;
  mapOverlayHeight: number;
  mapOverlayCorners: [number, number][];
  mapOverlayPngDataUrl: string;
}

export interface CalculationStatistics {
  validPixelCount: number;
  belowThresholdPixelCount: number;
  warningPixelCount: number;
  minimumDbm: number;
  maximumDbm: number;
  meanDbm: number;
  waterAffectedPixelCount: number;
  meanPathWaterFraction: number;
  propagationSeconds: number;
  totalSeconds: number;
}

export type TxGroundElevationSource = "dem" | "manual";

export interface CalculationResult {
  schemaVersion: 4;
  modelName: string;
  modelVersion: string;
  center: MapPoint;
  txGroundElevationM: number;
  txGroundElevationSource: TxGroundElevationSource;
  imageWidth: number;
  imageHeight: number;
  imageCorners: [number, number][];
  heatmapPngDataUrl: string;
  mapOverlayProjection: "EPSG:3857";
  mapOverlayWidth: number;
  mapOverlayHeight: number;
  mapOverlayCorners: [number, number][];
  mapOverlayPngDataUrl: string;
  mapOverlayFilterEncoding: "u8-dbm-floor-v1";
  mapOverlayFilterBase64: string;
  statistics: CalculationStatistics;
}

export interface SessionCoverageResult {
  id: string;
  result: CalculationResult;
  parameters: RadioParameters;
  completedAt: number;
}

export type ExportFormat = "png" | "pdf";

export interface ExportRequest {
  format: ExportFormat;
  suggestedFileName: string;
  reportPngDataUrl: string;
}

export interface ExportResult {
  cancelled: boolean;
  path: string | null;
  bytesWritten: number;
}

export type WorkflowState =
  | "idle"
  | "inspecting"
  | "estimating-download"
  | "download-required"
  | "downloading"
  | "ready"
  | "missing-data"
  | "calculating"
  | "completed"
  | "download-cancelled"
  | "cancelled"
  | "error";
