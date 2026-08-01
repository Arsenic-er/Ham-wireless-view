// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type {
  LinkAnalysisResult,
  LinkProfileSample,
} from "../lib/types";

export interface DistanceTick {
  valueM: number;
  label: string;
}

function niceStep(raw: number): number {
  if (!Number.isFinite(raw) || raw <= 0) return 1;
  const exponent = 10 ** Math.floor(Math.log10(raw));
  const fraction = raw / exponent;
  const niceFraction =
    fraction <= 1 ? 1 : fraction <= 2 ? 2 : fraction <= 5 ? 5 : 10;
  return niceFraction * exponent;
}

export function distanceTicks(distanceM: number, widthPx: number): DistanceTick[] {
  if (!Number.isFinite(distanceM) || distanceM <= 0) return [];
  const targetCount = Math.max(5, Math.min(9, Math.floor(widthPx / 115)));
  const step = niceStep(distanceM / targetCount);
  const useKilometres = distanceM >= 10_000;
  const ticks: DistanceTick[] = [];
  for (let value = 0; value < distanceM; value += step) {
    const display = useKilometres ? value / 1000 : value;
    ticks.push({
      valueM: value,
      label: useKilometres
        ? `${Number(display.toFixed(display < 10 ? 1 : 0))} km`
        : `${Math.round(display)} m`,
    });
  }
  const finalDisplay = useKilometres ? distanceM / 1000 : distanceM;
  ticks.push({
    valueM: distanceM,
    label: useKilometres
      ? `${Number(finalDisplay.toFixed(finalDisplay < 10 ? 1 : 0))} km`
      : `${Math.round(finalDisplay)} m`,
  });
  return ticks;
}

function yTicks(minimum: number, maximum: number): number[] {
  const span = Math.max(1, maximum - minimum);
  const step = niceStep(span / 5);
  const first = Math.ceil(minimum / step) * step;
  const ticks: number[] = [];
  for (let value = first; value <= maximum + step * 1e-9; value += step) {
    ticks.push(value);
  }
  return ticks;
}

function closestSample(
  samples: readonly LinkProfileSample[],
  distanceM: number,
): LinkProfileSample {
  let low = 0;
  let high = samples.length - 1;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (samples[middle].distanceM < distanceM) low = middle + 1;
    else high = middle;
  }
  if (low === 0) return samples[0];
  const previous = samples[low - 1];
  const current = samples[low];
  return Math.abs(previous.distanceM - distanceM) <=
    Math.abs(current.distanceM - distanceM)
    ? previous
    : current;
}

type LinkReasonKey =
  | "linkReasonDirect"
  | "linkReasonDiffraction"
  | "linkReasonFresnel"
  | "linkReasonFresnelGeometricLos"
  | "linkReasonRay"
  | "linkReasonTroposcatter"
  | "linkReasonSevere"
  | "linkReasonBudget"
  | "linkReasonPolarization"
  | "linkReasonModel";

export function classificationReasonKey(reason: string): LinkReasonKey {
  const normalized = reason.toLowerCase();
  if (normalized.includes("polar")) return "linkReasonPolarization";
  if (
    normalized.includes("negative-link-margin") ||
    normalized.includes("budget") ||
    normalized.includes("threshold") ||
    normalized.includes("power")
  ) {
    return "linkReasonBudget";
  }
  if (normalized.includes("severe") || normalized.includes("terrain")) {
    return "linkReasonSevere";
  }
  if (normalized.includes("diffraction")) {
    return "linkReasonDiffraction";
  }
  if (normalized.includes("troposcatter")) {
    return "linkReasonTroposcatter";
  }
  if (
    normalized.includes("fresnel") &&
    (normalized.includes("geometric-los") ||
      normalized.includes("geometric_los"))
  ) {
    return "linkReasonFresnelGeometricLos";
  }
  if (normalized.includes("fresnel")) {
    return "linkReasonFresnel";
  }
  if (
    normalized.includes("ray") ||
    normalized.includes("blocked") ||
    normalized.includes("obstructed")
  ) {
    return "linkReasonRay";
  }
  if (
    normalized.includes("direct") ||
    normalized.includes("clear") ||
    normalized.includes("line-of-sight")
  ) {
    return "linkReasonDirect";
  }
  return "linkReasonModel";
}

