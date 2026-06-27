# Install Verenu

Verenu is a free, open-source dictation app for Windows and macOS. There's no account, no subscription, and nothing to install beyond the app itself — you bring your own API key for transcription and cleanup (see [Add Your API Key](API_KEYS.md)).

## Windows

1. Download the latest installer from the [GitHub Releases](https://github.com/MONKE2525E/Verenu/releases) page:
   - `Verenu_X.Y.Z_x64_en-US.msi`, or
   - `Verenu_X.Y.Z_x64-setup.exe`
2. Run the installer and follow the prompts.
3. Launch Verenu. The first-run setup walks you through picking a provider and adding your API key.

Requires Windows 10 or 11. Verenu uses the built-in WebView2 runtime — no separate browser engine to install.

## macOS

1. Download the build that matches your Mac from the [GitHub Releases](https://github.com/MONKE2525E/Verenu/releases) page — Apple Silicon or Intel.
2. Move Verenu to your Applications folder and open it.
3. macOS will ask for a few permissions the first time you use Verenu — grant all of them, since dictation won't work correctly without them:
   - **Microphone** — to capture your voice while you hold the hotkey
   - **Accessibility** — to paste cleaned-up text into the app you're using and read text for corrections
   - **Input Monitoring** — to detect the global hotkey (Fn+Control) even when Verenu isn't the focused app

If macOS prompts for your login password when Verenu first saves your API key to Keychain, choose **Always Allow** — this avoids repeated prompts later.

## Build from source

If you'd rather build Verenu yourself:

**Prerequisites**
- Node.js 18+
- Rust and Cargo
- Windows: WebView2 (usually already installed)
- macOS: Xcode Command Line Tools (recommended)

```bash
git clone https://github.com/MONKE2525E/Verenu.git
cd Verenu
npm install
npm run tauri build
```

For development and contribution setup, see [CONTRIBUTING.md](CONTRIBUTING.md).

## If your browser or OS blocks the install

Verenu isn't signed with a paid code-signing certificate, so Windows, macOS, and some browsers may warn you that the installer is from an "unknown publisher" or flag it as potentially unsafe. This is normal for open-source apps distributed outside an app store — here's how to get past each warning.

### Browser download blocks (Chrome / Edge)

When you download the installer, Chrome or Edge may say the file "isn't commonly downloaded" or "could be dangerous":

1. Click the small arrow or **`...`** next to the blocked download in your browser's download bar (or Downloads page).
2. Choose **Keep** (Chrome) or **Keep anyway** (Edge).
3. If prompted again with "Show more" details, choose **Keep anyway** / **Download anyway**.

### Windows SmartScreen

When you run the installer, you may see **"Windows protected your PC"** with a blue "Don't run" button:

1. Click **More info**.
2. Click **Run anyway**.

### macOS Gatekeeper

When you open Verenu for the first time, macOS may say it **"can't be opened because it is from an unidentified developer"**:

- **Easiest**: Right-click (or Control-click) the Verenu app and choose **Open**, then click **Open** again in the confirmation dialog.
- **If that option isn't available** (macOS Sequoia and later sometimes hide it): go to **System Settings → Privacy & Security**, scroll down to the security message about Verenu, and click **Open Anyway**. You may need to enter your password and confirm **Open Anyway** once more.

### Verifying the download yourself

If you want independent confirmation that an installer is clean, every release on the [GitHub Releases](https://github.com/MONKE2525E/Verenu/releases) page includes a **VirusTotal Review** section with a scan link for each installer file. Find the link matching the file you downloaded (e.g. `Verenu_X.Y.Z_x64-setup.exe`) and open it to see the scan results.

## Next step

Once Verenu is installed, continue with [Add Your API Key](API_KEYS.md).

## Related Docs

<p align="center">
  <a href="API_KEYS.md"><img alt="Add API Key" src="https://img.shields.io/badge/Next-Add%20API%20Key-c44632"></a>
  <a href="FIRST_DICTATION.md"><img alt="First Dictation" src="https://img.shields.io/badge/Then-First%20Dictation-5b554a"></a>
  <a href="TROUBLESHOOTING.md"><img alt="Troubleshooting" src="https://img.shields.io/badge/Help-Troubleshooting-7e7266"></a>
  <a href="README.md"><img alt="Docs Index" src="https://img.shields.io/badge/Docs-Index-2b2422"></a>
</p>
