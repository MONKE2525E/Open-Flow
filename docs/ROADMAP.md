# Roadmap: Verenu

## Current line: 0.18.1

The current release is 0.18.1. Short-lived release work belongs in the `Unreleased` section of [CHANGELOG.md](CHANGELOG.md). This page records open follow-up work and longer-term status.

## Completed foundations

### Local models

Local transcription and local cleanup are part of the normal pipeline. Settings -> Models manages downloaded STT models, cleanup models, and the shared local cleanup runtime. Intel Mac local models remain deliberately gated until that path has been validated on real hardware. See [LOCAL_TRANSCRIPTION.md](LOCAL_TRANSCRIPTION.md).

### Models and settings

The model picker now supports curated and provider-reported cloud models, provider-prefixed defaults and fallback chains, local model download state, and persistence across reloads. Remaining model work should be recorded as a specific bug or feature instead of leaving the original redesign plan open.

## Open follow-up

### Groq credential reliability

The backend now classifies 401/403 responses, reports the provider and request metadata safely, and distinguishes retryable provider failures from invalid credentials. Keep long-running Groq credential soak testing as an open follow-up rather than describing the old credential-storage migration as current work.

## Completed: contextual formatting
- **Status**: The planned context engine is implemented. Verenu now uses a layered caret-local probe, clipboard-sniff fallback, and guarded history fallback for contextual casing and spacing across supported text controls.
- **Former failure pattern**:
    - The former implementation mostly trusted `LAST_INJECTION`, an internal tail of text injected by Verenu.
    - Mouse-click send buttons, browser chat inputs clearing themselves after submit, DOM rewrites, and some Enter/send paths can leave that tail stale.
    - Once stale, the next dictation can be treated as a mid-sentence continuation and the first letter is lowercased even though the target field is empty.
    - Punctuation loss must be debugged separately from casing. Some missing punctuation is likely cleanup/transcription output, not paste-time capitalization.
- **Design Principle**:
    - Never lowercase the first letter unless Verenu has high-confidence evidence that the cursor is still inside the same editable control and immediately follows a non-sentence-ending character.
    - Unknown context must fail closed: preserve the cleanup output casing, or capitalize only through a deterministic sentence-start rule. It must not randomly force lowercase.
    - The old injection tail can remain as a last-resort cache, but it must not be the primary source of truth.
- **Implementation history**: The following phases describe the work that produced the current implementation. Keep them as historical context, not as open tasks.
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
        - In `src-tauri/src/core/injection/mod.rs`, call the new context probe and decision module before building `adjusted`.
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
- **Relevant Files**: `src-tauri/src/core/injection/mod.rs`, `src-tauri/src/core/hotkey.rs`, `src-tauri/src/api/auto_learn.rs`, `src-tauri/src/api/prompts/mod.rs`, `src-tauri/src/api/cleanup.rs`, `src-tauri/src/pipeline/mod.rs`, `src/lib/components/settings/GeneralSection.svelte`, `tests/manual/`, `tests/OnePyFone.py`.

## Completed: shared cleanup prompt and context retention

The cleanup pipeline now assembles one shared prompt for every cleanup model, applies explicit intensity rules, preserves factual clauses and qualifiers, and gives snippet instructions a defined priority. Prompt regression tests cover the contract. Future prompt work should be recorded as a focused quality issue.

## Completed: fallback chains and model persistence

The current pipeline stores provider-prefixed model IDs, persists defaults and fallback chains, classifies retryable provider failures, and advances through configured candidates. Keep new failures and provider-specific gaps as separate issues rather than reopening the original hardening plan.

## Release stabilization

The project is now on 0.18.1. Use the current test profiles and platform checks for release verification; the old 0.10.0 versus 0.11.x gate is historical and no longer describes the release process.

