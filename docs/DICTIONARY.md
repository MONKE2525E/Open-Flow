# Legacy: Dictionary

> **Legacy page.** The standalone Dictionary page is hidden by default. Manage the same vocabulary per context group from the [Contexts](CONTEXTS.md) page's Vocabulary tab. Turn on **Settings -> General -> Legacy pages** to bring this page back for an older setup.

The Dictionary is Verenu's personal vocabulary list. Add names, brands, jargon, or other terms the AI should recognize and spell correctly. Entries apply wherever their context is active. The built-in **Everywhere** context is the fallback for entries that should apply across your dictation.

## Adding entries manually

Each entry has two parts:

- **Term**: the correct spelling you want Verenu to use, such as "Kubernetes", "Bjork", or "ChatGPT".
- **Often mistranscribed as** (optional): what the transcription model tends to write instead, such as "koobernetes" or "byork".

You can leave the second field empty if you only want Verenu to know a term. Add it when you know a specific mistake the AI makes.

## How corrections are applied

When Verenu sees a known mistake in a transcription, it replaces it with the correct term and matches whole words only. A partial match inside another word, such as "kube" inside "kubelet", is not replaced by accident.

## Auto-learn

Verenu can learn corrections automatically. When this is enabled, it watches what happens to the text it pastes for a short time afterward. If you consistently retype a word it got wrong, Verenu can add the correction to the active context's vocabulary.

- **Distinctive terms**, such as brand names and technical terms, can be learned after one correction.
- **Ordinary words** need to be corrected the same way twice within a couple of days before Verenu learns them.

Common everyday words are not silently replaced everywhere. Distinctive terms can use automatic find-and-replace. Ambiguous corrections are applied contextually during cleanup instead.

## Next step

For current app- and website-specific vocabulary, use the Vocabulary tab inside [Contexts](CONTEXTS.md).

## Related Docs

<p align="center">
  <a href="CONTEXTS.md"><img alt="Contexts" src="https://img.shields.io/badge/Contexts-Guide-a3352b"></a>
  <a href="CLEANUP_LEVELS.md"><img alt="Cleanup Levels" src="https://img.shields.io/badge/Cleanup-Levels-7e7266"></a>
  <a href="README.md"><img alt="Docs Index" src="https://img.shields.io/badge/Docs-Index-2b2422"></a>
</p>
