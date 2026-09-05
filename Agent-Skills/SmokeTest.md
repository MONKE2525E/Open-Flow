---
name: smoke-tests
description: Minimal guide to run and fix Verenu smoke tests by changing app code, never smoke tests.
---

# Smoke Tests - Verenu

## Non-negotiables

1. Do not edit `tests/smoke/*` unless the user explicitly asks.
2. Fix app code, not tests (`src/lib/views/*`, `src/lib/components/*`).
3. No fake pass hacks (no hidden nodes, hardcoded values, or selector-only shims).

## Current test contracts

- `test.cjs` (1420): app loads, `.nav-item` exists, `.app` visible.
- `test-app.cjs` (1420): App Mappings add flow works end-to-end.
- `playwright-test-ui.cjs` (1420): nav headings render, settings sections render, privacy toggles flip, settings closes.
- `playwright-test-fixes.cjs` (1420):
  - Advanced: `.toggle` exists
  - Advanced: `span.gain-value` exists
  - General: `button.badge.key-badge` contains `Ctrl`
  - About: `button.btn-ghost` contains `github.com/MONKE2525E/Verenu`
- `playwright-test-state.cjs` (1420): model and advanced-toggle persistence survive close/reopen.
- `playwright-test-pipeline.cjs` (no server): live API smoke checks, `SKIP` when keys are absent.
- `tests/integration/*`: browser-dev coverage for onboarding, settings/state surfaces, and offline handling.
- Rust pipeline fixture tests: deterministic fallback, cleanup, cache, history, and injection behavior in `cargo test`.

## Required class/selector contracts

- `nav-item`
- `app`
- `h1.page-h`
- `settings-modal`
- `settings-nav-item`
- `h2.settings-h`
- `toggle` with `role="switch"` and `aria-checked`
- `badge key-badge`
- `btn-ghost`
- `model-row` (+ `active`)

## Run commands

```bash
# Recommended default PR-grade gate
python tests/OnePyFone.py

# Explicit profiles
python tests/OnePyFone.py --profile fast
python tests/OnePyFone.py --profile live
python tests/OnePyFone.py --profile native
python tests/OnePyFone.py --profile full

# Browser tests run in parallel by default
python tests/OnePyFone.py --suite ui,accessibility,animation --workers 3 --fresh-server

# Target one stable test ID; use --sequential while debugging
python tests/OnePyFone.py --test accessibility.settings-focus --sequential

# npm entrypoints
npm test
npm run test:all
npm run test:live
npm run test:native

# Direct frozen smoke runs
npm run dev
node tests/smoke/playwright-test-ui.cjs
node tests/smoke/playwright-test-fixes.cjs
node tests/smoke/playwright-test-state.cjs
```

## Manual scripts

These are not part of any automated profile — run them by hand when investigating
the relevant subsystem:

- `tests/manual/auto-learn-harness.ps1` — drives the auto-learn correction flow on
  Windows. Starts the dev server, then exercises dictation/correction so you can
  watch promotion behavior. Run: `pwsh tests/manual/auto-learn-harness.ps1`.
- `tests/manual/hotkey.cjs` — manual hotkey/injection probe against the dev server.
- `tests/manual/setup-layout-bounds.js` — manual setup-wizard layout-bounds check.
- Pill cross-monitor placement (no script, requires real mixed-DPI hardware):
  on a Windows dual-monitor setup with different scale factors (e.g. 1440p +
  1080p), dictate on one monitor, switch focus to the other, dictate again,
  and confirm the pill is fully visible (no bottom clipping) on that *first*
  dictation on the new monitor — not just the second. Repeat in both monitor
  directions and at 100/125/150% scaling. Regression-tested at the unit
  level by `should_animate_cross_monitor_move` in
  `src-tauri/src/pipeline/pill_position.rs`.

## Notes

- `tests/smoke/*` stays frozen. Add new coverage in `tests/integration/` or Rust tests.
- The `fast` profile is the default and should stay deterministic: no live APIs, no microphone capture, no clipboard injection, no OS permission prompts.
- The `live` profile is opt-in and must never print API keys, clipboard contents, or real dictated text.
- The `native` profile is opt-in for platform/manual-adjacent checks and should explain skip reasons clearly.
- Use `--fresh-server` to avoid stale listener/process false failures.
- OnePyFone now shows live elapsed seconds while tests run.

