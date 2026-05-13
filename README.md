<p align="center">
  <img src="docs/banner.svg" alt="Open Flow" width="800"/>
</p>

<p align="center">
  Hold <kbd>Alt</kbd>+<kbd>Space</kbd>, speak, release — your words appear instantly, cleaned up by an LLM, in whatever app is in focus.<br/>
  <em>A free, self-hosted alternative to Wispr Flow. Bring your own API keys. No subscriptions.</em>
</p>

---

##  How it Works

Open Flow handles the entire pipeline from your microphone to your active application seamlessly:

1. **Record:** Hold <kbd>Alt</kbd>+<kbd>Space</kbd> to capture audio. A floating pill window shows live audio levels.
2. **Transcribe:** Audio is sent to your chosen transcription model to generate raw text.
3. **Clean & Format:** An LLM strips filler words, fixes grammar, and applies your context-specific formatting profile.
4. **Inject:** The polished text is instantly injected into your focused app via the clipboard (`Ctrl+V`).

---

##  Features

### Core Dictation
- **Hold-to-Record Hotkey:** Uses a low-level Windows keyboard hook for precise `<kbd>Alt</kbd>+<kbd>Space</kbd>` hold/release timing.
- **Mix & Match AI Providers:** Native support for Groq, OpenAI, and Google. Choose your preferred transcription and cleanup models independently.
- **LLM Cleanup:** Automatically removes filler words, fixes punctuation, and applies formatting.
- **Prompt Injection Protection:** The system prompt explicitly prevents the LLM from acting on any instructions accidentally spoken into the microphone.

### Customization
- **Formatting Profiles:** Switch between *Casual*, *Formal*, *Code*, and *Plain* modes. Profiles adapt automatically based on the active app.
- **Voice Commands:** Dictate commands mid-sentence, such as *"new paragraph"*, *"new line"*, *"bullet point"*, or *"open quote"*.
- **Accent Themes:** Choose your aesthetic: Terracotta (default), Moss, Slate, or Ink.
- **Dictionary & Snippets** *(Coming soon)*: Custom word substitutions, auto-learned corrections, and short-phrase template expansions.

### History & Data
- **Local Transcription History:** Every dictation is saved locally in SQLite, including raw/cleaned text, app name, model used, word count, and duration.

---

##  API Providers

| Provider | Transcription Model | Cleanup Model | Notes |
| :--- | :--- | :--- | :--- |
| **Groq** | `whisper-large-v3-turbo`| `llama-3.3-70b-versatile` | **Recommended:** ~0.5s latency, free tier available. |
| **OpenAI** | `gpt-4o-transcribe` | `gpt-4o-mini` | ~1.0s latency, best for thick accents and background noise. |
| **Google** | `gemini-2.5-flash` | `gemini-2.5-flash` | Audio encoded inline; uses a single API call when both are set to Google. |

*Note: API keys are securely stored locally using `tauri-plugin-store`. They are never written to the SQLite database or exposed in the UI after saving.*

### Getting Your API Key
* **Groq:** Sign up at [console.groq.com](https://console.groq.com) → Create key under *API Keys*.
* **OpenAI:** Go to [platform.openai.com/api-keys](https://platform.openai.com/api-keys) → Create secret key.
* **Google:** Go to [aistudio.google.com](https://aistudio.google.com) → Get API key.

*Paste your key into: Open Flow → Settings → API Keys.*

---

##  Setup & Installation

### Prerequisites
- Windows 10/11 (WebView2 required — ships with Win 11, installable on Win 10)
- [Rust + Cargo](https://rustup.rs/)
- [Node.js 18+](https://nodejs.org/)

### Run in Development
```bash
git clone [https://github.com/MONKE2525E/Open-Flow.git](https://github.com/MONKE2525E/Open-Flow.git)
cd Open-Flow
npm install
npm run tauri dev
