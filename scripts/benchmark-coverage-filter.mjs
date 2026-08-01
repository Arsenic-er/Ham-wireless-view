#!/usr/bin/env node
// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0


import { performance } from "node:perf_hooks";

const GRID_SIZE = 401;
const PIXEL_COUNT = GRID_SIZE * GRID_SIZE;

function positiveIntegerArgument(name, fallback) {
  const index = process.argv.indexOf(name);
  if (index === -1) return fallback;
  const value = Number(process.argv[index + 1]);
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`${name} must be a positive integer`);
  }
  return value;
}

function positiveNumberArgument(name, fallback) {
  const index = process.argv.indexOf(name);
  if (index === -1) return fallback;
  const value = Number(process.argv[index + 1]);
  if (!Number.isFinite(value) || value <= 0) {
    throw new Error(`${name} must be a positive number`);
  }
  return value;
}

const layers = positiveIntegerArgument("--layers", 8);
const iterations = positiveIntegerArgument("--iterations", 120);
const maximumP95Ms = positiveNumberArgument("--max-p95-ms", 150);

const filterBins = Array.from({ length: layers }, (_, layer) => {
  const bins = new Uint8Array(PIXEL_COUNT);
  for (let pixel = 0; pixel < PIXEL_COUNT; pixel += 1) {
    bins[pixel] = (pixel * 17 + layer * 29) % 82;
  }
  return bins;
});
const originalAlpha = Array.from({ length: layers }, (_, layer) => {
  const alpha = new Uint8ClampedArray(PIXEL_COUNT);
  alpha.fill(180 + (layer % 4) * 16);
  return alpha;
});
const rgba = Array.from(
  { length: layers },
  () => new Uint8ClampedArray(PIXEL_COUNT * 4).fill(255),
);

function applyThreshold(minimumVisibleBin) {
  let checksum = 0;
  for (let layer = 0; layer < layers; layer += 1) {
    const pixels = rgba[layer];
    const alpha = originalAlpha[layer];
    const bins = filterBins[layer];
    for (let pixel = 0; pixel < PIXEL_COUNT; pixel += 1) {
      const bin = bins[pixel];
      const value = bin !== 0 && bin >= minimumVisibleBin ? alpha[pixel] : 0;
      pixels[pixel * 4 + 3] = value;
      checksum = (checksum + value) >>> 0;
    }
  }
  return checksum;
}

for (let warmup = 0; warmup < 20; warmup += 1) {
  applyThreshold((warmup % 81) + 1);
}

const samples = [];
let checksum = 0;
for (let iteration = 0; iteration < iterations; iteration += 1) {
  const startedAt = performance.now();
  checksum ^= applyThreshold((iteration % 81) + 1);
  samples.push(performance.now() - startedAt);
}
samples.sort((left, right) => left - right);
const percentile = (fraction) =>
  samples[Math.min(samples.length - 1, Math.ceil(samples.length * fraction) - 1)];
const result = {
  scope: "server-cpu-only",
  grid: `${GRID_SIZE}x${GRID_SIZE}`,
  layers,
  iterations,
  medianMs: Number(percentile(0.5).toFixed(3)),
  p95Ms: Number(percentile(0.95).toFixed(3)),
  maxMs: Number(samples.at(-1).toFixed(3)),
  maximumP95Ms,
  checksum,
};
console.log(JSON.stringify(result));
if (result.p95Ms > maximumP95Ms) {
  console.error(
    `coverage filter p95 ${result.p95Ms} ms exceeded ${maximumP95Ms} ms`,
  );
  process.exitCode = 1;
}
