# Roadmap: Transcription Utility

## In Progress - 0.11.0

## 1. Models and Settings Redesign
- **Goal**: Finish the model picker cleanup so provider selection, advanced mode, and key validation feel predictable instead of fragile.
- **Implementation Plan**:
    - Keep simple mode and advanced mode in sync so changing defaults, fallbacks, and provider keys never leaves the UI in a half-valid state.
    - Make active transcription and cleanup chains easier to inspect before the user starts dictating.
    - Keep provider key validation and model selection persistence consistent across reloads and provider switches.
- **Relevant Files**: `src/lib/components/settings/ModelsSection.svelte`, `src/lib/components/settings/ApiKeysSection.svelte`, `src-tauri/src/data/store.rs`, `src-tauri/src/commands/mod.rs`.

## 2. Groq API Key Auth Regression (401/403 After Time)
- **Goal**: Fix the regression where Groq keys can work after save, then fail with `401` or `403` after roughly an hour until re-entered.
- **Implementation Plan**:
    - Reproduce with a soak test: save key once, run repeated transcription and cleanup calls for 2+ hours, and capture first failure timestamp and exact status code.
    - Compare 0.10.0 key path versus 0.11.x Windows Credential Manager path, including save, read, normalization, and request header generation.
    - Add temporary diagnostics that log sanitized key fingerprint continuity (`save -> read -> request`) for Groq and compare against OpenAI and Google behavior in the same session.
    - Capture provider response metadata (request ID, status, classified auth category, model name) so `invalid key` and `access denied` failures are separated.
    - Verify fallback and routing behavior on auth errors so a hidden provider or model switch is not masking the real source of failure.
- **Relevant Files**: `src-tauri/src/data/credentials.rs`, `src-tauri/src/api/transcription.rs`, `src-tauri/src/api/cleanup.rs`, `src-tauri/src/api/mod.rs`, `src-tauri/src/pipeline.rs`, `src-tauri/src/main.rs`.

## 3. Contextual Capitalization Reliability Regression
- **Goal**: Replace the brittle history-only contextual capitalization path with a deterministic context engine that reliably starts new chat messages with proper casing and preserves punctuation across Gemini, browser chat inputs, normal text boxes, and contenteditable editors.
- **Current Failure Pattern**:
    - The existing implementation mostly trusts `LAST_INJECTION`, an internal tail of text injected by Open Flow.
    - Mouse-click send buttons, browser chat inputs clearing themselves after submit, DOM rewrites, and some Enter/send paths can leave that tail stale.
    - Once stale, the next dictation can be treated as a mid-sentence continuation and the first letter is lowercased even though the target field is empty.
    - Punctuation loss must be debugged separately from casing. Some missing punctuation is likely cleanup/transcription output, not paste-time capitalization.
- **Design Principle**:
    - Never lowercase the first letter unless Open Flow has high-confidence evidence that the cursor is still inside the same editable control and immediately follows a non-sentence-ending character.
    - Unknown context must fail closed: preserve the cleanup output casing, or capitalize only through a deterministic sentence-start rule. It must not randomly force lowercase.
    - The old injection tail can remain as a last-resort cache, but it must not be the primary source of truth.
