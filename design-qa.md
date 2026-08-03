<!--
  Ham Wireless View
  Project creator and lead developer: Arsenic-er
  SPDX-FileCopyrightText: 2026 Arsenic-er
  SPDX-License-Identifier: Apache-2.0
-->

# Selection Hint Design QA

## Evidence

- Source visual truth: C:/Users/jiang/AppData/Local/Temp/codex-clipboard-4be1eae0-23f9-41be-a272-d52628326ea2.png.
- Source image: 1567 × 873 px, light-theme coverage mode before selecting the transmitter.
- Intended change: replace the large centered selection card with a compact prompt at the top of the map so the map remains unobstructed.
- Implementation: server workspace /home/ubuntu/hamheatmap, MapView.tsx and styles.css.
- Implementation screenshot: unavailable.
- Implementation viewport, CSS size, and density: not captured.
- State targeted by automated tests: coverage TX selection, link TX selection, and link RX selection.

## Full-view comparison evidence

The source attachment was visible in the conversation and shows the original large centered card obscuring the primary map selection area. The implementation could not be captured through the Codex Desktop in-app browser because browser startup failed in the Windows sandbox with helper_unknown_error: apply deny-read ACLs. A same-viewport visual comparison therefore could not be completed.

## Focused region comparison evidence

No valid focused implementation capture is available. Code-level dimensions are desktop top 62px, max-width 420px, min-height 48px, padding 8px 13px, and a 22 px crosshair; at widths up to 760 px, top 64px and max-width calc(100% - 24px). The prompt keeps pointer-events: none.

## Findings

- [P1] Browser-rendered visual evidence is unavailable.
  - Location: coverage and link selection prompt on the map.
  - Evidence: the source is visible, but there is no browser-rendered implementation screenshot, console inspection, or same-viewport interaction capture.
  - Impact: typography, exact spacing, overlap with map status/style controls, and narrow-screen appearance cannot be certified visually.
  - Fix: refresh the managed validation page through the SSH tunnel and manually confirm the coverage TX, link TX, and link RX states; repeat browser capture when the Codex Desktop ACL issue is resolved.

## Required fidelity surfaces

- Fonts and typography: existing project font stack and four-language copy are preserved; visual rendering is unverified.
- Spacing and layout rhythm: compact dimensions and responsive limits are implemented; same-viewport visual evidence is blocked.
- Colors and visual tokens: existing surface, border, accent, text, and shadow tokens are reused; visual rendering is unverified.
- Image quality and asset fidelity: no new images, icons, or generated assets were introduced.
- Copy and content: existing localized TX/RX selection strings are unchanged.

## Primary interactions and automated evidence

- Coverage prompt appears only before a transmitter is selected.
- Link prompt changes from TX to RX and disappears after both endpoints are selected.
- The prompt does not receive pointer events.
- TypeScript check passed.
- Full frontend suite passed: 17 files / 152 tests.
- Production build passed.
- Source attribution and README synchronization gates passed.
- Browser console inspection and direct pointer interaction remain blocked by the browser startup failure.

## Comparison history

1. Initial source review identified the centered selection card as a P1 obstruction.
2. The implementation moved and reduced the prompt, added responsive constraints, and retained non-blocking pointer behavior.
3. Post-fix visual comparison could not be captured because the managed browser failed before navigation.

## Implementation checklist

- [x] Move coverage selection guidance to the top of the map.
- [x] Apply the same layout to link TX and RX guidance.
- [x] Preserve four-language copy and light/dark theme tokens.
- [x] Keep map pointer input unobstructed.
- [x] Add automated state coverage.
- [ ] Complete same-viewport browser screenshot comparison and console inspection.
- [ ] Manually confirm layout through the current SSH-tunneled validation page.

final result: blocked
