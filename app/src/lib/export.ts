// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import i18n, { currentAppLocale } from "../i18n";
import type { AppLocale } from "../i18n/locale";
import type {
  CalculationResult,
  ExportFormat,
  RadioParameters,
} from "./types";

const REPORT_WIDTH = 1600;
const REPORT_HEIGHT = 1100;
const MAP_X = 72;
const MAP_Y = 196;
const MAP_SIZE = 780;
const FONT_FAMILY = '"Segoe UI", "Yu Gothic UI", Meiryo, "Microsoft JhengHei UI", "Microsoft YaHei UI", sans-serif';

export const DBM_COLOR_ANCHORS = [
  { dbm: -60, color: "#ff0000", position: 0 },
  { dbm: -75, color: "#ffa500", position: 0.1875 },
  { dbm: -90, color: "#ffff00", position: 0.375 },
  { dbm: -105, color: "#00b400", position: 0.5625 },
  { dbm: -120, color: "#00ffff", position: 0.75 },
  { dbm: -140, color: "#0000ff", position: 1 },
] as const;

export interface ExportReportModel {
  title: string;
  subtitle: string;
  warning: string;
  generatedAt: string;
  center: string;
  parameterRows: Array<[string, string]>;
  statisticRows: Array<[string, string]>;
  cornerLabels: string[];
}

function pad(value: number): string {
  return String(value).padStart(2, "0");
}

function formatGeneratedAt(value: Date): string {
  const offsetMinutes = -value.getTimezoneOffset();
  const sign = offsetMinutes >= 0 ? "+" : "-";
  const absoluteOffset = Math.abs(offsetMinutes);
  const zone = `UTC${sign}${pad(Math.floor(absoluteOffset / 60))}:${pad(absoluteOffset % 60)}`;
  return `${value.getFullYear()}-${pad(value.getMonth() + 1)}-${pad(value.getDate())} ${pad(value.getHours())}:${pad(value.getMinutes())}:${pad(value.getSeconds())} ${zone}`;
}

function displayPower(parameters: RadioParameters): string {
  return parameters.powerUnit === "watt"
    ? `${parameters.powerValue.toFixed(2)} W`
    : `${parameters.powerValue.toFixed(2)} dBm`;
}

function displayGain(value: number, unit: "dbi" | "dbd"): string {
  return `${value.toFixed(2)} ${unit === "dbi" ? "dBi" : "dBd"}`;
}

export function buildExportReportModel(
  result: CalculationResult,
  parameters: RadioParameters,
  generatedAt = new Date(),
  locale: AppLocale = currentAppLocale(),
): ExportReportModel {
  const t = i18n.getFixedT(locale);
  const integer = new Intl.NumberFormat(locale, { maximumFractionDigits: 0 });
  const statistics = result.statistics;
  const waterRatio = statistics.validPixelCount
    ? (statistics.waterAffectedPixelCount / statistics.validPixelCount) * 100
    : 0;
  return {
    title: t("reportTitle"),
    subtitle: `HamHeatmap ALPHA 0.1 · NTIA ITM v1.4 (668e4ab) · ${result.modelVersion} · Copernicus DEM GLO-90 DEM/WBM`,
    warning: t("reportWarning"),
    generatedAt: formatGeneratedAt(generatedAt),
    center: `${result.center.lat.toFixed(5)}°, ${result.center.lon.toFixed(5)}°`,
    parameterRows: [
      [t("reportScenario"), parameters.preset === "base-to-handheld" ? t("baseToHandheld") : t("handheldToBase")],
      [t("reportBandFrequency"), `${parameters.band === "vhf144" ? "144 MHz" : "430 MHz"} / ${parameters.frequencyMhz.toFixed(2)} MHz`],
      [t("reportTxPower"), displayPower(parameters)],
      [t("reportTxGain"), displayGain(parameters.txGainValue, parameters.txGainUnit)],
      [t("reportTxHeight"), `${parameters.txHeightM.toFixed(1)} m AGL`],
      [
        t("reportTxGround"),
        locale === "zh-CN" || locale === "zh-TW"
          ? `${result.txGroundElevationM.toFixed(1)} m AMSL（${result.txGroundElevationSource === "manual" ? t("manualOverride") : t("demAutomatic")}）`
          : `${result.txGroundElevationM.toFixed(1)} m AMSL (${result.txGroundElevationSource === "manual" ? t("manualOverride") : t("demAutomatic")})`,
      ],
      [t("reportRxGain"), displayGain(parameters.rxGainValue, parameters.rxGainUnit)],
      [t("reportRxHeight"), `${parameters.rxHeightM.toFixed(1)} m AGL`],
      [t("reportPolarization"), parameters.polarization === "vertical" ? t("vertical") : t("horizontal")],
    ],
    statisticRows: [
      [t("reportValidPixels"), integer.format(statistics.validPixelCount)],
      [t("reportMaximum"), `${statistics.maximumDbm.toFixed(1)} dBm`],
      [t("reportMean"), `${statistics.meanDbm.toFixed(1)} dBm`],
      [t("reportMinimum"), `${statistics.minimumDbm.toFixed(1)} dBm`],
      [t("reportBelow"), integer.format(statistics.belowThresholdPixelCount)],
      [t("reportWater"), `${waterRatio.toFixed(1)}%`],
      [t("reportDuration"), `${statistics.totalSeconds.toFixed(1)} s`],
    ],
    cornerLabels: result.imageCorners.map(
      ([lon, lat]) => `≈ ${lat.toFixed(3)}°, ${lon.toFixed(3)}°`,
    ),
  };
}

