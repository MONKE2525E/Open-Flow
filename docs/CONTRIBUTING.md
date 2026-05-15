# Contributing to Open Flow

Thanks for helping improve Open Flow. This project is a Windows-first Tauri desktop app, not an Electron app and not a hosted web app. Keep changes focused, lightweight, and privacy-respecting.

## Before You Start

- Read `CLAUDE.md` for repo-specific architecture notes and gotchas.
- Check `docs/ROADMAP.md` before working on large features, but do not treat future roadmap ideas as approved scope.
- Open an issue or discussion before broad rewrites, new providers, major UI changes, or anything that affects the transcription pipeline.
- Do not commit personal information, API keys, logs with user data, screenshots with private text, or local machine paths that identify someone.

## Setup

Prerequisites:

- Windows 10 or 11 with WebView2
- Node.js 18+
- Rust and Cargo

Install dependencies:

```bash
npm install
```

Run the app in development:

```bash
npm run tauri dev
```

Run the frontend-only Vite server:

```bash
npm run dev
```

## Development Rules

- Keep Tauri and WebView2. Do not replace the app shell with Electron.
- Keep API keys out of SQLite, logs, screenshots, fixtures, and test output.
- Use constants from `src-tauri/src/data/store.rs` for store keys. Do not add raw string keys.
- Keep `tests/smoke/` as a contract. Fix app code when smoke tests fail.
- Keep dependencies lean. This app has a low idle RAM target, so heavy packages need a damn good reason.
- Follow the existing Svelte, TypeScript, Rust, and Tailwind patterns before inventing new abstractions.

## Testing

Run the checks that match your change.

Frontend type check:

```bash
npm run check
```

Frontend and Rust lint:

```bash
npm run lint
```

Rust tests:

```bash
npm run test:rust
```

Smoke tests with the Vite dev server running on port 5173:

```bash
npm run dev
npm run test:smoke
npm run test:smoke:state
```

For UI-facing changes, use Playwright where applicable and include what you tested in the PR.

## Version Changes

When bumping the app version, update all three files together:

- `package.json`
- `src-tauri/tauri.conf.json`
- `src-tauri/Cargo.toml`

Do not hardcode app versions in Svelte files.

## Pull Requests

Before opening a PR:

- Keep the scope tight and explain the reason for the change.
- Add or update tests when behavior changes.
- Run the relevant checks and list the results.
- Include screenshots or short recordings for visible UI changes.
- Call out privacy, API provider, hotkey, clipboard, or database impacts.

If a check is not applicable or cannot be run, say so directly in the PR. Do not hand-wave it.
