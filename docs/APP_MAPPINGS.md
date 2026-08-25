# Legacy: App Mappings & Profiles

> **Legacy page.** App Mappings has been superseded by [Contexts](CONTEXTS.md), which covers the same per-app (and per-website) tone/cleanup overrides plus vocabulary and snippets in one place. The standalone App Mappings page is hidden by default; turn on **Settings → General → Legacy pages** to bring it back if you still rely on it directly.

App Mappings let Verenu automatically switch its tone — and optionally its cleanup intensity — based on which app you're dictating into.

## Why use this

The way you write a Slack message is different from the way you write an email or a line of code. Instead of changing your settings every time you switch apps, you set it up once per app and Verenu handles the rest.

## Setting up a mapping

1. Turn on **Settings → General → Legacy pages**, then open **Settings → App Mappings**.
2. Add an app — pick from your installed apps, or enter one manually.
3. Choose a **tone** for that app: Casual, Formal, or Very Casual.
4. Optionally, choose a **cleanup intensity** override (Verbatim, Light, Medium, or Direct) — or leave it as "Default" to use your global cleanup setting.

## Example setup

| App | Tone | Cleanup intensity |
| --- | --- | --- |
| Email client | Formal | Light |
| Chat / messaging apps | Casual | Medium |
| Code editor | — | Verbatim |

With this setup, dictating into your email client produces polished, professional text with light editing, while dictating into your code editor leaves your words untouched — perfect for comments or commit messages where exact wording matters.

## How app detection works

Verenu identifies the app you're dictating into by its executable name (Windows) or application bundle (macOS) — the same name shown next to each entry in App Mappings. If an app isn't in your installed-apps list, you can still add it manually.

## Next step

Revisit [Cleanup Levels](CLEANUP_LEVELS.md) to choose the right intensity for each app, or check [Privacy & Data](PRIVACY_SUMMARY.md) to understand what Verenu does with your data.

## Related Docs

<p align="center">
  <a href="CONTEXTS.md"><img alt="Contexts" src="https://img.shields.io/badge/Contexts-Guide-a3352b"></a>
  <a href="CLEANUP_LEVELS.md"><img alt="Cleanup Levels" src="https://img.shields.io/badge/Cleanup-Levels-c44632"></a>
  <a href="PRIVACY_SUMMARY.md"><img alt="Privacy Summary" src="https://img.shields.io/badge/Privacy-Summary-2b2422"></a>
</p>
