# Code Review & Refactoring Roadmap: Open Flow

**Reviewer**: Grumpy Senior Developer  
**Target Release**: Open Flow 0.11.0 / 0.12.0  
**Target Workspace**: [Open Flow Workspace](file:///g:/Open%20Flow)

---

## 1. Overview & Scoring Metrics

Below is the initial health check and technical debt evaluation of the Open Flow codebase. While the Tauri architecture provides a solid base for desktop integration, the current implementation carries critical performance bottlenecks, race conditions, memory leaks, and architectural coupling that must be addressed.

| Metric | Score | Rating | Summary |
| :--- | :--- | :--- | :--- |
| **Maintainability Score** | **3.8 / 10** | Needs Attention | Global state is fractured between Svelte 4 and Svelte 5 models. Large source files (`pipeline.rs` at 1700+ lines) violate the single-responsibility principle. IPC and database queries frequently swallow errors. |
| **Future Improvement Score** | **4.2 / 10** | Needs Attention | Core features (hotkey tracking, clipboard injection, and UI automation) are heavily coupled to Windows-specific Win32/COM APIs. Language heuristics are hardcoded to English. |
| **Technical Debt Score** | **9.2 / 10** | High Debt | Real-time safety violations occur on OS audio threads. High-frequency process walking consumes CPU resources during idle states. UI Automation polling is inefficient and error-prone. |

---

## 2. Refactoring PR Path (Focused PR Groups)

We have organized the technical debt and usability audit into **11 focused Pull Request groups**. Each PR group contains actionable task chunks with context links, step-by-step implementation instructions, and verification plans.

---

### [PR Group 1: UI - Reactivity & State Modernization]
*   **Goal**: Standardize Svelte state management using Svelte 5 Runes, eliminating double tracking, and ensuring proper IPC error handling.

#### Task 1.1: Migrating `stores.ts` to Svelte 5 Reactive States
*   **Context**: [stores.ts](file:///g:/Open%20Flow/src/lib/stores.ts#L1-L72), [App.svelte](file:///g:/Open%20Flow/src/App.svelte), [Sidebar.svelte](file:///g:/Open%20Flow/src/lib/components/layout/Sidebar.svelte), and view files.
*   **Problem Statement**: The application runs Svelte 5 but manages global state using legacy Svelte 4 `writable` stores. This forces the compiler to wrap subscriptions in compatibility layers, resulting in double tracking and inconsistent access syntax (`$store` vs standard runes).
*   **Actionable Implementation Steps**:
    1. Refactor [stores.ts](file:///g:/Open%20Flow/src/lib/stores.ts) to define a unified, reactive state class or export `$state` objects representing global UI properties: `currentPage`, `settingsOpen`, `dictionary`, `snippets`, `updateInfo`, and `isOnline`.
    2. Replace all instances of `writable` and `$currentPage`, `$settingsOpen`, and `$dictionary` in Svelte components with the new reactive state properties.
    3. Retain the exact HTML and CSS classes asserted by Playwright smoke tests (such as `.nav-item`, `.settings-modal`, `.settings-nav-item`, `.toggle`).
*   **Verification & Test Plan**:
    *   *Build Verification*: Run `npm run check` followed by `npm run lint` to confirm zero compilation or TypeScript errors.
    *   *State Test*: Run the Playwright test using `node tests/smoke/playwright-test-state.cjs` to verify that active page routing and modal states update correctly.

#### Task 1.2: Unifying IPC Error Handling & Logging
*   **Context**: [stores.ts](file:///g:/Open%20Flow/src/lib/stores.ts#L34-L39) (`fetchSnippets`) and [stores.ts](file:///g:/Open%20Flow/src/lib/stores.ts#L55-L60) (`fetchDictionary`).
*   **Problem Statement**: Key data-fetching functions swallow all errors inside their catch blocks (`catch { /* dev mode — no backend */ }`). This hides DB lock errors, IPC deserialization failures, and network bugs, leaving the UI empty without diagnostic feedback.
*   **Actionable Implementation Steps**:
    1. Replace empty catch blocks with structured console logs (`console.error("IPC call failed:", err)`).
    2. Check the error payload. If it's a critical Tauri exception, display a descriptive warning toast via the error system.
    3. Update Svelte lists to show an "Error loading terms" or "Offline mode" fallback indicator when the backend is unreachable.
*   **Verification & Test Plan**:
    *   *Failure Simulation*: Temporarily rename the backend command in Rust or throw a mock Tauri error in `get_snippets`. Verify that the UI displays a warning toast and logs the exact error details to the browser console.

---

### [PR Group 2: UI - Performance, Animations & Frame Loops]
*   **Goal**: Eliminate CPU cycles wasted on background animation frames and process scans during idle states.

#### Task 2.1: Conditional `requestAnimationFrame` Loop in Pill HUD
*   **Context**: [PillApp.svelte](file:///g:/Open%20Flow/src/PillApp.svelte#L103-L144).
*   **Problem Statement**: On mount, the floating pill window starts a `requestAnimationFrame` loop to animate audio visualizer bars. This loop runs continuously even when the app is in the `'idle'` state (transparent and click-through), consuming unnecessary CPU and GPU resources.
*   **Actionable Implementation Steps**:
    1. Remove the unconditional initialization of `requestAnimationFrame(animateBars)` in Svelte's `onMount`.
    2. Modify the `'pill-state'` event listener: when transitioning into `'recording'` or `'handsfree'` states, check if the loop is inactive and trigger `requestAnimationFrame(animateBars)`.
    3. When transitioning to `'idle'`, `'processing'`, or `'error'`, immediately cancel the loop using `cancelAnimationFrame(rafId)` and reset visualizer bar heights to their minimum floor height (`3px`).
*   **Verification & Test Plan**:
    *   *CPU Profile*: Launch the app in Tauri dev mode, open the frontend dev tools, select the Performance panel, and confirm that no frames are computed and script activity is 0% while the pill is in the `'idle'` state.

#### Task 2.2: Svelte List Transition Optimizations
*   **Context**: [Dictionary.svelte](file:///g:/Open%20Flow/src/lib/views/Dictionary.svelte) and [Snippets.svelte](file:///g:/Open%20Flow/src/lib/views/Snippets.svelte).
*   **Problem Statement**: When lists grow to hundreds of items, applying `in:fly`, `out:fade`, and `animate:flip` transitions directly on wrapper `div` nodes within `{#each}` loops causes major layout stutters and input lag when filtering terms.
*   **Actionable Implementation Steps**:
    1. Remove heavy fly/fade/flip transitions from the list row wrapper divs in [Dictionary.svelte](file:///g:/Open%20Flow/src/lib/views/Dictionary.svelte) and [Snippets.svelte](file:///g:/Open%20Flow/src/lib/views/Snippets.svelte).
    2. Add a simple debounce handler (e.g., 120ms) to the search input state updates to avoid filtering the DOM on every single keypress.
*   **Verification & Test Plan**:
    *   *Scale Test*: Populate the local SQLite database with 500 mock dictionary entries. Type rapidly in the search bar and verify that the layout does not freeze and key inputs are registered instantly.

---

### [PR Group 3: UI - Accessibility, Focus & Programmatic Height Fixes]
*   **Goal**: Align interactive inputs with layout focus designs and fix layout sizing bugs.

#### Task 3.1: Stopping Dropdown Escape Event Bubbling
*   **Context**: [GeneralSection.svelte](file:///g:/Open%20Flow/src/lib/components/settings/GeneralSection.svelte#L137-L142) and [Settings.svelte](file:///g:/Open%20Flow/src/lib/views/Settings.svelte#L67).
*   **Problem Statement**: The Settings modal closes when the user presses `Escape`. However, settings dropdown selectors (like Spoken Language or Microphone) do not capture and stop the `Escape` key event. Pressing `Escape` to close a dropdown propagates to the parent modal and closes the entire Settings sheet, losing unsaved form state.
*   **Actionable Implementation Steps**:
    1. Update the keydown event handlers inside the custom dropdown selections in [GeneralSection.svelte](file:///g:/Open%20Flow/src/lib/components/settings/GeneralSection.svelte).
    2. Add `e.stopPropagation()` when the `Escape` key is captured to dismiss the dropdown options menu, preventing it from reaching the parent modal event hook.
*   **Verification & Test Plan**:
    *   *Manual Action*: Open the Settings modal, expand the Spoken Language dropdown, and press `Escape`. Confirm that only the dropdown list closes, and the parent Settings modal remains open.

#### Task 3.2: Toggles and API Key Fields Outline Indicators
*   **Context**: [Toggle.svelte](file:///g:/Open%20Flow/src/lib/components/Toggle.svelte#L8-L28) and [ApiKeysSection.svelte](file:///g:/Open%20Flow/src/lib/components/settings/ApiKeysSection.svelte#L80-L90).
*   **Problem Statement**: The toggle component and API key input fields are accessible via tab navigation but lack focus styles (`:focus` or `:focus-visible`). Keyboard users cannot see which setting toggle or input is currently selected.
*   **Actionable Implementation Steps**:
    1. Add custom CSS styles for `.toggle:focus-visible` in [Toggle.svelte](file:///g:/Open%20Flow/src/lib/components/Toggle.svelte) using the theme's accent color (e.g. `outline: 2px solid var(--accent); outline-offset: 2px;`).
    2. Define `:focus` styling for `.key-input` in [ApiKeysSection.svelte](file:///g:/Open%20Flow/src/lib/components/settings/ApiKeysSection.svelte) (e.g., transition border color to `var(--arm-400)` and add a subtle focus shadow).
*   **Verification & Test Plan**:
    *   *Keyboard Walkthrough*: Open the Settings modal and press the `Tab` key repeatedly. Verify that a clear outline highlight follows the active toggle elements and API key inputs.

#### Task 3.3: Textarea Auto-Grow Svelte Action Recalculation
*   **Context**: [Snippets.svelte](file:///g:/Open%20Flow/src/lib/views/Snippets.svelte#L151-L156).
*   **Problem Statement**: The `autoGrow` text layout action listens for native `'input'` DOM events to dynamically adjust textarea heights. However, when users dictate text into a snippet expansion using the `<MicInputButton>`, Svelte updates the value programmatically. Since this bypasses standard browser key input events, the textarea fails to resize, clipping long text.
*   **Actionable Implementation Steps**:
    1. Modify the `autoGrow` action helper script (or the DOM binding) to watch for reactive value changes in the text variables, or trigger a manual resize function inside the Svelte component's state change listener.
    2. Alternatively, dispatch a synthetic `'input'` event from the `<MicInputButton>` component immediately after updating the textarea value programmatically.
*   **Verification & Test Plan**:
    *   *Dictation Input*: Click the mic button in the Snippets Edit Modal and dictate a multi-sentence phrase. Verify that the textarea automatically scales its height to fit the expanded text.

#### ✅ Task 3.4: Discoverable Clipboard Copy Button in History Card
*   **Context**: [Home.svelte](file:///g:/Open%20Flow/src/lib/views/Home.svelte#L530).
*   **Problem Statement**: The copy-to-clipboard button on dictation history entries is hidden (`opacity: 0; pointer-events: none;`) unless the user hovers over the card. This hides the feature from keyboard and mobile/tablet users.
*   **Actionable Implementation Steps**:
    1. Set a low base opacity (e.g., `opacity: 0.2`) on the copy button container so that it remains subtly visible.
    2. Elevate it to full opacity on mouse hover and keyboard focus (`:focus`, `:focus-visible`).
*   **Verification & Test Plan**:
    *   *Layout Check*: Verify that the copy-to-clipboard icon is visible at 20% opacity on the home dashboard history rows without hovering, and increases to 100% opacity on hover or focus.

---

### [PR Group 4: UI - Visual Polish, Spacing & Design System Alignment]
*   **Goal**: Rectify typography mismatches, color variables, and transition discrepancies across panels.

#### Task 4.1: Dropdown Overlay Mount/Unmount Transitions
*   **Context**: [GeneralSection.svelte](file:///g:/Open%20Flow/src/lib/components/settings/GeneralSection.svelte) (Language list), [PrivacySection.svelte](file:///g:/Open%20Flow/src/lib/components/settings/PrivacySection.svelte) (History list), and [AppMappingsEditor.svelte](file:///g:/Open%20Flow/src/lib/components/AppMappingsEditor.svelte).
*   **Problem Statement**: The Microphone dropdown features smooth Svelte transitions, but other dropdown menus mount instantly. This causes visual stuttering and inconsistent UI styling.
*   **Actionable Implementation Steps**:
    1. Extract standard transition configurations (`in:fly={{ y: 6, duration: 150 }}` and `out:fade={{ duration: 100 }}`) from the Microphone dropdown.
    2. Apply these transitions to all dropdown container overlays across the app.
    3. Bind the `open` CSS class to the Spoken Language selector SVG in [GeneralSection.svelte](file:///g:/Open%20Flow/src/lib/components/settings/GeneralSection.svelte) to rotate the arrow icon 180 degrees when expanded.
*   **Verification & Test Plan**:
    *   *Visual Verification*: Open and close each dropdown. Verify that all panels animate consistently and that the arrow icons rotate smoothly.

#### ✅ Task 4.2: Defining CSS Theme Variable `--paper-3`
*   **Context**: [theme.css](file:///g:/Open%20Flow/src/theme.css) and hover states in [AudioSection.svelte](file:///g:/Open%20Flow/src/lib/components/settings/AudioSection.svelte#L279-L282) and [Setup.svelte](file:///g:/Open%20Flow/src/lib/views/Setup.svelte#L977-L982).
*   **Problem Statement**: Cancel buttons call `background: var(--paper-3);` on hover, but this variable is not defined in the design token files. This results in the background falling back to transparent.
*   **Actionable Implementation Steps**:
    1. Open [theme.css](file:///g:/Open%20Flow/src/theme.css) (or variables declarations) and define the `--paper-3` variable in both light and dark theme configurations.
    2. Use a cohesive color tone (e.g., `#e6decb` in light mode and `#332720` in dark mode) or map the hover styles to the existing `--control-hover` token.
*   **Verification & Test Plan**:
    *   *Theme Audit*: Toggle the app between light and dark modes. Hover over the Cancel buttons in the calibration and setup screens, verifying that they show a clean background highlight.

#### ✅ Task 4.3: Correcting Monospace Font Typography
*   **Context**: [Style.svelte](file:///g:/Open%20Flow/src/lib/views/Style.svelte#L223-L232) ("New" tab badge), [Sidebar.svelte](file:///g:/Open%20Flow/src/lib/components/layout/Sidebar.svelte#L193-L203) ("Soon" lock tag), and [Settings.svelte](file:///g:/Open%20Flow/src/lib/views/Settings.svelte#L165-L172) (navigation labels and footer text).
*   **Problem Statement**: Status badges ("New", "Soon"), Settings headers, and footer credits are styled with `font-family: var(--mono);` (JetBrains Mono). This violates the design system principle that reserves monospace typography *exclusively* for technical tokens (filenames, keycodes, database records, etc.).
*   **Actionable Implementation Steps**:
    1. Locate `.lock-tag`, `.settings-section-label`, and `.settings-foot` styles in Svelte templates.
    2. Change their fonts to `var(--sans)` (Inter Tight) or remove custom font families to inherit the layout default. Apply a smaller font size or uppercase letter-spacing to preserve layout balance.
*   **Verification & Test Plan**:
    *   *Visual Verification*: Inspect elements in the Tauri DevTools to confirm that only technical tokens (like MB readouts, trigger codes, and files) are rendered in monospace, while general UI copy uses the sans-serif font.

---

### [PR Group 5: UI - Settings, Dictionary & Snippets UX Enhancements]
*   **Goal**: Polish input configurations, remove text clipping, and improve navigation flows.

#### ✅ Task 5.1: Scrollable Expansion Text inside Snippets Inspector
*   **Context**: [Snippets.svelte](file:///g:/Open%20Flow/src/lib/views/Snippets.svelte#L308-L315).
*   **Problem Statement**: The Snippets inspector truncates the template expansion preview text to 200 characters. Users cannot view long snippets without opening the Edit modal.
*   **Actionable Implementation Steps**:
    1. Remove the 200-character truncation helper (`inspExpansion`) inside the Snippets inspector panel.
    2. Render the complete expansion string in a scrollable, styled container that preserves multiline spacing.
*   **Verification & Test Plan**:
    *   *Manual Review*: Select a snippet with a long multi-paragraph template. Verify that the entire text is readable in the inspector pane and that the container displays scrollbars when needed.

#### ✅ Task 5.2: Secure API Key Deletion Option
*   **Context**: [ApiKeysSection.svelte](file:///g:/Open%20Flow/src/lib/components/settings/ApiKeysSection.svelte).
*   **Problem Statement**: Users can save API keys but cannot delete or clear them from the UI. Attempts to overwrite them with spaces are blocked, forcing users to edit the settings JSON file on disk.
*   **Actionable Implementation Steps**:
    1. Add a "Delete Key" or "Clear" button (`btn-ghost` styled in soft red) next to each saved API key field.
    2. Implement a confirm click callback that removes the key from the settings store.
*   **Verification & Test Plan**:
    *   *Key Cycle*: Save an API key, verify it displays as "Saved". Click the clear button, and confirm the field clears and the key is removed from the settings file.

#### ✅ Task 5.3: UI Feedback for Custom Models Accordions and Null Free-Space Fallbacks
*   **Context**: [ModelsSection.svelte](file:///g:/Open%20Flow/src/lib/components/settings/ModelsSection.svelte) and [PrivacySection.svelte](file:///g:/Open%20Flow/src/lib/components/settings/PrivacySection.svelte).
*   **Problem Statement**: Toggling "Custom models" does not open the model panels, hiding where to input names. Additionally, if the cache API returns a null value, the UI displays `0.0 GB free`, incorrectly suggesting the disk is full.
*   **Actionable Implementation Steps**:
    1. Modify the "Custom models" toggle callback to automatically expand the "Transcription" and "Clean-up" accordion panels when switched "on".
    2. In [PrivacySection.svelte](file:///g:/Open%20Flow/src/lib/components/settings/PrivacySection.svelte), change the fallback check so that if the cache size returns null, the text displays `status unavailable` or `calculating...` instead of `0.0 GB free`.
*   **Verification & Test Plan**:
    *   *Visual Verification*: Click the custom models toggle and confirm the settings accordions expand automatically. Check the cache size display under simulated null API responses.

#### Task 5.4: History Warning Placeholder for Failed Transcriptions
*   **Context**: [Home.svelte](file:///g:/Open%20Flow/src/lib/views/Home.svelte).
*   **Problem Statement**: If a transcription fails (e.g., API key quota exceeded), the history panel renders a blank row with only a timestamp, providing no error feedback.
*   **Actionable Implementation Steps**:
    1. Identify history cards where `clean_text` is null or empty.
    2. Render a red placeholder card showing a descriptive error message (e.g., *"Transcription failed: Check API keys or quota"*).
    3. Include a "Retry" button that allows users to reprocess the raw audio buffer.
*   **Verification & Test Plan**:
    *   *Error Test*: Mock a transcription failure, check that a warning placeholder is displayed in the history list, and confirm clicking "Retry" triggers the transcription pipeline again.

---

### [PR Group 6: UI - App Mappings & Memory Indicator Optimization]
*   **Goal**: Improve performance of local resource polling and align design actions.

#### ✅ Task 6.1: Active Memory Badge Range Colors & App mapping Button Styles
*   **Context**: [Sidebar.svelte](file:///g:/Open%20Flow/src/lib/components/layout/Sidebar.svelte#L278-L290), [AppMappingsEditor.svelte](file:///g:/Open%20Flow/src/lib/components/AppMappingsEditor.svelte#L266), and [AppMappingsEditor.svelte](file:///g:/Open%20Flow/src/lib/components/AppMappingsEditor.svelte#L225).
*   **Problem Statement**: The memory indicator bar in the sidebar uses the primary terracotta/orange accent color at all times, making normal memory usage look like a critical warning. In addition, the App Mappings "Add" button uses a secondary style, and the UI lacks feedback when app searches return empty.
*   **Actionable Implementation Steps**:
    1. In [Sidebar.svelte](file:///g:/Open%20Flow/src/lib/components/layout/Sidebar.svelte), change the memory meter bar to a neutral green or gray for normal ranges, switching to terracotta/orange only if usage exceeds `150MB`.
    2. Change the App Mappings "Add" button style from `btn-ghost` to `btn-primary` to match other create actions.
    3. Update the app search dropdown to display *"No matching apps found. Press Enter to map custom executable: 'your-app.exe'"* when search results are empty.
*   **Verification & Test Plan**:
    *   *Manual Audit*: Open the sidebar and check the meter color. Search for a non-existent app in mappings, verifying the search dropdown displays the fallback guide text.

---

### [PR Group 7: Backend - Audio Pipeline & Real-Time Safety]
*   **Goal**: Implement real-time thread safety for audio input callbacks and capture stream initialization failures.

#### Task 7.1: Lock-Free Audio Callback Stream
*   **Context**: [audio.rs](file:///g:/Open%20Flow/src-tauri/src/media/audio.rs).
*   **Problem Statement**: CPAL's input stream callback runs on a high-priority, real-time OS thread. Heap allocations (like `Vec::collect()`) and locking standard Mutexes (`lock_audio`) inside the callback violate real-time safety guarantees, which can lead to audio stuttering, clicks, and dropouts.
*   **Actionable Implementation Steps**:
    1. Add a lock-free ring buffer (such as `ringbuf` or `crossbeam-channel`) to `audio.rs`.
    2. Modify the CPAL input stream callback to write raw PCM float samples directly to the ring buffer.
    3. Spawn a background tokio thread to read samples from the ring buffer, apply gain scaling (3.5x), calculate RMS values, and write to WAV files on the heap.
*   **Verification & Test Plan**:
    *   *Build Test*: Run `npm run test:rust` to ensure compiling works. Run manual recording tests under high system load to verify clean, dropout-free audio captures.

#### Task 7.2: Stream Play Verification & Error Propagation
*   **Context**: [audio.rs](file:///g:/Open%20Flow/src-tauri/src/media/audio.rs#L93-L117) and [pipeline.rs](file:///g:/Open%20Flow/src-tauri/src/pipeline.rs#L993-L1010).
*   **Problem Statement**: `RecordingSession::start()` spawns the recording stream in a separate thread asynchronously and returns `Ok` immediately, even if stream initialization fails. The app acts like it is recording while capturing nothing, and subsequent errors are swallowed.
*   **Actionable Implementation Steps**:
    1. Refactor `RecordingSession::start()` to wait until the CPAL input stream is successfully initialized and playing before returning.
    2. Propagate stream startup errors to [pipeline.rs](file:///g:/Open%20Flow/src-tauri/src/pipeline.rs).
    3. Trigger the frontend `'open-flow:error'` event, displaying the error details on the HUD pill.
*   **Verification & Test Plan**:
    *   *Failure Simulation*: Disconnect or disable the microphone device, start recording, and verify the UI immediately displays a `Failed` error toast.

---

### [PR Group 8: Backend - Windows Native Keyboard Layout Mapper]
*   **Goal**: Translate virtual key codes to characters matching the user's active keyboard layout.

#### Task 8.1: Native Key Translation Layout Mapper
*   **Context**: [hotkey.rs](file:///g:/Open%20Flow/src-tauri/src/core/hotkey.rs#L69-L92) (`vk_to_char`).
*   **Problem Statement**: The keyboard hook converts virtual key codes to characters using a hardcoded QWERTY map. This causes wrong characters to be registered when using other keyboard layouts (like French AZERTY or German QWERTZ), resulting in formatting and capitalization errors.
*   **Actionable Implementation Steps**:
    1. Remove the hardcoded QWERTY map in `vk_to_char`.
    2. Implement native Win32 keyboard layout translation using API calls like `GetKeyboardLayout`, `GetKeyboardState`, `MapVirtualKeyW`, and `ToUnicode`.
    3. Handle shift states and resolve characters matching the active system layout.
*   **Verification & Test Plan**:
    *   *Layout Test*: Switch the active keyboard layout in Windows to AZERTY or QWERTZ. Type letters and confirm that the correct character representations are logged by the hotkey listener.

---

### [PR Group 9: Backend - Input Orchestration & Text Heuristics]
*   **Goal**: Clean up pipeline helper utilities, clipboard timings, and word boundaries.

#### Task 9.1: Extracting Number Word Tokenizer to a Dedicated Module
*   **Context**: [pipeline.rs](file:///g:/Open%20Flow/src-tauri/src/pipeline.rs#L342-L630).
*   **Problem Statement**: Math tokenizers and number parser helpers (e.g., `normalize_number_word_run`, `parse_number_word_integer`) are defined directly inside `pipeline.rs`, bloating it to over 1,700 lines and violating the single-responsibility principle.
*   **Actionable Implementation Steps**:
    1. Create a new utility file `src-tauri/src/system/number_parser.rs`.
    2. Move all number normalization, word value calculations, and digit conversions out of `pipeline.rs` into the new module.
    3. Expose a single entry point `number_parser::normalize(raw_text)` and call it from the pipeline orchestration thread.
*   **Verification & Test Plan**:
    *   *Unit Tests*: Run `npm run test:rust` to ensure compiling works. Add unit tests in `number_parser.rs` verifying specific mathematical conversion scenarios.

#### Task 9.2: Parameterized Clipboard Timing and Word Boundary Snippets
*   **Context**: [injection.rs](file:///g:/Open%20Flow/src-tauri/src/core/injection.rs#L200-L370) and [snippets.rs](file:///g:/Open%20Flow/src-tauri/src/data/snippets.rs#L46-L78).
*   **Problem Statement**: Clipboard injection relies on fragile thread sleeps that fail under system load. Also, snippet triggering performs naive substring replacement, corrupting matching parts of regular words.
*   **Actionable Implementation Steps**:
    1. Clean up hardcoded sleeps in [injection.rs](file:///g:/Open%20Flow/src-tauri/src/core/injection.rs) into parameterized constants.
    2. Add a retry-loop that checks if clipboard locks have been released before attempting to restore clipboard contents.
    3. Update `expand_snippets_from` in [snippets.rs](file:///g:/Open%20Flow/src-tauri/src/data/snippets.rs) to use word boundaries (regex `\b` or character boundaries), ensuring trigger expansions only apply to standalone words.
*   **Verification & Test Plan**:
    *   *Injection Test*: Trigger multiple rapid injections while under system load to verify that no paste operations fail.
    *   *Snippet boundary Test*: Create a snippet `app` -> `application`. Type "the app is on the apple tree" and check that it expands to "the application is on the apple tree" without corrupting "apple".

---

### [PR Group 10: Backend - Auto-Learn Engine Optimization]
*   **Goal**: Migrate from CPU-intensive COM polling loops to event-driven property change listeners.

#### Task 10.1: Event-Driven Windows UI Automation Listeners
*   **Context**: [auto_learn.rs](file:///g:/Open%20Flow/src-tauri/src/api/auto_learn.rs#L872-L973) and [auto_learn.rs](file:///g:/Open%20Flow/src-tauri/src/api/auto_learn.rs#L94).
*   **Problem Statement**: Auto-learn monitors query active controls by polling every 2 seconds, which is CPU intensive. Re-instantiating the COM UI Automation engine every tick also wastes resources.
*   **Actionable Implementation Steps**:
    1. Refactor the COM UI Automation logic in `auto_learn.rs` to register a property change listener (`AddAutomationEventHandler` or `AddPropertyChangedEventHandler`) for the Text pattern.
    2. Maintain a single `IUIAutomation` pointer instead of re-instantiating on every tick.
*   **Verification & Test Plan**:
    *   *Manual Review*: Open dictation, edit a transcribed word, and verify that corrections are logged immediately without running polling loops.

#### ✅ Task 10.2: Unicode Characters Length and Cursor Anchoring
*   **Context**: [auto_learn.rs](file:///g:/Open%20Flow/src-tauri/src/api/auto_learn.rs#L106-L113) and [auto_learn.rs](file:///g:/Open%20Flow/src-tauri/src/api/auto_learn.rs#L377-L392).
*   **Problem Statement**: String byte length checks (`.len()`) break distance checks on Unicode characters. Anchor searches also collide in documents when a sentence is repeated.
*   **Actionable Implementation Steps**:
    1. Replace `.len()` with `.chars().count()` for character comparison in edit distance calculations.
    2. Capture the baseline document text immediately before text injection, using local cursor context to anchor corrections.
*   **Verification & Test Plan**:
    *   *Unicode Test*: Run auto-learn on Cyrillic/Japanese inputs and verify edit distances map correctly. Verify corrections are only registered when within cursor vicinity.

---

### [PR Group 11: Backend - DB Migrations and IPC Async Execution]
*   **Goal**: Ensure transaction-based database migrations and prevent blocking commands from stalling the async executor.

#### Task 11.1: Transactional Migration Schemas
*   **Context**: [db.rs](file:///g:/Open%20Flow/src-tauri/src/data/db.rs#L118-L259).
*   **Problem Statement**: Schema updates run inline without transactions, swallowing execution errors and risking database corruption. Date values are also stored as formatted UTC strings prone to timezone drift.
*   **Actionable Implementation Steps**:
    1. Wrap migrations in explicit SQL transactions (`BEGIN TRANSACTION`, `COMMIT`). If an error occurs, roll back and abort application startup.
    2. Store datetime values as SQLite integer Unix Epoch timestamps.
*   **Verification & Test Plan**:
    *   *Failure Simulation*: Inject a mock SQL syntax error into a migration and confirm that database changes roll back cleanly and prevent startup.

#### Task 11.2: Async Tauri Command Refactoring
*   **Context**: [mod.rs](file:///g:/Open%20Flow/src-tauri/src/commands/mod.rs).
*   **Problem Statement**: Tauri commands performing heavy blocking I/O (like process scans or DB queries) are declared as synchronous `pub fn` functions, blocking Tokio's main thread pool.
*   **Actionable Implementation Steps**:
    1. Change Tauri command declarations to `async fn`.
    2. Wrap blocking operations (`get_installed_apps`, SQLite read queries) inside `tokio::task::spawn_blocking`.
*   **Verification & Test Plan**:
    *   *Concurrency Test*: Run a heavy process snapshot check and verify other async functions (like API callbacks or UI timers) run concurrently without blocking.
