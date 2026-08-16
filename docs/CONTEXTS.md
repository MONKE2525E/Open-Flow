# Contexts

Contexts are named groups that tie together everything app- and website-specific: which apps and sites trigger the group, a tone/cleanup override for it, and its own vocabulary (Dictionary) and Snippets — all in one place instead of three separate pages.

## Why use this

Previously, per-app tone lived in App Mappings, vocabulary lived in Dictionary, and expansions lived in Snippets — three unrelated pages you had to keep in sync yourself if, say, your "Work" vocabulary should only apply in your work apps. A context group ties all of that to the same set of apps and sites, so setting it up once covers tone, cleanup, vocabulary, and snippets together.

Every install starts with an **Everywhere** context group — the fallback used when nothing more specific matches — plus whatever groups you create.

## Setting up a context group

1. Open **Contexts** in the main navigation and click **New context group**.
2. Give it a name (30 characters max) and, optionally, an icon and color.
3. Attach apps and/or websites — the group activates automatically when you dictate into one of them.
4. Optionally override the **tone** and **cleanup intensity** for this group; leave either as "Use default" to fall back to your global setting.
5. Add **vocabulary** (Dictionary) and **Snippets** entries scoped to this group from the Vocabulary/Snippets tabs inside it.

## App and website matching

- Apps are matched by executable name (Windows) or bundle identifier (macOS), the same signal App Mappings used.
- Websites are matched by domain. When you dictate inside a supported browser, Verenu reads the active tab's address bar to determine the domain — no browser extension required.
- A domain you add is checked for DNS existence before it's accepted, so a typo can't silently create a website target that will never match anything.
- An app or website can only belong to one context group at a time; assigning it to a new group removes it from its previous one.

## Legacy pages

App Mappings, Dictionary, and Snippets still exist as standalone pages for anyone who prefers managing those separately, but they're hidden by default now that Contexts covers the same ground. Turn on **Settings → General → Legacy pages** to bring them back — see [App Mappings & Profiles](APP_MAPPINGS.md), [Dictionary](DICTIONARY.md), and [Snippets](SNIPPETS.md).

## Next step

Check [Cleanup Levels](CLEANUP_LEVELS.md) to choose the right cleanup intensity for a context group, or [Privacy & Data](PRIVACY_SUMMARY.md) to understand what a website check sends off device.

## Related Docs

<p align="center">
  <a href="APP_MAPPINGS.md"><img alt="App Mappings" src="https://img.shields.io/badge/App-Mappings-5b554a"></a>
  <a href="DICTIONARY.md"><img alt="Dictionary" src="https://img.shields.io/badge/Dictionary-Guide-7e7266"></a>
  <a href="SNIPPETS.md"><img alt="Snippets" src="https://img.shields.io/badge/Snippets-Guide-c44632"></a>
  <a href="PRIVACY_SUMMARY.md"><img alt="Privacy Summary" src="https://img.shields.io/badge/Privacy-Summary-2b2422"></a>
</p>
