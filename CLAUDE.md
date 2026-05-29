# CLAUDE.md

# Notes from user
- Github Repo: https://github.com/MONKE2525E/Open-Flow
- Use the Mono font very sparingly only use it when its in technical items like file names folder names, code, etc...
- docs/ROADMAP.md keeps recorded bugs and long term goals far future plans are not to be acted on unless the user requests so.
- currently working towards OpenFlow 0.11.0.
- Always add yourself as a co-author  in all commits you make e.g @Claude, @Codex, etc but dont add a note at the bottom of the PR description


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

# Smoke tests — unified runner (preferred)
python tests/OnePyFone.py                    # full suite, auto-starts Tauri dev server
python tests/OnePyFone.py --suite ui         # ui suite only
python tests/OnePyFone.py --suite rust       # Rust unit tests only (no server needed)
python tests/OnePyFone.py --vite             # use Vite :5173 instead of Tauri :1420

# Available suites: preflight | rust | pipeline | ui | state | animation
# pipeline suite calls live APIs — skipped automatically when API keys are absent
```

## Agent Skills

When executing tasks, refer to the guidelines in the `Agent-Skills/` directory:
- **Updating version**: See [`Agent-Skills/Updating_version.md`](Agent-Skills/Updating_version.md) for the required files to modify when bumping the application version.
- **Smoke Tests**: See [`Agent-Skills/SmokeTest.md`](Agent-Skills/SmokeTest.md) for testing procedures.
- **Release descriptions**: See [`Agent-Skills/Release_Description_Writing.md`](Agent-Skills/Release_Description_Writing.md) for the canonical format — always wrap output in a ` ```markdown ` code block.
- **Multi-agent parallel work**: See [`Agent-Skills/Multi_Agent_Parallel.md`](Agent-Skills/Multi_Agent_Parallel.md) for the permanent worktree slot workflow (`G:\Open Flow\worktrees\worktree-{1,2,3}`) used to run agents in parallel without port or branch conflicts.

## CI/CD & Review

- **PR checks workflow** (`.github/workflows/pr-checks.yml`) runs: `npm run check && npm run lint && npm run test:rust`
- **Dependency review** (`.github/workflows/dependency-review.yml`) gates new dependencies for supply-chain risk
- **Copilot review instructions** (`.github/copilot-instructions.md`) contain guidance on Rust Windows integration, frontend patterns, and smoke-test contracts — read these before major changes

## Project Structure

