<script lang="ts">
  import { invoke } from '../../tauri';
  import { formatIpcError, type Snippet } from '../../stores';
  import { modalFocusTrap } from '../../modalFocus';
  import MicInputButton from '../../components/MicInputButton.svelte';
  import { modalBackdrop, modalCard, MOTION_PX, motionPx } from '../../motion';
  import { autoGrow, countCodePoints, normalizeText, requireCreatedRecordMeta, TRIGGER_LIMIT } from './helpers';

  let {
    mode,
    snippet,
    onClose,
    onSaved,
  }: {
    mode: 'add' | 'edit';
    snippet?: Snippet;
    onClose: () => void;
    onSaved: (snippet: Snippet) => void;
  } = $props();

  // The modal mounts fresh each time it opens, so capturing the initial snippet
  // values once here is exactly the desired behavior.
  // svelte-ignore state_referenced_locally
  let draftTrigger = $state(snippet?.trigger ?? '');
  // svelte-ignore state_referenced_locally
  let draftExpansion = $state(snippet?.expansion ?? '');
  // svelte-ignore state_referenced_locally
  let draftInstructions = $state(snippet?.instructions ?? '');
  let saving = $state(false);
  let saveError = $state('');
  let triggerInput = $state<HTMLInputElement | null>(null);
  let expansionEl = $state<HTMLTextAreaElement | null>(null);
  let instructionsEl = $state<HTMLTextAreaElement | null>(null);

  async function saveModal() {
    // Read straight from the DOM elements. On WKWebView, `bind:value` can fail
    // to propagate a pasted value into reactive state before the click fires.
    // The element's live `.value` is always correct at click time.
    if (triggerInput) draftTrigger = triggerInput.value;
    if (expansionEl) draftExpansion = expansionEl.value;
    if (instructionsEl) draftInstructions = instructionsEl.value;

    const t = draftTrigger.trim();
    const e = normalizeText(draftExpansion);
    const i = normalizeText(draftInstructions);
    if (!t || !e) {
      saveError = 'Trigger and expansion are both required.';
      return;
    }
    if (countCodePoints(t) > TRIGGER_LIMIT) {
      saveError = `Trigger must be ${TRIGGER_LIMIT} characters or fewer.`;
      return;
    }
    saving = true;
    saveError = '';
    try {
      if (mode === 'add') {
        const created = requireCreatedRecordMeta(await invoke<unknown>('create_snippet', {
          trigger: t,
          expansion: e,
          instructions: i,
        }));
        onSaved({
          id: created.id,
          trigger: t,
          expansion: e,
          instructions: i,
          use_count: 0,
          created_at: created.created_at,
        });
      } else if (mode === 'edit' && snippet) {
        await invoke('edit_snippet', { id: snippet.id, trigger: t, expansion: e, instructions: i });
        onSaved({
          ...snippet,
          trigger: t,
          expansion: e,
          instructions: i,
        });
      }
      onClose();
    } catch (err) {
      const msg = formatIpcError(err);
      saveError = msg.includes('UNIQUE')
        ? 'A snippet with that trigger already exists.'
        : msg;
    }
    finally { saving = false; }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) saveModal();
  }

  $effect(() => {
    if (triggerInput) setTimeout(() => triggerInput?.focus(), 50);
  });

  $effect(() => {
    draftExpansion;
    autoGrow(expansionEl);
  });

  $effect(() => {
    draftInstructions;
    autoGrow(instructionsEl);
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="modal-overlay">
<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<button class="modal-backdrop" aria-label="Close dialog" onclick={onClose} in:modalBackdrop={{ duration: 180 }} out:modalBackdrop={{ duration: 160 }}></button>
<div
  class="modal-card"
  use:modalFocusTrap={{ active: true, initialFocus: () => triggerInput }}
  role="dialog"
  aria-modal="true"
  aria-labelledby="snippet-modal-title"
  tabindex="-1"
  in:modalCard={{ duration: 220, distance: motionPx(MOTION_PX.panel), scaleFrom: 0.97 }}
  out:modalCard={{ duration: 160, distance: motionPx(MOTION_PX.nudge), scaleFrom: 0.985 }}
>
  <div class="modal-header">
    <h2 id="snippet-modal-title" class="modal-title">{mode === 'add' ? 'New snippet' : 'Edit snippet'}</h2>
    <button class="icon-btn" onclick={onClose} aria-label="Close">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
    </button>
  </div>

  <div class="modal-body scrollbar-standard">
    <label class="field-label" for="trigger-input">
      Trigger
      <span class="char-count" class:over={countCodePoints(draftTrigger) > TRIGGER_LIMIT}>{countCodePoints(draftTrigger)}/{TRIGGER_LIMIT}</span>
    </label>
    <div class="input-row">
      <input
        id="trigger-input"
        class="field-input"
        type="text"
        placeholder="e.g. my email, my e-mail"
        bind:value={draftTrigger}
        bind:this={triggerInput}
        autocomplete="off"
        spellcheck="false"
      />
      <MicInputButton onResult={(t) => draftTrigger = t} />
    </div>
    <p class="field-hint">Speak any of these phrases to trigger the expansion. Separate multiple triggers with commas.</p>

    <label class="field-label" for="expansion-input">Expansion</label>
    <div class="input-row input-row--top">
      <textarea
        id="expansion-input"
        class="field-input scrollbar-standard"
        placeholder="e.g. hello@example.com"
        bind:value={draftExpansion}
        bind:this={expansionEl}
        rows="3"
        spellcheck="false"
      ></textarea>
      <MicInputButton onResult={(t) => draftExpansion = t} />
    </div>

    <label class="field-label instructions-label" for="instructions-input">
      <span class="instructions-label-text">
        <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" style="flex-shrink:0">
          <circle cx="12" cy="12" r="10"/><path d="M12 16v-4M12 8h.01"/>
        </svg>
        Cleanup instructions
      </span>
      <span class="field-optional">optional</span>
    </label>
    <textarea
      id="instructions-input"
      class="field-input instructions-input scrollbar-standard"
      placeholder="e.g. Don't add a period at the end of this phrase."
      bind:value={draftInstructions}
      bind:this={instructionsEl}
      rows="2"
      spellcheck="false"
    ></textarea>
    <p class="field-hint">Added to the cleanup model's system prompt only when this snippet is detected.</p>
  </div>

  <div class="modal-footer">
    {#if saveError}
      <p class="save-error">{saveError}</p>
    {/if}
    <div class="footer-actions">
      <button class="btn-ghost" onclick={onClose}>Cancel</button>
      <button
        class="btn-primary"
        onclick={saveModal}
        disabled={saving}
      >
        {#if saving}<span class="spinner"></span>{/if}
        {mode === 'add' ? 'Add snippet' : 'Save changes'}
      </button>
    </div>
  </div>
</div>
</div>

<style>
  .modal-overlay {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 40px 20px;
    box-sizing: border-box;
    overflow-y: auto;
  }

  .modal-backdrop {
    position: fixed;
    inset: 0;
    border: 0;
    padding: 0;
    appearance: none;
    background: var(--overlay);
    z-index: 0;
    outline: none;
  }

  .modal-card {
    position: relative;
    z-index: 1;
    margin: auto;
    isolation: isolate;
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    width: min(500px, 100%);
    max-height: 100%;
    box-shadow: var(--shadow-elev);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px 14px;
    border-bottom: 1px solid var(--line-soft);
    flex-shrink: 0;
  }

  .modal-title {
    font-family: var(--serif);
    font-size: 18px;
    font-weight: 500;
    letter-spacing: -0.015em;
    color: var(--ink);
    margin: 0;
  }

  .icon-btn {
    width: 26px; height: 26px;
    background: transparent;
    border: 0;
    border-radius: 6px;
    display: grid;
    place-items: center;
    color: var(--ink-mute);
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }
  .icon-btn:hover { background: var(--control-active); color: var(--ink-strong); }

  .modal-body {
    padding: 18px 20px 14px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    --field-input-max-height: 220px;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }

  .modal-footer {
    padding: 12px 20px 16px;
    border-top: 1px solid var(--line-soft);
    display: flex;
    flex-direction: column;
    gap: 10px;
    flex-shrink: 0;
  }

  .footer-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .save-error {
    font-size: 11.5px;
    color: var(--danger);
    margin: 0;
    padding: 6px 10px;
    background: var(--danger-bg);
    border: 1px solid var(--danger-line);
    border-radius: var(--r-sm);
  }

  .field-label {
    font-size: 11.5px;
    font-weight: 500;
    color: var(--ink-soft);
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 10px;
    margin-bottom: 5px;
  }
  .field-label:first-child { margin-top: 0; }

  .char-count {
    font-size: 10.5px;
    color: var(--ink-mute);
    font-weight: 400;
  }
  .char-count.over { color: var(--danger); }

  .input-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .input-row--top { align-items: flex-start; }

  .field-input {
    width: 100%;
    background: var(--paper);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    padding: 8px 11px;
    font-size: 13px;
    font-family: var(--sans);
    color: var(--ink-strong);
    outline: none;
    transition: border-color 0.15s;
    box-sizing: border-box;
    resize: none;
    line-height: 1.5;
  }
  .input-row .field-input { flex: 1; width: auto; min-width: 0; }
  .field-input:focus { border-color: var(--arm-400); }
  .field-input.scrollbar-standard {
    max-height: var(--field-input-max-height);
  }

  .field-hint {
    font-size: 11px;
    color: var(--ink-mute);
    margin: 3px 0 0;
  }

  .spinner {
    display: inline-block;
    width: 11px; height: 11px;
    border: 1.5px solid rgba(249,247,243,0.3);
    border-top-color: var(--amber-50);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
    flex-shrink: 0;
  }

  @keyframes spin { to { transform: rotate(360deg); } }

  .instructions-label {
    margin-top: 14px;
  }

  .instructions-label-text {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--ink-soft);
  }

  .field-optional {
    font-size: 10.5px;
    color: var(--ink-faint);
    font-weight: 400;
    font-style: italic;
  }

  .instructions-input {
    border-style: dashed;
    font-size: 12.5px;
  }
  .instructions-input:focus { border-color: var(--arm-400); border-style: solid; }

  .btn-ghost {
    background: transparent;
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 6px 14px;
    font-size: 12.5px;
    font-family: var(--sans);
    color: var(--ink-soft);
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }
  .btn-ghost:hover { background: var(--control-hover); color: var(--ink-strong); }

  .btn-primary {
    background: var(--ink);
    color: var(--amber-50);
    border: 0;
    border-radius: 8px;
    padding: 6px 14px;
    font-size: 12.5px;
    font-weight: 500;
    font-family: var(--sans);
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    white-space: nowrap;
    transition: opacity 0.15s;
  }
  .btn-primary:disabled { opacity: 0.4; cursor: default; }
  .btn-primary:not(:disabled):hover { opacity: 0.82; }
</style>
