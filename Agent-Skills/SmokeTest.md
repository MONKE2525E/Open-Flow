---
name: smoke-tests
description: Guides any AI agent through making the Open Flow smoke tests pass by fixing app code — never the tests.
---

# Smoke Tests — Open Flow

You are helping make the Open Flow smoke tests pass. Read every rule before touching any code.

## Hard Rules — No Exceptions

1. **Never edit any file inside `tests/smoke/`** unless the user explicitly grants permission for a specific session.
2. **Never fake functionality.** Do not hardcode expected values, hide elements, or add invisible nodes for Playwright. Every assertion must reflect real, working UI.
3. **Fix the app.** The Svelte components in `src/lib/views/` and `src/lib/components/` are where the work happens.

---

## The Six Tests and Their Expected Results

### `test.cjs` — port 5173 (Vite dev server)

Loads the app, waits for at least one `.nav-item` to appear, asserts at least 4 nav items exist and `.app` root is visible. Saves `screenshot.png`. Fails on any JS page errors.

**Pass criteria:** No JS errors on load, ≥4 `.nav-item` elements, `.app` root visible, screenshot writes.

---

### `test-app.cjs` — port 5173 (Vite dev server)

Clicks `.nav-item:has-text("Style")`, then `text=App Mappings`. Fills `input[placeholder="e.g. slack.exe"]` with `chrome.exe`, picks `casual`, clicks `button:has-text("Add Mapping")`, then waits for `text=chrome.exe` to appear.

**Pass criteria:** Style nav, App Mappings tab, and the add flow all work end-to-end. The mapping must render in the list via the real store/backend.

---

### `playwright-test-ui.cjs` — port 1420 (Tauri window)

Clicks each sidebar nav item and asserts the **view actually changes** by waiting for the corresponding `h1.page-h`:

| Nav label  | Expected heading  |
|------------|-------------------|
| Home       | `Welcome back`    |
| Dictionary | `Dictionary`      |
| Snippets   | `Snippets`        |
| Style      | `Style`           |

Then opens Settings, clicks each of the 6 section nav items and asserts `h2.settings-h` renders. Navigates to Privacy, finds all `.toggle` elements, reads `aria-checked` before each click, clicks, asserts `aria-checked` changed. Closes settings via `(10, 10)` click and asserts `.settings-modal` is hidden.

**Pass criteria:**
- All nav clicks produce the correct heading (not just no crash)
- Settings section clicks produce the correct `h2.settings-h`
- Each toggle's `aria-checked` flips on click
- `.settings-modal` becomes hidden after outside click

---

### `playwright-test-fixes.cjs` — port 1420 (Tauri window)

Uses `waitFor({ state: 'visible' })` (not `waitForTimeout`) on every element. Asserts:

| Location             | Selector                                              |
|----------------------|-------------------------------------------------------|
| Settings → Advanced  | `div.badge:has-text("30 days")`                       |
| Settings → Advanced  | `div.badge:has-text("Clipboard (Ctrl+V)")`            |
| Settings → General   | `button.badge.key-badge:has-text("Ctrl")`             |
| Settings → About     | `button.btn-ghost:has-text("github.com/MONKE2525E/Open-Flow")` |

**Pass criteria:** All four elements exist with the exact tag+class shown. A `<span class="badge">` won't satisfy `div.badge`; a `<button>` without `.btn-ghost` won't satisfy the About check.

---

### `playwright-test-state.cjs` — port 1420 (Tauri window)

Tests that state changes actually persist through a settings close/reopen cycle:

1. All 3 transcription and 3 cleanup model buttons render as `.model-row` elements.
2. At least one `.model-row.active` exists.
3. Clicking a non-active model row makes it active; closing and reopening settings shows the new selection still active.
4. Clicking a `.toggle` in Advanced changes `aria-checked`; closing and reopening settings shows the same value.
5. Restores original state at the end.

**Pass criteria:** Model selection and toggle state both survive a settings close/reopen cycle — proving the Tauri store write is happening.

---

### `playwright-test-pipeline.cjs` — no browser (Node.js only)

Standalone test that hits the API directly. Does **not** require `npm run tauri dev`.

1. Verifies `tests/smoke/smoke_test.wav` exists and has a valid `RIFF/WAVE` header.
2. Reads the store at `%APPDATA%\com.openflow.app\settings.json` to find configured API keys.
3. If a Groq or OpenAI key is found: calls the transcription API with the WAV and verifies non-empty output.
4. If transcript is obtained and Groq key exists: runs cleanup for all three profiles (`casual`, `formal`, `very_casual`) and checks:
   - Each profile returns non-empty text.
   - Profiles produce **distinct** outputs (differentiation is real).
   - `formal` output contains no contractions.
   - `very_casual` output starts with a lowercase word (only `I` may be uppercase).
5. If an OpenAI key is also present: runs a bonus gpt-4o-mini cleanup check.

**Pass criteria (with keys):** Transcription non-empty, all profiles distinct, format rules enforced.
**Skip (without keys):** Reports `SKIP` and exits 0 — not a failure.

---

## CSS Class Checklist

When fixing components, verify these classes are applied exactly as written:

- Sidebar nav buttons → `nav-item`
- App root div → `app`
- View headings → `h1.page-h` with the view's name as text
- Settings container → `settings-modal`
- Settings section buttons → `settings-nav-item`
- Settings section headings → `h2.settings-h` with the section name as text
- Privacy / Advanced toggles → `toggle` + `role="switch"` + `aria-checked` attribute
- Info badges (div) → `badge` on a `<div>`
- Hotkey badge → `badge key-badge` on a `<kbd>` element
- About GitHub button → `btn-ghost` on a `<button>`
- Model buttons → `model-row` (active selection adds `active`)

---

## How to Run

```bash
# Port-5173 tests — Vite dev server
npm run dev            # terminal 1
node tests/smoke/test.cjs
node tests/smoke/test-app.cjs

# Port-1420 tests — full Tauri window
npm run tauri dev      # terminal 1
node tests/smoke/playwright-test-ui.cjs
node tests/smoke/playwright-test-fixes.cjs
node tests/smoke/playwright-test-state.cjs

# Pipeline test — no server needed
node tests/smoke/playwright-test-pipeline.cjs
```

All tests exit with code 0 on pass and code 1 on failure so they integrate cleanly with CI.

---

## smoke_test.wav

`tests/smoke/smoke_test.wav` is gitignored. The user provides this separately as an ElevenLabs-generated voice recording. It must be a real WAV file (RIFF/WAVE header, ≥5 KB). See the user for the recording script.

---

## Slash Commands

- **Claude Code**: `/smoke-tests`
- **Gemini CLI**: `/smoke-tests`
- **Codex**: `/smoke-tests`
