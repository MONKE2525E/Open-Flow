# Snippets

Snippets turn a short spoken trigger into saved text. They are useful for email addresses, signatures, boilerplate replies, code fragments, and other text you use often.

Snippets belong to a context. Add them to **Everywhere** for a trigger you want available across Verenu, or add them to a specific context for app- or website-specific text.

## Create a snippet

1. Open **Contexts** and select the context where the snippet belongs.
2. Open the **Snippets** tab.
3. Choose **+ Snippet**.
4. Enter one or more **Trigger** phrases. Separate aliases with commas.
5. Enter the **Expansion** that Verenu should insert.
6. Optionally add **Cleanup instructions** for the expansion.
7. Save the snippet.

Triggers can be up to 300 characters. Expansion and instruction text can contain multiple lines.

For example:

| Field | Example |
| --- | --- |
| Trigger | `my email, email address` |
| Expansion | `hello@example.com` |
| Cleanup instructions | `Do not add a period after this address.` |

You can dictate into the Trigger and Expansion fields with the microphone buttons.

## How a snippet fires

Verenu has two snippet paths:

1. **The whole dictation is the trigger.** Verenu expands it directly and skips the cleanup model. This keeps a simple trigger fast and avoids a punctuation mark being added to the expansion.
2. **The trigger appears inside a longer dictation.** Verenu passes the matched snippet's instructions into cleanup, then expands the trigger in the resulting text. This lets the expansion fit naturally into the sentence.

Matching is case-insensitive and accepts punctuation that transcription may add around the trigger. Triggers must still be standalone phrases. A trigger such as `test` does not match inside `testing` or `pre-test`.

If multiple aliases or snippets match, Verenu prefers the longest matching trigger and ignores overlapping matches.

## Cleanup instructions

Instructions are optional. They are added to the cleanup model's prompt only when the snippet is detected. Verenu also enforces a few narrow formatting rules after cleanup when it can identify them, including:

- all caps
- no final period
- ending with an exclamation mark

Write instructions as a direct request. For example, `Do not add a period after this phrase.`

Use the context's **Custom instructions** when a rule applies to everything you dictate in that context. Use snippet instructions when the rule belongs only to one expansion.

## Reuse and move snippets

A snippet can belong to more than one context. Add the existing snippet to another context when you want to reuse it. From a snippet row menu, choose **Move to...** to remove it from the current context and assign it elsewhere.

Deleting a snippet removes it from every context. Deleting a context moves its snippets to Everywhere instead.

The Snippets list shows the usage count and the date the snippet was created. Search matches both triggers and expansions.

## Related docs

<p align="center">
  <a href="CONTEXTS.md"><img alt="Contexts" src="https://img.shields.io/badge/Contexts-Guide-a3352b"></a>
  <a href="VOCABULARY.md"><img alt="Vocabulary" src="https://img.shields.io/badge/Vocabulary-Guide-c44632"></a>
  <a href="CLEANUP_LEVELS.md"><img alt="Cleanup Levels" src="https://img.shields.io/badge/Cleanup-Levels-7e7266"></a>
</p>