export function suggestedExportFileName(
  result: CalculationResult,
  parameters: RadioParameters,
  format: ExportFormat,
  generatedAt = new Date(),
): string {
  const stamp = `${generatedAt.getFullYear()}${pad(generatedAt.getMonth() + 1)}${pad(generatedAt.getDate())}-${pad(generatedAt.getHours())}${pad(generatedAt.getMinutes())}${pad(generatedAt.getSeconds())}`;
  const frequency = parameters.frequencyMhz.toFixed(2).replace(".", "p");
  return `HamHeatmap_${frequency}MHz_${result.center.lat.toFixed(4)}_${result.center.lon.toFixed(4)}_${stamp}.${format}`;
}

function loadImage(
  dataUrl: string,
  t: ReturnType<typeof i18n.getFixedT>,
): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error(t("errorHeatmapRead")));
    image.src = dataUrl;
  });
}

function drawRow(
  context: CanvasRenderingContext2D,
  label: string,
  value: string,
  x: number,
  y: number,
): void {
  context.fillStyle = "#687980";
  context.font = `18px ${FONT_FAMILY}`;
  context.fillText(label, x, y);
  context.fillStyle = "#17242b";
  context.font = `600 20px ${FONT_FAMILY}`;
  context.textAlign = "right";
  context.fillText(value, 1520, y);
  context.textAlign = "left";
  context.strokeStyle = "#dbe3e6";
  context.lineWidth = 1;
  context.beginPath();
  context.moveTo(x, y + 14);
  context.lineTo(1520, y + 14);
  context.stroke();
}

function drawLegend(context: CanvasRenderingContext2D): void {
  const x = MAP_X;
  const y = MAP_Y + MAP_SIZE + 18;
  const width = MAP_SIZE;
  const gradient = context.createLinearGradient(x, 0, x + width, 0);
  DBM_COLOR_ANCHORS.forEach((anchor) => gradient.addColorStop(anchor.position, anchor.color));
  context.fillStyle = gradient;
  context.fillRect(x, y, width, 18);
  context.strokeStyle = "#b8c7cc";
  context.strokeRect(x, y, width, 18);
  context.fillStyle = "#465960";
  context.font = `16px ${FONT_FAMILY}`;
  DBM_COLOR_ANCHORS.forEach((anchor, index) => {
    context.textAlign = index === 0
      ? "left"
      : index === DBM_COLOR_ANCHORS.length - 1 ? "right" : "center";
    const label = index === 0 ? `≥ ${anchor.dbm}` : index === DBM_COLOR_ANCHORS.length - 1 ? `${anchor.dbm} dBm` : String(anchor.dbm);
    context.fillText(label, x + width * anchor.position, y + 42);
  });
  context.textAlign = "left";
}

