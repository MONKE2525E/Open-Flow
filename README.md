<p align="center">
  <img src="docs/banner.svg" alt="Open Flow" width="800"/>
</p>

<p align="center">
  <strong>Open-source AI dictation for Windows.</strong><br/>
  Hold <kbd>Alt+Space</kbd>, speak, release — your words appear instantly, cleaned up by an LLM, in whatever app is focused.<br/>
  A free, self-hosted alternative to Wispr Flow. You bring your own API keys; there is no subscription.
</p>

## How it works

```
Hold Alt+Space  →  mic captures audio
Release         →  audio sent to transcription model → raw text
                →  LLM cleans filler words, fixes grammar, applies your profile
                →  text injected into the focused app via clipboard (Ctrl+V)
```

The floating pill window shows live audio level bars while you record and fades away after injection.

---

## Features

- **Hold-to-record hotkey** — `Alt+Space` hold/release using a low-level Windows keyboard hook (not a tap shortcut library, so the exact hold/release timing is captured correctly)
- **Three AI providers** — Groq, OpenAI, and Google; mix and match transcription and cleanup models independently
- **LLM cleanup** — removes filler words, fixes punctuation, and applies a formatting profile based on context
- **Formatting profiles** — Casual, Formal, Code, and Plain modes; profile switches automatically based on the active app
- **Voice formatting commands** — say "new paragraph", "new line", "bullet point", "open quote" mid-dictation
- **Transcription history** — every dictation is saved locally in SQLite with raw + cleaned text, app name, model used, word count, and duration
- **Dictionary** *(coming soon)* — custom word substitutions and auto-learned corrections
- **Snippets** *(coming soon)* — trigger short phrases that expand into full templates
- **Accent themes** — Terracotta (default), Moss, Slate, and Ink
- **Prompt injection protection** — the cleanup prompt explicitly prevents the LLM from acting on instructions spoken into the mic

---

## API Providers

| Provider | Transcription model | Cleanup model | Notes |
|---|---|---|---|
| **Groq** | `whisper-large-v3-turbo` | `llama-3.3-70b-versatile` | ~0.5 s · free tier · **recommended** |
| **OpenAI** | `gpt-4o-transcribe` | `gpt-4o-mini` | ~1 s · best for accents and noise |
| **Google** | `gemini-2.5-flash` | `gemini-2.5-flash` | audio encoded inline, single API call when both set to Google |

Groq is recommended for getting started — their free tier is fast enough for real-time dictation.

---

## Setup

### Prerequisites

- Windows 10/11 (WebView2 required — ships with Windows 11, installable on 10)
- [Rust + Cargo](https://rustup.rs/)
- [Node.js 18+](https://nodejs.org/)
- A Groq, OpenAI, or Google AI API key

### Install & run

```bash
git clone https://github.com/MONKE2525E/Open-Flow.git
cd Open-Flow
npm install
npm run tauri dev
```

### Build release binary

```bash
npm run tauri build
```

The installer `.msi` and `.exe` are written to `src-tauri/target/release/bundle/`.

---

## Getting your API key

**Groq (recommended — free)**
1. Sign up at [console.groq.com](https://console.groq.com)
2. Create an API key under *API Keys*
3. Paste it into Open Flow → Settings → API Keys → Groq

**OpenAI**
1. Go to [platform.openai.com/api-keys](https://platform.openai.com/api-keys)
2. Create a new secret key
3. Paste it into Open Flow → Settings → API Keys → OpenAI

**Google**
1. Go to [aistudio.google.com](https://aistudio.google.com) → Get API key
2. Paste it into Open Flow → Settings → API Keys → Google

API keys are stored locally using `tauri-plugin-store` and are never written to the SQLite database or exposed in the UI after saving.

---

## Settings

| Setting | Default | Description |
|---|---|---|
| Hotkey | `Alt+Space` | Hold to record, release to process |
| Microphone | Default Device | Any WASAPI input device |
| Auto-cleanup | On | Run LLM cleanup on every transcription |
| Transcription model | Groq / whisper-large-v3-turbo | Audio → text |
| Cleanup model | Groq / llama-3.3-70b-versatile | Text polish + formatting |
| Injection method | Clipboard (Ctrl+V) | How text is placed into apps |

---

## Project structure

```
src/                        # Svelte 5 frontend
  App.svelte                # Root: routing, accent theme, event listeners
  PillApp.svelte            # Floating pill window (recording indicator)
  lib/
    stores.ts               # All Svelte stores
    components/layout/      # TitleBar, Sidebar, DictationPill
    views/                  # Home, Dictionary, Snippets, Settings, Style

src-tauri/src/
  main.rs                   # Tauri setup, command registration, pipeline orchestration
  api/
    transcription.rs        # POST audio → Groq / OpenAI / Google → raw text
    cleanup.rs              # POST raw text to LLM with profile system prompt
  core/
    hotkey.rs               # WH_KEYBOARD_LL hook, hold/release state machine
    injection.rs            # Clipboard-based text injection + Ctrl+V
    window_context.rs       # GetForegroundWindow → process name
  data/
    db.rs                   # SQLite schema + all queries (WAL mode)
    store.rs                # tauri-plugin-store key constants
    dictionary.rs           # Stub: apply_substitutions()
    snippets.rs             # Stub: expand_snippets()
  media/
    audio.rs                # CPAL mic capture → WAV, RMS level streaming
```

---

## Stack

- **Framework:** [Tauri 2.x](https://tauri.app/) — Rust backend + WebView2 frontend
- **Frontend:** Svelte 5 + TypeScript + Tailwind CSS v4
- **Database:** SQLite via `rusqlite` (direct, no ORM)
- **Audio:** `cpal` + `hound` for WAV encoding
- **Windows APIs:** `windows` crate — `SetWindowsHookExW`, `SendInput`, `GetForegroundWindow`
- **HTTP:** `reqwest` (async)
- **Async:** `tokio`

Target RAM usage: ~100 MB idle. No bundled Chromium — WebView2 only.

---

## Design

Open Flow uses a warm, earthy aesthetic:

- **Background:** soft amber paper (`#f9f7f3`)
- **Accent:** Japonica terracotta (`#d97757`)
- **Text:** Armadillo warm-dark palette
- **Typography:** Fraunces serif headings · Inter Tight body · JetBrains Mono mono
- **Accent themes:** Terracotta, Moss, Slate, Ink (switchable)

---

## Roadmap

- [x] Hold-to-record hotkey
- [x] Groq / OpenAI / Google transcription
- [x] LLM cleanup with formatting profiles
- [x] Transcription history (SQLite)
- [x] Settings UI — keys, models, microphone, themes
- [x] Floating pill window with audio level bars
- [ ] Dictionary — custom substitutions + auto-learn
- [ ] Snippets — trigger → expansion
- [ ] Profile auto-detection from active window
- [ ] Installer / auto-update

---

## License

MIT — see [LICENSE](LICENSE)

Open Flow is not affiliated with Wispr or any AI provider.
