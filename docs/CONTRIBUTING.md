# Contributing to Verenu

Verenu is a Tauri desktop app for Windows and macOS. Keep changes focused, lightweight, privacy-aware, and grounded in how the app actually works.

## Branch Flow

This repo is not trying to force everything through a heavyweight PR loop.

The default workflow is:

1. Most changes go straight onto `dev`.
2. `dev` gets reviewed and tested.
3. Once `dev` is in good shape, it is merged into `master`.
4. Releases are cut from that stabilized path.

If you have direct write access and the change is normal project work, commit to `dev`.

Use a PR when one of these is true:

- You do not have write access
- The change is risky, broad, or hard to review in a direct push
- You want line-by-line discussion before it lands
- The work changes release flow, privacy boundaries, provider behavior, core dictation behavior, or updater behavior

## Before You Start

- Read [`../CLAUDE.md`](../CLAUDE.md) for repo architecture, platform gotchas, and testing notes.
- Read [ARCHITECTURE.md](ARCHITECTURE.md) for the public architecture map.
- Read [TESTING.md](TESTING.md) for the full test matrix.
- Check [ROADMAP.md](ROADMAP.md) for current bugs and long-term context.
- Do not treat roadmap ideas as approved scope by default.
- Do not commit personal information, API keys, clipboard contents, local secrets, or screenshots with private text.
- If you change data flow or retention behavior, update [DATA_AND_PRIVACY.md](DATA_AND_PRIVACY.md).

## Setup

### Prerequisites

- Node.js 18+
- Rust and Cargo
- Python 3.8+ (required by `npm test` because the OnePyFone test runner is a Python script)
- Windows: WebView2
- macOS: Xcode Command Line Tools are recommended

### Install dependencies

```bash
npm install
```

### Run the app

```bash
npm run tauri dev
```

### Frontend-only server

```bash
npm run dev
```

## Platform Notes

### Windows

- Default hold-to-record hotkey is <kbd>Ctrl</kbd> + <kbd>Windows</kbd>.
- API keys live in Windows Credential Manager.
- Verenu uses native Windows APIs for hotkeys, focus tracking, and injection.
- Update installs should open the published installer asset, not auto-run downloaded bytes.

### macOS

- Default hold-to-record hotkey is <kbd>Fn</kbd> + <kbd>Control</kbd>.
- API keys live in Keychain.
- Real macOS testing matters because permissions are part of the feature, not an edge case.
- Changes that touch hotkeys, injection, onboarding, setup, or key storage should be checked on macOS if they can possibly affect it.

## Development Rules

- Keep Tauri. Do not replace the app shell with Electron.
- Keep API keys out of SQLite, logs, screenshots, fixtures, and test output.
- Use constants from [`../src-tauri/src/data/store.rs`](../src-tauri/src/data/store.rs) for store keys. Do not add raw string keys.
- Keep [`../tests/smoke/`](../tests/smoke/) as a contract. Fix app code when smoke tests fail.
- Keep dependencies lean. The app has a low idle RAM target.
- Follow existing Rust, Svelte, TypeScript, and Tailwind patterns before inventing new abstractions.
- If you change how data moves on or off device, document it clearly.
- Privacy-impacting logs must use redacted metadata only. Do not log dictated text, prompt bodies, snippet expansions, raw dictionary terms, API keys, or full local paths.

## File Size Expectations

These are guidelines, not religious law, but you should treat them seriously:

- Route files and settings components should stay under roughly 500 lines unless they are mostly static markup.
- Rust modules should stay under roughly 700 lines unless they are mostly tests or generated-style constants.
- If a UI file mixes persistence, keyboard handling, inspector layout, and modal editing in one place, split it.

## Testing

Run the checks that fit the change.

### Default test pass

```bash
npm test
```

### Other common commands

```bash
npm run check
npm run lint
npm run build
npm run test:rust
npm run test:smoke
npm audit --audit-level=moderate
```

### Dependency and security audit notes

- CI runs `npm audit --audit-level=moderate`.
- CI also installs and runs `cargo audit` against [`../src-tauri/Cargo.lock`](../src-tauri/Cargo.lock).
- Local work should run the same checks when practical, but `cargo audit` does not need to be preinstalled to make ordinary code changes.

### Targeted UI and state checks

```bash
npm run dev
python3 tests/OnePyFone.py --suite ui,state --no-server
```

On Windows, use `python` instead of `python3` if `python3` is not available in your shell.

Rules:

- Live API checks are opt-in.
- Never print API keys, clipboard contents, or real dictated text in test output.
- Keep [`../tests/smoke/`](../tests/smoke/) frozen.
- Add coverage in Rust tests or [`../tests/integration/`](../tests/integration/) when behavior changes.
- For UI-facing changes, use Playwright when it makes sense and say what you tested.

## Version Changes

When bumping the version, update all three files together:

- `package.json`
- [`../src-tauri/tauri.conf.json`](../src-tauri/tauri.conf.json)
- [`../src-tauri/Cargo.toml`](../src-tauri/Cargo.toml)

Do not hardcode version strings in the frontend.

## Review Notes

Whether work lands by direct commit or PR, the bar is the same:

- Keep the scope understandable
- Add or update tests when behavior changes
- Call out privacy, provider, hotkey, clipboard, updater, database, or platform impacts
- Say plainly if something was not tested

If you open a PR, target `dev` unless there is a specific reason not to.

## Related Docs

<p align="center">
  <a href="ARCHITECTURE.md"><img alt="Architecture" src="https://img.shields.io/badge/Architecture-Overview-5b554a"></a>
  <a href="TESTING.md"><img alt="Testing" src="https://img.shields.io/badge/Testing-Guide-c44632"></a>
  <a href="RELEASE.md"><img alt="Release Process" src="https://img.shields.io/badge/Release-Process-7e7266"></a>
  <a href="ROADMAP.md"><img alt="Roadmap" src="https://img.shields.io/badge/Roadmap-Status-2b2422"></a>
</p>
