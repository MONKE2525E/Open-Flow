# Release Process

This document is the practical release checklist for Verenu. For release note wording, use [`../Agent-Skills/Release_Description_Writing.md`](../Agent-Skills/Release_Description_Writing.md).

## Branch Flow

1. Merge reviewed pull requests directly into `master`.
2. Keep `master` green with the required CI checks.
3. The scheduled morning nightly release inspects `master` and publishes the
   next prerelease when enough changes have accumulated.
4. Use the manual installer workflow when an on-demand build is needed.

## Version Bump

Update these three files together:

- [`../package.json`](../package.json)
- [`../src-tauri/tauri.conf.json`](../src-tauri/tauri.conf.json)
- [`../src-tauri/Cargo.toml`](../src-tauri/Cargo.toml)

The version strings must match exactly. The frontend reads the version dynamically through Tauri, so do not hardcode release versions in Svelte files.

## Pre-Release Checks

Run the practical local gate:

```bash
npm install
npm run check
npm run lint
npm test
npm run test:rust
```

For UI-affecting work, also run the relevant Playwright smoke or integration tests. For provider, permission, injection, updater, hotkey, or installer changes, run the matching platform checks. Do not pretend a desktop integration was tested if it was only type-checked.

## Build Installers

The manual GitHub Actions workflow [`../.github/workflows/build-installers.yml`](../.github/workflows/build-installers.yml) checks out `master` and builds:

- Windows NSIS installer
- Windows MSI installer
- macOS Apple Silicon DMG
- macOS Intel DMG

The release folder should contain:

- `Verenu_<version>_x64-setup.exe`
- `Verenu_<version>_x64_en-US.msi`
- `Verenu_<version>_Apple_Silicon.dmg`
- `Verenu_<version>_Intel.dmg`
- `SHA256SUMS.txt`

Installer artifacts are committed under [`../installers/`](../installers/) with a versioned subfolder and also attached to the GitHub Release.

## Hashes

After placing the installers in the correct version folder under [`../installers/`](../installers/), regenerate hashes from that folder:

```bash
shasum -a 256 *.exe *.msi *.dmg > SHA256SUMS.txt
```

On Windows PowerShell, generate the same file with:

```powershell
Get-ChildItem *.exe,*.msi,*.dmg | ForEach-Object {
  $hash = (Get-FileHash $_ -Algorithm SHA256).Hash.ToLower()
  "{0} *{1}" -f $hash, $_.Name
} | Set-Content SHA256SUMS.txt
```

On Windows PowerShell, verify a file manually with:

```powershell
Get-FileHash .\Verenu_<version>_x64-setup.exe -Algorithm SHA256
```

The hashes in `SHA256SUMS.txt`, the committed files, GitHub Release assets, and release notes must agree.

## Release Notes

Release notes should include:

- A plain title: `Verenu <version> - <tagline>`
- A short app blurb.
- Version-specific changes.
- Evergreen features.
- Getting started steps.
- Default shortcuts.
- Lightweight and local positioning.
- VirusTotal links for all four installer files.

Do not include API keys, local paths, private screenshots, private user text, or maintainer-only secrets in release notes.

## Post-Release Checks

- Download each uploaded installer from GitHub Releases.
- Verify each hash against `SHA256SUMS.txt`.
- Open the app on Windows and macOS when possible.
- Confirm first-run setup can save an API key without exposing it.
- Confirm the release docs and installer folder match the shipped version.

## Related Docs

<p align="center">
  <a href="CHANGELOG.md"><img alt="Changelog" src="https://img.shields.io/badge/Project-Changelog-c44632"></a>
  <a href="TESTING.md"><img alt="Testing" src="https://img.shields.io/badge/Testing-Guide-5b554a"></a>
  <a href="macos-code-signing.md"><img alt="macOS Signing" src="https://img.shields.io/badge/macOS-Signing-7e7266"></a>
  <a href="README.md"><img alt="Docs Index" src="https://img.shields.io/badge/Docs-Index-2b2422"></a>
</p>
