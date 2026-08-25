## Thinking Path

<!--
  Required. Trace your reasoning from the top of the project down to this
  specific change. Start with what Verenu is, then narrow through the
  subsystem, the problem, and why this PR exists. Use blockquote style.
  Aim for 5-8 steps.
-->

> - Verenu is a Windows and macOS AI dictation app - hotkey hold -> audio capture -> transcription -> LLM cleanup -> text injection
> - [Which subsystem: hotkey / audio / pipeline / injection / settings / UI / data]
> - [What problem or gap exists]
> - [Why it needs to be addressed now]
> - This pull request ...
> - The benefit is ...

## What Changed

<!-- One bullet per logical unit of change. -->

-

## Verification

<!--
  How can a reviewer confirm this works?
  Include test commands, manual steps, or both.
  For UI changes: before/after screenshots.
  For pipeline changes: describe the test scenario (hotkey hold -> injection).
-->

-

## Risks

<!--
  What could go wrong? Call out: migration safety, breaking changes,
  clipboard/injection behavior shifts, API key exposure, hotkey timing,
  smoke test contract changes. Or "Low risk" if genuinely minor.
-->

-

> Check [`docs/ROADMAP.md`](../docs/ROADMAP.md) before opening feature PRs - work that overlaps with planned core changes may need to be redirected. See [`CLAUDE.md`](../CLAUDE.md) for architecture and contribution guidance.

## Model Used

<!--
  Required. Which AI model assisted with this change?
  Include provider, model ID/version, and any special modes
  (e.g., extended thinking, tool use, long context).
  Write "None - human-authored" if no model was used.
-->

-

## Checklist

- [ ] Thinking path traces from Verenu context down to this specific change
- [ ] Model used is specified (provider + version)
- [ ] Checked `docs/ROADMAP.md` - this PR does not duplicate planned work
- [ ] `npm run check` passes (TypeScript)
- [ ] `npm run lint` passes (ESLint + Clippy)
- [ ] `npm run test:rust` passes
- [ ] Smoke tests pass (see `Agent-Skills/SmokeTest.md` for commands)
- [ ] Tests added or updated where applicable
- [ ] UI changes include before/after screenshots
- [ ] No API keys, secrets, or personal data in code or logs
- [ ] If version bumped: all three files updated together (`package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`)
- [ ] Relevant documentation updated to reflect changes
- [ ] Risks documented above
