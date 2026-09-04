# Add your API key

Verenu does not require an account or subscription. Cloud providers use API keys that you provide. Local transcription and local cleanup do not require an API key.

## Choosing a provider

| Provider | Transcription | Cleanup | Notes |
| --- | --- | --- | --- |
| **Local** | On-device models | On-device models | No API key. Models are downloaded from Settings -> Models. |
| **Groq** (recommended) | `whisper-large-v3-turbo` | `qwen/qwen3.6-27b` | Free tier with generous limits and fast inference |
| **OpenAI** | `gpt-4o-transcribe` | `gpt-4o-mini` | Cloud transcription and cleanup |
| **Gemini (Google)** | `gemini-3.5-transcribe` | `gemini-3.5-flash-lite` | Dedicated transcription plus cloud cleanup |
| **AssemblyAI** | `universal-3-5-pro` or `universal-2` | Not available | Transcription-only provider |

One cloud provider key is enough to start dictating. You can add other keys and configure model fallbacks later in Settings. The local path needs no key.

The optional Dual model transcription strategy uses the existing transcription fallback chain and may require keys for more than one configured cloud provider. It is disabled by default.

## Getting a key

### Groq

1. Go to [console.groq.com/keys](https://console.groq.com/keys).
2. Sign in or create a free account.
3. Click **Create API Key**.
4. Copy the key and paste it into Verenu.

### OpenAI

1. Go to [platform.openai.com/api-keys](https://platform.openai.com/api-keys).
2. Sign in to your OpenAI account.
3. Click **Create new secret key**.
4. Copy the key and paste it into Verenu.

### Google

1. Go to [aistudio.google.com](https://aistudio.google.com).
2. Sign in with your Google account.
3. Click **Get API key** -> **Create API key**.
4. Copy the key and paste it into Verenu.

### AssemblyAI

1. Open the AssemblyAI dashboard.
2. Sign in or create an account.
3. Create or copy an API key from the API keys page.
4. Paste the key into Verenu. AssemblyAI can be selected for transcription, not cleanup.

## Where to enter it

- **First run**: setup lets you choose a cloud provider and paste its key, or choose a local model path.
- **Later**: open **Settings -> API Keys**, paste a key into the provider field, and save. A saved key shows only as saved. Use **Clear** to remove it.

## How keys are stored

Your API key never touches Verenu's database or settings file. It is stored using the operating system's secure credential storage:

- **Windows**: Windows Credential Manager
- **macOS**: Keychain

On macOS, if you are prompted for your login password the first time Verenu saves a key, choose **Always Allow** so you are not asked again.

## Next step

With a cloud key saved, or a local model selected, continue with [Your First Dictation](FIRST_DICTATION.md).

## Related docs

<p align="center">
  <a href="INSTALL.md"><img alt="Install" src="https://img.shields.io/badge/Back-Install-7e7266"></a>
  <a href="FIRST_DICTATION.md"><img alt="First Dictation" src="https://img.shields.io/badge/Next-First%20Dictation-c44632"></a>
  <a href="DATA_AND_PRIVACY.md"><img alt="Data And Privacy" src="https://img.shields.io/badge/Data-Privacy-5b554a"></a>
  <a href="README.md"><img alt="Docs Index" src="https://img.shields.io/badge/Docs-Index-2b2422"></a>
</p>
