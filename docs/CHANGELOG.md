# Changelog

Notable project changes are recorded here. GitHub Release pages remain the source for full release descriptions, installer assets, VirusTotal links, and platform-specific download notes.

## Unreleased

- Fixed hands-free conversion from an active hold-to-talk dictation ending when
  the original Windows modifier chord is released; stale release events now
  have a three-second handoff window before hands-free stop gestures are
  accepted.
- Replaced the Insights speaking-pace dial with a tick-scale meter that matches the rest of the page: the tile is now left-aligned on the same baseline grid as its two neighbours, quarter marks make the scale readable at a glance, the scale ceiling grows to always clear your personal best, and the best itself is marked on the scale instead of only being named underneath.
- Fixed the main window stuttering and spiking CPU when dragged or resized quickly on Windows: moving the window no longer re-runs title-bar and icon updates per mouse event, resizes coalesce to a single refresh once the size settles, title-bar metrics are only forwarded when visible values change, and themed icons reuse cached artwork instead of re-rendering on every focus or settings event.
- Restored the Windows tray icon's classic proportions — the accent-theming rework had grown the waveform to fill ~70% of the tile edge to edge; it sits back inside the tile with real margins, keeping the sharper native-size rendering.
- Recolored the running tray, taskbar/window, and macOS Dock icons with the selected accent while preserving their existing light/dark backgrounds and bundled launcher icons.
- Switched the default accent from terracotta to theme-neutral black in light mode and white in dark mode. Custom accents still override the full accent scale; the Home hotkey tile keeps colored accents exact and only lifts near-black neutrals to white for contrast.

- Context group app targets now survive versioned/nightly app updates by matching a close replacement name with publisher/developer evidence on Windows and macOS.
- Reworked cleanup prompting around one default shared by every model, with explicit rule priority, conservative ambiguity handling, multilingual preservation, self-corrections and repair commands, spoken symbols and spelling, technical-token reconstruction, restrained formatting, safer number treatment, and context-assisted disambiguation.
- **The cleanup prompt is now a single template used by every model**, edited from Clean-up → Edit prompt. It used to be stored per model, so an edit made on your default was silently ignored the moment a fallback model took over. An existing per-model edit is carried over.
- Fixed Gemini 3 requests failing with `Thinking level MINIMAL is not supported for this model` — affected both cleanup and transcription on newer Gemini 3 flash models, which accept `low` but not `minimal`.
- Fixed the cleanup prompt editor opening in the bottom-right corner instead of centred.
- The model picker now lists every model a provider reports, not just the curated ones, behind a **Show N more models** toggle at the foot of the list — so a newly released model is selectable the day it ships without waiting for a Verenu update. Non-text models (image, TTS, embedding, and similar) are filtered out.
- Fixed the model picker's search icon sitting below the centre of the search field.
- Fixed the settings sidebar highlight blinking off the item under the cursor while the selection pill travelled to it.
- Added a **Legacy pages** toggle (Settings → General) that hides the standalone App Mappings, Dictionary, and Snippets pages by default in favor of Contexts, and brings them back — along with a heads-up that they're no longer actively maintained — when turned on.
- Contexts is now hidden from the primary nav while Legacy pages is on, so there's only one place to manage app tones, vocabulary, and snippets at a time.
- Fixed the App Mappings list playing an entrance animation for every existing row on first load; rows now only animate on actual reorder, matching the Dictionary list.
- Reduced the context group name limit from 120 to 30 characters and added a live character counter, and enforced it client-side with an input `maxlength` (previously unenforced, allowing names that overflowed the page).
- Fixed the context icon color picker rendering as a full-width bar instead of a compact popup near the click point.
- Context group websites are now checked for DNS existence before being saved, so a typo can't silently create a website target that will never match anything.
- Added a subtle pop-in animation when adding an app or website to a context group, without replaying it for the rest of the list when switching between context groups.
- Added `docs/CONTEXTS.md` and marked App Mappings, Dictionary, and Snippets as legacy pages across the docs.
- Fixed the Insights page stacking its summary tiles too early on narrow windows, stranding the gauge in a half-empty row — the hero band, heatmap rail, and vocabulary sections now adapt to the available column width and stay side by side down to the minimum window size.