```
src/                        # Svelte 5 frontend
  App.svelte                # Root: routing, accent theme injection, event listeners
  PillApp.svelte            # Floating pill window (recording state display)
  main.ts / pill-main.ts    # Vite entry points for each window
  lib/
    stores.ts               # All Svelte writable stores (single file)
    settings.ts             # Typed settings registry: SettingsValueMap, saveSetting() helper, shared types (ProviderId, AppearanceMode, etc.)
    transcriptionLanguages.ts  # ISO 639-1 language list + TranscriptionLanguageCode type (frontend mirror of store.rs validation)
    icons.ts                # SVG icon definitions
    components/layout/      # TitleBar, Sidebar, DictationPill
    components/             # Shared: Toggle, Dropdown, MicInputButton
    components/settings/    # Settings sections: General, Models, ApiKeys, AppMappings, Privacy, Advanced, About
    views/                  # Home (main flow), Dictionary, Snippets, Settings, Setup (first-run API key entry), Style pages
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
      logger.rs             # Logging utilities
      memory.rs             # Memory monitoring
      text.rs               # Text utilities
      volume.rs             # Volume detection
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
| Google | `gemini-3.5-flash` (inline audio) | `gemini-3.5-flash` |

Groq is the recommended default — free tier, fast LPU inference. Google sends audio as base64 in the request body; Groq and OpenAI use multipart form upload. The cleanup request wraps transcription text in `<raw_dictation>` XML tags. Google cleanup sets `thinking_budget: 0`.

The `transcription_language` setting (ISO 639-1, default `en`) is sent to Groq/OpenAI as the `language` form field and to Gemini as a natural-language label via `transcription_language_label()` in `store.rs`. Supported languages: `src/lib/transcriptionLanguages.ts` (frontend), `is_supported_transcription_language()` (Rust).

**API fallback:** Retryable errors (timeouts, 429, 5xx) trigger automatic fallback to any configured fallback models. Fallback models are configured per task (transcription/cleanup) in the Models settings tab and stored as `transcription_fallback_models` / `cleanup_fallback_models` arrays. Fallback is implicit — if fallback models are configured, they are always tried in order. Quota errors (`QUOTA_EXCEEDED:` prefix string — use `quota_bail()` helper) fail immediately with no fallback. Non-retryable errors also fail immediately.

## Global Hotkey Behavior

- **Ctrl+Windows (hold)** → start recording
- **Ctrl+Windows (release)** → stop and process

The hotkey uses a raw `SetWindowsHookExW(WH_KEYBOARD_LL)` hook in `core/hotkey.rs`, not `tauri-plugin-global-shortcut`, because that plugin only fires on keydown — hold/release state requires the low-level hook.

## Formatting Profiles

Active window process name → profile → system prompt prefix sent to cleanup LLM.

Built-in profiles: `casual`, `formal`, `very_casual`. Profile system prompts live in `src-tauri/src/api/cleanup.rs`. `resolve_profile()` reads `AppMapping` entries (`Vec<AppMapping>`) from `tauri-plugin-store` at pipeline time to map foreground process name → profile name. Lookup key is the lowercase `.exe` name. Falls back to `default_tone` setting if no mapping found.

Store keys (API keys, settings) are defined as constants in `src-tauri/src/data/store.rs` — always use the constant, never a raw string literal. When adding a new setting, update both `store.rs` (Rust constant + validation) and `src/lib/settings.ts` (frontend `SettingsValueMap` type entry).

## Settings & Configuration

- **Frontend settings registry** lives in `src/lib/settings.ts` as the `SettingsValueMap` TypeScript type. Add new setting entries here.
- **Backend validation & constants** live in `src-tauri/src/data/store.rs`. All store keys are constants; never use raw string literals.
- **Type mirrors**: `transcriptionLanguages.ts` (frontend) mirrors the backend's supported language validation in `store.rs` — keep them synchronized.
- **API keys** are stored securely via `tauri-plugin-store`, never in SQLite or logs. Commands that check key presence return a boolean status only, never the key itself.

## Prompt Assembly (`prompts.rs`)

`get_system_prompt_with_extras()` builds the final system prompt by appending "FINAL OUTPUT OVERRIDES" at the end — this gives snippet instructions precedence over the base profile prompt when they conflict. User instructions are normalized to MUST/MUST NOT imperatives before insertion.

## Snippet System (`snippets.rs`)

Two execution paths:

1. **Pure expansion** — trigger found as a standalone token in text → replace directly with expansion (strips trailing punctuation added by the transcription model). If the *entire* transcription is a single trigger, this fast-paths past the cleanup LLM entirely.
2. **Instruction collection** — triggers found in text → gather their expansions as instructions → pass to `get_system_prompt_with_extras()` → cleanup LLM applies them contextually.

Cleanup instruction keywords are applied mechanically to LLM output *after* the API call: "all capitals" → uppercase; "no period" / "never add period" → strip trailing periods; "end with exclamation" → force `!`. Negated forms ("don't use all caps") are also checked to prevent conflicts.

## Auto-Learn System (`api/auto_learn.rs`)

After injection, monitors the focused text field for 30 seconds using Windows UI Automation. Each dictation session spawns its own monitor; concurrent monitors are intentional and necessary so corrections from separate dictations can each count toward the 2-session promotion threshold. Detects user corrections via Levenshtein edit distance (`edit_distance`, `is_spelling_correction`) — pairs must differ by ≤2 chars or ≤50% of max length. A `StableTextGate` requires two consecutive identical UIA reads before evaluating corrections, preventing mid-typing noise. A (wrong → correct) pair must be seen in 2 separate sessions within 2 days before being promoted to the dictionary. Within a session, `recorded_this_session` deduplicates so one noisy edit can't count twice.

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

### Version sync — **CRITICAL: all three files must be updated together**
Version must be updated in exactly three files simultaneously, or the build will break:
1. `package.json` (JSON version field)
2. `src-tauri/tauri.conf.json` (Tauri app version)
3. `src-tauri/Cargo.toml` (Rust package version)

The frontend reads the version dynamically via `@tauri-apps/api/app` `getVersion()` — no hardcoded version strings in Svelte files. See [`Agent-Skills/Updating_version.md`](Agent-Skills/Updating_version.md) for the exact procedure.

### Smoke test contracts
Files in `tests/smoke/` are a frozen contract — **never edit them**. Fix the app code to satisfy the tests, not the reverse. CSS classes that tests assert by exact name:

| Element | Required class / selector |
|---|---|
| Sidebar nav buttons | `nav-item` |
| Settings container | `settings-modal` |
| Settings section buttons | `settings-nav-item` |
| Privacy toggles | `toggle` with `role="switch"` and `aria-checked` |
| Info badges | `badge` on a `<div>` |
| Hotkey badge | `badge key-badge` on a `<kbd>` (contains "Ctrl") |
| About GitHub button | `btn-ghost` on a `<button>` |
| Model selector rows | `model-row` (active row also has `active`) |
| Advanced gain display | `span.gain-value` |
