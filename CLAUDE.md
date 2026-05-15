# CLAUDE.md

# Notes from user
- Run npm dev commands yourself please
- Github Repo: https://github.com/MONKE2525E/Open-Flow
- Use the Mono font very sparingly only use it when its in technical items like file names folder names, code, etc...
- docs/ROADMAP.md keeps recorded bugs and long term goals far future plans are not to be acted on unless the user requests so.


This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

Open Flow is an open-source AI dictation desktop app for Windows — a free, API-key-based alternative to the paid "Wispr Flow" app. Users supply their own API keys; there is no subscription. Target RAM usage is ~200MB idle.

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

See [docs/colors.md](docs/colors.md) for the complete color palette, typography, and theming system. The design uses a warm, earthy aesthetic with:
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

# Lint frontend and Rust
npm run lint

# Run Rust tests
npm run test:rust

# Run a single Rust test
cargo test --manifest-path src-tauri/Cargo.toml <test_name>

# Smoke tests — Vite dev server (port 5173)
npm run dev   # terminal 1
node tests/smoke/test.cjs
node tests/smoke/test-app.cjs
node tests/smoke/playwright-test-ui.cjs
node tests/smoke/playwright-test-state.cjs
node tests/smoke/playwright-test-fixes.cjs

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
    components/             # Shared: Toggle, Dropdown, MicInputButton
    components/settings/    # Settings sections: General, Models, ApiKeys, AppMappings, Privacy, Advanced, About
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
  manual/                   # Manual test scripts (hotkey, layout bounds) — not automated
```

## Core Data Flow

```
[Ctrl+Windows held]
  → audio.rs: CPAL captures mic PCM, applies 3.5× gain
  → Emits 'audio-level' every 50ms (RMS) → pill visualizer bars

[Ctrl+Windows released]
  → audio.rs: encode PCM → WAV in memory
  → pipeline.rs run_pipeline():
    1. Quality gates: reject if duration_ms < 700 or rms < 0.008
    2. Capture foreground HWND (before any async work — foreground may change mid-pipeline)
    3. transcription.rs → raw_text
    4. Pure-snippet fast-path: if entire transcription is a single trigger → expand directly, skip cleanup
    5. Otherwise: snippets.rs collect instruction-based triggers → prompts.rs assemble system prompt
    6. cleanup.rs → clean_text (LLM with assembled prompt + snippet instructions)
    7. snippets.rs expand_snippets() (remaining pure-token expansions in clean_text)
    8. dictionary.rs apply_substitutions() (applied last, to final text before injection)
    9. db.rs INSERT transcription record
    10. injection.rs: re-focus captured HWND → save clipboard → contextual-cap check → Ctrl+V → restore clipboard
    11. Emit 'open-flow:transcribed' to frontend
    12. auto_learn.rs: monitor focused text for 30s via UI Automation for corrections