## 0.18.0

The 0.18.0 release notes, installer assets, hashes, and platform-specific download details are available on the [GitHub release page](https://github.com/MONKE2525E/Verenu/releases/tag/v0.18.0).

## 0.15.1 beta - Audio & Polish

- Added configurable dictation sound cues for start, stop, cancel, and error transitions.
- Added a Windows-only option to pause active media sessions (Spotify, YouTube, etc.) during dictation and resume them afterward.
- Added macOS-only exclusive microphone access so other apps can't capture audio while dictating.
- Switched the Windows main window to native OS title bar chrome, recolored to match the app theme.
- Fixed the Windows tray Relaunch action silently closing the app instead of restarting it.
- Hardened the dictation pill window against exposing native chrome in hands-free mode on Windows.
- Evened out the Dictionary and Snippets sort segmented controls' selection highlight.
- Fixed Caps Lock casing getting partially undone by contextual capitalization when dictating mid-sentence.
- Fixed the dictation pill clipping on the first recording shown after switching monitors.
- Tightened the hands-free start chime timing and animated the pill's move across monitors.
- Signed macOS release builds with a persistent self-signed identity so permission grants carry over between updates.
- Replaced the periodic HTTP connectivity probe with native OS connectivity checks, eliminating background network traffic for that check.
- Organized project documentation around `docs/` instead of scattering full policy files at the repository root.
- Added GitHub issue templates for bug reports and feature requests.
- Added public docs for architecture, testing, release process, troubleshooting, security, support, code of conduct, and RAM/reliability constraints.
- Added npm package metadata and macOS signing script aliases so documented commands resolve.
- Refactored oversized backend Rust modules (`commands/settings.rs`, `main.rs`) into focused modules with no behavior change.

Installer hashes:

| File | SHA-256 |
| --- | --- |
| `Verenu_0.15.1_Apple_Silicon.dmg` | `51eb5d40be4814d460efe9baf6c6214652961dd3abbeee2e32b19178899b1529` |
| `Verenu_0.15.1_Intel.dmg` | `6770f15a82809c09741d4ef2b64e4428798bc865f0b7aea2be87a964e8407c2f` |
| `Verenu_0.15.1_x64-setup.exe` | `ac36e31871a4919e08f0d0a17386df65c77b4374f96eab908d15643d83de0f8c` |
| `Verenu_0.15.1_x64_en-US.msi` | `f605697435d7ce75ea1e1f6ecebc0e370d61158e8c97e410a1c8ab4b8fbe7ba6` |

## 0.15.0 beta - Polish

- Target-monitor dictation pill: shows the recording pill on whichever monitor the user is typing on instead of always using the primary monitor.
- Sharper pill visualizer: renders even visualizer bar widths across monitors with different DPI scaling.
- Smarter contextual capitalization: fixes a per-app style leak and corrects casing in empty and Chromium text boxes.
- Cleaner casual injection: skips a clipboard sniff that could corrupt output in the Very Casual profile.
- Sharper cleanup intensity: tightens Verbatim, Light, Medium, and Direct cleanup contracts.
- Native update notifications: surfaces new versions inside the app and supports in-place installs.
- Hardened macOS reliability: uses `RegisterEventHotKey`, stable code signing, and more reliable paste behavior.
- Slimmed backend: moved to backend-owned settings, trimmed dependencies, hardened logging, and split oversized modules.

Installer hashes:

| File | SHA-256 |
| --- | --- |
| `Verenu_0.15.0_Apple_Silicon.dmg` | `e3e02168ffe50eb9b62fa71f7bef75c592686d4544bcdd137f5ec5fed3d1aeba` |
| `Verenu_0.15.0_Intel.dmg` | `f66b6135b33bac9c07149e82fe8928f2b17b80c8d71244b540f1e42e196e144c` |
| `Verenu_0.15.0_x64-setup.exe` | `806375b6d295a0e4a47c1c0efa4a9e4c2b8c75ad7549d3fa0c6a0acf4c7bdab4` |
| `Verenu_0.15.0_x64_en-US.msi` | `b6cd8cf038cbb8b999e3b043f799ed1e504958eaa704ad41dbf305674459f3f6` |

## 0.14.1 beta - macOS Installer Fix

- Rebuilt macOS installers with ad-hoc code signing so the app bundle has a valid structural signature on Apple Silicon and Intel Macs.
- Bumped Verenu to `0.14.1`.
- Enabled Tauri ad-hoc macOS signing with `signingIdentity: "-"`.
- Updated the installer workflow to derive DMG filenames from `package.json`.
- Added CI verification that mounts each macOS DMG and runs `codesign --verify --deep --strict --verbose=4`.
- Confirmed both macOS CI builds report `Signature=adhoc`.

Installer hashes:

| File | SHA-256 |
| --- | --- |
| `Verenu_0.14.1_Apple_Silicon.dmg` | `C26886D38C3E686118D43165C061177092064CE6CD6FEF2B417BD5FBC7B74B97` |
| `Verenu_0.14.1_Intel.dmg` | `F451FA4E0B41A61610354215B8ADDE0A6133771AA84630F6791740DD70BFC028` |
| `Verenu_0.14.1_x64-setup.exe` | `5D2A5E99AC4D0CA03036B15A36F6EE39C6477F65821881D88CA1724159AF3967` |
| `Verenu_0.14.1_x64_en-US.msi` | `546F1F3A4436649A51C6F5753FF6D95AED3E197E1EC17F77B2EB1B220E94DC21` |

## 0.14.0 beta - UI Refresh

- Modular setup wizard: refactored first-run setup into focused per-step components.
- Reorganized settings: subdivided settings into labeled subgroups with setup wizard toggles.
- Streamlined API keys row: Save and Clear now flip inline with status feedback and accurate Gemini model listings.
- Automatic Caps Lock detection: adjusts dictation casing when Caps Lock is active.
- Full-screen cleanup prompt editor: added a dedicated modal with stronger injection handling.
- Per-model cleanup prompt templates: added provider-specific templates, a refusal guard, and an Advanced Models UI.
- Redesigned pill error display: matched the in-app error toast styling.
- Virtualized history list: improves scrolling performance for long histories.
- History retention enforcement: enforces configured retention windows and confirms deletion of older entries.
- Fixed UI rough edges: hands-free pill flicker, scrollbar alignment, and history retention dropdown ellipsis during animation.
- Hardened auto-update and autostart reliability with safer DB backup, async I/O changes, Windows autostart registry fixes, and backend module splits.

## 0.13.0 beta - Verenu

- Per-app cleanup intensity: app mappings can override global Verbatim, Light, Medium, or Direct cleanup intensity.
- Smarter auto-learn promotion: distinctive corrections can promote after one high-confidence session, while everyday-word corrections stay safer and contextual.
- Database self-healing: repairs installs left with a missing column after interrupted migrations.
- Hardened data migration, snippet usage tracking, and SQLite WAL preservation across updates.
- Removed the last Open Flow to Verenu transition code.
- Added the Data & Privacy documentation for local storage and provider data flow.

## 0.12.1 beta - Verenu Transition

- Forward-compatible update checks: checks both Open Flow and upcoming Verenu release sources so updates and About links keep working through the rename.

## 0.12.0 beta - macOS

- JSON backup import and export for settings, dictionary entries, and snippets.
- Secure macOS API key storage using native Keychain APIs.
- Two-phase microphone calibration for normal speech and whispering.
- Advanced snippet triggering with comma-separated triggers, punctuation-tolerant matching, and cache isolation.
- macOS permissions overhaul with visual status rows, Keychain checks, and polling.
- Self-injection detection with clipboard fallback when the app itself has focus.
- Contextual capitalization hardening using Windows UI Automation and macOS process hints.
- Optimistic UI updates for dictionary and snippet interactions using atomic SQLite returning statements.
- All-in-one test runner with a unified harness and mock provider support.

## 0.11.0 beta - Polish

- Transcription retry from Home: failed transcriptions can be rerun from history.
- Encrypted API key storage with Windows Credential Manager.
- Model fallback reliability: fallback chains try all configured fallback models in order.
- Pipeline feedback improvements for quality-gate rejections and API key errors.
- Model settings UX redesign with clearer fallback controls and labels.
- Dictionary and snippet limit UX with counters and nudges.
- Injection behavior refinements for contextual capitalization and auto-spacing.
- Offline and settings UI polish.
- Corrected WPM metrics to use raw transcription word counts.

## 0.10.0 beta - Local-First AI Dictation

- Automatic microphone gain calibration during setup and from Settings.
- Smart output rejection: deleting dictated text shortly after injection prunes stale cleanup cache and related auto-learn substitutions.
- Developer mode with verbose pipeline logs, downloadable session logs, and Force Setup On Launch.
- Onboarding improvements with restored appearance selection and no-scroll layout.
- Snippet inspector polish for modal height, overflow, and long previews.
- Auto-learn reliability hardening with stable-text gates, session deduplication, and tighter candidate filtering.
- Numeric cache normalization so numeric and written forms share cache keys.
- Profanity handling precedence fix across cleanup intensity and tone.
- Dictionary input clamping with code-point-safe truncation.
- Unified scrollbar styling across scrollable surfaces.

## 0.9.0 beta - Caching

- Local cleanup cache for repeated transcription cleanup responses.
- Cache key normalization across punctuation, casing, and trailing periods.
- Settings reorganization: moved core behavior, API fallback, auto-learn, Audio, and Apps into clearer locations.
- Auto-learn hardening with per-session stable text gates, within-session deduplication, and tighter candidate filtering.
- App mappings search fixes and scrollbar polish.
- Auto-learn regression matrix with JSON fixtures and a PowerShell harness.

## 0.8.0 beta - Local-First AI Dictation

- Spoken language selector for better non-English transcription accuracy.
- Silent auto-update without console flash or PowerShell execution-policy prompts.
- Pre-update SQLite database backup.
- App mappings redesign with a dedicated editor component.
- Dynamic theme-aware tray icon.
- CI and dependency automation with GitHub Actions and Dependabot.

## 0.7.0 beta - Local-First AI Dictation

- Contextual capitalization that can inspect text before the cursor and lowercase mid-sentence dictation.
- Voice input for dictionary and snippet fields.
- Smarter auto-learn detection using anchored spans, better word alignment, duplicate-session guards, and safer promotion rules.
- Relevant dictionary prompting so cleanup prompts prioritize matching dictionary entries.
- Manual update controls in About and a home update banner.
- Offline awareness with connectivity checks and a home-screen indicator.
- Pipeline hardening across transcription, cleanup, snippets, dictionary substitution, and injection.
- Audio reliability improvements with reduced buffer churn and hardened mono mixing.
- Database migration cleanup with `user_version` gating and safer SQLite lock handling.
- Theme system cleanup in `theme.css`.
- Expanded Rust unit and Playwright smoke test coverage.

## 0.6.0 beta - Local-First AI Dictation

- Redesigned setup and quick settings layout with a wider two-column grid.
- Stronger prompt injection protection with strict `<raw_dictation>` tag isolation.
- Unified quota error handling for API fallback behavior.
- Enhanced auto-learn dictionary with Windows UI Automation COM guards, fallbacks, and a longer monitor window.
- Inline audio transcription request support.
- Clipboard and hotkey hardening for Windows Unicode handling and registration failure paths.
- Robust update parsing for normalized release tags and comparisons.

## 0.5.0 beta - Local-First AI Dictation

- In-app update checks and one-click install.
- Microphone gain control with adjustable boost for quiet microphones.
- Dictionary substitution fixes for empty patterns and UTF-8 mixed-case behavior.
- Snippet period deduplication fix.
- Pill UI processing animation upgrade.

## 0.4.2 beta - Local-First AI Dictation

- Initial public Open Flow beta release notes.
- Windows desktop dictation app with hold-to-record hotkey, multiple AI providers, real-time recording indicator, cleanup profiles, clipboard injection, snippets, transcription history, settings, themes, and local-first storage.

## Related Docs

<p align="center">
  <a href="RELEASE.md"><img alt="Release Process" src="https://img.shields.io/badge/Release-Process-c44632"></a>
  <a href="README.md"><img alt="Docs Index" src="https://img.shields.io/badge/Docs-Index-5b554a"></a>
  <a href="../installers/README.md"><img alt="Installers" src="https://img.shields.io/badge/Installers-Layout-7e7266"></a>
</p>
