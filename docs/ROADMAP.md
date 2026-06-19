# Roadmap: Verenu

## In Progress - 0.14.0

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
    - The existing implementation mostly trusts `LAST_INJECTION`, an internal tail of text injected by Verenu.
    - Mouse-click send buttons, browser chat inputs clearing themselves after submit, DOM rewrites, and some Enter/send paths can leave that tail stale.
    - Once stale, the next dictation can be treated as a mid-sentence continuation and the first letter is lowercased even though the target field is empty.
    - Punctuation loss must be debugged separately from casing. Some missing punctuation is likely cleanup/transcription output, not paste-time capitalization.
- **Design Principle**:
    - Never lowercase the first letter unless Verenu has high-confidence evidence that the cursor is still inside the same editable control and immediately follows a non-sentence-ending character.
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
