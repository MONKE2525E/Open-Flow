---
name: smoke-tests
description: Guides any AI agent through making the Open Flow Playwright smoke tests pass by fixing app code — never the tests.
---

# Smoke Tests — Open Flow

You are helping make the Open Flow Playwright smoke tests pass. Read every rule before touching any code.

## Hard Rules — No Exceptions

1. **Never edit any file inside `tests/smoke/`.** This includes test.js, test-app.js, playwright-test-ui.cjs, playwright-test-fixes.cjs, and smoke_test.wav. The tests are the contract. They do not change.
2. **Never fake functionality.** Do not hardcode expected values into the DOM, do not hide elements that only appear for Playwright, do not add invisible nodes to satisfy a selector. Every passing assertion must reflect real, working UI.
3. **Fix the app.** The Svelte components in `src/lib/views/` and `src/lib/components/` are where the work happens. CSS class names in the components must exactly match what the tests assert.

---

## The Four Tests and Their Expected Results

### `test.js` — port 5173 (Vite dev server)
Loads the app, waits 2 s, checks that `document.body.innerHTML` contains the string `"app"`, saves a screenshot to `G:\Open Flow\screenshot.png`.

**Pass criteria:** No page-level JS errors, body contains content, screenshot writes without throwing.

---

### `test-app.js` — port 5173 (Vite dev server)
Opens the **Style** tab, then clicks the **App Mappings** sub-tab. Then:
1. Finds `input[placeholder="e.g. slack.exe"]`
2. Types `chrome.exe`, picks `casual` from the `<select>`, clicks `button:has-text("Add Mapping")`
3. Asserts `text=chrome.exe` appears on the page

**Pass criteria:** The App Mappings UI exists with those exact placeholder/select/button elements, and adding a mapping makes it visible in the list. The mapping must be real — it must go through the actual store or backend, not just render a static string.

---

### `playwright-test-ui.cjs` — port 1420 (Tauri window)
Clicks each sidebar nav item by class+text: `.nav-item:has-text("Home")`, `.nav-item:has-text("Dictionary")`, `.nav-item:has-text("Snippets")`, `.nav-item:has-text("Style")`.

Then clicks `.nav-item:has-text("Settings")` and asserts `.settings-modal` is visible.

Inside Settings, clicks each section: `.settings-nav-item:has-text("General")`, `"API Keys"`, `"Models"`, `"Privacy"`, `"Advanced"`, `"About"`.

On the Privacy page, finds all `.toggle` elements and clicks each one. Closes by clicking at coordinates `(10, 10)` (outside the modal).

**Pass criteria:**
- All four main nav items use class `nav-item`
- Settings opens a container with class `settings-modal`
- All six settings sections use class `settings-nav-item`
- Privacy page has at least one element with class `toggle`
- Clicking `(10, 10)` (outside the modal) closes it

---

### `playwright-test-fixes.cjs` — port 1420 (Tauri window)
Asserts that these exact elements are visible — tag, class, and text must all match:

| Location | Selector |
|---|---|
| Settings → Advanced | `div.badge` containing text `"30 days"` |
| Settings → Advanced | `div.badge` containing text `"Clipboard (Ctrl+V)"` |
| Settings → General | `kbd.badge.key-badge` containing text `"Alt Space"` |
| Settings → About | `button.btn-ghost` containing text `"github.com/MONKE2525E/Open-Flow"` |

**Pass criteria:** All four elements present with the exact tag+class combination shown above. A `<span class="badge">` will not satisfy `div.badge`. A `<button>` without class `btn-ghost` will not satisfy the About check.

---

## CSS Class Checklist

When fixing components, verify these classes are applied exactly as written:

- Sidebar nav buttons → `nav-item`
- Settings container → `settings-modal`
- Settings section buttons → `settings-nav-item`
- Privacy toggles → `toggle`
- Info badges (div) → `badge`
- Hotkey badge → `badge key-badge` on a `<kbd>` element
- About GitHub button → `btn-ghost` on a `<button>` element

---

## How to Run

```bash
# Port-5173 tests — Vite dev server only
npm run dev            # terminal 1
node tests/smoke/test.js
node tests/smoke/test-app.js

# Port-1420 tests — full Tauri window
npm run tauri dev      # terminal 1
node tests/smoke/playwright-test-ui.cjs
node tests/smoke/playwright-test-fixes.cjs
```

**Note:** `tests/smoke/smoke_test.wav` is gitignored. The user provides this file separately with an AI-generated voice. Do not try to create, generate, or commit it.

---

## Slash Commands
You can invoke this skill across different tools using the following commands:
- **Claude Code**: `/smoke-tests`
- **Gemini CLI**: `/smoke-tests`
- **Codex**: `/smoke-tests`