export async function createExportReportPngDataUrl(
  result: CalculationResult,
  parameters: RadioParameters,
  generatedAt = new Date(),
  locale: AppLocale = currentAppLocale(),
): Promise<string> {
  const t = i18n.getFixedT(locale);
  if (!result.heatmapPngDataUrl.startsWith("data:image/png;base64,")) {
    throw new Error(t("errorHeatmapPng"));
  }
  const [image, model] = await Promise.all([
    loadImage(result.heatmapPngDataUrl, t),
    Promise.resolve(buildExportReportModel(result, parameters, generatedAt, locale)),
  ]);
  const canvas = document.createElement("canvas");
  canvas.width = REPORT_WIDTH;
  canvas.height = REPORT_HEIGHT;
  const context = canvas.getContext("2d");
  if (!context) throw new Error(t("errorExportCanvas"));

  context.fillStyle = "#f4f7f7";
  context.fillRect(0, 0, REPORT_WIDTH, REPORT_HEIGHT);
  context.fillStyle = "#ffffff";
  context.fillRect(40, 34, REPORT_WIDTH - 80, REPORT_HEIGHT - 68);
  context.fillStyle = "#087f74";
  context.fillRect(40, 34, 12, 110);
  context.fillStyle = "#17242b";
  context.font = `700 42px ${FONT_FAMILY}`;
  context.fillText(model.title, 76, 86);
  context.fillStyle = "#65757d";
  context.font = `17px ${FONT_FAMILY}`;
  context.fillText(model.subtitle, 76, 124, 1050);
  context.textAlign = "right";
  context.fillText(t("reportGenerated", { time: model.generatedAt }), 1520, 86);
  context.textAlign = "left";

  context.fillStyle = "#fff2d9";
  context.fillRect(40, 154, REPORT_WIDTH - 80, 36);
  context.fillStyle = "#8b5b18";
  context.font = `16px ${FONT_FAMILY}`;
  context.fillText(model.warning, 72, 178, 1450);

  context.fillStyle = "#eaf0f2";
  context.fillRect(MAP_X, MAP_Y, MAP_SIZE, MAP_SIZE);
  context.strokeStyle = "#c3d2d7";
  context.lineWidth = 1;
  for (let index = 1; index < 4; index += 1) {
    const offset = (MAP_SIZE * index) / 4;
    context.beginPath();
    context.moveTo(MAP_X + offset, MAP_Y);
    context.lineTo(MAP_X + offset, MAP_Y + MAP_SIZE);
    context.moveTo(MAP_X, MAP_Y + offset);
    context.lineTo(MAP_X + MAP_SIZE, MAP_Y + offset);
    context.stroke();
  }
  context.imageSmoothingEnabled = true;
  context.imageSmoothingQuality = "high";
  context.drawImage(image, MAP_X, MAP_Y, MAP_SIZE, MAP_SIZE);
  context.strokeStyle = "#91a9b1";
  context.lineWidth = 2;
  context.strokeRect(MAP_X, MAP_Y, MAP_SIZE, MAP_SIZE);

  const centerX = MAP_X + MAP_SIZE / 2;
  const centerY = MAP_Y + MAP_SIZE / 2;
  context.strokeStyle = "#087f74";
  context.lineWidth = 3;
  context.setLineDash([12, 8]);
  context.beginPath();
  context.arc(centerX, centerY, MAP_SIZE / 2 - 3, 0, Math.PI * 2);
  context.stroke();
  context.setLineDash([]);

  const scaleWidth = (MAP_SIZE * 100) / 400;
  const scaleX = MAP_X + 28;
  const scaleY = MAP_Y + MAP_SIZE - 46;
  context.fillStyle = "rgba(255, 255, 255, 0.88)";
  context.fillRect(scaleX - 12, scaleY - 27, scaleWidth + 24, 48);
  context.strokeStyle = "#17242b";
  context.lineWidth = 4;
  context.beginPath();
  context.moveTo(scaleX, scaleY);
  context.lineTo(scaleX + scaleWidth, scaleY);
  context.moveTo(scaleX, scaleY - 8);
  context.lineTo(scaleX, scaleY + 8);
  context.moveTo(scaleX + scaleWidth, scaleY - 8);
  context.lineTo(scaleX + scaleWidth, scaleY + 8);
  context.stroke();
  context.fillStyle = "#17242b";
  context.font = `600 16px ${FONT_FAMILY}`;
  context.fillText("100 km", scaleX + scaleWidth / 2 - 29, scaleY - 10);

  context.fillStyle = "rgba(255, 92, 53, 0.22)";
  context.beginPath();
  context.arc(centerX, centerY, 18, 0, Math.PI * 2);
  context.fill();
  context.fillStyle = "#ff5c35";
  context.strokeStyle = "#ffffff";
  context.lineWidth = 3;
  context.beginPath();
  context.arc(centerX, centerY, 7, 0, Math.PI * 2);
  context.fill();
  context.stroke();

  context.fillStyle = "#465960";
  context.font = `15px ${FONT_FAMILY}`;
  context.fillText(model.cornerLabels[0] ?? "", MAP_X + 8, MAP_Y + 24);
  context.textAlign = "right";
  context.fillText(model.cornerLabels[1] ?? "", MAP_X + MAP_SIZE - 8, MAP_Y + 24);
  context.fillText(model.cornerLabels[2] ?? "", MAP_X + MAP_SIZE - 8, MAP_Y + MAP_SIZE - 10);
  context.textAlign = "left";
  context.fillText(model.cornerLabels[3] ?? "", MAP_X + 8, MAP_Y + MAP_SIZE - 10);

  context.fillStyle = "#17242b";
  context.font = `700 24px ${FONT_FAMILY}`;
  context.fillText(t("reportParameters"), 956, 230);
  context.fillStyle = "#087f74";
  context.font = `600 23px ${FONT_FAMILY}`;
  context.fillText(model.center, 956, 270);
  model.parameterRows.forEach(([label, value], index) => {
    drawRow(context, label, value, 956, 322 + index * 47);
  });

  context.fillStyle = "#17242b";
  context.font = `700 24px ${FONT_FAMILY}`;
  context.fillText(t("reportStatistics"), 956, 748);
  model.statisticRows.forEach(([label, value], index) => {
    drawRow(context, label, value, 956, 790 + index * 35);
  });

  context.fillStyle = "#687980";
  context.font = `14px ${FONT_FAMILY}`;
  context.fillText(t("reportLimitations"), 956, 1072, 560);
  drawLegend(context);
  return canvas.toDataURL("image/png");
}

