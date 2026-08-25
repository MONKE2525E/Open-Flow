# Vocabulary

Vocabulary tells Verenu about words that transcription models often miss, such as names, brands, product names, and technical terms.

Vocabulary belongs to a context. Add it to **Everywhere** when it should apply to all dictation, or add it to a specific context when it only belongs in certain apps or websites.

## Add a vocabulary entry

1. Open **Contexts** and select the context where the term belongs.
2. Open the **Vocabulary** tab.
3. Choose **+ Term**.
4. Enter the exact **Term** you want Verenu to use.
5. Optionally enter one or more values in **Often mistranscribed as**. Separate multiple mistakes with commas.
6. Save the term.

Terms and mistranscriptions can each be up to 120 characters.

The term field is the right choice when you want the cleanup model to recognize a word. The mistranscription field is useful when the transcription model repeatedly produces a specific wrong spelling. Leave it empty when there is no consistent mistake to replace.

You can dictate into either field with the small microphone button beside it.

## How vocabulary is used

When a context is active, Verenu uses its vocabulary while preparing and cleaning the dictation. Distinctive corrections can also be applied mechanically after cleanup. Common, ambiguous corrections are handled contextually so a learned correction does not rewrite every ordinary use of a word.

An entry can belong to several contexts. To reuse one, add it to another context rather than creating a duplicate. To remove it from the current context without deleting it, open the row menu and choose **Move to...**.

Deleting the entry removes it from every context. Deleting a context moves its entries to Everywhere instead.

## Auto-learned vocabulary

When Auto-learn is enabled, Verenu watches the focused text field for corrections after a dictation. Repeated corrections can become vocabulary entries automatically.

- Distinctive terms, such as brand names and technical words, can be promoted after one high-confidence correction.
- Ordinary words need repeated corrections before Verenu promotes them.
- Auto-learned entries show an indicator and confidence information in the Vocabulary list.

Auto-learn monitors the text field after insertion. It does not send the monitoring data to a Verenu server.

## Vocabulary or snippet?

Use **Vocabulary** when the output should stay in the sentence but use the right spelling or term.

Use a [Snippet](SNIPPETS.md) when a spoken trigger should insert a saved phrase, address, template, or block of text.

## Legacy page

Older installations may still show the standalone Dictionary page. It is hidden by default and is not the main setup path. Turn on **Settings -> General -> Legacy pages** only when you need to maintain an older view of the same local vocabulary data.

## Related docs

<p align="center">
  <a href="CONTEXTS.md"><img alt="Contexts" src="https://img.shields.io/badge/Contexts-Guide-a3352b"></a>
  <a href="SNIPPETS.md"><img alt="Snippets" src="https://img.shields.io/badge/Snippets-Guide-c44632"></a>
  <a href="DATA_AND_PRIVACY.md"><img alt="Data and Privacy" src="https://img.shields.io/badge/Data-Privacy-5b554a"></a>
</p>
