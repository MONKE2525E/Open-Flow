# Architecture

Verenu is a Tauri 2 desktop app. The frontend is Svelte 5 and TypeScript. The backend is Rust. It is not Electron and it is not a hosted web app.

## Runtime Shape

- [`../src/`](../src/) contains the Svelte app shown in the main window and the floating pill window.
- [`../src-tauri/src/`](../src-tauri/src/) contains the Rust backend, native OS integration, provider clients, local storage, and the dictation pipeline.
- [`../src-tauri/tauri.conf.json`](../src-tauri/tauri.conf.json) declares the main window. The floating pill window is created at runtime by Rust.
- [`../tests/`](../tests/) contains deterministic smoke, integration, native, and manual-adjacent tests.

## Dictation Pipeline

1. The global hotkey starts local microphone capture.
2. Releasing the hotkey stops capture and encodes audio as WAV.
3. Quality gates reject clips that are too short or too quiet.
4. The selected transcription provider receives audio.
5. Snippets and cleanup settings are assembled into a cleanup prompt when cleanup is enabled.
6. The selected cleanup model returns final text.

When Dual model transcription is enabled, the primary model and the first configured transcription fallback run concurrently. Later fallback models replace failed candidates until two models succeed or the chain is exhausted. The cleanup request receives both candidates in separate data sections and reconciles them before the normal snippet, dictionary, persistence, and injection stages.
7. Dictionary substitutions and snippet expansions are applied.
8. The transcription is stored locally in SQLite.
9. The text injection layer restores focus to the captured app and pastes through the clipboard.
10. Optional auto-learn monitoring watches for repeated user corrections.

## Important Modules

| Area | Files |
| --- | --- |
| Tauri setup and command registration | [`../src-tauri/src/main.rs`](../src-tauri/src/main.rs), [`../src-tauri/src/commands/`](../src-tauri/src/commands/) |
| Pipeline orchestration | [`../src-tauri/src/pipeline/`](../src-tauri/src/pipeline/) |
| Provider requests and cleanup prompts | [`../src-tauri/src/api/`](../src-tauri/src/api/) |
| Audio capture | [`../src-tauri/src/media/audio.rs`](../src-tauri/src/media/audio.rs) |
| Hotkeys | [`../src-tauri/src/core/hotkey/`](../src-tauri/src/core/hotkey/) |
| Text injection | [`../src-tauri/src/core/injection/`](../src-tauri/src/core/injection/) |
| Context probing and capitalization | [`../src-tauri/src/core/context_probe.rs`](../src-tauri/src/core/context_probe.rs), [`../src-tauri/src/core/text_context.rs`](../src-tauri/src/core/text_context.rs) |
| Local database and settings | [`../src-tauri/src/data/`](../src-tauri/src/data/) |
| Frontend settings and stores | [`../src/lib/settings.ts`](../src/lib/settings.ts), [`../src/lib/stores.ts`](../src/lib/stores.ts), [`../src/lib/stores.svelte.ts`](../src/lib/stores.svelte.ts) |
| Main app views | [`../src/lib/views/`](../src/lib/views/) |
| First-run setup | [`../src/lib/setup/`](../src/lib/setup/) |

## Storage Boundaries

- API keys live in Windows Credential Manager or macOS Keychain.
- Non-secret settings live in Tauri app data.
- History, dictionary entries, snippets, cleanup cache, and local app data live in SQLite.
- Logs should use redacted metadata only. Do not log dictated text, clipboard text, API keys, prompts, raw dictionary terms, snippet bodies, or full local paths.

## Platform Boundaries

Windows code uses native Windows APIs for hotkeys, foreground app detection, clipboard injection, Credential Manager, and UI Automation.

macOS code uses native APIs for hotkeys, Accessibility reads, clipboard injection, Keychain, app lookup, permissions, and audio behavior. macOS permission behavior differs between dev builds and installed app bundles, so macOS-facing changes need real platform checks when practical.

## Dependency Rules

Keep runtime dependencies lean. Verenu targets roughly 200 MB idle RAM and handles private user text plus API keys. New runtime dependencies need a clear reason, a supply-chain risk check, and a memory impact check.

## Related Docs

<p align="center">
  <a href="TESTING.md"><img alt="Testing" src="https://img.shields.io/badge/Testing-Guide-c44632"></a>
  <a href="CONTRIBUTING.md"><img alt="Contributing" src="https://img.shields.io/badge/Contributing-Guide-5b554a"></a>
  <a href="transcription-ram-reliability-plan.md"><img alt="RAM And Reliability" src="https://img.shields.io/badge/RAM-Reliability-7e7266"></a>
  <a href="README.md"><img alt="Docs Index" src="https://img.shields.io/badge/Docs-Index-2b2422"></a>
</p>
