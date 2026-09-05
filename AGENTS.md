# Verenu agent instructions

## Project model

Verenu is a Tauri 2 desktop dictation app for Windows and macOS. The Rust
backend owns native integration, SQLite, settings storage, provider calls, and
the dictation pipeline; the Svelte 5 frontend owns the application UI. Keep
the app privacy-aware, lightweight, and offline-capable where the feature
allows it. Do not replace Tauri with Electron or turn the product into a web
app.

Contexts are the primary home for app and website targets, tone, cleanup
intensity, custom instructions, vocabulary, and snippets. Legacy Dictionary,
Snippets, and App Mappings pages remain for compatibility; do not build new
features on them without an explicit request.

## Before changing code

- Read the relevant docs and nearby implementation before making assumptions.
- Read `DESIGN.md` before adding or restyling frontend controls. Reuse shared
  buttons, dropdowns, motion, colors, and selectors.
- Treat `docs/ROADMAP.md` as recorded bugs and future context, not approved
  scope.
- Check the `Unreleased` section of `docs/CHANGELOG.md`; the latest release is
  currently 0.18.0.
- Read the task-specific playbook in `Agent-Skills/` when one applies.

## Branches, commits, and files

- `master` is the only integration and release branch. Create short-lived
  branches from it and open pull requests directly into `master`; never use a
  `dev` integration branch.
- In GitButler workspaces, the target should be `origin/master`. Use `but` for
  Git writes and inspect dirty files or hunks before committing. Do not absorb
  unrelated work from other agents.
- Use `apply_patch` for local edits. Never kill a process you did not start.
- Keep API keys, dictated text, clipboard contents, private screenshots, and
  local secrets out of code, logs, fixtures, commits, and test output.
- Every commit needs an agent co-author trailer. Do not add a co-author note
  at the bottom of a pull request description.

## Commands and verification

```bash
npm install
npm run tauri dev
npm run check
npm run lint
npm run test:rust
npm test
npm run test:smoke
```

- `npm run check` is the minimum frontend check. Use `npm run lint` when Rust
  or frontend linting is relevant.
- Use `cargo test --manifest-path src-tauri/Cargo.toml <test_name>` for a
  focused Rust test, and `npm run test:unit` for focused frontend tests.
- `npm test` runs the deterministic fast profile. Live API and native profiles
  are opt-in; do not require credentials for ordinary checks.
- Keep `tests/smoke/` frozen. Fix application code when those contracts fail.
- Changes involving hotkeys, injection, permissions, onboarding, or Keychain
  need macOS verification when practical.
- For UI changes, use the relevant Playwright or integration test and report
  any skipped manual verification.

## Safety and contracts

- Store API keys only in the native credential store. Never put secrets in
  SQLite, `settings.json`, logs, or IPC responses. Use store-key constants from
  `src-tauri/src/data/store/mod.rs`, not raw key strings.
- Keep frontend settings types in `src/lib/settings.ts` synchronized with
  backend validation and store constants.
- Update application versions together in `package.json`,
  `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml`.
- Release installers belong under `installers/<version>/` with matching
  checksums; the scheduled nightly release reads `master`. Run the manual
  installer workflow from `master` for a production build.
- The Windows low-level hotkey callback must return quickly, with pipeline work
  spawned outside the hook. Never hide the pill window; hidden WebView state
  events can be lost.
- Text injection is clipboard-based and must restore focus and selection safely.
  Do not log raw dictated or cleaned text.
- Preserve exact selectors asserted by `tests/smoke/`, including accessibility
  roles and required classes, when changing the UI.

## Useful references

- Architecture and testing: `docs/ARCHITECTURE.md`, `docs/TESTING.md`
- Contribution and release flow: `docs/CONTRIBUTING.md`, `docs/RELEASE.md`
- Privacy: `docs/DATA_AND_PRIVACY.md`
- Shared frontend controls: `DESIGN.md`, `src/ui.css`
- CI and PR policy: `.github/workflows/`, `.github/pull_request_template.md`
