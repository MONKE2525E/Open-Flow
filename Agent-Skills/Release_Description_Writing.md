# Release Description Writing

This guide documents the canonical format for Verenu GitHub release descriptions. The style shown here matches the 0.12.0 release, incorporating cross-platform Windows and macOS support. Always wrap your markdown output inside of a ` ```markdown ` code block.

## Structure Overview

A release description has eight sections in this order:

1. **Title line** (H1) - version + 1-2 word tagline
2. **App blurb** - one-sentence elevator pitch, bold, always the same
3. **What's New** (H2) - version-specific changelog
4. **Features** (H2) - evergreen feature catalogue, updated as features land
5. **Getting Started** (H2) - static four-step onboarding
6. **Shortcuts** (H2) - static default hotkeys by platform
7. **Lightweight & Local** (H2) - static positioning block
8. **Virustotal Review** (H2) - per-release file hashes

---

## Section-by-Section Rules

### 1. Title (H1)

```
# Verenu <version> - <tagline>
```

- Version uses the public-facing number only (e.g., `0.12.0`), not the tag slug.
- Tagline is a 1-2 word tagline based on the release's flagship feature (e.g., `Smart Dictation` or `AI Dictation`), not a commit message.
- No emojis in the H1 title.

### 2. App Blurb

```
**Verenu** is a free, open-source AI dictation app for Windows and macOS. No subscriptions. You bring your own API keys.
```

This anchors new readers who land on a release page without context.

### 3. What's New

```
## What's New in <version>
- **Feature Name** - One sentence description starting with a verb
```

Rules:
- One bullet per distinct feature or improvement shipped in this version.
- Lead with the feature name in bold, then a dash, then a single plain-English sentence.
- Sentence starts with a verb ("Added", "Reworked", "Refactored", "Reduced") or describes what the app can now do.
- No trailing period on bullet items, consistent with previous releases.
- Order: user-facing features first, then infrastructure/reliability, then testing/tooling last.
- Do not include bug fixes that have no user-visible impact. Fold them into the relevant hardening bullet if needed.

### 4. Features

```
## Features

### <Category>
- **Bold label** - description
- Plain bullet for sub-items without a label
```

Rules:
- This section is evergreen. Update it as features land, not just at release time.
- Categories used: Transcription, Text Cleanup, Dictionary & Snippets, History & Stats, Settings.
- Bold-label bullets (`**Label** - description`) for major features. Use plain bullets for supporting details.
- Use bold format for hotkeys (e.g., `**Ctrl+Win**` for Windows, `**Option+Space**` for macOS) with no backticks.
- Keep descriptions present-tense and functional ("Start a continuous dictation session").

### 5. Getting Started

```
## Getting Started

1. Download the installer from releases
2. Add your API key for Groq, OpenAI, or Google
3. Hold **Ctrl+Win** (Windows) or **Option+Space** (macOS) and start talking
4. Release the hotkey and your cleaned-up text appears in the active app
```

This is static.

### 6. Shortcuts

```
## Shortcuts

- **Windows hold-to-record** - Hold **Ctrl+Win** to record, release to transcribe and inject
- **macOS hold-to-record** - Hold **Option+Space** to record, release to transcribe and inject
```

Rules:
- This is static unless the app's default shortcuts change.
- Use bold format for the key chords with no backticks.
- Keep the descriptions simple and human-readable.

### 7. Lightweight & Local

```
## Lightweight & Local

- ~200MB RAM idle target, native Tauri app, not Electron
- Local SQLite history
- API keys stored locally (Windows Credential Manager / macOS Keychain)
- No telemetry
```

This is static.

### 8. Virustotal Review

```
## Virustotal Review

- [Verenu_<version>_x64-setup.exe](<virustotal url>)
- [Verenu_<version>_x64_en-US.msi](<virustotal url>)
- [Verenu_<version>_Apple_Silicon.dmg](<virustotal url>)
- [Verenu_<version>_Intel.dmg](<virustotal url>)
```

Rules:
- All four installer files (Windows `.exe`/`.msi` and macOS `.dmg` for both Apple Silicon and Intel) must have their own VirusTotal link.
- Use exact display text formatting for file names (note the space in "Verenu"):
  - `Verenu_<version>_x64-setup.exe`
  - `Verenu_<version>_x64_en-US.msi`
  - `Verenu_<version>_Apple_Silicon.dmg`
  - `Verenu_<version>_Intel.dmg`
- Upload all files manually at virustotal.com and copy the result URLs.
