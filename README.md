<div align="center">
  <img src="docs/banner.svg" alt="Verenu" width="800"/>
</div>

<p align="center">
  Hold the hotkey, talk, release, and Verenu drops cleaned-up text into the app you were already using.
  <br/>
  <em>Local-first AI dictation for Windows and macOS. Bring your own API keys. No subscriptions. No telemetry.</em>
</p>

## What Verenu Is

Verenu is an open source desktop dictation app built with Tauri, Svelte, Rust, and SQLite.

It records locally, sends audio and text only to the AI providers you choose, keeps your app data on your machine, and avoids the usual Electron bloat. The goal is simple: fast dictation, predictable formatting, and privacy that is easy to understand.

## What It Does

- Hold-to-record dictation with global hotkeys
- Provider choice for transcription and cleanup
- Snippets, personal dictionary, and app-specific formatting profiles
- Local history, local settings, and local data export/import
- Optional auto-learn from repeated manual corrections

## Platform Support

Verenu supports both Windows and macOS.

### Windows

- Windows 10 and 11
- Uses WebView2, not Electron
- Default hold-to-record hotkey: <kbd>Ctrl</kbd> + <kbd>Windows</kbd>
- API keys are stored in Windows Credential Manager

### macOS

- Apple Silicon and Intel builds are supported
- Default hold-to-record hotkey: <kbd>Fn</kbd> + <kbd>Control</kbd>
- API keys are stored in Keychain
- Verenu needs macOS permissions for real-world use:
  - Microphone, to capture audio
  - Accessibility, to inject text and interact with focused apps
  - Input Monitoring, to detect the global hotkey while other apps are focused

macOS support is not an afterthought anymore. It is part of the normal app flow, and the repo includes macOS-specific hotkey, permissions, injection, updater, and key-storage logic.

## How It Works

1. Verenu records audio locally while you hold the hotkey.
2. When you release, it sends the audio to your chosen transcription provider.
3. It sends the resulting raw text to your chosen cleanup model so filler words, punctuation, tone, snippets, and formatting rules can be applied.
4. It pastes the final text back into the app that had focus when you started.
5. It stores local history and optional learning data on your machine.

## Data And Privacy

Verenu does not run its own servers. Your data either stays on your device or goes directly to the third-party providers you choose.

### Stays on your device

- API keys in Windows Credential Manager or macOS Keychain
- Settings in local app storage
- Transcription history in local SQLite
- Dictionary entries, snippets, and auto-learn data in local SQLite
- Update-dismiss state, model preferences, and app mappings
- Local logs unless you explicitly export them

### Leaves your device

- Recorded audio goes to your chosen transcription provider
- Raw transcription text goes to your chosen cleanup provider
- Snippet instructions, cleanup settings, and selected model metadata go with cleanup requests
- Active app context may be sent if you enable app-context hints
- Update checks hit GitHub release metadata

Read the full breakdown in [docs/DATA_AND_PRIVACY.md](docs/DATA_AND_PRIVACY.md).

## AI Providers

You choose the providers. Verenu does not lock you into one stack.

| Provider | Transcription | Cleanup |
| --- | --- | --- |
| Groq | `whisper-large-v3-turbo` | `llama-3.3-70b-versatile` |
| OpenAI | `gpt-4o-transcribe` | `gpt-4o-mini` |
| Google | `gemini-3.5-flash` | `gemini-3.5-flash` |

If you care about privacy, speed, retention, or cost, judge the provider on its own policy. Once data leaves Verenu and hits a provider API, that provider's rules apply.

## Setup

### Prerequisites

- Node.js 18+
- Rust and Cargo
- Windows: WebView2
- macOS: Xcode Command Line Tools are recommended for local builds

### Run in development

```bash
git clone https://github.com/MONKE2525E/Verenu.git
cd Verenu
npm install
npm run tauri dev
```

### Useful commands

```bash
npm test
npm run check
npm run lint
npm run test:rust
npm run test:live
npm run test:native
```

## Release Flow

Most day-to-day work lands on `dev` first.

The normal flow is:

1. Commit to `dev` for most changes.
2. Review and test on `dev`.
3. Merge `dev` into `main` when it is ready.
4. Cut and ship the release from there.

If you are contributing, read [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) before you start.

## Why Tauri

- No bundled Chromium
- Lower idle RAM
- Native OS integrations where they actually matter
- Better fit for a background dictation tool than a browser-shaped desktop app

## License

MIT. See [LICENSE](LICENSE).
