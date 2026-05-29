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
- **Goal**: Stop random capitalization behavior and make sentence-start detection reliable after punctuation and common separators.
- **Implementation Plan**:
    - Build a deterministic test matrix for punctuation and separators (`.`, `?`, `!`, `,`, `/`, newline, trailing spaces, mixed manual typing + dictation).
    - Trace the keyboard hook to confirm punctuation keys are consistently converted by `vk_to_char` across layouts and are not dropped by dead-key or shortcut paths.
    - Audit injection history invalidation rules (backspace, cursor movement, shortcut keys, window changes) to prevent stale or empty context from causing random lowercase or uppercase output.
    - Re-check interaction between contextual capitalization and tone/profile cleanup so lowercase transformations in cleanup are not being mistaken for injection bugs.
    - Add focused regression coverage for contextual caps to lock behavior before 0.11.0 release.
- **Relevant Files**: `src-tauri/src/core/hotkey.rs`, `src-tauri/src/core/injection.rs`, `src/lib/components/settings/GeneralSection.svelte`, `src-tauri/src/api/prompts.rs`, `tests/manual/`, `tests/OnePyFone.py`.

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
