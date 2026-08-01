// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from "vitest";

import {
  coverageCircleCoordinates,
  directWgs84,
  heatmapImageCorners,
  inverseWgs84DistanceM,
  maidenheadLocator,
} from "./geodesy";

function haversineDistanceM(
  from: { lat: number; lon: number },
  to: { lat: number; lon: number },
): number {
  const radiusM = 6_371_008.8;
  const toRadians = (value: number) => (value * Math.PI) / 180;
  const phi1 = toRadians(from.lat);
  const phi2 = toRadians(to.lat);
  const deltaPhi = phi2 - phi1;
  const deltaLambda = toRadians(to.lon - from.lon);
  const a =
    Math.sin(deltaPhi / 2) ** 2 +
    Math.cos(phi1) * Math.cos(phi2) * Math.sin(deltaLambda / 2) ** 2;
  return 2 * radiusM * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a));
}

describe("WGS84 display geometry", () => {
  const center = { lat: 30.5, lon: 103.5 };

  it("builds a closed fixed-radius coverage circle", () => {
    const coordinates = coverageCircleCoordinates(center);
    expect(coordinates).toHaveLength(181);
    expect(coordinates.at(-1)).toEqual(coordinates[0]);
    for (const [lon, lat] of coordinates.slice(0, -1)) {
      expect(Math.abs(haversineDistanceM(center, { lon, lat }) - 200_000)).toBeLessThan(
        700,
      );
    }
  });

  it("places image corners on the 200 km square diagonals", () => {
    for (const [lon, lat] of heatmapImageCorners(center)) {
      expect(
        Math.abs(haversineDistanceM(center, { lon, lat }) - 200_000 * Math.sqrt(2)),
      ).toBeLessThan(900);
    }
  });

  it("round-trips the exact 1 km and 200 km WGS84 link boundaries", () => {
    for (const distanceM of [1_000, 200_000]) {
      const endpoint = directWgs84(center, 73, distanceM);
      expect(inverseWgs84DistanceM(center, endpoint)).toBeCloseTo(
        distanceM,
        5,
      );
    }
  });

  it("keeps cardinal directions stable", () => {
    const north = directWgs84(center, 0, 200_000);
    const east = directWgs84(center, 90, 200_000);
    expect(north.lat).toBeGreaterThan(center.lat);
    expect(Math.abs(north.lon - center.lon)).toBeLessThan(1e-9);
    expect(east.lon).toBeGreaterThan(center.lon);
  });

  it("formats a six-character Maidenhead locator", () => {
    expect(maidenheadLocator(center)).toBe("OM10sm");
  });
});
