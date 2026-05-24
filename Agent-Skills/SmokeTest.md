---
name: smoke-tests
description: Minimal guide to run and fix Open Flow smoke tests by changing app code, never smoke tests.
---

# Smoke Tests - Open Flow

## Non-negotiables

1. Do not edit `tests/smoke/*` unless the user explicitly asks.
2. Fix app code, not tests (`src/lib/views/*`, `src/lib/components/*`).
3. No fake pass hacks (no hidden nodes, hardcoded values, or selector-only shims).

## Current test contracts

- `test.cjs` (5173): app loads, `.nav-item` exists, `.app` visible.
- `test-app.cjs` (5173): App Mappings add flow works end-to-end.
- `playwright-test-ui.cjs` (1420): nav headings render, settings sections render, privacy toggles flip, settings closes.
- `playwright-test-fixes.cjs` (1420):
  - Advanced: `.toggle` exists
  - Advanced: `span.gain-value` exists
  - General: `button.badge.key-badge` contains `Ctrl`
  - About: `button.btn-ghost` contains `github.com/MONKE2525E/Open-Flow`
- `playwright-test-state.cjs` (1420): model and advanced-toggle persistence survive close/reopen.
- `playwright-test-pipeline.cjs` (no server): WAV + API pipeline checks, `SKIP` when keys are absent.

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
# Recommended unified run
python tests/OnePyFone.py --suite all --fresh-server

# Parallel-safe suites only
python tests/OnePyFone.py --suite ui,animation --parallel --workers 3 --fresh-server

# Direct runs
npm run dev
node tests/smoke/test.cjs
node tests/smoke/test-app.cjs

npm run tauri dev
node tests/smoke/playwright-test-ui.cjs
node tests/smoke/playwright-test-fixes.cjs
node tests/smoke/playwright-test-state.cjs

node tests/smoke/playwright-test-pipeline.cjs
```

## Notes

- Use `--fresh-server` to avoid stale listener/process false failures.
- OnePyFone now shows live elapsed seconds while tests run.
