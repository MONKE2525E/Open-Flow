# CLAUDE.md

# Notes from user
- Github Repo: https://github.com/MONKE2525E/Verenu
- Use the Mono font very sparingly only use it when its in technical items like file names folder names, code, etc...
- docs/ROADMAP.md keeps recorded bugs and long term goals far future plans are not to be acted on unless the user requests so.
- Latest release is 0.17.0. In-progress (unreleased) work lives under "Unreleased" in docs/CHANGELOG.md — check it before assuming a feature's current state.
- Always add yourself as a co-author  in all commits you make e.g @Claude, @Codex, @google-antigravity, etc but dont add a note at the bottom of the PR description

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

Verenu is an open-source AI dictation desktop app for Windows and macOS — a free, API-key-based alternative to the paid "Wispr Flow" app. Users supply their own API keys; there is no subscription. Target RAM usage is ~200MB idle.

Core model: hold the hotkey, talk, release; Verenu transcribes (cloud or fully local), optionally cleans the text up with an LLM, and pastes it into the app that had focus. **Context groups** are the primary organizing feature: a named group ties together target apps/websites, a tone/cleanup-intensity override, custom instructions, and scoped dictionary/snippet items. Local transcription (Parakeet V3) and local LLM cleanup are first-class, with models/runtimes downloaded on demand.

## Stack