```

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

Groq is the recommended default — free tier, fast LPU inference. Google uses base64-encoded audio in the request body; Groq/OpenAI use multipart form upload. The cleanup API wraps transcription text in `<raw_dictation>` XML delimiters before sending. Google cleanup sets `thinking_budget: 0` to disable deep thinking.

**API fallback:** Retryable errors (timeouts, 429, 5xx) trigger automatic fallback to a secondary provider when `api_fallback_enabled` is true. Fallback chains: groq→[openai, google], openai→[groq, google], google→[groq, openai]. Quota errors (`QUOTA_EXCEEDED:` prefix string — use `quota_bail()` helper) fail immediately with no fallback. Non-retryable errors also fail immediately.

## Global Hotkey Behavior

- **Ctrl+Windows (hold)** → start recording
- **Ctrl+Windows (release)** → stop and process

The hotkey uses a raw `SetWindowsHookExW(WH_KEYBOARD_LL)` hook in `core/hotkey.rs`, not `tauri-plugin-global-shortcut`, because that plugin only fires on keydown — hold/release state requires the low-level hook.

## Formatting Profiles

Active window process name → profile → system prompt prefix sent to cleanup LLM.

Built-in profiles: `casual`, `formal`, `email`, `excited`, `very_casual`. Profile system prompts live in `src-tauri/src/api/cleanup.rs`. `resolve_profile()` reads `AppMapping` entries (`Vec<AppMapping>`) from `tauri-plugin-store` at pipeline time to map foreground process name → profile name. Lookup key is the lowercase `.exe` name. Falls back to `default_tone` setting if no mapping found.

Store keys (API keys, settings) are defined as constants in `src-tauri/src/data/store.rs` — always use the constant, never a raw string literal.

## Prompt Assembly (`prompts.rs`)

`get_system_prompt_with_extras()` builds the final system prompt by appending "FINAL OUTPUT OVERRIDES" at the end — this gives snippet instructions precedence over the base profile prompt when they conflict. User instructions are normalized to MUST/MUST NOT imperatives before insertion.

## Snippet System (`snippets.rs`)

Two execution paths:

1. **Pure expansion** — trigger found as a standalone token in text → replace directly with expansion (strips trailing punctuation added by the transcription model). If the *entire* transcription is a single trigger, this fast-paths past the cleanup LLM entirely.
2. **Instruction collection** — triggers found in text → gather their expansions as instructions → pass to `get_system_prompt_with_extras()` → cleanup LLM applies them contextually.

Cleanup instruction keywords are applied mechanically to LLM output *after* the API call: "all capitals" → uppercase; "no period" / "never add period" → strip trailing periods; "end with exclamation" → force `!`. Negated forms ("don't use all caps") are also checked to prevent conflicts.

## Auto-Learn System (`api/auto_learn.rs`)

After injection, monitors the focused text field for 30 seconds using Windows UI Automation. Only one monitor runs at a time (`MONITOR_ACTIVE` flag prevents overlaps). Detects user corrections via Levenshtein edit distance (`edit_distance`, `is_spelling_correction`) — pairs must differ by ≤2 chars or ≤50% of max length. A (wrong → correct) pair must be observed ≥2 times within 7 days before being promoted to the dictionary. Word count growth is capped at 2× + 10 original words to filter out unrelated edits.

Requires COM initialization (`CoInitializeEx`) per thread via a `ComGuard` — UI Automation will fail silently without it.

## Key Design Constraints

- **No bundled browser.** Tauri uses Windows WebView2 — keep this. Never switch to Electron.
- **RAM target: ~200MB idle.** Profile before adding any heavy JS dependency. See [docs/transcription-ram-reliability-plan.md](docs/transcription-ram-reliability-plan.md) for the prioritized list of known memory and reliability issues in the Rust pipeline.
- **Text injection is clipboard-based.** `SendInput` character-by-character is unreliable across apps; clipboard + Ctrl+V works everywhere.
- **API keys never touch the DB.** Use `tauri-plugin-store` only. Commands that check key presence return a boolean status, never the key itself.
- **MVP scope:** transcription + history + dictionary + snippets + cleanup + hotkey. Insights/stats and IDE integrations are post-MVP.

## Shared Frontend Components

Three reusable components in `src/lib/components/`:

- **Toggle** — `<Toggle checked={bool} onchange={(v) => ...} />`. Renders as `<div class="toggle" role="switch" aria-checked>`. Required by smoke tests — the `toggle` class must stay.
- **Dropdown** — `<Dropdown bind:open closeSelector="">`. Handles click-outside to close; `closeSelector` exempts an element from triggering close.
- **MicInputButton** — `<MicInputButton onResult={(text) => ...} />`. Drives a recording state machine (idle → recording → loading) via `start_input_recording` / `stop_and_transcribe_input` Tauri commands. Shows spinner while loading.

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

### Injection — contextual capitalization and timing
`injection.rs` peeks at the character before the cursor using Shift+Left + Ctrl+C, then inspects the clipboard. If the preceding character is not a sentence-ending `.!?\n\r`, the first letter of the injected text is lowercased (mid-sentence join). Two timing constraints: 60ms wait after the Shift+Left+Ctrl+C sequence for the clipboard to populate, and 30ms between releasing modifier keys and sending Ctrl+V — without this gap, some apps miss the Ctrl key in the same message-pump cycle.

### HWND capture — foreground window before async work
The foreground window HWND is captured at the very start of `run_pipeline()`, before any async API call. This ensures Ctrl+V is sent to the correct window even if the user switches apps during the transcription/cleanup round-trip. The captured HWND is re-focused just before injection.

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
| Model selector rows | `model-row` |
