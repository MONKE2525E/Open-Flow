<div align="center">
  <img src="docs/banner.svg" alt="Verenu" width="800"/>
</div>

<p align="center">
  Hold the hotkey, talk, release, and Verenu drops cleaned-up text into the app you were already using.
  <br/>
  <em>Local-first AI dictation for Windows and macOS. Bring your own cloud API keys when needed. No subscriptions. No telemetry.</em>
</p>

<p align="center">
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-2b2422"></a>
  <a href="docs/DATA_AND_PRIVACY.md"><img alt="Privacy: local-first" src="https://img.shields.io/badge/privacy-local--first-c44632"></a>
  <a href=".github/workflows/pr-checks.yml"><img alt="PR checks" src="https://img.shields.io/badge/checks-PR%20workflow-5b554a"></a>
</p>

## What Verenu Is

Verenu is an open source desktop dictation app built with Tauri, Svelte, Rust, and SQLite.

It records locally, sends audio and text only to the AI providers you choose, keeps your app data on your machine, and avoids the usual Electron bloat. The goal is simple: fast dictation, predictable formatting, and privacy that is easy to understand.

## What It Does

- Hold-to-record dictation with global hotkeys, plus a handsfree toggle mode
- Contexts: group apps and websites with the tone, cleanup rules, custom instructions, vocabulary, and snippets that belong there
- Provider choice for transcription and cleanup, including fully local Parakeet V3 transcription and local LLM cleanup
- Local history, insights, local settings, and local data export/import
- Optional auto-learn from repeated manual corrections

Contexts are the main place to configure app-specific behavior. They connect apps and websites to cleanup, tone, custom instructions, vocabulary, and snippets. The old standalone pages remain available only as legacy compatibility pages.

For more details: [Contexts](docs/CONTEXTS.md), [Vocabulary](docs/VOCABULARY.md), [Snippets](docs/SNIPPETS.md), [Cleanup Levels](docs/CLEANUP_LEVELS.md), and [Local Transcription](docs/LOCAL_TRANSCRIPTION.md).

Appearance can follow the operating system or stay in light or dark mode. Page surfaces use neutral near-white and charcoal colors, and the accent can be changed independently to any six-digit hex color. See [Appearance and settings](docs/APPEARANCE.md).

## Platform Support

Verenu supports both Windows and macOS.

### Windows

- Windows 10 and 11
- Uses WebView2, not Electron
- Default hold-to-record hotkey: <kbd>Ctrl</kbd> + <kbd>Windows</kbd>
- API keys are stored in Windows Credential Manager

### macOS

- Apple Silicon and Intel builds are supported
- Default hold-to-record hotkey: <kbd>Option</kbd> + <kbd>Space</kbd>
- API keys are stored in Keychain
- Verenu needs macOS permissions for real-world use:
  - Microphone, to capture audio
  - Accessibility, to inject text and interact with focused apps
  - Notifications are optional and only affect status and update alerts

macOS support is not an afterthought anymore. It is part of the normal app flow, and the repo includes macOS-specific hotkey, permissions, injection, updater, and key-storage logic.

For more details: [Install Verenu](docs/INSTALL.md), [Troubleshooting](docs/TROUBLESHOOTING.md), and [macOS code signing](docs/macos-code-signing.md).

## How It Works

1. Verenu records audio locally while you hold the hotkey.
2. When you release, it either transcribes locally or sends the audio to your chosen cloud transcription provider.
3. If cleanup is enabled, it sends the resulting raw text to your chosen cleanup model so filler words, punctuation, tone, context instructions, and formatting rules can be applied.
4. It pastes the final text back into the app that had focus when you started.
5. It stores local history and optional learning data on your machine.

For more details: [Your First Dictation](docs/FIRST_DICTATION.md) and [Architecture](docs/ARCHITECTURE.md).

## Data And Privacy

Verenu's own server (`api.verenu.com`) serves only public app metadata — release info, download links, and provider status. Your dictated audio and text either stay on your device or go directly to the third-party providers you choose; they never touch a Verenu server.