- **Implementation Plan**:
    - **Phase 0 - Instrument the bug without leaking text**:
        - Add verbose diagnostics around injection decisions, but never log dictated text, clipboard contents, user names, emails, prompt contents, or full field contents.
        - Log only metadata: target HWND, process name, focused element identity hash, context source (`uia`, `selection_probe`, `history_tail`, `unknown`), previous-character class (`empty`, `sentence_end`, `word_char`, `space`, `separator`), capitalization decision, punctuation status, and whether the field looked empty.
        - Add a debug-only counter for stale-history invalidations so repeated Gemini failures can be measured instead of guessed.
    - **Phase 1 - Extract pure decision logic**:
        - Move capitalization and auto-spacing decisions out of `inject_text()` into a pure module, likely `src-tauri/src/core/text_context.rs`.
        - Define a small data contract such as `CursorContext { source, confidence, previous_char, field_empty, same_edit_control, boundary_generation }`.
        - Define output as `InjectionDecision { casing_action, spacing_action, reason }`.
        - Unit-test the pure rules first: empty field, after `.`, `?`, `!`, newline, trailing spaces after sentence end, comma, slash, plain word char, selected text unknown, stale context, and provider output that already starts uppercase.
    - **Phase 2 - Build a real pre-paste context probe**:
        - Before modifying casing, try to read the focused editable control in the captured target window.
        - Prefer Windows UI Automation because this repo already uses UIA in `src-tauri/src/api/auto_learn.rs`.
        - Capture enough identity to know whether this is the same edit control as the last injection: HWND, process, focused element runtime ID or a stable fallback hash, and control type.
        - If UIA can read selection/caret-adjacent text, use it as the primary context source.
        - If UIA can only tell that the field is empty, treat that as a new-message boundary and do not lowercase.
        - If UIA is unavailable or opaque, fall back to a guarded selection probe: save clipboard, select one character before the caret, copy, restore selection/clipboard, and time out quickly. This must fail closed if anything looks unsafe.
        - Use `LAST_INJECTION` only when the probe is unavailable and the same edit-control identity is still valid.
    - **Phase 3 - Detect message send boundaries**:
        - Treat "message sent away" as a boundary event, not as ordinary text.
        - In the low-level keyboard hook, classify Enter separately from printable newlines. Plain Enter in browser/chat-like controls should invalidate the injection tail as a submit boundary. Shift+Enter can remain a line-break boundary.
        - Add low-level mouse invalidation or focused-control invalidation for send-button clicks. A click outside the tracked edit control should clear the old tail unless the next pre-paste probe proves otherwise.
        - Track a `boundary_generation` number that increments on submit-like Enter, Escape, Tab, focus/control changes, mouse clicks outside the edit control, and destructive shortcuts.
        - Store the generation with `LAST_INJECTION`; reject history when the generation has changed.
        - After paste, optionally run a short UIA post-injection watch for chat apps: if the input becomes empty or the focused control changes within a small window after Enter/click, mark the next dictation as a fresh message.
    - **Phase 4 - Separate punctuation reliability from casing**:
        - Add a punctuation audit before injection: output ends with terminal punctuation, output ends with separator, output has no punctuation, output is code/list/URL-like, or snippet/prompt explicitly requested no punctuation.
        - Tighten cleanup prompt contracts for Gemini and other cleanup models so normal sentences get sentence punctuation unless the user dictated otherwise.
        - Add a deterministic terminal-punctuation finalizer only for safe cases: natural-language sentence, cleanup enabled, not `very_casual`, not code-like, not a pure snippet, not an explicit "no punctuation" instruction.
        - Do not hide punctuation issues inside the capitalization engine. If cleanup returns punctuation-free text, the logs and tests should identify it as cleanup/punctuation, not contextual caps.
    - **Phase 5 - Replace the injection flow**:
        - In `src-tauri/src/core/injection.rs`, call the new context probe and decision module before building `adjusted`.
        - Preserve the existing clipboard paste path, refocus timing, modifier gap, and clipboard restoration because those are hard-won Windows reliability pieces.
        - Stop lowercasing from stale history. History can suggest mid-sentence only when it is fresh, same HWND, same edit control, same boundary generation, and no stronger probe exists.
        - Keep auto-spacing on the same decision path so spacing and capitalization cannot disagree about whether the cursor is mid-sentence.
    - **Phase 6 - Regression test matrix**:
        - Add Rust unit tests for pure context decisions and punctuation finalization.
        - Add Windows-focused integration/manual tests covering Notepad, normal `<textarea>`, contenteditable, Gemini/ChatGPT-style input clearing, Enter submit, Shift+Enter newline, click send, Backspace, cursor movement, and app switching.
        - Add a Playwright fixture page for browser inputs with textarea, contenteditable, fake chat send button, and DOM-clearing submit behavior. Use it to reproduce the "sent away but next dictation starts lowercase" path.
        - Keep the test fixture free of real user content and API keys.
    - **Acceptance Criteria**:
        - Dictating into an empty Gemini message box after sending a previous message starts with uppercase when the cleanup output is sentence-like.
        - Dictating after `.`, `?`, `!`, or newline starts a new sentence.
        - Dictating after a comma, slash, or ordinary word character lowercases only when the same edit control and caret context are confirmed.
        - Clicking a send button, pressing plain Enter to submit, switching windows, or losing focused-control identity cannot cause stale lowercase.
        - Punctuation failures are reproducible in tests or logs as cleanup/punctuation issues, not misattributed to capitalization.
        - No diagnostic path logs private dictated text, clipboard text, names, emails, API keys, or full field contents.
