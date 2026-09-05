# Setup wizard screenshots

Screenshots for the "how do I get an API key?" tutorial on step 2 of the first-run
wizard (`src/lib/setup/steps/ApiKeyStep.svelte`).

## How it works

`ApiKeyStep.svelte` picks these up with
`import.meta.glob('/src/assets/setup/*.png', { eager: true, query: '?url', import: 'default' })`.

The glob is **root-absolute on purpose** — a `'../../../assets/setup/*.png'` relative
glob does not resolve from inside a `.svelte` module and silently matches nothing.

Nothing is imported by name, so **missing files are not an error** — a slot with no
image renders a placeholder frame. Drop a PNG in with the right filename and it
appears on the next dev-server start. (Vite resolves the glob at startup; adding the
first file to an empty folder needs a restart, not just a reload.)

## Filenames

`<provider>-<step>-<slug>.png` — the step number is what matters, the slug is for
humans. Captions come from `providerGuides[provider].steps` in
`src/lib/setup/setupData.ts`, and pair 1:1 with these by index. If you change one,
change the other.

```
groq-1-signin.png       groq-2-keys.png       groq-3-create.png       groq-4-copy.png
google-1-signin.png     google-2-apikey.png   google-3-create.png     google-4-copy.png
openai-1-signin.png     openai-2-keys.png     openai-3-create.png     openai-4-copy.png
```

`local` has no tutorial (no key required) and needs no images.

OpenAI's four cover key creation only. Adding a billing card is deliberately not
shown — that flow changes often and is outside what the wizard is teaching.

## Capturing and annotating

Current set: 1200×675 (16:9 at 2x for the carousel frame), cropped tightly around
the relevant provider panel. The target uses a 3px coral outline and soft halo,
with one short instruction label and an arrow that ends beside the control.
Instruction labels never cover the control being explained.

The crop is **not** expanded to 16:9 — it is scaled to fit and letterboxed onto a
band sampled from the crop's own edge. Every provider's key list is mostly empty
below the fold, so growing the crop to reach 16:9 shrank the action to a postage
stamp in a sea of blank page.

- **Crop to the relevant panel**, not the whole desktop, and don't start a crop
  mid-sentence — cut-off text reads as a broken image.
- **Redact every key-shaped value with an opaque block.** Blur is not sufficient;
  these images ship inside the installer.
- Use the same opaque treatment for account emails, organization names, project
  names/numbers, billing details, and other account identifiers.
- Either theme is fine; the frame has a neutral border and does not tint the image.
- Export as an indexed PNG and keep each image under approximately 200KB. They are
  bundled into the app binary.

Before replacing an asset, compare it with the original at full size. Reject an
edit if provider copy, controls, layout, or theme changed. The annotation and
privacy redactions are the only pixels that may intentionally diverge.
