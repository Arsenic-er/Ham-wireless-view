// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import type { MapPoint, SessionCoverageResult } from "./types";

export const MAX_SESSION_COVERAGES = 8;

function samePoint(left: MapPoint, right: MapPoint): boolean {
  return left.lat === right.lat && left.lon === right.lon;
}

export function mergeSessionCoverage(
  current: readonly SessionCoverageResult[],
  next: SessionCoverageResult,
): SessionCoverageResult[] {
  return [
    ...current.filter(
      (entry) => !samePoint(entry.result.center, next.result.center),
    ),
    next,
  ].slice(-MAX_SESSION_COVERAGES);
}