export function LinkProfileChart({ result }: { result: LinkAnalysisResult }) {
  const { t } = useTranslation();
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [width, setWidth] = useState(960);
  const [cursor, setCursor] = useState<LinkProfileSample | null>(null);
  const height = 286;
  const margins = { left: 62, right: 22, top: 18, bottom: 40 };
  const plotWidth = Math.max(120, width - margins.left - margins.right);
  const plotHeight = height - margins.top - margins.bottom;

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const update = () => setWidth(Math.max(420, Math.round(container.clientWidth || 960)));
    update();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(update);
    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  const geometry = useMemo(() => {
    const values = result.profile.flatMap((sample) => [
      sample.adjustedTerrainM,
      sample.losHeightM - sample.fresnelRadiusM,
      sample.losHeightM + sample.fresnelRadiusM,
    ]);
    let minimum = Math.min(...values);
    let maximum = Math.max(...values);
    const padding = Math.max(10, (maximum - minimum) * 0.08);
    minimum -= padding;
    maximum += padding;
    const x = (distanceM: number) =>
      margins.left + (distanceM / result.distanceM) * plotWidth;
    const y = (elevationM: number) =>
      margins.top + ((maximum - elevationM) / (maximum - minimum)) * plotHeight;
    const line = (selector: (sample: LinkProfileSample) => number) =>
      result.profile
        .map(
          (sample, index) =>
            `${index === 0 ? "M" : "L"} ${x(sample.distanceM).toFixed(2)} ${y(
              selector(sample),
            ).toFixed(2)}`,
        )
        .join(" ");
    const lower = [...result.profile]
      .reverse()
      .map((sample) =>
        `L ${x(sample.distanceM).toFixed(2)} ${y(
          sample.losHeightM - sample.fresnelRadiusM,
        ).toFixed(2)}`,
      )
      .join(" ");
    const fresnelEnvelope = `${line(
      (sample) => sample.losHeightM + sample.fresnelRadiusM,
    )} ${lower} Z`;
    const terrainLine = line((sample) => sample.adjustedTerrainM);
    const terrainArea = `${terrainLine} L ${x(result.distanceM).toFixed(
      2,
    )} ${y(minimum).toFixed(2)} L ${x(0).toFixed(2)} ${y(minimum).toFixed(2)} Z`;
    return {
      minimum,
      maximum,
      x,
      y,
      terrainArea,
      terrainLine,
      losLine: line((sample) => sample.losHeightM),
      fresnelEnvelope,
      fresnelUpper: line((sample) => sample.losHeightM + sample.fresnelRadiusM),
      fresnelLower: line((sample) => sample.losHeightM - sample.fresnelRadiusM),
      fresnelClearance60: line(
        (sample) => sample.losHeightM - sample.fresnelRadiusM * 0.6,
      ),
    };
  }, [plotHeight, plotWidth, result]);

  const critical = result.profile[result.criticalSampleIndex] ?? result.profile[0];
  const xAxisTicks = distanceTicks(result.distanceM, plotWidth);
  const yAxisTicks = yTicks(geometry.minimum, geometry.maximum);
  const classificationKey =
    result.classification === "direct-los"
      ? "linkClassDirect"
      : result.classification === "obstructed-usable"
        ? "linkClassObstructed"
        : "linkClassUnavailable";
  const cursorX = cursor ? geometry.x(cursor.distanceM) : 0;

  return (
    <section
      className="link-profile-panel"
      aria-label={t("linkProfileAria")}
      ref={containerRef}
    >
      <div className="link-profile-summary">
        <div>
          <span>{t("linkAnalysisResult")}</span>
          <strong className={`link-classification ${result.classification}`}>
            {t(classificationKey)}
          </strong>
        </div>
        <div>
          <span>{t("linkDistance")}</span>
          <strong>{(result.distanceM / 1000).toFixed(2)} km</strong>
        </div>
        <div>
          <span>{t("predictedRxPower")}</span>
          <strong>{result.predictedRxPowerDbm.toFixed(1)} dBm</strong>
        </div>
        <div>
          <span>{t("linkMargin")}</span>
          <strong>{result.linkMarginDb >= 0 ? "+" : ""}{result.linkMarginDb.toFixed(1)} dB</strong>
        </div>
        {result.critical && <em>{t("criticalResult")}</em>}
      </div>
      <div className="link-profile-diagnostics" aria-label={t("linkDecisionEvidence")}>
        <p className="link-reason">
          <strong>{t("linkDecisionEvidence")}</strong>
          <span>{t(classificationReasonKey(result.classificationReason))}</span>
        </p>
        <div className="link-diagnostic-values">
          <span>
            {t("linkGeometricLos")}: <strong>{t(result.geometricLos ? "linkYes" : "linkNo")}</strong>
          </span>
          <span>
            {t("linkFresnelClear60")}: <strong>{t(result.fresnelClearance60 ? "linkYes" : "linkNo")}</strong>
          </span>
          <span>
            {t("linkItmMode")}: <strong>{result.itmMode}</strong>
          </span>
          <span>
            {t("linkBasicLoss")}: <strong>{result.itmBasicTransmissionLossDb.toFixed(1)} dB</strong>
          </span>
          <span>
            {t("receiverThreshold")}: <strong>{result.receiverThresholdDbm.toFixed(1)} dBm</strong>
          </span>
        </div>
        {result.polarizationMismatchLossDb > 0 && (
          <p className="link-polarization-warning">
            {t("crossPolarizationAssumption", {
              loss: result.polarizationMismatchLossDb.toFixed(0),
            })}
          </p>
        )}
        <p className="link-prediction-disclaimer">{t("linkPredictionDisclaimer")}</p>
      </div>
      <div className="link-profile-chart-wrap">
        <svg
          className="link-profile-chart"
          viewBox={`0 0 ${width} ${height}`}
          role="img"
          aria-label={t("linkProfileDescription", {
            distance: (result.distanceM / 1000).toFixed(2),
          })}
          onPointerMove={(event) => {
            const bounds = event.currentTarget.getBoundingClientRect();
            const localX = ((event.clientX - bounds.left) / Math.max(1, bounds.width)) * width;
            const clamped = Math.max(margins.left, Math.min(width - margins.right, localX));
            const distance =
              ((clamped - margins.left) / Math.max(1, plotWidth)) * result.distanceM;
            setCursor(closestSample(result.profile, distance));
          }}
          onPointerLeave={() => setCursor(null)}
        >
          <g className="profile-grid">
            {yAxisTicks.map((tick) => (
              <g key={tick}>
                <line
                  x1={margins.left}
                  x2={width - margins.right}
                  y1={geometry.y(tick)}
                  y2={geometry.y(tick)}
                />
                <text x={margins.left - 8} y={geometry.y(tick) + 3} textAnchor="end">
                  {Math.round(tick)}
                </text>
              </g>
            ))}
            {xAxisTicks.map((tick) => (
              <g key={tick.valueM}>
                <line
                  x1={geometry.x(tick.valueM)}
                  x2={geometry.x(tick.valueM)}
                  y1={margins.top}
                  y2={height - margins.bottom}
                />
                <text
                  x={geometry.x(tick.valueM)}
                  y={height - 15}
                  textAnchor={
                    tick.valueM === 0
                      ? "start"
                      : tick.valueM === result.distanceM
                        ? "end"
                        : "middle"
                  }
                >
                  {tick.label}
                </text>
              </g>
            ))}
          </g>
          <path className="profile-fresnel-fill" d={geometry.fresnelEnvelope} />
          <path className="profile-fresnel-edge" d={geometry.fresnelUpper} />
          <path className="profile-fresnel-edge" d={geometry.fresnelLower} />
          <path className="profile-fresnel-sixty" d={geometry.fresnelClearance60} />
          <path className="profile-terrain-fill" d={geometry.terrainArea} />
          <path className="profile-terrain-line" d={geometry.terrainLine} />
          <path className="profile-los-line" d={geometry.losLine} />
          <circle
            className="profile-critical-point"
            cx={geometry.x(critical.distanceM)}
            cy={geometry.y(critical.adjustedTerrainM)}
            r={5}
          />
          <text
            className="profile-axis-title"
            x={14}
            y={margins.top + plotHeight / 2}
            transform={`rotate(-90 14 ${margins.top + plotHeight / 2})`}
            textAnchor="middle"
          >
            {t("elevationAmsl")}
          </text>
          {cursor && (
            <g className="profile-cursor">
              <line
                x1={cursorX}
                x2={cursorX}
                y1={margins.top}
                y2={height - margins.bottom}
              />
              <circle
                cx={cursorX}
                cy={geometry.y(cursor.adjustedTerrainM)}
                r={4}
              />
            </g>
          )}
        </svg>
        {cursor && (
          <div
            className="profile-tooltip"
            style={{
              left: `${Math.min(78, Math.max(3, (cursorX / width) * 100))}%`,
            }}
            role="status"
          >
            <strong>{(cursor.distanceM / 1000).toFixed(2)} km</strong>
            <span>{t("terrainElevation")}: {cursor.terrainElevationM.toFixed(1)} m</span>
            <span>{t("earthBulge")}: {cursor.earthBulgeM.toFixed(1)} m</span>
            <span>{t("radioRayHeight")}: {cursor.losHeightM.toFixed(1)} m</span>
            <span>F1: ±{cursor.fresnelRadiusM.toFixed(1)} m</span>
          </div>
        )}
      </div>
      <div className="link-profile-legend">
        <span className="terrain">{t("terrainWithCurvature")}</span>
        <span className="los">{t("radioRay")}</span>
        <span className="fresnel">{t("firstFresnelZone")}</span>
        <span className="sixty">{t("fresnelSixtyBoundary")}</span>
        <span>{t("verticalExaggerationNotice")}</span>
      </div>
    </section>
  );
}
