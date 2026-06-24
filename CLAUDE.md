# CLAUDE.md

# Notes from user
- Github Repo: https://github.com/MONKE2525E/Verenu
- Use the Mono font very sparingly only use it when its in technical items like file names folder names, code, etc...
- docs/ROADMAP.md keeps recorded bugs and long term goals far future plans are not to be acted on unless the user requests so.
- currently working towards Verenu 0.15.0.
- Always add yourself as a co-author  in all commits you make e.g @Claude, @Codex, @google-antigravity, etc but dont add a note at the bottom of the PR description

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

Verenu is an open-source AI dictation desktop app for Windows and macOS — a free, API-key-based alternative to the paid "Wispr Flow" app. Users supply their own API keys; there is no subscription. Target RAM usage is ~200MB idle.

## Stack

- **Framework:** Tauri 2.x (Rust backend + WebView2 frontend — not Electron, not a web app)
- **Frontend:** Svelte 5 + TypeScript + Tailwind CSS v4
- **Database:** SQLite via `rusqlite` (direct, not `tauri-plugin-sql`)
- **Settings store:** backend-owned JSON at Tauri `BaseDirectory::AppData/settings.json` via `src-tauri/src/data/store.rs` (non-secret settings only; see [Settings & Configuration](#settings--configuration) for where API keys actually live)
- **Audio capture:** `cpal` + `hound` for WAV encoding + `nnnoiseless` for noise reduction
- **Windows native APIs:** `windows` crate (hotkey hook, active window, SendInput, UI Automation, Credential Manager)
- **macOS native APIs:** `core-graphics` (`CGEventTap` global hotkey), `security-framework` (Keychain), `accessibility-sys` (AX API), `objc2`/`objc2-foundation` (AppKit interop — `NSWorkspace`, `NSPasteboard`), `coreaudio-rs` (native mute control)
- **HTTP:** `reqwest` (async API calls to AI providers)
- **Async runtime:** `tokio`
- **Utilities:** `chrono` (timestamps), `anyhow` (error handling)

## Design System

See [docs/colors.md](docs/colors.md) for the complete color palette, typography, and theming system.

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

# Unified tests — preferred entrypoint
npm test                                     # fast profile
npm run test:all                             # same fast profile explicitly
npm run test:full                            # fast + live + native (everything, incl. opt-in suites)
npm run test:live                            # live API checks, skips when keys are absent
npm run test:native                          # platform/manual-adjacent checks
python tests/OnePyFone.py                    # fast profile, auto-starts Vite :1420
python tests/OnePyFone.py --profile full     # fast + live + native
python tests/OnePyFone.py --suite ui,state   # targeted suites

# Available profiles: fast | live | native | full
# Available suites: preflight | frontend | rust | contract | ui | state | animation | pipeline | native
# fast is deterministic and CI-friendly; live/native are opt-in

# Run Playwright smoke tests directly (bypasses the OnePyFone harness)
npm run test:smoke                           # tests/smoke/*.cjs in sequence
npm run test:smoke:state                     # state-machine smoke test only
```

## Agent Skills

When executing tasks, refer to the guidelines in the `Agent-Skills/` directory:
- **Updating version**: See [`Agent-Skills/Updating_version.md`](Agent-Skills/Updating_version.md) for the required files to modify when bumping the application version.
- **Smoke Tests**: See [`Agent-Skills/SmokeTest.md`](Agent-Skills/SmokeTest.md) for testing procedures.
- **Release descriptions**: See [`Agent-Skills/Release_Description_Writing.md`](Agent-Skills/Release_Description_Writing.md) for the canonical format — always wrap output in a ` ```markdown ` code block.

## CI/CD & Review

- **PR checks workflow** (`.github/workflows/pr-checks.yml`) keeps frontend and OS-matrix Rust jobs, then runs the unified fast profile with JSON/JUnit reports
- **Extended profiles workflow** (`.github/workflows/extended-test-profiles.yml`) runs opt-in live/native profiles on schedule or manual dispatch
- **Build installers workflow** (`.github/workflows/build-installers.yml`) is manual (`workflow_dispatch`) — builds release installers on demand, not on every push
- **Dependency review** (`.github/workflows/dependency-review.yml`) gates new dependencies for supply-chain risk
- **Copilot review instructions** (`.github/copilot-instructions.md`) contain guidance on Rust Windows integration, frontend patterns, and smoke-test contracts — read these before major changes

## Project Structure

```
src/                        # Svelte 5 frontend
  App.svelte                # Root: routing, accent theme injection, event listeners
  PillApp.svelte            # Floating pill window (recording state display)
  main.ts / pill-main.ts    # Vite entry points for each window
  lib/
    stores.ts               # Svelte writable stores (legacy; most stores are still here)
    stores.svelte.ts        # Svelte 5 runes-based stores (new additions go here)
    settings.ts             # Typed settings registry: SettingsValueMap, saveSetting() helper, shared types (ProviderId, AppearanceMode, etc.)
    transcriptionLanguages.ts  # ISO 639-1 language list + TranscriptionLanguageCode type (frontend mirror of store.rs validation)
    calibration.ts          # Mic gain auto-calibration state machine (loud/whisper phases)
    appMappings.ts          # App-to-profile mapping store helpers
    platform.ts             # Runtime platform detection (Windows vs macOS)
    motion.ts               # Animation/transition utilities
    modalFocus.ts           # Reusable modal focus trap + focus restoration helper
    tauri.ts                # Typed wrapper around @tauri-apps/api invoke/listen
    icons.ts                # SVG icon definitions
    components/layout/      # TitleBar, Sidebar, DictationPill
    components/             # Shared: Toggle, Dropdown, MicInputButton
    components/settings/    # Settings sections: General, Models, ApiKeys, AppMappings, Privacy, Audio, Advanced, About
    views/                  # Home (main flow + paginated history), Dictionary, Snippets, Settings, Setup (first-run wizard orchestrator), Style pages
    setup/                  # First-run wizard: SetupShell.svelte (shared chrome/header/action bar) + steps/ (Intro, Provider, ApiKey, Permissions [macOS], WritingStyle, Appearance, QuickSettings, Calibration, TryIt, Done)
    calibrationCopy.ts       # Localized copy strings for the mic-calibration UI (companion to calibration.ts)
src-tauri/
  src/
    main.rs                 # Tauri setup, state initialization, command registration
    pipeline/
      mod.rs                # run_pipeline() orchestration, quality gates, pill window creation/resize
      finalize.rs           # final persistence, injection, frontend event emission
      pill.rs               # floating pill window helpers/state
      fixture.rs            # pipeline test/debug fixtures
      tests.rs              # pipeline-focused tests
    commands/mod.rs         # All #[tauri::command] handlers (extracted from main.rs)
    testing.rs              # Test fixture infrastructure — cfg(test)/debug_assertions only
    api/
      mod.rs                 # Shared retry/auth-error classification: AuthErrorCategory, is_retryable_provider_error(), quota_bail()
      client.rs              # Shared reqwest client construction
      gemini_types.rs        # Gemini request/response (de)serialization types
      transcription.rs      # POST audio to Groq/OpenAI/Google → raw text
      cleanup.rs            # POST raw text to LLM with profile system prompt
      auto_learn.rs         # Auto-learn coordinator
      auto_learn/
        correction.rs       # Candidate detection and correction ranking helpers
      prompts/
        mod.rs              # Prompt assembly entrypoints
        transcription.rs    # Transcription-facing prompt text
        cleanup_rules.rs    # Cleanup rule assembly
        cleanup_templates.rs # Provider/model prompt templates
        gemini.rs           # Gemini-specific generation config
        tests.rs            # Prompt tests
      updater.rs            # Auto-update logic
    core/
      hotkey/
        mod.rs               # Platform-dispatch + shared chord/handsfree state
        win.rs                # Windows: SetWindowsHookExW(WH_KEYBOARD_LL) hold/release hook
        mac.rs                # macOS: CGEventTap (listen-only) hold/release hook
      injection/
        mod.rs              # Shared clipboard-based text injection + platform dispatch
        windows.rs          # Windows paste/clipboard path
        macos.rs            # macOS paste/clipboard path
      context_probe.rs      # Layered InjectionContextProbe: caret-local read (UIA/AX) → clipboard-sniff → history fallback
      context_probe_macos.rs # macOS-only half of context_probe (AX-based caret read)
      text_context.rs       # SentenceContext / InjectionPrefixClass classification used by context_probe + injection
      window_context.rs     # Foreground window → process name (GetForegroundWindow / NSWorkspace)
    data/
      db/
        mod.rs              # SQLite schema (inline) + migrations + shared DB entrypoints
        transcriptions.rs   # History queries, insert/delete, pagination
        dictionary.rs       # Dictionary table queries
        snippets.rs         # Snippet table queries
        validation.rs       # Import/export and row validation helpers
      store.rs              # settings key constants plus backend-owned JSON settings store (NOT api keys; see credentials.rs)
      credentials.rs        # API key storage: Windows Credential Manager (CredWriteW/CredReadW) / macOS Keychain (security-framework). Never settings.json, never SQLite.
      dictionary.rs         # Dictionary substitution logic
      snippets.rs           # Snippet expansion (pure and instruction-based paths)
    media/
      audio.rs              # CPAL mic capture → WAV, RMS level streaming
    system/
      apps.rs               # Registry scan for installed apps + process dedup
      mac_app.rs             # macOS-only: NSWorkspace app lookup, NSPasteboard helpers
      macos_ax_text_marker.m # Objective-C shim for AX text-marker APIs (compiled via cc, see build-dependencies)
      logger.rs               # Logging utilities
      memory.rs              # Memory monitoring
      number_parser.rs        # Spoken-number → digit parsing
      text.rs               # Text utilities
      volume.rs             # Volume detection
  Cargo.toml
  tauri.conf.json           # Declares only the "main" window (1100×720). The pill window is NOT statically configured.
tests/
  smoke/                    # Playwright smoke tests — NEVER edit these files
  manual/                   # Manual test scripts (hotkey, layout bounds) — not automated
```

The pill window is created at runtime by `create_pill_if_needed()` in `pipeline/mod.rs` (`WebviewWindowBuilder`, always-on-top, transparent, decorations off, initial size 380x44) — it is resized per recording state rather than recreated, so don't look for its dimensions in `tauri.conf.json`.

## Core Data Flow

```
[Ctrl+Windows held]
  → audio.rs: CPAL captures mic PCM, applies 3.5× gain
  → Emits 'audio-level' every 50ms (RMS) → pill visualizer bars

[Ctrl+Windows released]
  → audio.rs: encode PCM → WAV in memory
  → pipeline/mod.rs run_pipeline():
    1. Quality gates: reject if duration_ms < 700 or rms < 0.008
    2. Capture foreground HWND (before any async work — foreground may change mid-pipeline)
    3. transcription.rs → raw_text
    4. Pure-snippet fast-path: if entire transcription is a single trigger → expand directly, skip cleanup
    5. Otherwise: snippets.rs collect instruction-based triggers → api/prompts/mod.rs: assemble system prompt
    6. cleanup.rs → clean_text (LLM with assembled prompt + snippet instructions)
    7. snippets.rs expand_snippets() (remaining pure-token expansions in clean_text)
    8. dictionary.rs apply_substitutions() (applied last, to final text before injection)
    9. data/db/transcriptions.rs INSERT transcription record
    10. core/injection/mod.rs: re-focus captured HWND → save clipboard → contextual-cap check → Ctrl+V → restore clipboard
    11. Emit 'verenu:transcribed' to frontend
    12. auto_learn.rs: monitor focused text for 60s (MONITOR_WINDOW_SECS) via UI Automation/AX for corrections
```

## SQLite Schema

Schema is defined inline in `src-tauri/src/data/db/mod.rs` (not in migration files). Three tables:

```sql
transcriptions  (id, raw_text, clean_text, app_name, profile, api_used, words, duration_ms, created_at)
dictionary      (id, wrong, correct, auto_learned, correction_count, created_at)
snippets        (id, trigger, expansion, use_count, created_at)
```

WAL mode is enabled. Migrations use `execute_batch` wrapped in explicit `BEGIN/COMMIT/ROLLBACK` — a failed migration rolls back fully rather than leaving a partial schema. API keys are never stored in SQLite — they live in the OS credential store (`data/credentials.rs`).

## API Providers

| Provider | Transcription | Cleanup |
|---|---|---|
| Groq | `whisper-large-v3-turbo` | `llama-3.3-70b-versatile` |
| OpenAI | `gpt-4o-transcribe` | `gpt-4o-mini` |
| Google | `gemini-3.5-flash` (inline audio) | `gemini-3.5-flash` |

Groq is the recommended default — free tier, fast LPU inference. Google sends audio as base64 in the request body; Groq and OpenAI use multipart form upload. The cleanup request wraps transcription text in `<raw_dictation>` XML tags. Google cleanup sets `thinking_budget: 0`.

The `transcription_language` setting (ISO 639-1, default `en`) is sent to Groq/OpenAI as the `language` form field and to Gemini as a natural-language label via `transcription_language_label()` in `store.rs`. Supported languages: `src/lib/transcriptionLanguages.ts` (frontend), `is_supported_transcription_language()` (Rust).

**API fallback:** Retryable errors (timeouts, 429, 5xx) trigger automatic fallback to any configured fallback models. Fallback models are configured per task (transcription/cleanup) in the Models settings tab and stored as `transcription_fallback_models` / `cleanup_fallback_models` arrays. Fallback is implicit — if fallback models are configured, they are always tried in order. Quota errors (`QUOTA_EXCEEDED:` prefix string — use `quota_bail()` helper) fail immediately with no fallback. Non-retryable errors also fail immediately. `is_retryable_provider_error()` in `api/mod.rs` is the single source of truth for what counts as retryable (reqwest timeout/connect errors, HTTP 408/429/5xx, and a few message-substring heuristics).

**401 handling:** `api/mod.rs` classifies unauthorized responses into `AuthErrorCategory` (`InvalidOrRevokedKey`, `ScopeOrAccountRestriction`, `UnknownUnauthorized`) via `classify_unauthorized_body()`, and encodes the result as a structured `AUTH_401|provider=...|category=...` error string (`auth_401_error()` / `parse_auth_401_error()`) so the frontend can show a category-specific message instead of a generic failure.

## Global Hotkey Behavior

- **Windows:** Ctrl+Windows (hold) → start recording; release → stop and process
- **macOS:** Fn+Control (hold/release) via `CGEventTap`

`core/hotkey/` is split by platform: `win.rs` uses a raw `SetWindowsHookExW(WH_KEYBOARD_LL)` hook, `mac.rs` uses a listen-only `CGEventTap`, and `mod.rs` holds the platform dispatch plus shared chord/handsfree state. `tauri-plugin-global-shortcut` is a declared dependency but is not used for the hold/release hotkey — that plugin only fires on keydown, so hold/release state requires the low-level hook on both platforms.

## Formatting Profiles

Active window process name → profile → system prompt prefix sent to cleanup LLM.

Built-in profiles: `casual`, `formal`, `very_casual`. Profile system prompts live in `src-tauri/src/api/cleanup.rs`. `resolve_app_mapping()` reads `AppMapping` entries (`Vec<AppMapping>`) from the backend settings snapshot at pipeline time to map foreground process name → profile name. Lookup key is the lowercase `.exe` name. Falls back to `default_tone` setting if no mapping found.

Store keys (settings, plus the API key *identifiers* used as credential usernames) are defined as constants in `src-tauri/src/data/store.rs` — always use the constant, never a raw string literal. When adding a new setting, update both `store.rs` (Rust constant + validation) and `src/lib/settings.ts` (frontend `SettingsValueMap` type entry).

## Settings & Configuration

- **Frontend settings registry** lives in `src/lib/settings.ts` as the `SettingsValueMap` TypeScript type. Add new setting entries here.
- **Backend settings storage, validation & constants** live in `src-tauri/src/data/store.rs` and `src-tauri/src/commands/settings.rs`. All settings keys are constants; never use raw string literals.
- **Type mirrors**: `transcriptionLanguages.ts` (frontend) mirrors the backend's supported language validation in `store.rs` — keep them synchronized.
- **API keys do not live in settings.json.** They are stored in the native OS credential store via `src-tauri/src/data/credentials.rs`: Windows Credential Manager (`CredWriteW`/`CredReadW`/`CredDeleteW`, target `{user}.verenu`) or macOS Keychain (`security-framework` generic-password items). `store.rs`'s `KEY_GROQ`/`KEY_OPENAI`/`KEY_GOOGLE` constants are only used as the credential's username/identifier; the secret value itself never touches settings.json or SQLite. Commands that check key presence return a boolean status only, never the key itself.

## Prompt Assembly (`api/prompts/`)

`get_system_prompt_with_extras()` builds the final system prompt by appending "FINAL OUTPUT OVERRIDES" at the end — this gives snippet instructions precedence over the base profile prompt when they conflict. User instructions are normalized to MUST/MUST NOT imperatives before insertion.

## Snippet System (`snippets.rs`)

Two execution paths:

1. **Pure expansion** — trigger found as a standalone token in text → replace directly with expansion (strips trailing punctuation added by the transcription model). If the *entire* transcription is a single trigger, this fast-paths past the cleanup LLM entirely.
2. **Instruction collection** — triggers found in text → gather their expansions as instructions → pass to `get_system_prompt_with_extras()` → cleanup LLM applies them contextually.

Cleanup instruction keywords are applied mechanically to LLM output *after* the API call: "all capitals" → uppercase; "no period" / "never add period" → strip trailing periods; "end with exclamation" → force `!`. Negated forms ("don't use all caps") are also checked to prevent conflicts.

## Auto-Learn System (`api/auto_learn.rs`)

After injection, monitors the focused text field for 60 seconds (`MONITOR_WINDOW_SECS`) using UI Automation on Windows or the AX API on macOS. Each dictation session spawns its own monitor; concurrent monitors are intentional and necessary so corrections from separate dictations can each count toward the promotion threshold. Detects user corrections via Levenshtein edit distance (`edit_distance`, `is_candidate_correction`) — pairs must differ by ≤2 chars or ≤50% of max length. A `StableTextGate` requires two consecutive identical reads before evaluating corrections, preventing mid-typing noise. Promotion is confidence-tiered: a (wrong → correct) pair whose accumulated `confidence_avg` reaches `FAST_PROMOTION_CONFIDENCE` (0.70 — reachable only by distinctive corrections, e.g. brand/technical terms with a small edit distance) promotes after a single session; all other pairs need 2 separate sessions within 2 days. Within a session, `recorded_this_session` deduplicates so one noisy edit can't count twice.

On Windows, requires COM initialization (`CoInitializeEx`) per thread via a `ComGuard` — UI Automation will fail silently without it. The macOS path uses the AX API instead and has no COM equivalent.

Auto-learned dictionary entries whose `mistake` is a plain, non-distinctive word (per `has_distinctive_features` in `system/text.rs`) are excluded from the mechanical find/replace in `apply_substitutions_from` — they're corrected contextually via the cleanup LLM prompt instead, so a mis-learned pair (e.g. "rock" → "Groq") can't clobber legitimate uses of the common word. Manual dictionary entries are never gated.

## Key Design Constraints

- **No bundled browser.** Tauri uses Windows WebView2 — keep this. Never switch to Electron.
- **RAM target: ~200MB idle.** Profile before adding any heavy JS dependency. See [docs/transcription-ram-reliability-plan.md](docs/transcription-ram-reliability-plan.md) for the prioritized list of known memory and reliability issues in the Rust pipeline.
- **Text injection is clipboard-based.** `SendInput` character-by-character is unreliable across apps; clipboard + Ctrl+V works everywhere.
- **API keys never touch the DB, logs, or settings.json.** They live only in the OS credential store (`data/credentials.rs`). Commands that check key presence return a boolean status, never the key itself.
- **MVP scope:** transcription + history + dictionary + snippets + cleanup + hotkey. Insights/stats and IDE integrations are post-MVP.

## Shared Frontend Components

Three reusable components in `src/lib/components/`:

- **Toggle** — `<Toggle checked={bool} onchange={(v) => ...} />`. Renders as `<div class="toggle" role="switch" aria-checked>`. Required by smoke tests — the `toggle` class must stay.
- **Dropdown** — `<Dropdown bind:open closeSelector="">`. Handles click-outside to close; `closeSelector` exempts an element from triggering close.
- **MicInputButton** — `<MicInputButton onResult={(text) => ...} />`. Drives a recording state machine (idle → recording → loading) via `start_input_recording` / `stop_and_transcribe_input` Tauri commands. Shows spinner while loading.

## Patterns & Gotchas

### Hotkey hook — callback timing
The `WH_KEYBOARD_LL` callback in `core/hotkey/win.rs` must return within ~300ms or Windows silently kills it. All actual work (pipeline, async calls) must happen in a spawned tokio task, not in the hook body. The hook only sends a `HotkeyEvent` enum over a channel.

### Pill window — never hide it
Hiding the pill window suspends the WebView2 renderer. The next state event emitted while it is hidden will be silently dropped, leaving the pill stuck. Keep it always-visible but click-through + transparent in idle state. Emit state events *after* showing the window, not before. The pill uses `SW_SHOWNOACTIVATE` so it appears without stealing focus from the target app. In idle/passive states the pill is click-through; only in "handsfree" state does it accept real cursor events for buttons.

### Recording quality gates
`run_pipeline()` rejects recordings below two thresholds before calling any API:
- `duration_ms < 700` — avoids Whisper hallucinations on short clips
- `rms < 0.008` — near-silence, likely accidental activation

Rejection shows an error message on the pill (`reject_with_pill()` in `pipeline/mod.rs`, distinguishing "Recording too short" vs "Audio too quiet — check your mic"); the in-app mic button (`transcribe_input_only()` / `MicInputButton.svelte`) shows the equivalent message inline. These thresholds are currently magic numbers in `pipeline/mod.rs`. Recordings are also capped at 15 minutes in `media/audio.rs`; truncated captures abort with a user-facing error instead of silently transcribing partial audio.

### Injection — contextual capitalization and timing
Capitalization decisions come from a layered `InjectionContextProbe` (`core/context_probe.rs` + `core/text_context.rs`), not a single trick. It tries, in order: a caret-local read via the platform accessibility API (UI Automation on Windows, AX on macOS, `ContextProbeSource::CaretLocal`) → a clipboard-sniff fallback (select one char back with Shift+Left/Cmd+Shift+Left, copy, inspect, then restore the selection — `ContextProbeSource::ClipboardSniff`) → a history-based guess (`HistoryFallback`) when the control is unsupported or permission is missing. The result classifies into `SentenceContext` (`NewSentence`/`MidSentence`/`Unknown`); if not `NewSentence`, the first letter of the injected text is lowercased (mid-sentence join). Timing constants in `core/injection/mod.rs`: `MODIFIER_GAP_MS` = 30ms between releasing modifier keys and sending Ctrl+V/Cmd+V (without this gap, some apps miss the Ctrl key in the same message-pump cycle), and ~30ms waits between the sniff's key-down/key-up stages for the clipboard to populate.

### HWND capture — foreground window before async work
The foreground window HWND is captured at the very start of `run_pipeline()`, before any async API call. This ensures Ctrl+V is sent to the correct window even if the user switches apps during the transcription/cleanup round-trip. The captured HWND is re-focused just before injection.

### Developer mode and verbose logs
Unlocking Developer mode from About no longer enables verbose logging on its own. The Developer panel now reads the backend logging flag and defaults to `off`; verbose logging must be enabled explicitly there, where the warning is visible. When adding new logs, prefer redacted metadata such as counts, ids, model names, and filenames. Do not log dictated text, raw snippet expansions, raw dictionary terms, full prompts, or full local paths.

Verbose logging is still subject to the privacy rule. Never add `*_full` logs for raw dictation, cleaned text, prompts, clipboard contents, app context text, dictionary values, snippet expansions, or frontend-supplied messages. If debugging needs correlation, log character counts, line counts, stable fingerprints, provider/model ids, and request ids instead.

### History loading
Recent transcription history is paginated from the backend. `get_recent()` accepts optional `limit` and `offset` arguments and defaults to `100, 0`; Home appends older pages via `query_recent_page()` instead of loading the entire table every time.


### Error handling convention
Use `anyhow::Result` throughout Rust. Pipeline errors call `show_error_pill()` which logs, emits `verenu:error` to the frontend (caught as a toast in `App.svelte`), and returns without crashing. Match this pattern for any new error path in the pipeline.

### Version sync — **CRITICAL: all three files must be updated together**
Version must be updated in exactly three files simultaneously, or the build will break:
1. `package.json` (JSON version field)
2. `src-tauri/tauri.conf.json` (Tauri app version)
3. `src-tauri/Cargo.toml` (Rust package version)

The frontend reads the version dynamically via `@tauri-apps/api/app` `getVersion()` — no hardcoded version strings in Svelte files. See [`Agent-Skills/Updating_version.md`](Agent-Skills/Updating_version.md) for the exact procedure.

### Release installers — tracked under `/installers`
Built release installers are committed to the repo under `installers/<version>/` (e.g. `installers/0.15.0/`) so anyone can download a release straight from the file tree, in addition to the GitHub Release. This intentionally overrides the old "ship only via Releases" rule — only the Rust build tree (`src-tauri/target/`) stays gitignored, not these.

Each version folder contains all four installers plus a `SHA256SUMS.txt`:
- `Verenu_<version>_x64-setup.exe` (Windows NSIS)
- `Verenu_<version>_x64_en-US.msi` (Windows MSI)
- `Verenu_<version>_Apple_Silicon.dmg` (macOS arm64)
- `Verenu_<version>_Intel.dmg` (macOS Intel)

When cutting a release: create `installers/<version>/`, copy the four installers in (Windows from a local `npm run tauri build`, macOS DMGs from the `build-installers.yml` CI run), regenerate `SHA256SUMS.txt` (`sha256sum *.exe *.msi *.dmg > SHA256SUMS.txt`), and commit alongside the version bump. The hashes must match the files attached to the GitHub Release and the VirusTotal entries in the release notes. See [`installers/README.md`](installers/README.md) for layout and verification.

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




