# Field-strength filter performance gate

`scripts/benchmark-coverage-filter.mjs` replays the alpha update path for eight 401 x 401 overlays. It runs 20 warmups and 120 measured iterations by default and reports median, p95, and maximum time:

```bash
node scripts/benchmark-coverage-filter.mjs
```

The automated gate is `p95 < 150 ms` for this server CPU microbenchmark. This deliberately generous limit detects order-of-magnitude regressions; it is not a 60 FPS promise and is not a substitute for a Windows device result. The server script excludes MapLibre texture uploads, browser compositing, GPU drivers, pointer events, and Windows WebView2 scheduling.

Before a Windows release, drag the threshold on the target 64-bit Windows device with eight coverage layers visible and record input responsiveness and map frame time. The implementation should coalesce work to animation frames, but the timing from this script must not be presented as a Windows end-to-end result.
