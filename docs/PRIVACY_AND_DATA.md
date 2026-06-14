# Privacy & Data

Verenu doesn't run its own servers, doesn't have an account system, and doesn't collect telemetry or analytics. Your data either stays on your device, or goes directly to the AI provider you chose — and nothing else.

## What stays on your device

- Your API keys (Windows Credential Manager / macOS Keychain)
- Your settings, provider preferences, app mappings, and tone preferences
- Your transcription history
- Your Dictionary entries, Snippets, and auto-learn data
- Local logs, unless you explicitly export them

## What leaves your device

- **Recorded audio** — sent to the transcription provider you chose, when you finish a dictation
- **Raw transcription text** — sent to your chosen cleanup provider, if cleanup is enabled
- **Cleanup context** — snippet instructions, cleanup settings, and model metadata, sent along with cleanup requests
- **Active app context** — only if you've enabled app-context hints
- **Update checks** — a request to GitHub for release metadata (no dictated text, history, or keys included)

## One important caveat

Once your audio or text reaches a third-party AI provider (Groq, OpenAI, or Google — whichever you chose), that provider's own retention and privacy policies apply. Verenu has no control over what happens on their end — choose a provider whose policies you're comfortable with.

## Want the full breakdown?

This page covers the essentials. For the complete technical breakdown — including a feature-by-feature data map, backup/export contents, and key storage details — see [DATA_AND_PRIVACY.md](DATA_AND_PRIVACY.md).
