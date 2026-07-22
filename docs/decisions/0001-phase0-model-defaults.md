# ADR-0001: Phase 0 propagation defaults

- Status: accepted for Phase 0 benchmarks only
- Date: 2026-07-16
- Scope: technical validation; not a final production land/water decision

## Decision

The first reproducible benchmark uses a versioned `ModelDefaults::PHASE0_V1`
structure with:

- climate `5` (continental temperate);
- sea-level surface refractivity `N_0 = 301` N-units;
- variability mode `mdvar = 12` (mobile mode with location variability
  eliminated, following the official v1.4 point-to-point examples);
- time/location/situation at `50/50/50`;
- provisional land values `epsilon = 15` and `sigma = 0.008 S/m`, matching the
  official v1.4 point-to-point reference cases;
- vertical polarization unless a test explicitly selects horizontal.

These values make the regression and performance baselines deterministic. They
do not settle China's regional climate treatment, the final land parameters,
the unified water parameters, or the mixed-path interpolation rule. Those
remain blocked on a separately sourced model/data decision before production.

## Consequences

- Phase 0 results are reproducible and contain no scattered model constants.
- Official NTIA reference cases retain their own per-row inputs and do not use
  these defaults.
- Benchmark reports must record defaults version `phase0-v1`.
- A later defaults change requires a new version and complete propagation
  regression, continuity, land/water, and performance tests.

