# Dictionary

Your Dictionary is your personal vocabulary list. Add words or phrases the AI should know — names, brands, jargon, anything niche — and they get used in every transcription so Verenu recognizes them and spells them the way you want.

## Adding entries manually

Each entry has two parts:

- **Term** — the correct spelling you want Verenu to use (e.g. "Kubernetes", "Björk", "ChatGPT")
- **Often mistranscribed as** *(optional)* — what the transcription model tends to write instead (e.g. "koobernetes", "byork")

You don't need to fill in the second field if you just want Verenu to be aware of a term — only add it if you know a specific mistake the AI tends to make.

## How corrections are applied

When Verenu sees a known mistake in a transcription, it replaces it with your correct term — matching whole words only, so a partial match inside another word (like "kube" inside "kubelet") won't get swapped by accident.

## Auto-learn

Verenu can also learn corrections automatically — no manual entry required. When this is enabled, Verenu watches what happens to the text it pastes for a short time afterward. If you consistently retype a word it got wrong, Verenu notices the pattern and can add it to your Dictionary on its own.

- **Distinctive terms** — brand names, technical terms, anything unusual — can be learned after just **one** correction.
- **Ordinary words** need to be corrected the same way **twice within a couple of days** before Verenu learns them, to avoid learning from a one-off typo.

**Safeguard**: common everyday words are never silently auto-replaced everywhere in your text. Only distinctive terms get this automatic find-and-replace treatment. If Verenu learns an ambiguous correction involving a common word, it's applied contextually during cleanup instead — so it won't accidentally rewrite every legitimate use of that word.

## Next step

Set up [Snippets](SNIPPETS.md) for phrases you say often, or [App Mappings & Profiles](APP_MAPPINGS.md) for per-app behavior.
