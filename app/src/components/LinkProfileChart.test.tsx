// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import i18n from "../i18n";
import type { LinkAnalysisResult } from "../lib/types";
import {
  classificationReasonKey,
  distanceTicks,
  LinkProfileChart,
} from "./LinkProfileChart";

const result: LinkAnalysisResult = {
  schemaVersion: 1,
  classification: "obstructed-usable",
  classificationReason: "diffraction",
  distanceM: 200_000,
  initialBearingDeg: 90,
  finalBearingDeg: 270,
  frequencyMhz: 145,
  wavelengthM: 2.0675,
  sampleSpacingM: 100_000,
  sampleCount: 3,
  effectiveEarthRadiusM: 8_494_678.4,
  kFactor: 4 / 3,
  txGroundElevationM: 100,
  rxGroundElevationM: 120,
  txAntennaElevationM: 120,
  rxAntennaElevationM: 130,
  geometricLos: false,
  fresnelClearance60: false,
  minimumLosClearanceM: -30,
  minimumFresnelClearanceRatio: -0.1,
  criticalSampleIndex: 1,
  itmMode: "diffraction",
  itmBasicTransmissionLossDb: 145,
  itmWarnings: 0,
  waterFraction: 0,
  coPolarizedReferencePowerDbm: -105,
  polarizationMismatchLossDb: 0,
  predictedRxPowerDbm: -105,
  receiverThresholdDbm: -120,
  linkMarginDb: 15,
  critical: false,
  profile: [
    { distanceM: 0, lat: 30, lon: 103, terrainElevationM: 100, earthBulgeM: 0, adjustedTerrainM: 100, losHeightM: 120, fresnelRadiusM: 0 },
    { distanceM: 100_000, lat: 30.5, lon: 104, terrainElevationM: 250, earthBulgeM: 588.6, adjustedTerrainM: 838.6, losHeightM: 125, fresnelRadiusM: 321.5 },
    { distanceM: 200_000, lat: 31, lon: 105, terrainElevationM: 120, earthBulgeM: 0, adjustedTerrainM: 120, losHeightM: 130, fresnelRadiusM: 0 },
  ],
};

describe("LinkProfileChart", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });

  it("uses dynamic 1/2/5 distance ticks with exact endpoints", () => {
    const ticks = distanceTicks(200_000, 960);
    expect(ticks[0]).toEqual({ valueM: 0, label: "0 km" });
    expect(ticks.at(-1)).toEqual({ valueM: 200_000, label: "200 km" });
    expect(ticks.length).toBeGreaterThanOrEqual(5);
    const step = ticks[1].valueM - ticks[0].valueM;
    const exponent = 10 ** Math.floor(Math.log10(step));
    expect([1, 2, 5, 10]).toContain(step / exponent);
  });

  it("draws terrain, LOS, the full F1 envelope, 60% boundary, and critical point", () => {
    const { container } = render(<LinkProfileChart result={result} />);
    expect(container.querySelector(".profile-terrain-fill")?.getAttribute("d")).toContain("L");
    expect(container.querySelector(".profile-los-line")?.getAttribute("d")).toContain("L");
    expect(container.querySelector(".profile-fresnel-fill")?.getAttribute("d")).toContain("Z");
    expect(container.querySelector(".profile-fresnel-sixty")?.getAttribute("d")).toContain("L");
    expect(container.querySelector(".profile-critical-point")).toBeTruthy();
    expect(screen.getByText(/纵向比例已放大/)).toBeTruthy();
  });

  it("maps precise backend reasons without contradicting usable classifications", () => {
    expect(
      classificationReasonKey(
        "positive-margin-severe-obstruction-modeled-usable",
      ),
    ).toBe("linkReasonSevere");
    expect(
      classificationReasonKey(
        "positive-margin-fresnel-intrusion-geometric-los",
      ),
    ).toBe("linkReasonFresnelGeometricLos");
    expect(
      classificationReasonKey("positive-margin-fresnel-obstructed"),
    ).toBe("linkReasonFresnel");
    expect(
      classificationReasonKey("positive-margin-diffraction"),
    ).toBe("linkReasonDiffraction");
    expect(
      classificationReasonKey("positive-margin-troposcatter"),
    ).toBe("linkReasonTroposcatter");
    expect(classificationReasonKey("future-model-code")).toBe(
      "linkReasonModel",
    );

    render(
      <LinkProfileChart
        result={{
          ...result,
          classificationReason:
            "positive-margin-fresnel-intrusion-geometric-los",
          geometricLos: true,
        }}
      />,
    );
    expect(
      screen.getByText(/几何射线仍然通视.*60% 第一菲涅尔区净空不足/),
    ).toBeTruthy();
    expect(screen.getAllByText(/不保证现场实际通联/).length).toBeGreaterThan(0);
  });

  it("uses complete profile samples for the pointer tooltip", () => {
    const { container } = render(<LinkProfileChart result={result} />);
    const svg = container.querySelector("svg");
    if (!svg) throw new Error("missing profile SVG");
    Object.defineProperty(svg, "getBoundingClientRect", {
      configurable: true,
      value: () => ({ left: 0, top: 0, width: 960, height: 286, right: 960, bottom: 286, x: 0, y: 0, toJSON: () => ({}) }),
    });
    fireEvent.pointerMove(svg, { clientX: 480, clientY: 100 });
    const tooltip = container.querySelector(".profile-tooltip");
    expect(tooltip?.textContent).toContain("588.6 m");
    expect(tooltip?.textContent).toContain("F1: ±321.5 m");
  });
});
