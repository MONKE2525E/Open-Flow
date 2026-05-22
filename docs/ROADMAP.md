# Roadmap: Transcription Utility

## In Progress — 0.11.0

- Models & Settings redesign: simple/advanced mode toggle, live model chain preview, provider key validation UI
- Cleanup model selection persistence fix
- Bug fixes: quota retryability, slash in model IDs, prefix routing

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
