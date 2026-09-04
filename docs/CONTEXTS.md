# Contexts

Contexts are Verenu's main way to control what happens in different places. A context group connects a set of apps or websites with the tone, cleanup level, instructions, vocabulary, and snippets you want there.

For example, you might create:

- **Development** for your editor and terminal, with Off cleanup and technical vocabulary.
- **Writing** for your document apps, with Medium cleanup and a formal tone.
- **Support** for your help desk, with custom instructions for dates, names, and reply style.

## How context matching works

Verenu chooses one active context for each dictation:

1. If the active browser tab matches a context website, that website context wins.
2. Otherwise, if the foreground app matches a context app, that app context wins.
3. If nothing matches, Verenu uses **Everywhere**.

Website matching is more specific than app matching. A website target such as `mail.google.com` can therefore use a different context from the rest of the browser. Verenu reads the active tab's domain from the browser address bar. No browser extension is required.

The app target is the executable name on Windows or the bundle identifier on macOS. Context targets are normalized when saved. A website is normalized to its hostname, and Verenu checks that the domain exists before saving it.

## The Everywhere context

Every install has an **Everywhere** context. It is the fallback when no app or website context matches.

Everywhere can contain vocabulary and snippets that should apply to ordinary dictation. It cannot have app or website targets because it already covers everything else.

## Create a context

1. Open **Contexts** from the sidebar and choose **New context group**.
2. Enter a name. Names can be up to 30 characters.
3. Optionally choose an icon and color.
4. Choose a **Tone** override, or leave it at **Use default**.
5. Choose a **Cleanup** override, or leave it at **Use default**.
6. Add **Custom instructions** if this context needs a rule the normal cleanup settings do not cover. The field accepts up to 300 characters and is sent to the cleanup model whenever the context is active.
7. Add apps and websites. You can add more from the context page later.
8. Save the context group.

The **Advanced** section has one context-specific option, **Disable smart formatting**. It turns off automatic spacing and capitalization for that context. Use it for destinations where Verenu should leave those decisions alone.

## Add content to a context

Each context has two tabs:

- [Vocabulary](VOCABULARY.md) contains words and phrases Verenu should recognize or correct.
- [Snippets](SNIPPETS.md) contains spoken triggers that expand into saved text.

You can assign the same vocabulary entry or snippet to more than one context. Use a row's menu and choose **Move to...** when you want to remove it from the current context and place it in another one.

Deleting a context removes its app and website targets. Its vocabulary and snippets are returned to Everywhere so they are not orphaned.

## Change an existing context

Select a context in the sidebar and use its context menu to edit, pin, recolor, or delete it. The context page shows its targets, word and dictation counts, vocabulary, and snippets.

To move an app or website to another context, add it to the new context. Each executable and each hostname can belong to only one targeted context at a time. Assigning an existing target elsewhere moves that target.

## When to use the old pages

The standalone App Mappings, Dictionary, and Snippets pages are legacy compatibility pages. They are hidden by default under **Settings -> General -> Legacy pages**. Use Contexts for new setup and for managing existing content by location.

## Related docs

<p align="center">
  <a href="VOCABULARY.md"><img alt="Vocabulary" src="https://img.shields.io/badge/Vocabulary-Guide-a3352b"></a>
  <a href="SNIPPETS.md"><img alt="Snippets" src="https://img.shields.io/badge/Snippets-Guide-c44632"></a>
  <a href="CLEANUP_LEVELS.md"><img alt="Cleanup Levels" src="https://img.shields.io/badge/Cleanup-Levels-7e7266"></a>
  <a href="PRIVACY_SUMMARY.md"><img alt="Privacy Summary" src="https://img.shields.io/badge/Privacy-Summary-2b2422"></a>
</p>