- **Relevant Files**: `src-tauri/src/core/injection.rs`, `src-tauri/src/core/hotkey.rs`, `src-tauri/src/api/auto_learn.rs`, `src-tauri/src/api/prompts.rs`, `src-tauri/src/api/cleanup.rs`, `src-tauri/src/pipeline.rs`, `src/lib/components/settings/GeneralSection.svelte`, `tests/manual/`, `tests/OnePyFone.py`.

## 4. Model-Specific Prompt Contracts and Context Retention
- **Goal**: Replace generic cleanup prompting with model-specific contracts that are token-efficient while preserving essential context, especially on `light` mode.
- **Implementation Plan**:
    - Create provider/model-specific cleanup prompt templates instead of one generalized instruction path for all models.
    - Define explicit edit budgets per intensity (`none`, `light`, `medium`, `high`) and enforce "must keep" constraints for factual clauses, entities, and user intent.
    - Add regression fixtures where losing a single clause changes meaning, and compare outputs against known-good 0.10.0 medium-mode behavior.
    - Audit snippet overrides, dictionary substitutions, and post-cleanup transforms so context is not dropped after the model already returned a good output.
    - Add prompt-size and token-usage observability to validate efficiency improvements without over-compressing user content.
- **Relevant Files**: `src-tauri/src/api/prompts.rs`, `src-tauri/src/api/cleanup.rs`, `src-tauri/src/pipeline.rs`, `src-tauri/src/data/snippets.rs`, `src-tauri/src/data/dictionary.rs`, `src/lib/components/settings/ModelsSection.svelte`.

## 5. Fallback Chain and Model Persistence Hardening
- **Goal**: Make transcription fallback, cleanup fallback, and model persistence behave consistently under real failure modes.
- **Implementation Plan**:
    - Trace transcription fallback end-to-end and align retry rules with cleanup fallback for `429`, timeout, and `5xx` scenarios.
    - Validate `401` and `403` handling so non-retryable auth failures stop cleanly and retryable failures advance to the next configured model.
    - Ensure provider-prefixed IDs, slash-containing model names, and custom entries round-trip correctly between frontend and backend settings.
    - Add restart persistence checks for default models and fallback chains after provider switches and advanced-mode edits.
- **Relevant Files**: `src-tauri/src/pipeline.rs`, `src-tauri/src/api/transcription.rs`, `src-tauri/src/api/mod.rs`, `src-tauri/src/data/store.rs`, `src/lib/components/settings/ModelsSection.svelte`, `src-tauri/src/commands/mod.rs`.

## 6. Pre-Release Stabilization Gate
- **Goal**: Hold release until Groq auth reliability, contextual capitalization, light/medium cleanup quality, and fallback behavior match or beat 0.10.0 baseline.
- **Implementation Plan**:
    - Run direct A/B checks on 0.10.0 versus 0.11.x for Groq auth duration, capitalization behavior, cleanup preservation, and fallback reliability.
    - Add targeted smoke and manual checks for the failure paths reported in daily dictation use.
    - Keep release blocked until the core dictation loop is measurably better, not just feature-complete.
- **Relevant Files**: `tests/OnePyFone.py`, `tests/smoke/`, `tests/manual/`, `src-tauri/src/pipeline.rs`, `src-tauri/src/api/transcription.rs`, `src-tauri/src/api/cleanup.rs`, `src-tauri/src/core/injection.rs`.

---

## macOS Production Bugs (Production-only — works in `npm run tauri dev`)

These bugs only surface in the fully installed `.app` bundle. All three are suspected to stem from differences in how macOS handles permissions, bundle identity, and audio device access for a signed/notarized installed app versus a dev-mode process.

---

### macOS-1: Multiple "Open Flow" Entries in Launchpad / Spotlight ✓ Fixed

