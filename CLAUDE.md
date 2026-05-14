# CLAUDE.md

# Notes from user
- Run npm dev commands yourself please
- Github Repo: https://github.com/MONKE2525E/Open-Flow
- Use the Mono font very sparingly only use it when its in technical items like file names folder names, code, etc...
- ROADMAP.md Keeps recorded bugs and long term goals far future plans are not to be acted on unless the user requests so.


This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

Open Flow is an open-source AI dictation desktop app for Windows — a free, API-key-based alternative to the paid "Wispr Flow" app. Users supply their own API keys; there is no subscription. Target RAM usage is ~100MB idle.

## Stack

- **Framework:** Tauri 2.x (Rust backend + WebView2 frontend — not Electron, not a web app)
- **Frontend:** Svelte 5 + TypeScript + Tailwind CSS v4
- **Database:** SQLite via `rusqlite` (direct, not `tauri-plugin-sql`)
- **Settings store:** `tauri-plugin-store`
- **Audio capture:** `cpal` + `hound` for WAV encoding
- **Windows native APIs:** `windows` crate (hotkey hook, active window, SendInput)
- **HTTP:** `reqwest` (async API calls to AI providers)
- **Async runtime:** `tokio`

## Design System

See [colors.md](colors.md) for the complete color palette, typography, and theming system. The design uses a warm, earthy aesthetic with:
- Soft-amber paper background (`#f9f7f3`)
- Japonica terracotta accent (`#d97757`)
- Armadillo warm-dark text palette
- Fraunces serif, Inter Tight sans, JetBrains Mono monospace typography
- Accent themes (Terracotta, Moss, Slate, Ink) applied via OKLch CSS variables in `App.svelte`

## Build & Dev Commands

```bash
# Install JS dependencies
npm install

# Run in development (hot-reload)
npm run tauri dev

# Build release binary
npm run tauri build

# Type-check frontend only
npm run check

# Lint frontend
npm run lint

# Run Rust tests
cargo test --manifest-path src-tauri/Cargo.toml

# Run a single Rust test
cargo test --manifest-path src-tauri/Cargo.toml <test_name>
```

## Agent Skills

When executing tasks, refer to the guidelines in the `Agent-Skills/` directory:
- **Updating version**: See [`Agent-Skills/Updating_version.md`](Agent-Skills/Updating_version.md) for the required files to modify when bumping the application version.
- **Smoke Tests**: See [`Agent-Skills/SmokeTest.md`](Agent-Skills/SmokeTest.md) for testing procedures.

## Project Structure

```
src/                        # Svelte 5 frontend
  App.svelte                # Root: routing, accent theme injection, event listeners
  PillApp.svelte            # Floating pill window (recording state display)
  main.ts / pill-main.ts    # Vite entry points for each window
  lib/
    stores.ts               # All Svelte writable stores (single file)
    icons.ts                # SVG icon definitions
    components/layout/      # TitleBar, Sidebar, DictationPill
    views/                  # Home, Dictionary, Snippets, Settings, Style pages
src-tauri/
  src/
    main.rs                 # Tauri setup, command registration, run_pipeline()
    api/
      transcription.rs      # POST audio to Groq/OpenAI/Google → raw text
      cleanup.rs            # POST raw text to LLM with profile system prompt
    core/
      hotkey.rs             # WH_KEYBOARD_LL hook, hold/release state machine
      injection.rs          # Clipboard-based text injection + Ctrl+V
      window_context.rs     # GetForegroundWindow → process name
    data/
      db.rs                 # SQLite schema (inline) + all queries
      store.rs              # tauri-plugin-store key constants
      dictionary.rs         # STUB: apply_substitutions() not yet implemented
      snippets.rs           # STUB: expand_snippets() not yet implemented
    media/
      audio.rs              # CPAL mic capture → WAV, RMS level streaming
  Cargo.toml
  tauri.conf.json           # 2 windows: main (1100×720) + pill (220×60 always-on-top)
```

## Core Data Flow

