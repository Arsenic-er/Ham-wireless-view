// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import i18n from "../i18n";
import type { LinkAnalysisResult } from "../lib/types";
import { LinkProfileDialog } from "./LinkProfileDialog";

const result: LinkAnalysisResult = {
  schemaVersion: 1,
  classification: "direct-los",
  classificationReason: "clear",
  distanceM: 1_000,
  initialBearingDeg: 90,
  finalBearingDeg: 270,
  frequencyMhz: 145,
  wavelengthM: 2.0675,
  sampleSpacingM: 1_000,
  sampleCount: 2,
  effectiveEarthRadiusM: 8_494_678.4,
  kFactor: 4 / 3,
  txGroundElevationM: 100,
  rxGroundElevationM: 100,
  txAntennaElevationM: 120,
  rxAntennaElevationM: 120,
  geometricLos: true,
  fresnelClearance60: true,
  minimumLosClearanceM: 20,
  minimumFresnelClearanceRatio: 1,
  criticalSampleIndex: 0,
  itmMode: "line-of-sight",
  itmBasicTransmissionLossDb: 90,
  itmWarnings: 0,
  waterFraction: 0,
  coPolarizedReferencePowerDbm: -70,
  polarizationMismatchLossDb: 0,
  predictedRxPowerDbm: -70,
  receiverThresholdDbm: -120,
  linkMarginDb: 50,
  critical: false,
  profile: [
    {
      distanceM: 0,
      lat: 30,
      lon: 103,
      terrainElevationM: 100,
      earthBulgeM: 0,
      adjustedTerrainM: 100,
      losHeightM: 120,
      fresnelRadiusM: 0,
    },
    {
      distanceM: 1_000,
      lat: 30,
      lon: 103.01,
      terrainElevationM: 100,
      earthBulgeM: 0,
      adjustedTerrainM: 100,
      losHeightM: 120,
      fresnelRadiusM: 0,
    },
  ],
};

function Harness({ onClose }: { onClose: () => void }) {
  const [dimmed, setDimmed] = useState(false);
  const [mapClicks, setMapClicks] = useState(0);
  return (
    <>
      <button type="button" onClick={() => setMapClicks((count) => count + 1)}>
        map surface {mapClicks}
      </button>
      <LinkProfileDialog
        result={result}
        dimmed={dimmed}
        onActivate={() => setDimmed(false)}
        onInteractOutside={() => setDimmed(true)}
        onClose={onClose}
      />
    </>
  );
}

describe("LinkProfileDialog", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });

  afterEach(() => cleanup());

  it("dims after an outside pointerdown without blocking the underlying map action", () => {
    render(<Harness onClose={() => undefined} />);

    const dialog = screen.getByRole("dialog", { name: "链路剖面分析" });
    const mapSurface = screen.getByRole("button", { name: "map surface 0" });
    expect(dialog.getAttribute("aria-modal")).toBe("false");
    expect(dialog.closest(".modal-backdrop")).toBeNull();

    fireEvent.pointerDown(mapSurface);
    fireEvent.click(mapSurface);

    expect(dialog.classList.contains("is-dimmed")).toBe(true);
    expect(screen.getByRole("button", { name: "map surface 1" })).toBeTruthy();

    fireEvent.pointerDown(dialog);
    expect(dialog.classList.contains("is-dimmed")).toBe(false);
  });

  it("closes on the close button and Escape", () => {
    const onClose = vi.fn();
    render(<Harness onClose={onClose} />);

    fireEvent.click(screen.getByRole("button", { name: "关闭链路剖面" }));
    expect(onClose).toHaveBeenCalledTimes(1);

    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
