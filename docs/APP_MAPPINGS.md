# App Mappings & Profiles

App Mappings let Verenu automatically switch its tone — and optionally its cleanup intensity — based on which app you're dictating into.

## Why use this

The way you write a Slack message is different from the way you write an email or a line of code. Instead of changing your settings every time you switch apps, you set it up once per app and Verenu handles the rest.

## Setting up a mapping

1. Open **Settings → App Mappings**.
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

Revisit [Cleanup Levels](CLEANUP_LEVELS.md) to choose the right intensity for each app, or check [Privacy & Data](PRIVACY_AND_DATA.md) to understand what Verenu does with your data.

## Related Docs

<p align="center">
  <a href="CLEANUP_LEVELS.md"><img alt="Cleanup Levels" src="https://img.shields.io/badge/Cleanup-Levels-c44632"></a>
  <a href="DICTIONARY.md"><img alt="Dictionary" src="https://img.shields.io/badge/Dictionary-Guide-5b554a"></a>
  <a href="SNIPPETS.md"><img alt="Snippets" src="https://img.shields.io/badge/Snippets-Guide-7e7266"></a>
  <a href="PRIVACY_AND_DATA.md"><img alt="Privacy Summary" src="https://img.shields.io/badge/Privacy-Summary-2b2422"></a>
</p>
