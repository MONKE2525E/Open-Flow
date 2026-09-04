# Verenu architecture

Verenu is a Tauri desktop app. The Svelte frontend owns the interface and local state, while the Rust backend owns audio capture, provider calls, local models, persistence, and text injection.

## Main components

| Area | Implementation | Responsibility |
| --- | --- | --- |
| Desktop shell | Tauri 2 | Windows and macOS windows, commands, events, and packaging |
| Frontend | Svelte 5, TypeScript, Tailwind CSS | Setup, settings, contexts, history, insights, and the dictation pill |
| Backend | Rust, Tokio | Pipeline orchestration, platform integration, provider requests, and background work |
| Audio | `cpal`, `hound`, `nnnoiseless` | Microphone capture, WAV encoding, gain, and noise reduction |
| Cloud AI | `reqwest` provider clients | Cloud transcription and cleanup through the configured providers |
| Local transcription | `transcribe-rs` and downloaded model files | On-device speech-to-text |
| Local cleanup | Managed local LLM runtime and downloaded model files | On-device text cleanup |
| Storage | SQLite plus an app-data JSON settings file | History, contexts, vocabulary, snippets, insights, cache, and non-secret settings |
| Credentials | Windows Credential Manager or macOS Keychain | API keys, kept outside SQLite and the settings file |

## Dictation flow

1. The user holds the platform hotkey. The audio layer captures microphone PCM and sends RMS levels to the floating pill.
2. Releasing the hotkey stops capture and hands the recording to the pipeline.
3. The pipeline rejects recordings below its duration or volume thresholds before making an AI request.
4. It captures the foreground window, reads a browser domain when the target is a supported browser, and resolves the matching Context. The built-in Everywhere context is the fallback.
5. Style resolution combines the matching Context with legacy App Mapping settings and the global tone setting.
6. The transcription chain runs the selected cloud or local model. Dual transcription can run the primary model and the first fallback concurrently, then reconcile two successful candidates.
7. The pipeline checks the cleanup cache and the pure-snippet fast path. If cleanup is needed, it assembles the shared prompt and calls the selected cloud or local cleanup model.
8. Cleanup guards can retry unusable output once, then fall back to the pre-cleanup text. Remaining snippets and context-scoped vocabulary are applied afterward.
9. The final text and metadata are written to local SQLite history.
10. The injection layer restores focus to the captured window, pastes through the clipboard, restores the previous clipboard contents, and emits the completed-dictation event.
11. Auto-learn can monitor the focused field for a limited period to detect user corrections.

## Contexts and style

Contexts are the primary way to configure app- and website-specific behavior. A Context contains:

- executable and website targets
- optional tone, cleanup intensity, and custom instructions
- vocabulary and snippets scoped to that Context

Verenu resolves a foreground executable and, when available, a browser domain to one Context. If nothing matches, it uses Everywhere. App Mappings, Dictionary, and Snippets remain available as legacy pages, hidden by default.

## Persistence and data boundaries

- API keys live only in the operating system credential store.
- Settings live in the Tauri app-data directory as JSON.
- History, Contexts, vocabulary, snippets, insights, and cleanup-cache records live in local SQLite.
- Downloaded transcription models are stored below the app-data `models/stt` directory.
- Downloaded cleanup models and the local cleanup runtime are stored below the app-data `models` directory.
- Cloud transcription sends audio to the selected transcription provider.
- Cloud cleanup sends raw transcript text and the cleanup context to the selected cleanup provider.
- Local transcription and local cleanup keep the processing data on the device after the required model files have been downloaded.

## Platform integration

- Windows uses a low-level keyboard hook for hold/release state, UI Automation for focused-text reads and Auto-learn, native clipboard paste, and Credential Manager.
- macOS uses Carbon `RegisterEventHotKey`, Accessibility APIs for focused-text reads and paste, NSPasteboard for clipboard work, and Keychain for credentials.
- The recording pill is created at runtime as an always-on-top, transparent Tauri window. It stays available while idle and becomes interactive only when its state requires user input.

## Code map

- `src/` contains the Svelte application, settings registry, stores, setup wizard, views, and shared components.
- `src-tauri/src/pipeline/` contains pipeline orchestration and its transcription, cleanup, style, injection, persistence, and repair stages.
- `src-tauri/src/api/` contains cloud provider clients, prompt assembly, status checks, and update logic.
- `src-tauri/src/local_stt/` and `src-tauri/src/local_llm/` manage local model manifests, downloads, verification, runtimes, and inference.
- `src-tauri/src/core/` contains hotkeys, injection, Context resolution, browser probing, and contextual formatting.
- `src-tauri/src/data/` contains SQLite access, the settings store, credentials, vocabulary, and snippets.
- `tests/` contains the unified test runner, smoke tests, integration tests, and manual platform checks.
