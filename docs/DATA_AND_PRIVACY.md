# Verenu Data And Privacy

This document explains what Verenu keeps on device, what it sends off device, and what it does not do.

## Core Principles

- Verenu does not run its own servers.
- There is no built-in telemetry, analytics, or ad-tech pipeline.
- Your data either stays on your machine or goes directly to the providers you configure.
- If data leaves your machine, it leaves because a feature needs it and the provider endpoint is part of that feature.

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

- Recent logs stay local unless you explicitly export them
- Exported logs are created only when you trigger that action

### Backups

Manual export/import stays local unless you choose to move the file elsewhere.

Current backup export includes:

- Settings
- Dictionary
- Snippets
- Derived stats

Current backup export does not include full transcription history.

## What Leaves Your Device

### Audio

When you finish a dictation, Verenu sends recorded audio to the transcription provider you selected.

That can be:

- Groq
- OpenAI
- Google

### Text sent to cleanup models

After transcription, Verenu can send text to a cleanup model so it can:

- remove filler words
- fix punctuation
- apply formatting rules
- apply snippet instructions
- apply tone or cleanup intensity

That means raw transcription text leaves your device when cleanup is enabled.

### Optional context

Depending on your settings and the feature being used, Verenu may also send:

- formatting profile or tone selection
- snippet instructions
- selected model metadata
- active app context, if app-context hints are enabled

### Update checks

Verenu checks GitHub release metadata for updates.

That request does not include your dictated text, history, snippets, or API keys.

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
| Transcription | local capture state | audio to selected transcription provider |
| Cleanup | local settings and local cache | raw transcription text and cleanup context to selected cleanup provider |
| Dictionary and snippets | SQLite | nothing by default |
| Auto-learn | local monitoring data and promoted entries | nothing by default |
| Update check | current app state stays local | GitHub release metadata request |
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
- backup/export contents
- logging behavior
- new network calls

If you cannot explain the data flow in plain English, the feature is not documented well enough yet.