## Intel Mac support for local models (currently gated off)
- **Status**: Deliberately unavailable, not a bug. Local on-device STT/LLM (`local_stt`, `local_llm`) is blocked entirely on Intel (x86_64) Mac builds via `system::platform::is_macos_intel()` — the download commands return an error and the Settings → Models UI shows an explanatory notice instead of the download tiles.
- **Why**: Zero real-world testing on Intel hardware (neither the maintainer nor any current tester owns an Intel Mac), and Intel Macs are old enough now to be both increasingly uncommon and generally underpowered for local LLM inference specifically. Shipping an untested first run there is a worse outcome than a clear "not yet" pointing at cloud providers.
- **What already exists**: the Metal (Apple Silicon) vs. CPU (Intel) backend split for the local LLM runtime already has real code paths in `local_llm/binary.rs::detect_backend()` and downloads real `macos-x64` llama.cpp release assets — this was never architecturally impossible, just unvalidated.
- **To unblock**: get access to real Intel Mac hardware (see options discussed with the maintainer: ask existing beta testers first, or a short paid rental from a Mac cloud provider as a fallback) and manually validate the full local-models flow — setup, download, verify/extract, and actual transcription/cleanup inference — before removing the gate in `system/platform.rs`.
- **CI note**: `pr-checks.yml`'s `rust` job now includes `macos-13` (real Intel silicon, not the `macos-latest` arm64 runner used for the release pipeline's cross-compiled Intel DMG) so `cargo test`/`clippy` at least run natively on Intel per PR — this catches build-level regressions but not the local-model runtime behavior itself, which needs a human on real hardware.
- **Relevant Files**: `src-tauri/src/system/platform.rs`, `src-tauri/src/commands/local_stt.rs`, `src-tauri/src/commands/local_llm.rs`, `src-tauri/src/commands/system.rs` (`local_models_supported_on_this_platform`), `src/lib/components/settings/ModelsSection.svelte`, `.github/workflows/pr-checks.yml`.

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
- `src-tauri/src/pipeline/mod.rs` — `start_recording_session_ex`
- `src/lib/calibration.ts:109` — `invoke('start_calibration_monitoring')` error catch → silent `cancelCalibration()`
- `src-tauri/tauri.conf.json` — entitlements and Info.plist keys for microphone access

---

### macOS-4: Other Suspected Production-Only Issues (Unconfirmed) ✓ Hardened

These are educated guesses at problems that are likely to exist in the fully installed build but have not yet been confirmed:

- **Keychain access differs between dev and production builds**: API keys are stored via macOS Keychain. If the bundle identifier or code-signing team changes between a dev build and the production build, Keychain access may fail silently or prompt with unexpected dialogs. Relevant: `src-tauri/src/data/credentials.rs`.
- **Pill window may not appear correctly without Accessibility/Screen Recording permission**: The always-on-top pill window uses a high window level. On macOS, windows above a certain level may require Screen Recording permission to be visible over other apps' content. Relevant: `src-tauri/tauri.conf.json`, `src/PillApp.svelte`.
- **Text injection (Cmd+V) may fail silently**: `core/injection/macos.rs` on macOS uses `CGEventCreateKeyboardEvent` to synthesize Cmd+V. This requires Accessibility permission (same as the hotkey tap). If Accessibility was denied or the tap setup failed (see macOS-2), injection will fail with no visible error. Relevant: `src-tauri/src/core/injection/mod.rs`.

---

## Shipped in 0.10.0
- Automatic microphone gain calibration (setup flow + Audio settings page)
- Auto-learn dictionary reliability hardening and observability
- Hidden developer mode with opt-in verbose logs and Force Setup On Launch toggle
- Numeric cleanup cache normalization
- Profanity handling precedence fix across cleanup intensity and tone
- Dictionary input clamping (50-char, code-point-safe)
- Stale cache and dictionary pruning on quick output deletion
- Full UI scrollbar consistency pass
- Snippet inspector polish (scrollbar, modal height cap, truncation)
