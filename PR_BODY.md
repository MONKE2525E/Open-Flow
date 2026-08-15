## Thinking Path

> - Verenu is a Windows and macOS AI dictation app: hotkey hold -> audio capture -> transcription -> LLM cleanup -> text injection.
> - This change spans the data, pipeline, and UI analytics subsystems.
> - Verenu had no user-facing usage and cost analytics surface backed by local dictation history.
> - The 0.17.0 integration line needs Insights without importing unrelated history from the prior branch.
> - This pull request extracts the Insights feature and its review fixes onto `dev`.
> - The benefit is a local-first Insights page with usage, streak, word, model, and cost analytics.

## What Changed

- Added local SQLite Insights aggregation and recording/history instrumentation.
- Added the Insights navigation view and analytics components.
- Added range filtering, charts, heatmap, word/model/cost breakdowns, and pricing helpers.
- Included the related review-fix rounds through `5bffecf5`.
- Excluded the unrelated Chromium injection commit and merge history from `codex/insights-analytics-page`.

## Verification

- `npm run check` passes.
- `npm run lint` passes, including Clippy with `-D warnings`.
- `npm test` was started, but the fast native harness exceeded five minutes while waiting on Cargo locks from concurrent builds; CI should run the authoritative test matrix.
- Diff reviewed against `dev`; branch contains only the selected Insights commit range.

## Risks

- SQLite schema/migration and aggregation logic affect historical transcription data.
- Recording/finalization instrumentation and text-injection-adjacent plumbing are touched to collect analytics safely.
- No API keys or secrets are written; analytics remain local.

## Model Used

- OpenAI GPT-5.6 Luna via Codex, with tool use and long-context reasoning.

## Checklist

- [x] Thinking path traces from Verenu context down to this specific change
- [x] Model used is specified
- [x] Checked `docs/ROADMAP.md`; this PR does not duplicate planned work
- [x] `npm run check` passes
- [x] `npm run lint` passes
- [ ] `npm run test:rust` passes
- [ ] Smoke tests pass
- [ ] Tests added or updated where applicable
- [ ] UI changes include before/after screenshots
- [x] No API keys, secrets, or personal data in code or logs
- [x] Risks documented above
