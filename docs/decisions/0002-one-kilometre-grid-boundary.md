# ADR-0002: Use ITM at the one-kilometre grid boundary

- Status: accepted
- Date: 2026-07-16

## Context

The initial design proposed free-space loss at distances up to 1 km and ITM
above 1 km. A Phase 0 flat-profile measurement at 145 MHz produced 75.68 dB
free-space loss at 1 km but 89.22 dB ITM loss at 1.001 km: an artificial
13.5 dB boundary jump.

The fixed product grid has 1 km spacing. Its center is the transmitter and has
no receiver result; every valid receiver pixel is therefore at least 1.0 km
away. The pinned NTIA ITM v1.4 implementation accepts exactly 1.0 km with a
successful return, no warning flags, and line-of-sight mode.

## Decision

Use ITM for every valid non-center coverage-grid pixel, including the four
pixels at exactly 1.0 km. Do not blend free-space and ITM loss in the MVP
coverage raster. Keep the free-space function only for diagnostics and future
features outside the fixed coverage grid.

## Consequences

- The heatmap has one propagation model and no artificial model splice.
- The center pixel remains NoData beneath the transmitter marker.
- Any future feature that evaluates distances below 1 km needs a separate,
  tested model decision and cannot silently reuse this coverage-grid rule.

