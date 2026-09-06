# Verenu 0.18.1 - Contexts & Polish

**Verenu** is a free, open-source AI dictation app for Windows and macOS. No subscriptions. You bring your own API keys.

## What's New in 0.18.1

- **Pure black-and-white icons** - Removed orange from native and packaged icons so they follow light and dark appearance modes
- **Reliable hands-free conversion** - Keeps a converted hold-to-talk session alive when stale Windows modifier-release events arrive
- **Insights pace scale** - Replaced the speaking-pace dial with a readable tick scale that marks the personal best
- **Smoother Windows windows** - Coalesced drag and resize refreshes, reduced CPU spikes, and reused themed icon artwork
- **Context-aware app matching** - Keeps context app targets working across versioned and nightly app updates when publisher evidence matches
- **Shared cleanup prompt** - Uses one editable cleanup template for every model, including fallback models, with clearer rules for multilingual text, numbers, symbols, and technical terms
- **Broader model selection** - Lists provider-reported text models behind an opt-in expansion while filtering out non-text models
- **Legacy page controls** - Hides the older App Mappings, Dictionary, and Snippets pages by default while keeping them available for compatibility
- **Context polish** - Adds website DNS checks, shorter context names with a live counter, compact icon color selection, and focused add-item animation
- **Appearance and layout fixes** - Improves light-mode icon contrast, restores tray icon proportions, and keeps Insights usable at narrow window sizes

## Features

### Transcription

- **Hold-to-record dictation** - Start a global dictation session with a platform hotkey and inject the result into the focused app
- **Hands-free mode** - Toggle continuous recording when hold-to-record does not fit the task
- **Provider choice** - Use local Parakeet V3 transcription or a supported cloud provider

### Text cleanup

- **Cleanup profiles** - Choose how much formatting, punctuation, tone, and filler-word cleanup to apply
- **Context instructions** - Apply app- or website-specific tone and cleanup rules
- **Model fallbacks** - Keep dictation available when a provider or model fails

### Dictionary & snippets

- **Vocabulary** - Teach Verenu names, terms, and spellings that matter to you
- **Snippets** - Expand spoken triggers into saved text
- **Auto-learn** - Optionally learn from repeated manual corrections

### History & stats

- **Local history** - Review past dictations and retry failed transcriptions
- **Insights** - Track speaking pace, usage, and learned vocabulary locally
- **Import and export** - Move app data between installations with local files

### Settings

- **Windows and macOS** - Supports Windows 10/11, Apple Silicon, and Intel Macs
- **Local key storage** - Stores API keys in Windows Credential Manager or macOS Keychain
- **Themes and accents** - Follow the system appearance or choose light/dark mode and a custom accent

## Getting Started

1. Download the installer from releases
2. Add your API key for Groq, OpenAI, or Google
3. Hold **Ctrl+Win** on Windows or **Option+Space** on macOS and start talking
4. Release the hotkey and your cleaned-up text appears in the active app

## Shortcuts

- **Windows hold-to-record** - Hold **Ctrl+Win** to record, release to transcribe and inject
- **macOS hold-to-record** - Hold **Option+Space** to record, release to transcribe and inject

## Lightweight & Local

- Around 200 MB RAM idle target, using native Tauri rather than Electron
- Local SQLite history and settings
- API keys stored in Windows Credential Manager or macOS Keychain
- No telemetry

## Virustotal Review

Upload the four release installers to VirusTotal after the GitHub assets are available, then replace each placeholder with its scan URL:

- `Verenu_0.18.1_x64-setup.exe` - TODO: add VirusTotal URL
- `Verenu_0.18.1_x64_en-US.msi` - TODO: add VirusTotal URL
- `Verenu_0.18.1_Apple_Silicon.dmg` - TODO: add VirusTotal URL
- `Verenu_0.18.1_Intel.dmg` - TODO: add VirusTotal URL