### Stays on your device

- API keys in Windows Credential Manager or macOS Keychain
- Settings in local app storage
- Transcription history in local SQLite
- Context groups, vocabulary, snippets, and auto-learn data in local SQLite
- Update-dismiss state, model preferences, and context group targets
- Local logs unless you explicitly export them

### Leaves your device

- Local transcription plus local cleanup, or Cleanup Off, keeps audio and transcript on device after the model download
- Local transcription plus cloud cleanup keeps audio on device but sends transcript text to the cleanup provider
- Cloud transcription sends recorded audio to your chosen transcription provider
- Context instructions, cleanup settings, and selected model metadata go with cleanup requests
- Active app context may be sent if you enable app-context hints
- Update checks hit GitHub release metadata
- Provider status and health checks hit `api.verenu.com` (public status only, no dictated content, keys, or history). You can disable these background checks in Settings → Privacy.

Read the full breakdown in [docs/DATA_AND_PRIVACY.md](docs/DATA_AND_PRIVACY.md).

## AI Providers

You choose the providers. Verenu does not lock you into one stack.

| Provider | Transcription | Cleanup |
| --- | --- | --- |
| Local | On-device models | On-device models or none |
| Groq | `whisper-large-v3-turbo` | `qwen/qwen3.6-27b` |
| OpenAI | `gpt-4o-transcribe` | `gpt-4o-mini` |
| Google | `gemini-3.5-transcribe` | `gemini-3.5-flash-lite` |
| AssemblyAI | `universal-3-5-pro` or `universal-2` | not available |

If you care about privacy, speed, retention, or cost, judge the provider on its own policy. Once data leaves Verenu and hits a provider API, that provider's rules apply. Local transcription with cloud cleanup is still not fully local because the transcript text leaves the device.

### Enhanced transcription

Advanced Models includes an optional Dual model transcription strategy. It runs the primary model and the first configured transcription fallback together, then sends two successful candidates to the cleanup model for reconciliation. If a candidate fails, later fallbacks are tried until two models work or the chain is exhausted. This can improve word choices when providers disagree, but it uses another transcription request and may add latency.

For more details: [Add Your API Key](docs/API_KEYS.md) and [Privacy & Data](docs/PRIVACY_SUMMARY.md).

## Setup

### Prerequisites

- Node.js 18+
- Rust and Cargo
- Python 3.8+ (required by `npm test`; the OnePyFone test runner is a Python script)
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

For more details: [Install Verenu](docs/INSTALL.md), [Contributing](docs/CONTRIBUTING.md), and [Testing](docs/TESTING.md).

## Release Flow

`master` is the only shared integration and release branch.

The normal flow is:

1. Create a short-lived feature or fix branch from `master`.
2. Open a pull request directly into `master`.
3. Let the required CI checks run, review the change, and merge it into `master`.
4. The morning nightly workflow and manual installer workflow use `master`.

If you are contributing, read [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) before you start.

For more details: [Release Process](docs/RELEASE.md), [Changelog](docs/CHANGELOG.md), and [Contributing](docs/CONTRIBUTING.md).

## Why Tauri

- No bundled Chromium
- Lower idle RAM
- Native OS integrations where they actually matter
- Better fit for a background dictation tool than a browser-shaped desktop app

For more details: [Architecture](docs/ARCHITECTURE.md) and [Transcription RAM and reliability plan](docs/transcription-ram-reliability-plan.md).

## Contact

- Website: [verenu.com](https://verenu.com)
- General inquiries: [hello@verenu.com](mailto:hello@verenu.com)
- Support: [docs/SUPPORT.md](docs/SUPPORT.md) or [support@verenu.com](mailto:support@verenu.com)
- Security: [docs/SECURITY.md](docs/SECURITY.md) or [security@verenu.com](mailto:security@verenu.com)

## License

MIT. See [LICENSE](LICENSE).
