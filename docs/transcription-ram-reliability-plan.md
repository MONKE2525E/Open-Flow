# Transcription RAM And Reliability Plan

Verenu targets roughly 200 MB idle RAM. This plan records the constraints and checks that should shape future pipeline work.

## Current Priority

The core dictation loop matters more than feature count:

1. Capture audio reliably.
2. Reject bad recordings before provider calls.
3. Send only the data needed for transcription and cleanup.
4. Preserve user intent during cleanup.
5. Inject into the original target app.
6. Store local history without leaking private content.
7. Keep idle memory low.

## RAM Risks To Watch

- Long-lived audio buffers that survive past a pipeline session.
- Unbounded transcription history loaded into the frontend.
- Provider response bodies or prompts retained after use.
- Large logs, debug snapshots, or verbose traces.
- Heavy frontend dependencies for small UI wins.
- Caches without size, age, or invalidation rules.
- Extra background loops that keep running while the app is idle.

## Reliability Risks To Watch

- Hotkey callbacks doing work synchronously instead of handing off quickly.
- Foreground window capture happening after async provider calls.
- Clipboard restore failures.
- Context probing that logs or stores private text.
- Cleanup prompts that drop factual clauses or rewrite too aggressively.
- Fallback chains that retry non-retryable auth failures.
- macOS permission state differing between dev builds and installed app bundles.
- Windows UI Automation calls running without COM initialization.

## Required Safeguards

- Keep quality gates before provider calls.
- Keep API keys in OS credential storage only.
- Keep settings validation on both frontend and backend.
- Keep history pagination. Do not load the whole database into the Home view.
- Keep [`../tests/smoke/`](../tests/smoke/) frozen and fix app code instead.
- Use redacted diagnostics: counts, ids, source labels, model names, status codes, and stable fingerprints.
- Do not log dictated text, clipboard contents, prompt bodies, API keys, raw dictionary terms, snippet expansions, or full local paths.

## Measurement Checklist

Before merging a change that affects the dictation pipeline, audio, cleanup, context probing, injection, history, or background tasks:

- Run `npm test`.
- Run `npm run test:rust`.
- Run targeted Playwright checks for UI-facing changes.
- Check memory behavior manually when adding long-lived state, caches, new dependencies, or background loops.
- Confirm failure paths show user-facing errors without crashing.
- Confirm logs and test output do not expose private content.

## When Adding Dependencies

New runtime dependencies need a real reason. Ask:

- Does this replace complex local code or add essential platform support?
- Is it active and maintained?
- Does it pull in large transitive dependencies?
- Does it run in the frontend bundle, Rust backend, or both?
- Does it increase idle work or memory?
- Does it handle private text, API keys, files, or network traffic?

If the answer is vague, do not add the dependency.

## Open Follow-Ups

These are tracked elsewhere but are relevant to this plan:

- Model fallback and persistence hardening in [ROADMAP.md](ROADMAP.md).
- Contextual capitalization reliability in [ROADMAP.md](ROADMAP.md).
- Model-specific cleanup prompt contracts in [ROADMAP.md](ROADMAP.md).
- macOS production permission and signing notes in [macos-code-signing.md](macos-code-signing.md).

## Related Docs

<p align="center">
  <a href="ARCHITECTURE.md"><img alt="Architecture" src="https://img.shields.io/badge/Architecture-Overview-5b554a"></a>
  <a href="TESTING.md"><img alt="Testing" src="https://img.shields.io/badge/Testing-Guide-c44632"></a>
  <a href="README.md"><img alt="Docs Index" src="https://img.shields.io/badge/Docs-Index-2b2422"></a>
</p>
