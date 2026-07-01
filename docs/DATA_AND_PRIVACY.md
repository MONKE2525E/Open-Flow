# Verenu Data And Privacy

This document explains what Verenu keeps on device, what it sends off device, and what it does not do.

## Core Principles

- Verenu does not run its own servers.
- There is no built-in telemetry, analytics, or ad-tech pipeline.
- Your data either stays on your machine or goes directly to the providers you configure.
- If data leaves your machine, it leaves because a feature needs it and the provider endpoint is part of that feature.
- Safety defaults beat convenience. On Windows, updates download or open the published installer instead of auto-executing downloaded bytes.

## What Stays On Device

### API keys

- Windows: stored in Windows Credential Manager
- macOS: stored in Keychain
- Legacy plaintext storage is migrated away from older formats where possible

### App data

Stored locally in app storage and SQLite:

- Settings
- Provider and model preferences
- App mappings and tone preferences
- Transcription history
- Dictionary entries
- Snippets
- Auto-learn events and candidate data
- Update-dismiss state

### Logs

- Recent logs stay local unless you explicitly export them.
- Exported logs are created only when you trigger that action.
- Unlocking Developer mode does not enable verbose logging by itself.
- Verbose logging must be enabled explicitly from the Developer panel.

Current logging paths are intended to use redacted metadata rather than raw private content. That means counts, ids, model names, app identifiers, filenames, and redacted path labels are preferred over dictated text, prompt bodies, raw dictionary terms, raw snippet expansions, or full local paths.

Even with that hardening, log exports can still contain sensitive operational detail. Do not share them casually.

### Backups

Manual export and import stay local unless you choose to move the file elsewhere.

Current backup export includes:

- Settings
- Dictionary
- Snippets
- Derived stats

Current backup export does not include full transcription history.

Import and restore paths validate supported setting values and reject oversized prompt overrides, snippet bodies, and unsupported app-mapping values instead of silently accepting junk.

## What Leaves Your Device

### Audio

When you finish a dictation, Verenu either transcribes audio locally or sends recorded audio to the transcription provider you selected.

That can be:

- Local Parakeet V3
- Groq
- OpenAI
- Google

If transcription is local, audio stays on the device after the model download.

### Text sent to cleanup models

After transcription, Verenu can send text to a cleanup model so it can:

- remove filler words
- fix punctuation
- apply formatting rules
- apply snippet instructions
- apply tone or cleanup intensity

That means raw transcription text leaves your device when cleanup is enabled, including when transcription itself ran locally.

### Optional context

Depending on your settings and the feature being used, Verenu may also send:

- formatting profile or tone selection
- snippet instructions
- selected model metadata
- active app context, if app-context hints are enabled

### Update checks and downloads

Verenu checks GitHub release metadata for updates.

That request does not include your dictated text, history, snippets, or API keys.

On Windows and macOS, installing an update opens the published GitHub asset so the platform installer flow can take over. Verenu does not auto-run a downloaded Windows executable from a fixed temp path.

### Connectivity check

While the app window is open, Verenu periodically sends a lightweight `HEAD` request to `api.github.com` to detect whether you are online and show the offline indicator.

That request carries no dictated text, history, snippets, or API keys.

## History Loading

The Home view loads recent transcription history in pages of 100 items by default and can request older pages on demand.

This changes UI loading behavior, not storage location. The full history database still lives on your machine unless you export or delete it.

## What Verenu Does Not Send

Verenu does not send any of this to a Verenu-owned server, because there is no Verenu-owned server in the product path today:

- transcription history
- dictionary entries by default
- snippets by default
- local settings backups by default
- analytics events
- user profiles
- payment data

That said, once data is sent to a third-party AI provider, that provider's retention and privacy rules apply. Verenu cannot override that.

## Data Map By Feature

| Feature | Stays local | Leaves device |
| --- | --- | --- |
| Hold-to-record audio capture | audio before release | nothing until transcription starts |
| Local transcription + Cleanup Off | audio, transcript, settings, and history | nothing after the model download |
| Local transcription + cloud cleanup | audio, local model, local capture state | transcript text and cleanup context to selected cleanup provider |
| Cloud transcription | local capture state | audio to selected transcription provider |
| Cleanup | local settings and local cache | raw transcription text and cleanup context to selected cleanup provider |
| Dictionary and snippets | SQLite | nothing by default |
| Auto-learn | local monitoring data and promoted entries | nothing by default |
| Update check | current app state stays local | GitHub release metadata request |
| Connectivity check | current app state stays local | periodic `HEAD` request to `api.github.com` |
| Export data | backup file on local disk | nothing unless you share the file yourself |
| Logs export | log file on local disk | nothing unless you share the file yourself |

## macOS And Windows Key Storage

Verenu treats OS credential storage as the source of truth for API keys:

- Windows uses Credential Manager
- macOS uses Keychain

That is separate from the SQLite app database on purpose. API keys should not be living in the transcription database.

## If You Change Privacy Behavior

If you contribute code that changes any of the following, update this file and the README:

- what data leaves the device
- which provider receives what
- what gets stored locally
- backup or export contents
- logging behavior
- new network calls
- updater download or installer behavior
- local transcription model behavior or privacy claims

If you cannot explain the data flow in plain English, the feature is not documented well enough yet.

## Related Docs

<p align="center">
  <a href="PRIVACY_SUMMARY.md"><img alt="Privacy Summary" src="https://img.shields.io/badge/Privacy-Summary-c44632"></a>
  <a href="API_KEYS.md"><img alt="API Keys" src="https://img.shields.io/badge/API-Keys-5b554a"></a>
  <a href="TROUBLESHOOTING.md"><img alt="Troubleshooting" src="https://img.shields.io/badge/Help-Troubleshooting-7e7266"></a>
  <a href="README.md"><img alt="Docs Index" src="https://img.shields.io/badge/Docs-Index-2b2422"></a>
</p>
