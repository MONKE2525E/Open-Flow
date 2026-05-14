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
- **Audio capture:** `cpal` + `hound` for WAV encoding + `nnnoiseless` for noise reduction
- **Windows native APIs:** `windows` crate (hotkey hook, active window, SendInput, UI Automation)
- **HTTP:** `reqwest` (async API calls to AI providers)
- **Async runtime:** `tokio`
- **Utilities:** `chrono` (timestamps), `anyhow` (error handling)

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

# Smoke tests — Vite dev server (port 5173)
npm run dev   # terminal 1
node tests/smoke/test.js
node tests/smoke/test-app.js

# Smoke tests — full Tauri window (port 1420)
npm run tauri dev   # terminal 1
node tests/smoke/playwright-test-ui.cjs
node tests/smoke/playwright-test-fixes.cjs
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
    main.rs                 # Tauri setup, command registration
    pipeline.rs             # run_pipeline() orchestration, quality gates
    api/
      transcription.rs      # POST audio to Groq/OpenAI/Google → raw text
      cleanup.rs            # POST raw text to LLM with profile system prompt
      auto_learn.rs         # Post-injection correction monitor (Windows UI Automation)
      prompts.rs            # System prompt assembly with recency-bias overrides
      updater.rs            # Auto-update logic
    core/
      hotkey.rs             # WH_KEYBOARD_LL hook, hold/release state machine
      injection.rs          # Clipboard-based text injection + Ctrl+V
      window_context.rs     # GetForegroundWindow → process name
    data/
      db.rs                 # SQLite schema (inline) + migrations + all queries
      store.rs              # tauri-plugin-store key constants
      dictionary.rs         # Dictionary substitution logic
      snippets.rs           # Snippet expansion (pure and instruction-based paths)
    media/
      audio.rs              # CPAL mic capture → WAV, RMS level streaming
    system/
      apps.rs               # Registry scan for installed apps + process dedup
  Cargo.toml
  tauri.conf.json           # 2 windows: main (1100×720) + pill (220×60 always-on-top)
tests/
  smoke/                    # Playwright smoke tests — NEVER edit these files
```

## Core Data Flow

```
[Alt+Space held]
  → audio.rs: CPAL captures mic PCM, applies 3.5× gain
  → Emits 'audio-level' every 50ms (RMS) → pill visualizer bars

[Alt+Space released]
  → audio.rs: encode PCM → WAV in memory
  → pipeline.rs run_pipeline():
    1. Quality gates: reject if duration_ms < 700 or rms < 0.008
    2. transcription.rs → raw_text
    3. dictionary.rs apply_substitutions()
    4. prompts.rs get_system_prompt_with_extras() → assembles profile prompt + snippet instructions
    5. cleanup.rs → clean_text (LLM with assembled prompt)
    6. snippets.rs expand_snippets() (pure expansions applied directly)
    7. db.rs INSERT transcription record
    8. injection.rs: save clipboard → write clean_text → Ctrl+V → restore clipboard
    9. Emit 'open-flow:transcribed' to frontend
    10. auto_learn.rs: monitor focused text for 30s via UI Automation for corrections
