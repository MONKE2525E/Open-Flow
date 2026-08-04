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

Current set: 1200×675 (16:9 at 2x for the carousel frame), cropped tight on the
action, with the target ringed and a short instruction in the margin joined by an
arrow. Instruction pills sit *beside* the control, never over it.

The crop is **not** expanded to 16:9 — it is scaled to fit and letterboxed onto a
band sampled from the crop's own edge. Every provider's key list is mostly empty
below the fold, so growing the crop to reach 16:9 shrank the action to a postage
stamp in a sea of blank page.

- **Crop to the relevant panel**, not the whole desktop, and don't start a crop
  mid-sentence — cut-off text reads as a broken image.
- **Redact every real key.** Blur or overwrite the secret before saving — these ship
  inside the installer.
- Also redact account emails, org names, and billing details.
- Either theme is fine; the frame has a neutral border and does not tint the image.
- Keep them under ~200KB each. They are bundled into the app binary.
