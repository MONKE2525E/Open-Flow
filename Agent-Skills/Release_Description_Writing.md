# Release Description Writing

This guide documents the canonical format for Open Flow GitHub release descriptions. The style shown here matches the 0.7.0-beta release exactly — that is the reference implementation. also, always put your markup output inside of a code box e.g ``markdown``.


---

## Structure Overview

A release description has six sections in this order:

1. **Title line** (H1) — version + tagline
2. **App blurb** — one-sentence elevator pitch, bold, always the same
3. **What's New** (H2) — version-specific changelog
4. **Features** (H2) — evergreen feature catalogue, updated as features land
5. **Getting Started** (H2) — static four-step onboarding
6. **Lightweight & Local** (H2) — static positioning block
7. **Virustotal Review** (H2) — per-release file hashes

---

## Section-by-Section Rules

### 1. Title (H1)

```
# Open Flow <version> - <tagline>
```

- Version uses the public-facing number only (e.g., `0.7.0`), not the tag slug (`Open-Flow-0.7.0-beta`).
- Tagline is a short, evocative phrase — not a commit message. It describes the product identity, not the diff. "Local-First AI Dictation" is the current default; change it only if the release meaningfully shifts identity.
- No emoji in the title.

### 2. App Blurb

```
**Open Flow** is a free, open-source AI dictation app for Windows. No subscriptions. You bring your own API keys.
```

This is static. Do not paraphrase it. It anchors new readers who land on a release page without context.

### 3. What's New

```
## What's New in <version>
- **Feature Name** - One sentence description starting with a verb
```

Rules:
- One bullet per distinct feature or improvement shipped in this version.
- Lead with the feature name in bold, then a dash, then a single plain-English sentence.
- Sentence starts with a verb ("Added", "Reworked", "Refactored", "Reduced") or describes what the app *can now do* ("Open Flow can now…").
- No trailing period on bullet items — consistent with the reference release.
- Order: user-facing features first, then infrastructure/reliability, then testing/tooling last.
- Do not include bug fixes that have no user-visible impact — fold them into the relevant hardening bullet if needed.
- Keep bullets specific. "Pipeline Hardening - Refactored transcription, cleanup, snippets, dictionary substitution, and injection into smaller safer paths with poisoned-lock handling" names what changed and why it matters.

### 4. Features

```
## Features

### <Category>
- **Bold label** - description
- Plain bullet for sub-items without a label
```

Rules:
- This section is **evergreen** — update it as features land, not just at release time.
- Categories currently used: Transcription, Text Cleanup, Dictionary & Snippets, History & Stats, Settings.
- Add a new category if a release introduces a substantial new surface area.
- Bold-label bullets (`**Label** - description`) for major features; plain bullets for supporting details.
- The hotkey format is always `**Ctrl+Win**` (bold, no backticks).
- Keep descriptions present-tense and functional ("Start a continuous dictation session"), not past-tense ("Added a way to start…").

### 5. Getting Started

```
## Getting Started

1. Download the installer from releases
2. Add your API key for Groq, OpenAI, or Google
3. Hold **Ctrl+Win** and start talking
4. Release the hotkey and your cleaned-up text appears in the active app
```

This is static. Edit only if the fundamental onboarding flow changes (e.g., a different hotkey or a new required setup step).

### 6. Lightweight & Local

```
## Lightweight & Local

- ~200MB RAM idle target, native Tauri app, not Electron
- Local SQLite history
- API keys stored locally
- No telemetry
```

This is static. Update the RAM figure only if the target changes meaningfully.

### 7. Virustotal Review

```
## Virustotal Review

- [Open Flow_<version>_x64-setup.exe](<virustotal url>)
- [Open Flow_<version>_x64_en-US.msi](<virustotal url>)
```

Rules:
- Both installer artifacts must have their own VirusTotal link.
- Use display text `Open Flow_<version>_x64-setup.exe` and `Open Flow_<version>_x64_en-US.msi` — note the space between "Open" and "Flow" in the display name.
- Upload both files manually at virustotal.com — automated upload via Playwright is blocked by RECAPTCHA. Copy the result URLs after analysis completes. Never reuse links from a previous version.

---

## Comparison: Published 0.7.0-beta vs. This Style

The 0.7.0-beta release (tag `Open-Flow-0.7.0-beta`, published 2026-05-15) matches this guide exactly — it is the reference implementation. No structural deviations were found:

| Check | Result |
|---|---|
| H1 format `# Open Flow X.Y.Z - Tagline` | ✓ |
| Static app blurb present and unmodified | ✓ |
| What's New bullets: bold name, dash, sentence, no trailing period | ✓ |
| What's New ordering: user-facing → infra → tooling | ✓ |
| Features section uses bold-label pattern for major items | ✓ |
| Getting Started is the canonical four steps | ✓ |
| Lightweight & Local block is static and unchanged | ✓ |
| Virustotal section has both artifacts with correct display names | ✓ |

---

## Quick Checklist Before Publishing

- [ ] Version number in title matches `package.json` / `tauri.conf.json` / `Cargo.toml`
- [ ] App blurb is unchanged
- [ ] Every What's New bullet describes something that actually shipped in this version
- [ ] Features section reflects current app capabilities (not just what's new)
- [ ] VirusTotal links are fresh builds from this release, not a previous one
- [ ] Both `.exe` and `.msi` artifacts are attached to the release
