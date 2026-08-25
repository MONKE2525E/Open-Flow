# Privacy & Data

Verenu doesn't run its own servers, doesn't have an account system, and doesn't collect telemetry or analytics. Your data either stays on your device, or goes directly to the AI provider you chose, and nothing else.

## What stays on your device

- Your API keys (Windows Credential Manager / macOS Keychain)
- Your settings, provider preferences, context groups, targets, and tone preferences
- Your transcription history
- Your context vocabulary, snippets, and auto-learn data
- Local logs, unless you explicitly export them

## What leaves your device

- **Recorded audio** goes to the transcription provider you chose when you finish a dictation unless transcription is local
- **Raw transcription text** goes to your chosen cleanup provider if cleanup is enabled
- **Repair complaints and bounded dictation context** go to your configured AI provider only after you press Analyze in the repair pill
- **Cleanup context** goes along with cleanup requests, including context instructions, cleanup settings, and model metadata
- **Active app context** leaves your device only if you've enabled app-context hints
- **Context website checks** send the domain you typed (nothing else) to DNS when you attach a website to a context group, to confirm it actually exists before saving it
- **Update checks** request GitHub release metadata without sending dictated text, history, or keys
- **Verenu service checks** optionally request public provider status and health data from `api.verenu.com`; disable them in Settings → Privacy

## One important caveat

Once your audio or text reaches a third-party AI provider like Groq, OpenAI, or Google, that provider's own retention and privacy policies apply. Verenu has no control over what happens on their end.

If you want the strictest local path today, use local transcription with `Cleanup: Off`. Local transcription with cloud cleanup still sends transcript text to the cleanup provider.

Repair complaints are not stored remotely or used for analytics. The repair
model proposes a supported change, but Verenu does not apply it until you
explicitly approve it in the pill.

## Want the full breakdown?

This page covers the essentials. For the full technical breakdown, including a feature-by-feature data map, backup and export contents, and key storage details, see [DATA_AND_PRIVACY.md](DATA_AND_PRIVACY.md).

## Related Docs

<p align="center">
  <a href="DATA_AND_PRIVACY.md"><img alt="Full Privacy Doc" src="https://img.shields.io/badge/Full-Privacy%20Doc-c44632"></a>
  <a href="API_KEYS.md"><img alt="API Keys" src="https://img.shields.io/badge/API-Keys-5b554a"></a>
  <a href="SECURITY.md"><img alt="Security Policy" src="https://img.shields.io/badge/Security-Policy-2b2422"></a>
</p>
