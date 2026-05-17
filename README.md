<div align="center">
  <img src="docs/banner.svg" alt="Open Flow" width="800"/>
</div>

<p align="center">
  Hold <kbd>Ctrl</kbd>+<kbd>Windows</kbd>, speak, release — your words appear instantly, cleaned up by an LLM, in whatever app is in focus.<br/>
  <em>Free, lightweight, local-first AI dictation. Bring your own API keys. No subscriptions. No telemetry. ~200MB RAM.</em>
</p>

---

## Why Open Flow?

**Lightweight & Fast**
- Uses just ~200MB of RAM idle runs silently in the background without bloat.
- Native Tauri app, not Electron. No bundled browser overhead.
- Latency-optimized: Groq transcription completes in ~0.5 seconds.

**Local-First & Private**
- All your transcription history lives in a local SQLite database on your machine.
- API keys are stored securely on your system, never in the cloud.
- No tracking, no telemetry, no analytics.
- Only audio is sent to your chosen AI provider (Groq, OpenAI, or Google) — you control which.

**Free & Open Source**
- No monthly subscription. Ever. Bring your own API keys.
- Completely self-hosted. You own your data and your setup.
- Open source under [MIT license](LICENSE) audit the code, fork it, use it however you like.

---

##  How it Works

Open Flow handles the entire pipeline from your microphone to your active application seamlessly:

1. **Record:** Hold <kbd>Ctrl</kbd>+<kbd>Windows</kbd> to capture audio locally. A floating pill window shows live audio levels in real time.
2. **Transcribe:** Audio is sent to your chosen transcription model (Groq, OpenAI, or Google) to generate raw text.
3. **Clean & Format:** An LLM strips filler words, fixes grammar, and applies your context-specific formatting profile all text processing stays in your control.
4. **Inject:** The polished text is instantly injected into your focused app via the clipboard (`Ctrl+V`), no permissions required.

---

##  Features

### Core Dictation
- **Hold-to-Record Hotkey:** Uses a low-level Windows keyboard hook for precise <kbd>Ctrl</kbd>+<kbd>Windows</kbd> hold/release timing. Works in any app.
- **Mix & Match AI Providers:** Native support for Groq, OpenAI, and Google. Choose your preferred transcription and cleanup models independently you're not locked in.
- **Smart LLM Cleanup:** Automatically removes filler words, fixes punctuation, and applies formatting — all powered by open APIs you control.
- **Prompt Injection Protection:** Uses `<raw_dictation>` isolation boundaries to explicitly prevent the LLM from acting on any instructions accidentally spoken into the microphone.

### Customization
- **Formatting Profiles:** Switch between *Casual*, *Formal*, and *Very Casual* modes. Profiles adapt automatically based on the active app.
- **Cleanup Intensity:** Pick how aggressively Open Flow rewrites your dictation: *Verbatim*, *Light*, *Medium*, or *Direct*.
- **Snippets:** Create custom abbreviations and auto-expand them during dictation.
- **Snippet Cleanup Instructions:** Add per-snippet cleanup rules like all caps, no ending period, or always ending with an exclamation mark.
- **Personal Dictionary:** Teach Open Flow names, brands, and jargon so the cleanup model preserves the exact spelling you want.
- **Accent Themes:** Choose your aesthetic: Terracotta (default), Moss, Slate, or Ink.

### Privacy & Offline
- **Local Transcription History:** Every dictation is saved locally in SQLite on your machine, including raw/cleaned text, model used, word count, and duration.
- **Auto-Learn Dictionary:** Optionally watch for repeated manual corrections after injection and add them back into your dictionary automatically.
- **No Cloud Storage:** Your data never leaves your computer unless you explicitly sync it.
- **Local Key Storage:** API keys and preferences are stored locally on your machine, not in the transcription database.

---

##  Your Choice of AI Provider

Open Flow is provider-agnostic — pick whichever service you trust, use the cheapest, or switch between them on the fly. All API keys stay on your machine.

| Provider | Transcription Model | Cleanup Model | Latency | Cost |
| :--- | :--- | :--- | :--- | :--- |
| **Groq** | `whisper-large-v3-turbo`| `llama-3.3-70b-versatile` | ~1.0s | Free tier available ⭐ |
| **OpenAI** | `gpt-4o-transcribe` | `gpt-4o-mini` | ~1.0s | ~$0.01–0.03 per request |
| **Google** | `gemini-2.5-flash` | `gemini-2.5-flash` | ~3-5s | Free tier (limited quota) |

**Security:** API keys are stored locally using OS-level encryption (`tauri-plugin-store`). They're never written to the database, synced to the cloud, or logged anywhere.

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

### Windows Security Warning
Because Open Flow is built by a solo developer and the app is not code-signed with an expensive Microsoft-trusted certificate, Windows SmartScreen or your browser may show warnings when downloading or opening the installer. That is expected for unsigned indie software.

If you want extra reassurance, upload the release file to [VirusTotal](https://www.virustotal.com/) and inspect the results before running it.

### Run in Development
```bash
git clone https://github.com/MONKE2525E/Open-Flow.git
cd Open-Flow
npm install
npm run tauri dev
```

---

## Tech Stack: Built for Efficiency

Open Flow is intentionally lightweight. Here's why:

- **Tauri 2.x** — Native Windows app with a minimal WebView2 bridge. No Electron bloat.
- **Svelte 5** — Lean, compiler-driven UI framework. Tiny bundle size.
- **Rust Backend** — Fast, memory-safe, zero-cost abstractions. Audio capture, hotkey handling, and clipboard injection run with near-zero overhead.
- **SQLite** — Embedded database. No server, no network latency, no extra process.

**Result:** ~200MB RAM idle. Starts in <200ms. Runs silently in the background without slowing down your system.