- **Framework:** Tauri 2.x (Rust backend + WebView2 frontend — not Electron, not a web app)
- **Frontend:** Svelte 5 + TypeScript + Tailwind CSS v4
- **Database:** SQLite via `rusqlite` (direct, not `tauri-plugin-sql`)
- **Settings store:** backend-owned JSON at Tauri `BaseDirectory::AppData/settings.json` via `src-tauri/src/data/store.rs` (non-secret settings only; see [Settings & Configuration](#settings--configuration) for where API keys actually live)
- **Audio capture:** `cpal` + `hound` for WAV encoding + `nnnoiseless` for noise reduction
- **Local transcription:** `transcribe-rs` (ONNX runtime, Silero VAD) running Parakeet V3 fully offline
- **Local cleanup:** managed local LLM server process (`src-tauri/src/local_llm/`) — binary runtime + models downloaded on demand, driven over a local HTTP endpoint
- **Windows native APIs:** `windows` crate (hotkey hook, active window, SendInput, UI Automation, Credential Manager)
- **macOS native APIs:** `core-graphics` (`CGEventTap` global hotkey), `security-framework` (Keychain), `accessibility-sys` (AX API), `objc2`/`objc2-foundation` (AppKit interop — `NSWorkspace`, `NSPasteboard`), `coreaudio-rs` (native mute control)
- **HTTP:** `reqwest` (async API calls to AI providers)
- **Async runtime:** `tokio`
- **Utilities:** `chrono` (timestamps), `anyhow` (error handling)

## Design System

Read [DESIGN.md](DESIGN.md) before creating or restyling frontend controls. It is the source of truth for shared button variants, dropdown anatomy, UI motion, and the complete control inventory. See [docs/colors.md](docs/colors.md) for the complete color palette, typography, and theming system.

Shared interactive styles live in `src/ui.css`; do not recreate `.btn-primary`, `.btn-ghost`, `.btn-danger`, or `.ui-dropdown-*` styles inside a Svelte component. Preserve feature-specific controls when they communicate a distinct state, such as navigation, recording, selection cards, or system permissions.

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
python tests/OnePyFone.py --test accessibility.settings-focus  # stable test ID/name filter

# Available profiles: fast | live | native | full
# Available suites: preflight | unit | frontend | rust | contract | ui | accessibility | state | performance | animation | pipeline | native
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

- `master` is the only shared integration and release branch. Open pull requests directly into `master`; do not use an intermediate `dev` branch or merge `dev` into `master`
- GitButler workspaces should use `origin/master` as their target; verify with `but config target` before creating a pull request
- **PR checks workflow** (`.github/workflows/pr-checks.yml`) runs for pull requests targeting `master`, keeps frontend and OS-matrix Rust jobs, then runs the unified fast profile with JSON/JUnit reports
- **Morning nightly release workflow** (`.github/workflows/morning-release.yml`) runs on schedule or manual dispatch, compares the current `master` snapshot with the latest reachable nightly baseline, and publishes prerelease installers from `master`
- **Extended profiles workflow** (`.github/workflows/extended-test-profiles.yml`) runs opt-in live/native profiles on schedule or manual dispatch
- **Build installers workflow** (`.github/workflows/build-installers.yml`) is manual (`workflow_dispatch`) — builds release installers on demand, not on every push
- **Dependency review** (`.github/workflows/dependency-review.yml`) gates new dependencies for supply-chain risk
- **Copilot review instructions** (`.github/copilot-instructions.md`) contain guidance on Rust Windows integration, frontend patterns, and smoke-test contracts — read these before major changes

## Project Structure

```
src/                        # Svelte 5 frontend
  App.svelte                # Root: routing, accent theme injection, event listeners
  PillApp.svelte            # Floating pill window (recording/handsfree/repair states)
  main.ts / pill-main.ts    # Vite entry points for each window
  lib/
    stores.svelte.ts        # Svelte 5 runes-based app stores
    settings.ts             # Typed settings registry: SettingsValueMap, saveSetting() helper, shared types (ProviderId, AppearanceMode, etc.)
    settingsSections.ts     # Settings section ids/order + visibility (macOnly/devOnly/legacyOnly)
    transcriptionLanguages.ts  # ISO 639-1 language list + TranscriptionLanguageCode type (frontend mirror of store.rs validation)
    calibration.ts          # Mic gain auto-calibration state machine (loud/whisper phases)
    platform.ts             # Runtime platform detection (Windows vs macOS)
    motion.ts               # Animation/transition utilities
    modalFocus.ts           # Reusable modal focus trap + focus restoration helper
    errors.ts               # classifyIpcError + shared IPC error handling
    serviceStatus.ts        # Polling client for api.verenu.com provider status/health
    tauri.ts                # Typed wrapper around @tauri-apps/api invoke/listen (large; split candidate)
    icons.ts                # SVG icon definitions
    components/layout/      # TitleBar, Sidebar (incl. Contexts sidebar section), DictationPill
    components/             # Shared: Toggle, Dropdown, MicInputButton
    components/settings/    # Settings sections: General, Models, ApiKeys, AppMappings (legacy), Privacy, Audio, Advanced, About
    views/                  # Home (main flow + paginated history), Contexts (primary), Insights, Settings, Setup, Style pages
    views/dictionary|home|insights|snippets/    # per-view subcomponents + helpers.ts (Contexts.svelte has no subfolder yet — known monolith)
    setup/                  # First-run wizard: SetupShell.svelte + steps/
    calibrationCopy.ts       # Localized copy strings for the mic-calibration UI (companion to calibration.ts)
src-tauri/
  src/
    main.rs                 # Module wiring, Tauri builder, command registration
    app_setup.rs            # Startup glue: readiness watchdog, data dirs, relaunch helpers
    app_hotkey.rs           # Cross-platform hotkey registration glue
    app_tray.rs             # Tray icon
    pipeline/
      mod.rs                # run_pipeline() orchestration, quality gates, pill window creation/resize
      stages_style.rs       # Style resolution: App Mappings, Context overrides, tone priority
      stages_transcription.rs  # Audio capture handoff, quality gates, transcription paths
      stages_cleanup.rs     # Cleanup guards, local cleanup, cache, provider chains, orchestration
      state.rs              # Shared pipeline state machine (recording sessions, exclusive mic)
      session.rs            # Recording session lifecycle
      chains.rs             # Model chain validation (primary + fallbacks)
      cache.rs              # In-pipeline cleanup-cache helpers (table lives in data/db/cleanup_cache.rs)
      gates.rs              # Quality gates (duration/RMS/hallucination filters)
      finalize.rs           # Final persistence, injection, frontend event emission
      repair.rs             # Approval-gated post-dictation repair (session, model calls, mutation)
      repair_proposal.rs    # Repair proposal types + deterministic validation (pure, no I/O)
      pill.rs / pill_animation.rs / pill_position.rs  # Floating pill window, animations, multi-monitor placement
      clipboard_phrase.rs   # Clipboard phrase handling
      fixture.rs            # pipeline test/debug fixtures
      tests.rs              # pipeline-focused tests
    commands/               # All #[tauri::command] handlers, split by domain:
      mod.rs recording.rs settings.rs contexts.rs history.rs library.rs
      local_llm.rs local_stt.rs permissions.rs service_status.rs system.rs updater.rs
    local_llm/              # Local cleanup runtime: managed server process, model/runtime downloads
    local_stt/              # Local transcription: Parakeet V3 engine, model downloads
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
        focused_text.rs monitor.rs rejection.rs
      prompts/
        mod.rs              # Prompt assembly entrypoints
        transcription.rs    # Transcription-facing prompt text
        cleanup_rules.rs    # Cleanup rule assembly
        cleanup_templates.rs # Provider/model prompt templates
        gemini.rs           # Gemini-specific generation config
        tests.rs            # Prompt tests
      updater.rs            # Auto-update logic
      service_status.rs     # Client for api.verenu.com (provider status + health), filtered to selected providers
    core/
      hotkey/
        mod.rs               # Platform-dispatch + shared chord/handsfree state
        win.rs                # Windows: SetWindowsHookExW(WH_KEYBOARD_LL) hold/release hook + ChordStateMachine
        mac.rs                # macOS: Carbon RegisterEventHotKey via `global-hotkey` crate
      injection/
        mod.rs              # Shared clipboard-based text injection + platform dispatch
        windows.rs          # Windows paste/clipboard path
        macos.rs            # macOS paste/clipboard path
      context.rs            # Foreground executable+domain → Context group resolution
      context_probe.rs      # Layered InjectionContextProbe: caret-local read (UIA/AX) → clipboard-sniff → history fallback
      context_probe_macos.rs # macOS-only half of context_probe (AX-based caret read)
      text_context.rs       # SentenceContext / InjectionPrefixClass classification used by context_probe + injection
      window_context.rs     # Foreground window → process name (GetForegroundWindow / NSWorkspace)
      browser_probe.rs      # Best-effort browser address-bar domain read (no extension; never blocks the pipeline)
    data/
      db/
        mod.rs              # SQLite shared DB entrypoints (tests live in tests.rs)
        schema.rs           # Schema (inline) + versioned migrations with pre-migration backup
        transcriptions.rs   # History queries, insert/delete, pagination
        contexts.rs         # Context groups, exe/website targets, dictionary/snippet scoping
        dictionary.rs snippets.rs insights.rs cleanup_cache.rs validation.rs
      store/                # mod.rs (handle + key constants), config.rs (pipeline/audio config), tests.rs
                            # backend-owned JSON settings store (NOT api keys; see credentials.rs)
      credentials.rs        # API key storage: Windows Credential Manager (CredWriteW/CredReadW) / macOS Keychain (security-framework). Never settings.json, never SQLite.
      dictionary.rs         # Dictionary substitution logic
      snippets.rs           # Snippet expansion (pure and instruction-based paths)
    media/
      audio.rs              # CPAL mic capture → WAV, RMS level streaming
      sound.rs              # Playback/mute coordination (coordinated_unmute)
    system/
      apps.rs               # Registry scan for installed apps + process dedup, AppMapping type
      mac_app.rs             # macOS-only: NSWorkspace app lookup, NSPasteboard helpers
      macos_ax_text_marker.m # Objective-C shim for AX text-marker APIs (compiled via cc, see build-dependencies)
      media_control.rs      # DictationMediaPauseGuard (pauses media during dictation)
      logger.rs               # Logging utilities
      memory.rs              # Memory monitoring
      number_parser.rs        # Spoken-number → digit parsing
      text.rs               # Text utilities
      volume.rs             # Volume detection
  Cargo.toml
  tauri.conf.json           # Declares only the "main" window (1100×720). The pill window is NOT statically configured.
tests/
  smoke/                    # Playwright smoke tests — NEVER edit these files
  integration/              # Feature Playwright tests (settings, onboarding, history, offline…) — editable
  manual/                   # Manual test scripts (hotkey, layout bounds) — not automated
  OnePyFone.py              # Unified stdlib-only test runner (profiles: fast|live|native|full)
```

The pill window is created at runtime by `create_pill_if_needed()` in `pipeline/mod.rs` (`WebviewWindowBuilder`, always-on-top, transparent, decorations off, initial size 380x44) — it is resized per recording state rather than recreated, so don't look for its dimensions in `tauri.conf.json`.

## Core Data Flow

```
[Ctrl+Windows held]
  → audio.rs: CPAL captures mic PCM, applies gain
  → Emits 'audio-level' every 50ms (RMS) → pill visualizer bars

[Ctrl+Windows released]
  → audio.rs: encode PCM → WAV in memory
  → pipeline/mod.rs run_pipeline():
    1. Quality gates: reject if duration too short or RMS below the gain-aware floor
    2. Capture foreground HWND (before any async work — foreground may change mid-pipeline)
    3. If the foreground process is a known browser, browser_probe.rs reads the
       active tab's address-bar domain (best-effort, never blocks)
    4. context::resolve_context(): foreground exe (+ domain) → matching Context
       group, or the built-in "Everywhere" fallback
    5. Style overrides: Context tone/intensity → App Mapping → global default_tone
    6. transcription.rs (or local_stt Parakeet) → raw_text; dual-model mode runs
       primary + first fallback concurrently and reconciles both candidates
    7. Cleanup-cache lookup (key includes model, prompt inputs, and ctx:<id>) —
       hit skips the LLM call entirely
    8. Pure-snippet fast-path: if entire transcription is a single trigger → expand
       directly, skip cleanup
    9. Otherwise: snippets.rs collect context/instruction-based triggers →
       api/prompts/mod.rs: assemble system prompt
    10. cleanup.rs (or local_llm) → clean_text; refusal/artifact/fabrication guards
        may retry once, then fall back to pre-cleanup text
    11. snippets.rs expand_snippets() (remaining pure-token expansions in clean_text)
    12. dictionary.rs apply_substitutions() (context-scoped entries; applied last)
    13. data/db/transcriptions.rs INSERT transcription record (with context_id)
    14. core/injection/mod.rs: re-focus captured HWND → save clipboard →
        contextual-cap check → Ctrl+V → restore clipboard
    15. Emit 'verenu:transcribed' to frontend
    16. auto_learn.rs: monitor focused text for 60s (MONITOR_WINDOW_SECS) via UI
        Automation/AX for corrections
```

## SQLite Schema

Schema is defined inline in `src-tauri/src/data/db/schema.rs` with versioned migrations (`PRAGMA user_version`, `execute_batch` wrapped in explicit `BEGIN/COMMIT/ROLLBACK`, pre-migration backup at the v2 boundary). Tables, grouped:

```sql
-- Core content
transcriptions       (id, raw_text, clean_text, app_name, profile, api_used, words,
                      duration_ms, context_id, created_at)
dictionary           (id, wrong, correct, auto_learned, correction_count, created_at)
snippets             (id, trigger, expansion, use_count, created_at)

-- Context groups (primary organizer; see "Contexts & Style Resolution")
contexts             (id, name, is_everywhere, icon, tone, cleanup_intensity, color,
                      custom_instructions, pinned_at, …)
context_targets      (context_id → contexts, executable)           -- exe/bundle-id matches
context_website_targets (context_id → contexts, domain)            -- domain matches
dictionary_contexts  (context_id, dictionary_id)                    -- vocab scoped to a group
snippet_contexts     (context_id, snippet_id)                       -- snippets scoped to a group

-- Learning / repair
pending_corrections  auto_learn_events  auto_learn_candidates

-- Ops / analytics
cleanup_cache        (dedup key incl. ctx:<id>, hit_count, expires_at, is_snippet)
lifetime_stats  seeded_defaults  api_calls
```

WAL mode is enabled. A failed migration rolls back fully rather than leaving a partial schema, and `open()` self-heals certain legacy shapes (e.g. missing cleanup-cache epoch columns — covered by tests). API keys are never stored in SQLite — they live in the OS credential store (`data/credentials.rs`).

## API Providers

| Provider | Transcription | Cleanup |
|---|---|---|
| Groq | `whisper-large-v3-turbo` | `llama-3.3-70b-versatile` |
| OpenAI | `gpt-4o-transcribe` | `gpt-4o-mini` |
| Google | `gemini-3.5-transcribe` | `gemini-3.5-flash-lite` |
| Local | Parakeet V3 (`transcribe-rs`, ONNX + Silero VAD) | managed local LLM server (model-dependent) |

Groq is the recommended cloud default — free tier, fast LPU inference. Google sends audio as base64 in the request body; Groq and OpenAI use multipart form upload. The cleanup request wraps transcription text in `<raw_dictation>` XML tags. Google Gemini 3.x cleanup sends `thinkingLevel: "minimal"`; Gemini 2.5 Flash/Flash-Lite use `thinkingBudget: 0`.

Local models are first-class, not a side path: `local_stt/` runs Parakeet V3 fully offline; `local_llm/` spawns a managed server process (binary runtime downloaded on demand) and talks to it over a local HTTP endpoint. "Fully local" means local transcription paired with local (or no) cleanup. Model/runtime downloads, cancellation, and deletion are all exposed as Tauri commands (`commands/local_stt.rs`, `commands/local_llm.rs`).

The `transcription_language` setting (ISO 639-1, default `en`) is sent to Groq/OpenAI as the `language` form field and to Gemini as a natural-language label via `transcription_language_label()` in `store.rs`. Supported languages: `src/lib/transcriptionLanguages.ts` (frontend), `is_supported_transcription_language()` (Rust).

**API fallback:** Retryable errors (timeouts, 429, 5xx) trigger automatic fallback to any configured fallback models. Fallback models are configured per task (transcription/cleanup) in the Models settings tab and stored as `transcription_fallback_models` / `cleanup_fallback_models` arrays. Fallback is implicit — if fallback models are configured, they are always tried in order. Quota errors (`QUOTA_EXCEEDED:` prefix string — use `quota_bail()` helper) fail immediately with no fallback. Non-retryable errors also fail immediately. `is_retryable_provider_error()` in `api/mod.rs` is the single source of truth for what counts as retryable (reqwest timeout/connect errors, HTTP 408/429/5xx, and a few message-substring heuristics).

**401 handling:** `api/mod.rs` classifies unauthorized responses into `AuthErrorCategory` (`InvalidOrRevokedKey`, `ScopeOrAccountRestriction`, `UnknownUnauthorized`) via `classify_unauthorized_body()`, and encodes the result as a structured `AUTH_401|provider=...|category=...` error string (`auth_401_error()` / `parse_auth_401_error()`) so the frontend can show a category-specific message instead of a generic failure.

## Verenu Status API (`api/service_status.rs`)

The frontend polls `api.verenu.com/v1/provider-status` every 5 minutes (`src/lib/serviceStatus.ts`) and shows an in-app banner (`ProviderStatusBanner.svelte`, replacing the update-available banner when both are pending) only when a provider the user has actually selected for transcription or cleanup is flagged with a real issue. "Real issue" means the backend's `showToUsers` flag is true AND `status` is neither `operational` nor `unknown` (`unknown` means the provider doesn't publish a machine-readable feed, not that something is broken) — `filter_alerts()` in `service_status.rs` is the single source of truth for that gate. `api.verenu.com/v1/health` is polled every 20 minutes into `appStore.apiHealthy` with no UI yet, for future use. Settings → Developer has a "Run Check" button (`check_provider_status_raw`) that shows the unfiltered API response for debugging.

About has an opt-in "Beta updates" toggle backed by `beta_updates_enabled`. Enabling it requires a warning confirmation and switches update checks to published prereleases/nightlies from `master`; stable checks also use `master` but ignore prereleases. The updater ignores drafts and selects the highest numeric version on the selected channel.

If a transcription or cleanup call fails with a provider-side error (`is_retryable_provider_error()` — quota, or a retryable timeout/429/5xx), the pipeline emits `verenu:recheck-provider-status` so the frontend re-polls immediately instead of waiting for the next scheduled check. These are the app's only calls to a Verenu-owned server; they are plain GETs that never include dictated audio, text, keys, or history (see [docs/DATA_AND_PRIVACY.md](docs/DATA_AND_PRIVACY.md)).

## Global Hotkey Behavior

- **Windows:** Ctrl+Windows (hold) → start recording; release → stop and process. Implemented with a raw `SetWindowsHookExW(WH_KEYBOARD_LL)` hook; both chord keys and the repair hotkey are user-configurable.
- **macOS:** Carbon `RegisterEventHotKey` via the `global-hotkey` crate (app_hotkey.rs / core/hotkey/mac.rs). This needs **no** Input Monitoring or Accessibility permission, but Carbon hotkeys require a real key — a pure modifier chord (the old CGEventTap Fn+Control) is no longer possible on macOS.

Beyond hold-to-dictate, the hook layer handles:
- **Handsfree mode** — double-tap the chord (or press Space while holding it) toggles a discrete recording session; Escape cancels.
- **Repair hotkey** (default Ctrl+Alt+Z) — opens the approval-gated repair complaint box.
- **Copy-last shortcut** (default Ctrl+Alt+C) — re-copies the last dictation to the clipboard.
- Availability probes (`is_hotkey_available`) register/unregister the candidate combo so Settings can warn about conflicts.

`core/hotkey/` is split by platform: `win.rs` owns the low-level hook plus the `ChordStateMachine` (unit-tested: autorepeat suppression, double-tap windows, gesture resets that preserve key ownership), `mac.rs` owns the Carbon path, and `mod.rs` holds platform dispatch plus shared chord/handsfree state. `tauri-plugin-global-shortcut` is a declared dependency but is not used for the hold/release hotkey — hold/release state requires the low-level hook on Windows.

## Contexts & Style Resolution

Context groups are the primary organizer (0.17.0). A context ties together:
- **Targets** — executables (`context_targets`, matched case-insensitively by `.exe` name on Windows / bundle id on macOS) and website domains (`context_website_targets`, matched against the browser address-bar domain read by `core/browser_probe.rs`). A target belongs to at most one context; assigning it elsewhere moves it. Website domains are DNS-checked before being accepted.
- **Style** — optional `tone` and `cleanup_intensity` overrides and `custom_instructions` appended to the cleanup prompt.
- **Content** — dictionary and snippet items scoped to the group via the `dictionary_contexts` / `snippet_contexts` junction tables.

Every install has a built-in **Everywhere** context (id 1, `EVERYWHERE_CONTEXT_ID`) — the fallback when no target matches; it can be renamed/restyled but never deleted. User contexts are capped (`MAX_USER_CONTEXTS` = 200), names are limited to 30 chars.

**Resolution order** (`core/context.rs::resolve_context` → `stages_style.rs::apply_app_style_overrides`): foreground exe + browser domain → matching context, else Everywhere. Effective style is then `Context tone/intensity → App Mapping profile/intensity → global default_tone`. The context override wins because it can match per-website, which exe-keyed App Mappings cannot.

**Legacy mode:** App Mappings, Dictionary, and Snippets still exist as standalone pages but are hidden by default. `Settings → General → Legacy pages` (`legacyFeaturesEnabled`) swaps them back in and hides Contexts from the nav — the two surfaces are mutually exclusive by design. New UI work should target Contexts; legacy pages are maintenance-only.

**Pipeline touchpoints:** the resolved `context_id` is stored on every transcription row, scopes cleanup-cache keys (`|ctx:<id>`), scopes repair dictionary writes, and feeds `apply_app_style_overrides`. The recording pill pre-resolves the profile (exe-only, fast path) at recording start and re-resolves with the domain at processing time so the shown style always matches what runs.

Built-in tone profiles: `casual`, `formal`, `very_casual`. Profile system prompts live in `src-tauri/src/api/cleanup.rs`.

**Insights** (primary nav view) is read-only analytics over the local DB: usage charts, streaks, cost breakdown, context stats (`data/db/insights.rs`, `commands` `get_insights`, `src/lib/views/insights/`).

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
- **Contexts are the primary surface.** Dictionary, Snippets, and App Mappings survive as legacy pages (hidden by default; mutually exclusive with Contexts via the Legacy pages toggle). Don't build new features on the legacy pages.
- **Scope:** transcription + history + contexts (tone/vocabulary/snippets per group) + cleanup + hotkey + local models + insights. IDE integrations remain post-MVP.

## Shared Frontend Components

Three reusable components in `src/lib/components/`:

- **Toggle** — `<Toggle checked={bool} onchange={(v) => ...} />`. Renders as `<div class="toggle" role="switch" aria-checked>`. Required by smoke tests — the `toggle` class must stay.
- **Dropdown** — `<Dropdown bind:open closeSelector="">`. Handles click-outside to close; `closeSelector` exempts an element from triggering close.
- **MicInputButton** — `<MicInputButton onResult={(text) => ...} />`. Drives a recording state machine (idle → recording → loading) via `start_input_recording` / `stop_and_transcribe_input` Tauri commands. Shows spinner while loading.

Store conventions: `stores.svelte.ts` (runes) is the live store module. `stores.ts` is an empty legacy shim kept only so old imports don't break — do not add anything to it. Settings-section visibility (mac/dev/legacy gating) is centralized in `settingsSections.ts`; IPC errors go through `classifyIpcError()` in `errors.ts`.

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
| Settings container | `settings-page` |
| Settings section buttons | `settings-nav-item` |
| Leave-settings control | `settings-back` |
| About version footer | `settings-foot` (**only** rendered on the About section) |
| Privacy toggles | `toggle` with `role="switch"` and `aria-checked` |
| Info badges | `badge` on a `<div>` |
| Hotkey badge | `badge key-badge` on a `<kbd>` (contains "Ctrl") |
| About GitHub button | `btn-ghost` on a `<button>` |
| Model task tiles | `task-tile` (exactly 2: Transcription, Clean-up) |
| Model picker triggers | `tile-btn-primary` (Change) / `add-fallback` |
| Model picker dialog | `picker-card` (modal, portalled to `<body>`) |
| Model picker rows | `row-main`; setup CTAs carry `row-state-cta` |
| Fallback chain chips | `fallback-chip-item` |
| Advanced gain display | `span.gain-value` |

Since 0.15.0 settings is a full-screen page beside the sidebar, not a modal:
`.settings-page` is a plain content surface, so don't give it card styling
(elevated background, border, radius). It was called `.settings-modal` until
that release — expect the old name in older branches and issues.

The smoke files were amended once, with explicit owner sign-off, when settings
became full-screen: the version footer moved to the About section only, routine
open/close now uses `.settings-back` instead of a click at viewport (10, 10),
and `.settings-modal` was renamed. Treat them as frozen again — that was a
one-off, not a precedent.
Behaviour specific to the full-screen settings page (rail morph, travelling
highlight, page transition, footer placement) is covered separately in
`tests/integration/playwright-test-fullscreen-settings-dev.cjs`, which is *not*
frozen and is the right place to add new settings coverage.




