# Efficiency pass, September 2026

The local checkout was the reference, including its in-progress durable audio recovery and settings work. This pass follows `transcription-ram-reliability-plan.md`. It adds no runtime dependencies, changes no controls or styles, and leaves microphone gain, denoising, resampling mathematics, quality gates, provider fallbacks, and injection order intact.

## Before and after

Polling rates below exclude initial loads, explicit user actions, and backend events. Completion-based scheduling also prevents requests accumulating behind a slow backend.

| Work | Before | After |
| --- | --- | --- |
| Idle sync reconciliation, when sync is enabled | 60 IPC requests/minute | 2/minute, a 96.7% reduction |
| Visible active pairing | 1-second polling plus events | Same cadence plus events; concurrent event refreshes coalesce |
| Hidden sync | 1-second fallback | 30-second fallback; incoming pairing events still refresh immediately |
| Sidebar memory diagnostic while hidden | 12 process-tree measurements/minute | 0; immediate refresh on return |
| Sidebar while visible | 5-second interval | Same nominal cadence, no overlapping requests |
| Insights while visible | 6 fallback database queries/minute | 1/minute, an 83.3% reduction; dictation events still refresh immediately |
| Insights while hidden | 6 fallback queries/minute plus dictation events | No periodic queries or dictation-triggered queries; reconciles on return |
| Mounted macOS permission view while hidden | Up to 40 snapshots/minute | 0 periodic snapshots; visible 1.5-second checks and explicit permission actions remain |
| Connectivity | Visible 60-second checks; startup check even if hidden | Same visible cadence and failure-event rechecks; hidden startup polling suspended; overlapping checks suppressed |
| Unused service health | One startup request, then 3/hour, plus opt-in check | Removed automatic work; backend diagnostic command remains available |
| Sync worker with no discovered devices | Peer database query every 5 seconds | Returns before querying the database |
| Icon cache retention | Unbounded module-level maps | Each cache limited to 128 entries and 4 MiB of resolved string data, using conservative UTF-16 accounting |
| Expired retry/resume audio | Retained until a later user action or replacement | Released by the existing 30-second maintenance loop after the 10-minute window |

The icon limits concern cache-owned references. Visible components and in-flight requests can still own images independently. Eviction does not blank mounted icons. Negative results remain cached while resident, and recently used entries survive preferentially.

Sync is already opt-in in this checkout. The sync reductions do not apply to users who have it disabled. Listeners register before the initial status reconstruction, and a 30-second fallback remains even while hidden. A missed idle pairing event can therefore take up to 30 seconds to recover instead of one second. Once visible pairing is known, the fast cadence resumes.

## Audio allocation measurements

A deterministic Rust fixture encodes 60 seconds of synthetic 16 kHz audio with the previous grow-from-empty WAV allocation and the new allocation. Both outputs are byte-identical and round-trip through hound.

| Measurement | Before | After |
| --- | ---: | ---: |
| WAV vector capacity | 2,883,584 bytes | 1,920,044 bytes |
| Live sample/WAV buffer capacity during stop at 16 kHz, excluding the capture queue | 10,563,584 bytes | 5,760,044 bytes |

The second row is a 45.5% reduction in these buffers, not a claim about total process RAM. At 16 kHz the input allocation moves into the result instead of being cloned. Other input rates retain the same resampling output, tested at 8, 16, 44.1, 48, and 96 kHz, and release the native-rate buffer before WAV encoding. During resampling at other rates, input and output still overlap.

The 320,000-sample queue retains its existing capacity and backpressure behavior. It now drops before output allocation once the callback and worker stop. Its sample payload alone is 1,280,000 bytes, excluding queue bookkeeping. Failed stream construction, unsupported formats, and playback startup failures now stop and join the processing worker instead of leaving it polling every 2 ms with its queue and optional recovery sink retained.

Expired capture cleanup drops only the state's audio references. A live pipeline's shared `Arc`/`Bytes` owners remain valid. Small expiry metadata remains so retry errors and recovery bookkeeping keep their existing behavior. The recovery files are not deleted by this maintenance operation.

## Other investigations

