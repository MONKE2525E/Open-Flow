# Testing

Use the smallest test pass that covers the change, then run broader checks before release or risky PRs.

## Default Gate

```bash
npm test
```

This runs the OnePyFone fast profile. It is deterministic and CI-friendly: no live APIs, no microphone capture, no OS permission prompts, and no real clipboard injection.

## Common Local Checks

```bash
npm run check
npm run lint
npm run build
npm run test:rust
npm run test:smoke
```

`npm run lint` runs frontend type-checking plus Rust Clippy with warnings denied.

## OnePyFone Profiles

```bash
npm run test:all
npm run test:full
npm run test:live
npm run test:native
```

| Profile | Purpose |
| --- | --- |
| `fast` | Default deterministic suite for PRs and normal local work |
| `live` | Provider/API checks, skipped when keys are absent |
| `native` | Platform and manual-adjacent checks |
| `full` | Fast, live, and native profiles |

You can target suites directly:

```bash
python tests/OnePyFone.py --suite ui,state
python tests/OnePyFone.py --suite ui,animation --parallel --workers 3 --fresh-server
```

## Playwright

Use Playwright for UI-facing changes when the app can be exercised through the browser dev server.

```bash
npm run dev
node tests/smoke/playwright-test-ui.cjs
node tests/smoke/playwright-test-fixes.cjs
node tests/smoke/playwright-test-state.cjs
```

`tests/smoke/` is a frozen contract. Do not edit those files unless the user explicitly asks. Fix app code to satisfy them. Add new browser coverage in `tests/integration/`.

## Rust Tests

```bash
npm run test:rust
cargo test --manifest-path src-tauri/Cargo.toml <test_name>
```

Rust tests cover pure logic, provider error classification, prompt assembly, pipeline fixtures, SQLite behavior, context decisions, snippets, dictionary behavior, and data validation.

## Privacy Rules For Tests

- Do not print API keys.
- Do not print clipboard contents.
- Do not include real dictated text in fixtures or output.
- Do not commit screenshots with private text.
- Live provider tests must skip cleanly when secrets are unavailable.

## CI

GitHub Actions currently run:

- Frontend type-check and build.
- npm and Rust dependency audits.
- Rust Clippy and Rust tests on Windows and macOS.
- OnePyFone fast profile with JSON and JUnit reports.
- Extended live/native profiles on schedule or manual dispatch.
- Manual installer builds through `workflow_dispatch`.

## Related Docs

<p align="center">
  <a href="ARCHITECTURE.md"><img alt="Architecture" src="https://img.shields.io/badge/Architecture-Overview-5b554a"></a>
  <a href="CONTRIBUTING.md"><img alt="Contributing" src="https://img.shields.io/badge/Contributing-Guide-c44632"></a>
  <a href="RELEASE.md"><img alt="Release Process" src="https://img.shields.io/badge/Release-Process-7e7266"></a>
  <a href="README.md"><img alt="Docs Index" src="https://img.shields.io/badge/Docs-Index-2b2422"></a>
</p>
