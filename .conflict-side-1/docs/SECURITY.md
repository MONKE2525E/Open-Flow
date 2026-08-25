# Security Policy

Verenu handles private dictated text and user-supplied API keys, so security reports should avoid public disclosure until there is a fix or mitigation.

## Supported Versions

Security fixes are targeted at the current development line and the latest public release.

## Reporting A Vulnerability

Email [security@verenu.com](mailto:security@verenu.com). GitHub private vulnerability reporting is not enabled for this repository, so do not use it. Do not open a public issue or disclose vulnerability details publicly, and do not include exploit details, API keys, private dictated text, logs with secrets, or screenshots with private content until a fix or mitigation is in place.

Good reports include:

- A short description of the issue.
- Affected platform: Windows, macOS, or both.
- Affected version or commit.
- Reproduction steps using fake data.
- Expected impact.
- Whether the issue exposes API keys, local transcripts, clipboard contents, provider responses, files, or logs.

## Security Boundaries

These are treated as sensitive:

- API keys.
- Dictated audio and text.
- Clipboard contents.
- Transcription history.
- Context vocabulary and snippets.
- Cleanup prompts and provider request bodies.
- Full local file paths.
- Logs that include private operational details.

## Dependency Security

GitHub Actions run npm audit, Rust audit, and dependency review. New runtime dependencies need a clear reason because Verenu is a local-first privacy-sensitive desktop app.

## Related Docs

<p align="center">
  <a href="DATA_AND_PRIVACY.md"><img alt="Data And Privacy" src="https://img.shields.io/badge/Data-Privacy-c44632"></a>
  <a href="SUPPORT.md"><img alt="Support" src="https://img.shields.io/badge/Support-Issues-5b554a"></a>
  <a href="CONTRIBUTING.md"><img alt="Contributing" src="https://img.shields.io/badge/Contributing-Guide-7e7266"></a>
  <a href="README.md"><img alt="Docs Index" src="https://img.shields.io/badge/Docs-Index-2b2422"></a>
</p>
