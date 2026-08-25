# Contexts

Contexts are named groups for the apps and websites where you dictate. Each group can have its own tone, cleanup intensity, custom instructions, vocabulary, and snippets.

## Why use this

Contexts replaced the old standalone App Mappings, Dictionary, and Snippets pages as the main way to configure app-specific behavior. A single group keeps its targets, cleanup preferences, vocabulary, and snippets together, so a "Work" setup can follow you across the apps and websites where you need it.

Every install starts with an **Everywhere** context group — the fallback used when nothing more specific matches — plus whatever groups you create.

## Setting up a context group

1. Open **Contexts** in the main navigation and click **New context group**.
2. Give it a name (30 characters max) and, optionally, an icon and color.
3. Attach apps and/or websites — the group activates automatically when you dictate into one of them.
4. Optionally override the **tone** and **cleanup intensity** for this group; leave either as "Use default" to fall back to your global setting.
5. Add vocabulary and snippets scoped to this group from its Vocabulary and Snippets tabs.

## App and website matching

- Apps are matched by executable name (Windows) or bundle identifier (macOS).
- Websites are matched by domain. When you dictate inside a supported browser, Verenu reads the active tab's address bar to determine the domain — no browser extension required.
- A domain you add is checked for DNS existence before it's accepted, so a typo can't silently create a website target that will never match anything.
- An app or website can only belong to one context group at a time; assigning it to a new group removes it from its previous one.

## Legacy pages

The standalone App Mappings, Dictionary, and Snippets pages are hidden by default. Turn on **Settings → General → Legacy pages** to bring them back for an older setup or a one-off migration. New configuration belongs in Contexts.

## Next step

Check [Cleanup Levels](CLEANUP_LEVELS.md) to choose the right cleanup intensity for a context group, or [Privacy & Data](PRIVACY_SUMMARY.md) to understand what a website check sends off device.

## Related Docs

<p align="center">
  <a href="CLEANUP_LEVELS.md"><img alt="Cleanup Levels" src="https://img.shields.io/badge/Cleanup-Levels-c44632"></a>
  <a href="PRIVACY_SUMMARY.md"><img alt="Privacy Summary" src="https://img.shields.io/badge/Privacy-Summary-2b2422"></a>
</p>
