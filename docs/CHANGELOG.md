# Changelog

Notable project changes are recorded here. GitHub Release pages remain the source of truth for release metadata.

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
- Tightened the handsfree start chime timing and animated the pill's move across monitors.
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
