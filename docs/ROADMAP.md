# Roadmap: Transcription Utility

## In Progress — 0.11.0

## 1. Models & Settings Redesign
- **Goal**: Finish the model picker cleanup so provider selection, advanced mode, and key validation feel predictable instead of fragile.
- **Implementation Plan**: 
    - Keep simple mode and advanced mode in sync so changing defaults, fallbacks, and provider keys never leaves the UI in a half-valid state.
    - Make active transcription and cleanup chains easier to inspect before the user starts dictating.
    - Tighten provider key validation and model selection persistence so saved settings survive reloads cleanly.
- **Relevant Files**: `src/lib/components/settings/ModelsSection.svelte`, `src/lib/components/settings/ApiKeysSection.svelte`, `src-tauri/src/data/store.rs`, `src-tauri/src/commands/mod.rs`.

## 2. Groq 401 Stability Regression After 0.11.0
- **Goal**: Fix reports where a newly-entered Groq key works briefly, then starts failing with `401 Unauthorized` until the key is re-entered.
- **Implementation Plan**: 
    - Compare the 0.10.0 plaintext-store path against the 0.11.0 Windows Credential Manager path to find where Groq handling diverged.
    - Capture request IDs and sanitized Groq error bodies in verbose logs so auth failures can be separated into bad-key, access, and unknown categories.
    - Verify that normalized Groq key bytes remain identical from save to read and are not being corrupted by credential migration or retrieval.
- **Relevant Files**: `src-tauri/src/data/credentials.rs`, `src-tauri/src/api/transcription.rs`, `src-tauri/src/api/cleanup.rs`, `src-tauri/src/api/mod.rs`, `src-tauri/src/main.rs`.

## 3. Cleanup Quality Regression on Light Mode
- **Goal**: Make `light` cleanup actually conservative again so it stops stripping critical context and acting closer to Direct.
- **Implementation Plan**: 
    - Tighten separation between `light`, `medium`, and `high` prompt instructions so each mode has a clearly enforced editing budget.
    - Add regression fixtures using longer, context-heavy dictation where losing one clause changes the meaning.
    - Audit post-cleanup transforms and snippet or dictionary interactions so conservative cleanup cannot silently collapse meaning after the model returns output.
- **Relevant Files**: `src-tauri/src/api/cleanup.rs`, `src-tauri/src/api/prompts.rs`, `src-tauri/src/pipeline.rs`, `src-tauri/src/data/db.rs`.

## 4. Transcription Fallback Chain Reliability
- **Goal**: Fix the voice fallback path so transcription fallback works as reliably as cleanup fallback.
- **Implementation Plan**: 
    - Trace the transcription fallback chain end-to-end and confirm retryable provider failures actually advance to the next transcription model.
    - Test missing-key, `401`, `429`, timeout, and `5xx` scenarios so the fallback decision logic stops breaking only on the voice side.
    - Verify that the frontend model-chain UI writes the exact fallback state the backend expects to read.
- **Relevant Files**: `src-tauri/src/pipeline.rs`, `src-tauri/src/api/transcription.rs`, `src-tauri/src/api/mod.rs`, `src-tauri/src/data/store.rs`, `src/lib/components/settings/ModelsSection.svelte`.

## 5. Cleanup Model Selection Persistence Fix
- **Goal**: Keep cleanup model selections stable across reloads and provider switches.
- **Implementation Plan**: 
    - Recheck any migration or normalization logic that rewrites stored cleanup model IDs.
    - Confirm provider-prefixed model IDs always round-trip correctly between frontend state and backend store values.
    - Add a quick regression check for switching providers, closing the app, and reopening without losing the intended cleanup default.
- **Relevant Files**: `src/lib/components/settings/ModelsSection.svelte`, `src-tauri/src/data/store.rs`, `src-tauri/src/commands/mod.rs`.

## 6. Remaining 0.11.0 Backend Bug Fixes
- **Goal**: Finish the lower-level bug cleanup around quota retryability, slash handling in model IDs, and provider prefix routing.
- **Implementation Plan**: 
    - Verify quota and retry rules are consistent between transcription and cleanup flows.
    - Keep model ID validation strict enough to block garbage while still allowing valid provider-specific identifiers.
    - Confirm custom model entry and prefix routing do not silently send requests to the wrong provider.
- **Relevant Files**: `src-tauri/src/api/mod.rs`, `src-tauri/src/data/store.rs`, `src-tauri/src/pipeline.rs`, `src/lib/components/settings/ModelsSection.svelte`.

## 7. Pre-Release Stabilization Pass
- **Goal**: Hold the next release until Groq auth reliability, conservative cleanup behavior, and transcription fallback all beat or match 0.10.0 baseline behavior.
- **Implementation Plan**: 
    - Run direct A/B checks on 0.10.0 versus 0.11.x for Groq auth, light cleanup output, and transcription fallback.
    - Add targeted provider diagnostics and smoke coverage for the failure paths that have been showing up in real use.
    - Refuse the next release until the core dictation loop feels better, not just newer.
- **Relevant Files**: `tests/OnePyFone.py`, `tests/smoke/`, `src-tauri/src/pipeline.rs`, `src-tauri/src/api/transcription.rs`, `src-tauri/src/api/cleanup.rs`.

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


# Far Future & Monetization (The Funding Plan)

## 1. Cloud Sync ($2/mo Subscription)
- **Goal**: Sync custom dictionaries, snippets, and API keys across devices.
- **Rules**: 
    - Must be 100% optional. 
    - Use Supabase for database, efficient data storage.

## 2. Managed "Cloud Optimized" Routing
- **Goal**: One-click model selection where the cloud picks the best/cheapest model for the audio length.
- **Implementation**: 
    - **Pay-as-you-go** with a thin **10% markup** over raw token costs.
    - Aggressive context caching to reduce user latency and cost.

## 3. Opt-in Analytics (PostHog)
- **Goal**: Track feature usage to guide development.
- **Strict Rule**: 100% Opt-in. Transparency regarding what is being tracked.