**Symptom**: Three (or more) separate "Open Flow" icons appear in Launchpad / App Store search after installing, when only one should be present.

**Suspected Cause**: macOS LaunchServices (the daemon that powers Launchpad and Spotlight) scans and indexes every `.app` bundle it can find across the filesystem — not just `/Applications`. During development, Tauri produces app bundles in multiple locations:
- `src-tauri/target/release/bundle/macos/Open Flow.app`
- `src-tauri/target/x86_64-apple-darwin/release/bundle/macos/Open Flow.app` (Intel cross-compile)
- Any `.dmg` volumes previously mounted

LaunchServices caches these and keeps them visible even after the bundles are moved or deleted, until the database is explicitly refreshed. The `tauri-plugin-single-instance` plugin (registered in `main.rs:136`) correctly prevents two *processes* from running simultaneously, but it does nothing about the visual duplication in the Launchpad/Spotlight index.

**Note**: This may partially be a developer environment artifact — installing from a clean DMG on a machine that has never run `npm run tauri dev` may not reproduce it.

**Relevant Files**:
- `src-tauri/tauri.conf.json` — bundle identifier (`bundle.identifier`); LaunchServices keys off this
- `src-tauri/target/` — dev build artifacts that get indexed
- `src-tauri/src/main.rs:136` — single-instance plugin registration (runtime-only, not LaunchServices)

---

### macOS-2: Accessibility Permission Never Recognized After Granting ✓ Fixed

**Symptom**: The setup permission step (or the system prompt) shows "Needs access" even after the user grants Accessibility permission in System Settings. The UI keeps saying it wasn't granted no matter how many times the button is pressed. The hotkey and dictation never work at all in production.

**Suspected Cause — TCC cache staleness**: `check_accessibility_permission` (in `commands/mod.rs:1015`) calls `AXIsProcessTrustedWithOptions(false)`. On macOS 13+, the TCC (Transparency, Consent, and Control) daemon is out-of-process and its response can be cached within the running process's session. Granting permission after the app has started may not be reflected until the app is restarted — `AXIsProcessTrustedWithOptions` continues returning `false` for the life of that process. The Setup polling loop (`Setup.svelte:231`) polls every 5 seconds but always gets the stale cached value. There is currently no instruction to the user to restart the app after granting.

**Suspected Cause — Wrong bundle identity**: If the user is running a build-artifact `.app` from the dev target directory rather than the installed DMG copy, macOS may associate the permission with the bundle path it trusts, while the running process is a different path. Because macOS links accessibility permissions to bundle path + code-signing identity, a mismatch means permission to one bundle never applies to another even if both have the same bundle identifier.

**Suspected Cause — CGEventTap not retried after permission is granted**: The `CGEventTap` for the global hotkey is created once in `setup_hotkey()` at app launch (`main.rs:572`). If Accessibility permission is not yet granted at that moment, `CGEventTap::new()` returns `None` (tap creation fails silently or emits an error to the frontend). There is no mechanism to re-attempt tap creation after the user grants permission. The user must fully quit and relaunch the app for the tap to be created. The current flow does not communicate this restart requirement.

**Relevant Files**:
- `src-tauri/src/commands/mod.rs:1015` — `check_accessibility_permission` / `AXIsProcessTrustedWithOptions`
- `src-tauri/src/core/hotkey/mac.rs` — `CGEventTap` creation (listen-only tap, fails without Accessibility permission)
- `src-tauri/src/main.rs:572` — `setup_hotkey()` called once at startup, no retry
- `src/lib/views/Setup.svelte:229` — polling loop; only checks status, no restart-required messaging

---

### macOS-3: Auto Calibration Disappears Instantly ✓ Fixed

**Symptom**: Clicking "Auto calibration" on the Audio settings page causes the calibration panel to appear and immediately vanish, as if it completed in zero milliseconds. No countdown, no microphone activity bar.