- `system/memory.rs` measures the process tree, so suspending the hidden sidebar avoids both IPC and native enumeration. RAM/VRAM pressure probes already run only while a local model is loaded. Manual resource diagnostics remain on demand.
- Local STT and LLM idle unloading already skips unloaded models. LLM request admission protects active cleanup. STT drops its engine and LLM stops its child server. Their memory policies and 30-second pressure checks remain unchanged to preserve model readiness and safety behavior.
- Audio capture keeps its native-rate processed buffer and 2 ms worker wait during recording. Changing buffering, worker wakeups, denoising, or streaming resampling would need native device and long-recording measurements. The startup failure leak is fixed without changing the capture callback.
- `CapturedAudio` already shares WAV bytes and 16 kHz samples across retries and parallel transcription. Those clones are reference-counted handles. Successful finalization clears the matching retry slot; expired unsuccessful/cancelled slots were the remaining retention gap addressed here.
- Auto-learn monitors have a bounded 60-second lifetime and per-session deduplication. Concurrent monitors intentionally preserve corrections from separate dictations. Event hooks have fallback polling. Their timing is unchanged because reducing it could miss corrections.
- Sync's listener blocks on incoming connections; its five-second worker preserves discovery, retry backoff, and change reconciliation. Only the no-target database work was removed.
- Home history remains paginated, with the backend page limit clamped to 1–500. The unbounded `query_recent` entrypoint is used by the test fixture, not Home.
- Insights streams text rows but builds a vocabulary map for exact unique-word counts and ranking. It now moves strings from that map into the ranking vector instead of duplicating the complete vocabulary. SQL date semantics and exact analytics remain unchanged; the larger saving comes from fewer refreshes.
- Cleanup cache entries live in SQLite, have expiry/inactivity rules, and are pruned at startup. This is not an in-memory whole-history cache. No schema or cache-key behavior changed.
- Provider model catalog caching has freshness/retry rules. Download/model listener registration already protects against late unlisten promises and remounts. These remain unchanged.
- The logger ring is limited to 1,000 entries. Silero VAD creates its inference object within the per-recording analysis call; its static state is the embedded model and staged path, not retained session audio. No new logs contain dictated text or private content.
- Provider-status checks stay at five minutes and automatic updates at six hours. Both can notify users while the window is hidden, so suspending them would change behavior. The separate API-health value had no readers and no UI.
- Route imports and frontend dependencies are unchanged. Lazy-loading views could trade startup work for first-navigation latency; that tradeoff was not justified by this pass's measurements.

## Verification

- Frontend unit tests: 101 passed, including polling visibility, slow requests, trailing event reconciliation, disposal, error recovery, adaptive intervals, bounded icon-cache eviction, and sync listener startup/hidden pairing/late-registration cleanup.
- `npm run check`: zero errors and warnings; macOS identity contract passed.
- Production build passed from the resolved checkout path. It retains the existing large-chunk warning.
- Audio tests: 7 passed, including worker cleanup, sample equivalence, WAV equivalence/allocation, and existing gain/backpressure tests.
- Expired capture test passed, including preservation of fresh captures and other live shared owners.
- Final `npm run test:rust`: 630 passed, 1 failed, 4 ignored. The failure is `api::prompts::tests::every_cleanup_tone_combination_renders_the_actual_contract`, reporting 933 tokens for medium/formal. The prompt files had unrelated edits before this pass and were not changed here.
- Final `npm test`: 26 passed, 5 failed. Besides the prompt failure, four browser tests timed out during navigation on the development server: content surfaces, element contracts, app mount, and app mappings. An earlier full run had 28 passes and three failures. The onboarding failure from that run passed on the final run.
- Targeted checks against the production build passed for offline errors, content surfaces, and simulated macOS permissions. These use browser fixtures, not native microphone/TCC calls.
- Further unchanged smoke tests against the production build passed app mounting and the App Mappings add/remove flow. The element-contract test reached Settings, then timed out looking for a `Microphone` tab. The current app labels this tab `Audio`; neither that UI label nor the frozen test was changed by this pass.

The existing performance test and unchanged baseline passed in the final full run:

| Metric | Measured | Existing budget |
| --- | ---: | ---: |
| Navigation visible | 523.04 ms | 2,500 ms |
| Settings open | 55.80 ms | 900 ms |
| Section change p95 | 389.80 ms | 650 ms |
| Settings close | 855.32 ms | 1,000 ms |
| Tasks over 50 ms | 1 | 4 |
| Uncaught errors | 0 | 0 |

The same test against the production build also passed: 85.55/44.69/396.38/849.77 ms respectively, zero long tasks and errors. Those timings are from a different serving environment and are not a before/after startup speed claim. The pre-edit browser measurement was blocked by a missing Chromium executable, which was subsequently installed. Repeated dev-server runs also encountered navigation stalls. Launching the build through the `G:\Verenu` alias produced an emitted-HTML path error; building from the resolved checkout path passed.

No controlled native idle-RAM, OS wakeup, real microphone, or macOS hardware run was performed. Allocation capacities and deterministic polling tests establish the reductions above; native process working set, capture under device backpressure, and installed macOS permission behavior remain manual verification limits. Smoke tests and performance budgets were not edited.