```

**Google shortcut:** When Google is selected for both transcription and cleanup, `transcribe_and_cleanup_gemini()` combines both steps into a single API call to save a round-trip.

## SQLite Schema

Schema is defined inline in `src-tauri/src/data/db.rs` (not in migration files). Three tables:

```sql
transcriptions  (id, raw_text, clean_text, app_name, profile, api_used, words, duration_ms, created_at)
dictionary      (id, wrong, correct, auto_learned, correction_count, created_at)
snippets        (id, trigger, expansion, use_count, created_at)
```

WAL mode is enabled. Migrations use `execute_batch` wrapped in explicit `BEGIN/COMMIT/ROLLBACK` — a failed migration rolls back fully rather than leaving a partial schema. API keys are never stored in SQLite — use `tauri-plugin-store` only.

## API Providers

| Provider | Transcription | Cleanup |
|---|---|---|
| Groq | `whisper-large-v3-turbo` | `llama-3.3-70b-versatile` |
| OpenAI | `gpt-4o-transcribe` | `gpt-4o-mini` |
| Google | `gemini-2.5-flash` (inline audio) | `gemini-2.5-flash` |

Groq is the recommended default — free tier, fast LPU inference. Google uses base64-encoded audio in the request body; Groq/OpenAI use multipart form upload. The cleanup API wraps transcription text in `<raw_dictation>` XML delimiters before sending. Google cleanup sets `thinking_budget: 0` to disable deep thinking. 429 quota-exceeded errors are handled distinctly from other API errors.

## Global Hotkey Behavior

- **Alt+Space (hold)** → start recording
- **Alt+Space (release)** → stop and process

The hotkey uses a raw `SetWindowsHookExW(WH_KEYBOARD_LL)` hook in `core/hotkey.rs`, not `tauri-plugin-global-shortcut`, because that plugin only fires on keydown — hold/release state requires the low-level hook.

## Formatting Profiles

Active window process name → profile → system prompt prefix sent to cleanup LLM.

Built-in profiles: `casual`, `formal`, `email`, `excited`, `very_casual`. Profile system prompts live in `src-tauri/src/api/cleanup.rs`. `resolve_profile()` reads `AppMapping` entries (`Vec<AppMapping>`) from `tauri-plugin-store` at pipeline time to map foreground process name → profile name.

## Prompt Assembly (`prompts.rs`)

`get_system_prompt_with_extras()` builds the final system prompt by appending "FINAL OUTPUT OVERRIDES" at the end — this gives snippet instructions precedence over the base profile prompt when they conflict. User instructions are normalized to MUST/MUST NOT imperatives before insertion.

## Snippet System (`snippets.rs`)

Two execution paths:

1. **Pure expansion** — trigger found as a standalone token in transcribed text → replace directly with expansion (strips trailing punctuation added by the transcription model).
2. **Instruction collection** — triggers found in text → gather their expansions as instructions → pass to `get_system_prompt_with_extras()` → cleanup LLM applies them contextually.

## Auto-Learn System (`api/auto_learn.rs`)

After injection, monitors the focused text field for 30 seconds using Windows UI Automation. Detects user corrections by comparing pre/post text with Levenshtein edit distance (`edit_distance`, `is_spelling_correction`). A (wrong → correct) pair must be observed ≥3 times within 7 days before being promoted to the dictionary. Word count growth is capped at 2× + 10 original words to filter out unrelated edits.

## Key Design Constraints

- **No bundled browser.** Tauri uses Windows WebView2 — keep this. Never switch to Electron.
- **RAM target: ~200MB idle.** Profile before adding any heavy JS dependency.
- **Text injection is clipboard-based.** `SendInput` character-by-character is unreliable across apps; clipboard + Ctrl+V works everywhere.
- **API keys never touch the DB.** Use `tauri-plugin-store` only. Commands that check key presence return a boolean status, never the key itself.
- **MVP scope:** transcription + history + dictionary + snippets + cleanup + hotkey. Insights/stats and IDE integrations are post-MVP.

## Patterns & Gotchas

### Hotkey hook — callback timing
The `WH_KEYBOARD_LL` callback in `core/hotkey.rs` must return within ~300ms or Windows silently kills it. All actual work (pipeline, async calls) must happen in a spawned tokio task, not in the hook body. The hook only sends a `HotkeyEvent` enum over a channel.

### Pill window — never hide it
Hiding the pill window suspends the WebView2 renderer. The next state event emitted while it is hidden will be silently dropped, leaving the pill stuck. Keep it always-visible but click-through + transparent in idle state. Emit state events *after* showing the window, not before. The pill uses `SW_SHOWNOACTIVATE` so it appears without stealing focus from the target app. In idle/passive states the pill is click-through; only in "handsfree" state does it accept real cursor events for buttons.

### Recording quality gates
`run_pipeline()` silently rejects recordings below two thresholds before calling any API:
- `duration_ms < 700` — avoids Whisper hallucinations on short clips
- `rms < 0.008` — near-silence, likely accidental activation

No user-facing feedback is shown when rejected. These are currently magic numbers in `pipeline.rs`.

### Error handling convention
Use `anyhow::Result` throughout Rust. Pipeline errors call `show_error_pill()` which logs, emits `open-flow:error` to the frontend (caught as a toast in `App.svelte`), and returns without crashing. Match this pattern for any new error path in the pipeline.

### Version sync
Version must be updated in exactly three files simultaneously: `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml`. The frontend reads the version dynamically via `@tauri-apps/api/app` `getVersion()` — no hardcoded version strings in Svelte files.

### Smoke test contracts
Files in `tests/smoke/` are a frozen contract — **never edit them**. Fix the app code to satisfy the tests, not the reverse. CSS classes that tests assert by exact name:

| Element | Required class |
|---|---|
| Sidebar nav buttons | `nav-item` |
| Settings container | `settings-modal` |
| Settings section buttons | `settings-nav-item` |
| Privacy toggles | `toggle` |
| Info badges | `badge` on a `<div>` |
| Hotkey badge | `badge key-badge` on a `<kbd>` |
| About GitHub button | `btn-ghost` on a `<button>` |