**Suspected Cause — Microphone permission failure in production**: The `start_calibration_monitoring` command (`commands/mod.rs:536`) calls `start_recording_session_ex` in a `spawn_blocking` task, which ultimately calls `cpal`'s `build_input_stream` in `media/audio.rs`. In a fully-installed macOS app, Core Audio requires the `com.apple.security.device.audio-input` entitlement and an active microphone permission grant in TCC. If either is missing or if the TCC status is `not_determined` (permission never asked) or `denied`, `cpal` immediately returns an error. This error propagates back to the frontend as a rejected `invoke('start_calibration_monitoring')` call. The frontend's catch block in `calibration.ts:111` responds by calling `cancelCalibration()`, which sets `isCalibrating` back to `false` — collapsing the panel as if nothing happened. There is no error message shown to the user.

**Suspected Cause — Entitlement misconfiguration in production bundle**: In dev mode, Tauri runs the binary directly and macOS may extend microphone access more permissively. In the signed production `.app`, if the `NSMicrophoneUsageDescription` Info.plist key or the audio-input entitlement is missing or misconfigured, the OS will deny access regardless of what the TCC database says.

**Suspected Cause — Race with `starting` flag**: `start_calibration_monitoring` sets `st.starting = true` before the spawn_blocking call and resets it after. If the `lock_state` call after `spawn_blocking` fails (e.g., lock poisoned), `starting` stays `true` permanently. A subsequent calibration attempt returns "Already recording" which is also silently swallowed.

**Relevant Files**:
- `src-tauri/src/commands/mod.rs:536` — `start_calibration_monitoring` and `st.starting` guard
- `src-tauri/src/media/audio.rs` — `cpal` stream build, where mic permission failure would surface
- `src-tauri/src/pipeline.rs` — `start_recording_session_ex`
- `src/lib/calibration.ts:109` — `invoke('start_calibration_monitoring')` error catch → silent `cancelCalibration()`
- `src-tauri/tauri.conf.json` — entitlements and Info.plist keys for microphone access

---

### macOS-4: Other Suspected Production-Only Issues (Unconfirmed) ✓ Hardened

These are educated guesses at problems that are likely to exist in the fully installed build but have not yet been confirmed:

- **Keychain access differs between dev and production builds**: API keys are stored via `tauri-plugin-store` / macOS Keychain. If the bundle identifier or code-signing team changes between a dev build and the production build, Keychain access may fail silently or prompt with unexpected dialogs. Relevant: `src-tauri/src/data/credentials.rs`.
- **Pill window may not appear correctly without Accessibility/Screen Recording permission**: The always-on-top pill window uses a high window level. On macOS, windows above a certain level may require Screen Recording permission to be visible over other apps' content. Relevant: `src-tauri/tauri.conf.json`, `src/PillApp.svelte`.
- **Text injection (Cmd+V) may fail silently**: `injection.rs` on macOS uses `CGEventCreateKeyboardEvent` to synthesize Cmd+V. This requires Accessibility permission (same as the hotkey tap). If Accessibility was denied or the tap setup failed (see macOS-2), injection will fail with no visible error. Relevant: `src-tauri/src/core/injection.rs`.

---

## Shipped in 0.10.0
- Automatic microphone gain calibration (setup flow + Audio settings page)
- Auto-learn dictionary reliability hardening and observability
- Hidden developer mode with real-time verbose logs and Force Setup On Launch toggle
- Numeric cleanup cache normalization
- Profanity handling precedence fix across cleanup intensity and tone
- Dictionary input clamping (50-char, code-point-safe)
- Stale cache and dictionary pruning on quick output deletion
- Full UI scrollbar consistency pass
- Snippet inspector polish (scrollbar, modal height cap, truncation)


# Far Future and Monetization (The Funding Plan)

## 1. Cloud Sync ($2/mo Subscription)
- **Goal**: Sync custom dictionaries, snippets, and API keys across devices.
- **Rules**:
    - Must be 100% optional.
    - Use Supabase for database, efficient data storage.

## 2. Managed "Cloud Optimized" Routing
- **Goal**: One-click model selection where the cloud picks the best or cheapest model for the audio length.
- **Implementation**:
    - **Pay-as-you-go** with a thin **10% markup** over raw token costs.
    - Aggressive context caching to reduce user latency and cost.

## 3. Opt-in Analytics (PostHog)
- **Goal**: Track feature usage to guide development.
- **Strict Rule**: 100% Opt-in. Transparency regarding what is being tracked.
