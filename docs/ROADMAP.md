# Roadmap: Transcription Utility

## 1. Set up flow automatic mic gain calibration.
- **Goal**: automatically calibrate the microphone gain by having the user say something in my normal speaking voice, and pick it up with a microphone in the already existing setup flow.
- **Implementation**: record the user's max decibel level and try to get it within a specific range when they're normally talking. Integrate automatic microphone gain into its own page in the settings menu. Make sure there's a user-friendly skip button that has nice animations and is consistent with the rest of the UI.


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
