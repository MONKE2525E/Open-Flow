# Verenu Review Instructions

Verenu is a Tauri 2 desktop app for Windows, with a Rust backend and a Svelte 5 TypeScript frontend. Treat it as a desktop app, not a normal web app or Electron app.

Review Rust changes for Windows integration bugs, async blocking, hotkey hook timing, clipboard injection timing, UI Automation COM initialization, and API-key handling. API keys must never be written to SQLite, logs, telemetry, or PR comments.

Review frontend changes against the existing Svelte 5 patterns and smoke-test contracts. Do not recommend changes to files under `tests/smoke/`; app code must satisfy those tests.

Keep memory usage in mind. The app targets about 200 MB idle, so call out heavy frontend dependencies, long-lived buffers, unbounded caches, and extra background work.

When reviewing dependency changes, be strict about supply-chain risk. This repo handles dictation text and user API keys, so new runtime dependencies need a clear reason.

Expected checks are `npm run check`, `npm run lint`, and `npm run test:rust`. For UI changes, also run the existing smoke tests with Playwright.
