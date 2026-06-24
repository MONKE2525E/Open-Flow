# Installers

Prebuilt Verenu installers, committed here so anyone can grab a release
directly from the repository without waiting on a maintainer. Each release has
its own version folder (e.g. [`0.15.0/`](0.15.0/)).

The same files are also attached to the matching
[GitHub Release](https://github.com/MONKE2525E/Verenu/releases), which remains
the primary, scannable download location (every release lists VirusTotal links
in its notes).

## Files per release

| File | Platform |
|---|---|
| `Verenu_<version>_x64-setup.exe` | Windows (NSIS installer) |
| `Verenu_<version>_x64_en-US.msi` | Windows (MSI installer) |
| `Verenu_<version>_Apple_Silicon.dmg` | macOS (Apple Silicon / arm64) |
| `Verenu_<version>_Intel.dmg` | macOS (Intel / x64) |

## Verifying a download

Each version folder includes a `SHA256SUMS.txt`. After downloading, verify the
file is intact:

```bash
# from inside the version folder
sha256sum -c SHA256SUMS.txt
```

```powershell
# Windows PowerShell
Get-FileHash .\Verenu_0.15.0_x64-setup.exe -Algorithm SHA256
```

## Note on macOS signing

The macOS `.dmg` builds are ad-hoc signed (not Developer ID signed or
notarized), so Gatekeeper may require right-click → **Open** or
**System Settings → Privacy & Security → Open Anyway** on first launch.