```
[Alt+Space held]
  → audio.rs: CPAL captures mic PCM, applies 3.5× gain
  → Emits 'audio-level' every 50ms (RMS) → pill visualizer bars

[Alt+Space released]
  → audio.rs: encode PCM → WAV in memory
  → run_pipeline() in main.rs:
    1. transcription.rs → raw_text
    2. dictionary.rs apply_substitutions() [STUB — no-op]
    3. cleanup.rs → clean_text (LLM with profile prompt)
    4. snippets.rs expand_snippets() [STUB — no-op]
    5. db.rs INSERT transcription record
    6. injection.rs: save clipboard → write clean_text → Ctrl+V → restore clipboard
    7. Emit 'open-flow:transcribed' to frontend
```

## SQLite Schema

Schema is defined inline in `src-tauri/src/data/db.rs` (not in migration files). Three tables:

```sql
transcriptions  (id, raw_text, clean_text, app_name, profile, api_used, words, duration_ms, created_at)
dictionary      (id, wrong, correct, auto_learned, correction_count, created_at)
snippets        (id, trigger, expansion, use_count, created_at)
```

WAL mode is enabled. API keys are never stored in SQLite — use `tauri-plugin-store` only.

## API Providers

| Provider | Transcription | Cleanup |
|---|---|---|
| Groq | `whisper-large-v3-turbo` | `llama-3.3-70b-versatile` |
| OpenAI | `gpt-4o-transcribe` | `gpt-4o-mini` |
| Google | `gemini-2.5-flash` (inline audio) | `gemini-2.5-flash` |

Groq is the recommended default — free tier, fast LPU inference. Google uses base64-encoded audio in the request body; Groq/OpenAI use multipart form upload.

## Global Hotkey Behavior

- **Alt+Space (hold)** → start recording
- **Alt+Space (release)** → stop and process

The hotkey uses a raw `SetWindowsHookExW(WH_KEYBOARD_LL)` hook in `core/hotkey.rs`, not `tauri-plugin-global-shortcut`, because that plugin only fires on keydown — hold/release state requires the low-level hook. Profile mapping (active window process → profile name) is partially hardcoded in `main.rs` and will eventually read from the store.

## Formatting Profiles

Active window process name → profile → system prompt prefix sent to cleanup LLM.

Built-in profiles: `casual`, `formal`, `email`, `excited`, `very_casual`. Profile system prompts live in `src-tauri/src/api/cleanup.rs`.

## Key Design Constraints

- **No bundled browser.** Tauri uses Windows WebView2 — keep this. Never switch to Electron.
- **RAM target: ~200MB idle.** Profile before adding any heavy JS dependency.
- **Text injection is clipboard-based.** `SendInput` character-by-character is unreliable across apps; clipboard + Ctrl+V works everywhere.
- **API keys never touch the DB.** Use `tauri-plugin-store` only. Commands that check key presence return a boolean status, never the key itself.
- **Dictionary and snippets are stubs.** The DB tables and UI exist, but `apply_substitutions()` and `expand_snippets()` are no-ops pending implementation.
- **MVP scope:** transcription + history + dictionary + snippets + cleanup + hotkey. Insights/stats and IDE integrations are post-MVP.

## Patterns & Gotchas

### Hotkey hook — callback timing
The `WH_KEYBOARD_LL` callback in `core/hotkey.rs` must return within ~300ms or Windows silently kills it. All actual work (pipeline, async calls) must happen in a spawned tokio task, not in the hook body. The hook only sends a `HotkeyEvent` enum over a channel.

### Pill window — never hide it
Hiding the pill window suspends the WebView2 renderer. The next state event emitted while it is hidden will be silently dropped, leaving the pill stuck. Keep it always-visible but click-through + transparent in idle state. Emit state events *after* showing the window, not before.

### Recording quality gates
`run_pipeline()` silently rejects recordings below two thresholds before calling any API:
- `duration_ms < 700` — avoids Whisper hallucinations on short clips
- `rms < 0.008` — near-silence, likely accidental activation

No user-facing feedback is shown when rejected. These are currently magic numbers in `pipeline.rs`.

### Error handling convention
Use `anyhow::Result` throughout Rust. Pipeline errors call `show_error_pill()` which logs, emits `open-flow:error` to the frontend (caught as a toast in `App.svelte`), and returns without crashing. Match this pattern for any new error path in the pipeline.

### App profile mapping
`resolve_profile()` reads `AppMapping` entries (`Vec<AppMapping>`) from `tauri-plugin-store` at pipeline time to map foreground process name → profile name. Built-in profile system prompts are defined in `api/cleanup.rs`.
