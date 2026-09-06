<script lang="ts">
  import { fly, fade } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { invoke } from '../../tauri';
  import { formatIpcError, type DictionaryEntry } from '../../stores';
  import { modalFocusTrap } from '../../modalFocus';
  import MicInputButton from '../../components/MicInputButton.svelte';
  import { modalBackdrop, modalCard, MOTION_PX, motionPx } from '../../motion';
  import { countCodePoints, MISTAKE_LIMIT, requireCreatedRecordMeta, TERM_LIMIT } from './helpers';

  let {
    mode,
    entry,
    onClose,
    onSaved,
    onGoToSnippets,
  }: {
    mode: 'add' | 'edit';
    entry?: DictionaryEntry;
    onClose: () => void;
    onSaved: (entry: DictionaryEntry) => void;
    onGoToSnippets: () => void;
  } = $props();

  // The modal mounts fresh each time it opens, so capturing the initial entry
  // values once here is exactly the desired behavior.
  // svelte-ignore state_referenced_locally
  let draftTerm = $state(entry?.term ?? '');
  // svelte-ignore state_referenced_locally
  let draftMistake = $state(entry?.mistake ?? '');
  let saving = $state(false);
  let saveError = $state('');
  let termInput = $state<HTMLInputElement | null>(null);
  let mistakeInput = $state<HTMLInputElement | null>(null);

  async function saveModal() {
    // Read directly from DOM elements at click time to bypass WKWebView
    // bind:value paste-sync lag, matching the pattern in Snippets.svelte.
    if (termInput) draftTerm = termInput.value;
    if (mistakeInput) draftMistake = mistakeInput.value;

    const term = draftTerm.trim();
    const mistakeValue = draftMistake.trim();
    const mistake = mistakeValue || null;
    if (!term) { saveError = 'Term is required.'; return; }
    if (countCodePoints(term) > TERM_LIMIT) {
      saveError = `Term must be ${TERM_LIMIT} characters or fewer.`;
      return;
    }
    if (mistake && countCodePoints(mistake) > MISTAKE_LIMIT) {
      saveError = `"Often mistranscribed as" must be ${MISTAKE_LIMIT} characters or fewer.`;
      return;
    }
    saving = true; saveError = '';
    try {
      if (mode === 'add') {
        const created = requireCreatedRecordMeta(
          await invoke<unknown>('create_dictionary_entry', { term, mistake }),
          'create_dictionary_entry',
        );
        onSaved({
          id: created.id,
          term,
          mistake,
          auto_learned: false,
          correction_count: 0,
          confidence_tier: 'manual',
          last_seen_at: null,
          created_at: created.created_at,
        });
      } else if (mode === 'edit' && entry) {
        await invoke('edit_dictionary_entry', { id: entry.id, term, mistake });
        onSaved({
          ...entry,
          term,
          mistake,
        });
      }
      onClose();
    } catch (err) {
      const msg = formatIpcError(err);
      saveError = msg.includes('UNIQUE') ? 'That term already exists.' : msg;
    } finally { saving = false; }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) saveModal();
  }

  $effect(() => {
    if (termInput) setTimeout(() => termInput?.focus(), 50);
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<button class="modal-backdrop" aria-label="Close dialog" onclick={onClose} in:modalBackdrop={{ duration: 180 }} out:modalBackdrop={{ duration: 160 }}></button>
<div
  class="modal-card"
  use:modalFocusTrap={{ active: true, initialFocus: () => termInput }}
  role="dialog"
  aria-modal="true"
  aria-labelledby="dictionary-modal-title"
  tabindex="-1"
  in:modalCard={{ duration: 220, distance: motionPx(MOTION_PX.panel), scaleFrom: 0.97 }}
  out:modalCard={{ duration: 160, distance: motionPx(MOTION_PX.nudge), scaleFrom: 0.985 }}
>
  <div class="modal-header">
    <h2 id="dictionary-modal-title" class="modal-title">{mode === 'add' ? 'Add term' : 'Edit term'}</h2>
    <button class="icon-btn" onclick={onClose} aria-label="Close">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
    </button>
  </div>

  <div class="modal-body">
    <label class="field-label" for="dict-term">
      Term
      <span class="char-count" class:over={countCodePoints(draftTerm) >= TERM_LIMIT}>{countCodePoints(draftTerm)}/{TERM_LIMIT}</span>
    </label>
    <div class="input-row">
      <input
        id="dict-term"
        class="field-input"
        type="text"
        placeholder="e.g. Kubernetes, Björk, ChatGPT"
        bind:value={draftTerm}
        bind:this={termInput}
        autocomplete="off"
        spellcheck="false"
      />
      <MicInputButton onResult={(t) => draftTerm = t} />
    </div>
    <p class="field-hint">The exact word or phrase you want the AI to use.</p>

    <label class="field-label" for="dict-mistake">
      Often mistranscribed as <span class="field-optional">optional</span>
      <span class="char-count" class:over={countCodePoints(draftMistake) >= MISTAKE_LIMIT}>{countCodePoints(draftMistake)}/{MISTAKE_LIMIT}</span>
    </label>
    <div class="input-row">
      <input
        id="dict-mistake"
        class="field-input"
        type="text"
        placeholder="e.g. koobernetes, koobernettis"
        bind:value={draftMistake}
        bind:this={mistakeInput}
        autocomplete="off"
        spellcheck="false"
      />
      <MicInputButton onResult={(t) => draftMistake = t} />
    </div>
    <p class="field-hint">What the transcription model typically writes instead. Separate multiple mistranscriptions with commas. Skip if the term just needs to be in the AI's awareness.</p>
  </div>

  <div class="modal-footer">
    {#if saveError}
      <p class="save-error">{saveError}</p>
    {/if}
    {#if draftTerm.length >= TERM_LIMIT}
      <button
        class="snippet-nudge"
        onclick={onGoToSnippets}
        in:fly={{ y: 5, duration: 220, easing: expoOut }}
        out:fade={{ duration: 100 }}
      >Maybe this would be better as a snippet.</button>
    {/if}
    <div class="footer-actions">
      <button class="btn-ghost" onclick={onClose}>Cancel</button>
      <button
        class="btn-primary"
        onclick={saveModal}
        disabled={saving}
      >
        {#if saving}<span class="spinner"></span>{/if}
        {mode === 'add' ? 'Add term' : 'Save changes'}
      </button>
    </div>
  </div>
</div>

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    border: 0;
    padding: 0;
    appearance: none;
    background: var(--overlay);
    z-index: 50;
    outline: none;
  }

  .modal-card {
    position: fixed;
    top: 50%;
    left: 50%;
    translate: -50% -50%;
    z-index: 51;
    isolation: isolate;
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    width: min(460px, calc(100vw - 40px));
    box-shadow: var(--shadow-elev);
    overflow: hidden;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px 14px;
    border-bottom: 1px solid var(--line-soft);
  }

  .modal-title {
    font-family: var(--sans);
    font-size: 17px;
    font-weight: 600;
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
    gap: 2px;
  }

  .modal-footer {
    padding: 12px 20px 16px;
    border-top: 1px solid var(--line-soft);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .footer-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .field-label {
    font-size: 11.5px;
    font-weight: 500;
    color: var(--ink-soft);
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
    margin-bottom: 5px;
  }
  .field-label:first-child { margin-top: 0; }

  .field-optional {
    font-size: 10.5px;
    color: var(--ink-faint);
    font-weight: 400;
    font-style: italic;
  }

  .input-row { display: flex; align-items: center; gap: 6px; }

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
    line-height: 1.5;
  }
  .input-row .field-input { flex: 1; width: auto; min-width: 0; }
  .field-input:focus { border-color: var(--arm-400); }

  .field-hint { font-size: 11px; color: var(--ink-mute); margin: 4px 0 0; }

  .char-count { font-size: 10.5px; color: var(--ink-mute); font-weight: 400; margin-left: auto; }
  .char-count.over { color: var(--danger); }

  .snippet-nudge {
    background: transparent;
    border: 0;
    padding: 0;
    margin: 0;
    font-size: 11.5px;
    color: var(--accent-ink);
    font-family: var(--sans);
    cursor: pointer;
    text-align: left;
    text-decoration: underline;
    text-underline-offset: 2px;
    text-decoration-color: color-mix(in oklab, var(--accent-ink) 40%, transparent);
    transition: text-decoration-color 0.15s, color 0.15s;
  }
  .snippet-nudge:hover {
    text-decoration-color: var(--accent-ink);
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

</style>
