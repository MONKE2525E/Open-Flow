# Add Your API Key

Verenu doesn't have its own servers or subscription — it sends your audio and text directly to an AI provider you choose, using an API key you provide. You only need one provider to get started.

## Choosing a provider

| Provider | Transcription | Cleanup | Notes |
| --- | --- | --- | --- |
| **Groq** (recommended) | `whisper-large-v3-turbo` | `qwen/qwen3.6-27b` | Free tier with generous limits, and the fastest of the three (LPU inference) |
| **OpenAI** | `gpt-4o-transcribe` | `gpt-4o-mini` | Best cleanup quality |
| **Google** | `gemini-3.5-transcribe` | `gemini-3.5-flash-lite` | Dedicated transcription plus cheap, fast cleanup |

You can add keys for more than one provider and configure fallback models later in Settings, but a single Groq key is enough to start dictating immediately.

The optional Dual model transcription strategy uses the existing transcription fallback chain and therefore may require API keys for more than one configured provider. It is disabled by default.

## Getting a key

### Groq
1. Go to [console.groq.com/keys](https://console.groq.com/keys)
2. Sign in or create a free account
3. Click **Create API Key**
4. Copy the key and paste it into Verenu

### OpenAI
1. Go to [platform.openai.com/api-keys](https://platform.openai.com/api-keys)
2. Sign in to your OpenAI account
3. Click **Create new secret key**
4. Copy the key and paste it into Verenu

### Google
1. Go to [aistudio.google.com](https://aistudio.google.com)
2. Sign in with your Google account
3. Click **Get API key** → **Create API key**
4. Copy the key and paste it into Verenu

## Where to enter it

- **First run**: Verenu's setup walks you through choosing a provider and pasting in your key on the spot.
- **Later**: open **Settings → API Keys**, paste your key into the field for any provider, and save. A saved key shows as "● saved" — the key itself is never shown again or readable from the UI. Use **Clear** to remove a stored key.

## How keys are stored

Your API key never touches Verenu's database. It's stored using your operating system's secure credential storage:

- **Windows**: Windows Credential Manager
- **macOS**: Keychain

On macOS, if you're prompted for your login password the first time Verenu saves a key, choose **Always Allow** so you aren't asked again.

## Next step

With a key saved, you're ready for [Your First Dictation](FIRST_DICTATION.md).

## Related Docs

<p align="center">
  <a href="INSTALL.md"><img alt="Install" src="https://img.shields.io/badge/Back-Install-7e7266"></a>
  <a href="FIRST_DICTATION.md"><img alt="First Dictation" src="https://img.shields.io/badge/Next-First%20Dictation-c44632"></a>
  <a href="DATA_AND_PRIVACY.md"><img alt="Data And Privacy" src="https://img.shields.io/badge/Data-Privacy-5b554a"></a>
  <a href="README.md"><img alt="Docs Index" src="https://img.shields.io/badge/Docs-Index-2b2422"></a>
</p>
