# Verenu Data And Privacy

This document explains what Verenu keeps on device, what it sends off device, and what it does not do.

## Core Principles

- Verenu's own server (`api.verenu.com`) serves only public app metadata — release info, download links, and provider status. It never receives your dictated audio, transcripts, API keys, or history.
- There is no built-in telemetry, analytics, or ad-tech pipeline.
- Your dictated audio and text either stay on your machine or go directly to the AI providers you configure — never through a Verenu server.
- If data leaves your machine, it leaves because a feature needs it: either the AI provider endpoint that feature depends on, or Verenu's own public status/update endpoint.
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
- Context groups, their app/website targets, and per-context tone/cleanup overrides
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

With Dual model transcription enabled, the same audio may be sent to two or more providers from the configured transcription chain until two candidates succeed. Both successful candidates are then sent to the selected cleanup provider. Failed candidates do not fail a successful transcription.

### Post-dictation repair diagnosis

The floating pill's repair flow has no telemetry, analytics event, remote
complaint store, or Verenu feedback backend. A provider request happens only
after you enter or dictate a complaint and press **Analyze**. The request uses
the provider/model already configured for Verenu and contains only the
complaint plus bounded diagnostic context from that dictation: raw,
cleaned-before-dictionary, and private delivered-text excerpts; the target
executable and bare browser domain; the resolved context identity; the
allowlisted settings; and dictionary records that participated in the
pipeline.

Repair diagnosis does not send audio, API keys, history, full URLs or page
titles, actual clipboard-restored text, unrelated settings or applications,
full dictionary/snippet data, prompt overrides, or internal logs. Complaint
text and the diagnostic snapshot are transient. The model can propose only a
small typed allowlist of dictionary repairs and global setting changes. Rust
validates the proposal and the pill requires explicit **Apply** approval before
any local configuration is mutated.

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

### Context website checks

When you attach a website to a context group, Verenu resolves the domain over DNS before accepting it, so a typo can't create a website target that will never match anything. This is a plain DNS lookup, not an HTTP request — it does not fetch the site, send cookies, or reveal your IP to the site owner beyond what any DNS resolution already does. Nothing about your dictation, history, or other settings is included; only the domain you typed leaves your device, to your configured DNS resolver.

### Website favicons

To show a real site icon next to a website context target (in the sidebar and on the context page), Verenu asks Google's public favicon service for that site's icon. Only the bare hostname you attached (for example `mail.google.com`) is sent — never a full URL, path, dictation, history, or API key — and the site itself is never contacted.

Each hostname is requested at most once: the result is cached on disk under the app data directory, including the "this site has no icon" outcome, so the icon is not re-fetched when the sidebar rerenders, you switch pages, or you restart the app. If the lookup fails, the row falls back to a generic globe glyph.

### Connectivity check

While the app window is open, Verenu periodically sends a lightweight `HEAD` request to `api.github.com` to detect whether you are online and show the offline indicator.

That request carries no dictated text, history, snippets, or API keys.

### Provider status checks

Verenu periodically asks `api.verenu.com` for the operating status of the transcription and cleanup providers you have selected:

- Settings → Privacy includes **Allow Verenu service checks**, enabled by default. Turn it off to stop these background requests. Verenu clears the in-memory status and health results when you disable the setting.
- Every 5 minutes, it fetches provider status and shows an in-app banner only if the status API flags a real problem for a provider you have actually selected. A provider reporting `operational` or `unknown` does not trigger a banner, and providers you have not selected never surface regardless of their status.
- Every 20 minutes, it does a plain up/down health check of `api.verenu.com` itself. This currently has no UI; the result is kept in memory for future features.
- If a transcription or cleanup call fails in a way that looks provider-side (a quota error, or a retryable timeout/429/5xx), Verenu immediately re-checks provider status instead of waiting for the next scheduled poll.

These are plain GET requests with no request body. They never include your dictated audio, transcripts, history, dictionary, snippets, prompts, or API keys — Verenu's server only ever sends back public status data in response, it does not receive anything from you beyond the bare HTTP request.

## History Loading

The Home view loads recent transcription history in pages of 100 items by default and can request older pages on demand.

This changes UI loading behavior, not storage location. The full history database still lives on your machine unless you export or delete it.

## What Verenu Does Not Send

`api.verenu.com` is in the product path today (release/update metadata and provider status), but even so, Verenu does not send any of this to that server or any other Verenu-owned server:

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
| Repair diagnosis | complaint and bounded in-memory snapshot | complaint and allowlisted bounded diagnostic context to the configured repair provider, only after Analyze |
| Dictionary and snippets | SQLite | nothing by default |
| Context website check | current app state stays local | the typed domain, via a plain DNS lookup, when you attach a website to a context group |
| Auto-learn | local monitoring data and promoted entries | nothing by default |
| Update check | current app state stays local | GitHub release metadata request |
| Connectivity check | current app state stays local | periodic `HEAD` request to `api.github.com` |
| Provider status check | current app state stays local | optional periodic GET to `api.verenu.com/v1/provider-status` (every 5 min, plus an immediate recheck after a provider-side pipeline failure) |
| API health check | current app state stays local | optional periodic GET to `api.verenu.com/v1/health` (every 20 min) |
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
