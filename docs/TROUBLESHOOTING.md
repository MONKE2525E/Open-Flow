# Troubleshooting

This page covers common install, setup, dictation, and development problems.

## Install Warnings

Verenu is distributed outside app stores and may not be signed with a paid platform certificate.

On Windows, SmartScreen may show "Windows protected your PC". Click **More info**, then **Run anyway** if you trust the release source.

On macOS, Gatekeeper may block first launch. Right-click Verenu, choose **Open**, then confirm. If that is unavailable, use **System Settings > Privacy & Security > Open Anyway**.

Download installers from GitHub Releases or the tracked [`../installers/`](../installers/) tree. Verify hashes with `SHA256SUMS.txt` when in doubt.

## Setup Cannot Save An API Key

- Check that the key was copied without extra spaces.
- Try saving only one provider key first.
- On macOS, choose **Always Allow** if Keychain prompts for access.
- If a key keeps failing after it was saved, clear it in Settings and save it again.

API keys should never appear in logs, screenshots, issues, PR comments, or exported test output.

## Dictation Produces No Text

Verenu intentionally rejects recordings that are too short or too quiet. The pill shows the reason instead of failing silently.

- Hold the hotkey for more than about 0.7 seconds.
- Check the microphone input device.
- Open Settings -> Audio and run microphone calibration.
- Watch the floating pill bars while speaking. No movement usually means the app is not receiving usable audio.

## macOS Permissions

macOS needs permissions for real use:

- Microphone for recording.
- Accessibility for text injection and focused text reads.

If Microphone or Accessibility permissions look granted in System Settings but Verenu behaves as if they are missing:

1. Fully quit Verenu.
2. Reopen Verenu from the installed app location.
3. In Settings, use the permissions tools to reset stale grants if needed.
4. Grant permissions again when prompted.

For local macOS developer builds, see [macOS code signing](macos-code-signing.md).

## Text Goes Into The Wrong App

Verenu captures the focused app before provider calls start, then tries to restore focus before paste. If injection lands somewhere unexpected:

- Avoid switching targets until the pill finishes processing.
- Check whether the target app blocks clipboard paste or synthetic key events.
- On macOS, confirm Accessibility permission.
- On Windows, try a standard text editor to separate app-specific behavior from Verenu behavior.

## Cleanup Output Is Too Edited

Change cleanup intensity in [Cleanup Levels](CLEANUP_LEVELS.md):

- **Off** to keep the raw transcript.
- **Light** for filler removal only.
- **Medium** for normal cleanup.
- **Strong** for concise rewriting that preserves important details.

For app-specific behavior, configure [Contexts](CONTEXTS.md) so code editors, chat apps, and email clients can use different cleanup settings.

## Development Server Problems

Use the Tauri dev command for full desktop behavior:

```bash
npm run tauri dev
```

Use the frontend-only server when you only need browser-based UI tests:

```bash
npm run dev
```

If Playwright or smoke tests behave strangely, restart with a fresh server:

```bash
python3 tests/OnePyFone.py --suite ui,state --fresh-server
```

On Windows, use `python` instead of `python3` if `python3` is not available in your shell.

## Reporting A Problem

Use the GitHub issue forms. Include:

- OS and version.
- Verenu version.
- Provider and model names, not API keys.
- What you expected.
- What happened.
- Test commands or manual steps you tried.

Do not include private dictated text, API keys, emails, full local paths, screenshots with private content, or log excerpts containing private content.

## Related Docs

<p align="center">
  <a href="INSTALL.md"><img alt="Install" src="https://img.shields.io/badge/Install-Guide-7e7266"></a>
  <a href="FIRST_DICTATION.md"><img alt="First Dictation" src="https://img.shields.io/badge/First-Dictation-c44632"></a>
  <a href="DATA_AND_PRIVACY.md"><img alt="Data And Privacy" src="https://img.shields.io/badge/Data-Privacy-5b554a"></a>
  <a href="SUPPORT.md"><img alt="Support" src="https://img.shields.io/badge/Support-Issues-2b2422"></a>
</p>
