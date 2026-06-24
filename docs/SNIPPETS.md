# Snippets

Snippets let you say a short trigger phrase and have Verenu expand it into something longer — a signature, a boilerplate response, a piece of code, anything you type often.

## Creating a snippet

Each snippet has:

- **Trigger** — the phrase you say to activate it. You can list multiple aliases separated by commas (e.g. "Gemini Goal, Gemini Gold") so close transcriptions of the same phrase all work.
- **Expansion** — the text that gets inserted in place of the trigger.
- **Instructions** *(optional)* — extra formatting guidance for how the expansion should be applied.

## How snippets fire

There are two ways a snippet can trigger, depending on what you say:

1. **Whole-dictation match (fast path)** — If your entire dictation is just the trigger phrase, Verenu swaps it directly for the expansion and skips the cleanup step entirely. This is instant.
2. **Trigger within a longer dictation** — If the trigger appears as part of something longer you said, Verenu passes its instructions to the cleanup step, so the expansion is woven into your sentence naturally rather than dropped in verbatim.

## Formatting instructions

You can add plain-language formatting rules in the Instructions field, and Verenu enforces some of these mechanically after cleanup so they always apply:

- **"all caps"** — the result is converted to uppercase.
- **"no period"** — any trailing period is stripped from the result.
- **"end with exclamation"** — the result ends with `!`.

You can also negate these (e.g. "don't use all caps") if a snippet's expansion would otherwise conflict with your tone settings.

## Next step

See [Dictionary](DICTIONARY.md) for vocabulary corrections, or [App Mappings & Profiles](APP_MAPPINGS.md) for per-app tone and cleanup settings.
