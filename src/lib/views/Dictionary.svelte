<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { fly, fade } from 'svelte/transition';
  import { flip } from 'svelte/animate';
  import { expoOut } from 'svelte/easing';
  import { dictionary, fetchDictionary, type DictionaryEntry } from '../stores';

  let search = $state('');
  let modal = $state<{ mode: 'add' | 'edit'; entry?: DictionaryEntry } | null>(null);
  let deleteTarget = $state<number | null>(null);
  let draftTerm = $state('');
  let draftMistake = $state('');
  let termInput = $state<HTMLInputElement | null>(null);
  let saving = $state(false);
  let saveError = $state('');

  const TERM_LIMIT = 120;
  const MISTAKE_LIMIT = 120;

  const filtered = $derived.by(() => {
    const q = search.trim().toLowerCase();
    const list = q
      ? $dictionary.filter(e =>
          e.term.toLowerCase().includes(q) ||
          (e.mistake ?? '').toLowerCase().includes(q)
        )
      : [...$dictionary];
    return list.sort((a, b) => b.created_at.localeCompare(a.created_at));
  });

  onMount(() => { fetchDictionary(); });

  function openAdd() {
    draftTerm = '';
    draftMistake = '';
    modal = { mode: 'add' };
  }

  function openEdit(e: DictionaryEntry) {
    draftTerm = e.term;
    draftMistake = e.mistake ?? '';
    modal = { mode: 'edit', entry: e };
    deleteTarget = null;
  }

  function closeModal() { modal = null; saveError = ''; }

  async function saveModal() {
    const term = draftTerm.trim();
    const mistake = draftMistake.trim() || null;
    if (!term) return;
    saving = true; saveError = '';
    try {
      if (modal?.mode === 'add') {
        await invoke('create_dictionary_entry', { term, mistake });
      } else if (modal?.mode === 'edit' && modal.entry) {
        await invoke('edit_dictionary_entry', { id: modal.entry.id, term, mistake });
      }
      await fetchDictionary();
      closeModal();
    } catch (err) {
      const msg = String(err);
      saveError = msg.includes('UNIQUE') ? 'That term already exists.' : 'Failed to save.';
    } finally { saving = false; }
  }

  async function confirmDelete(id: number) {
    if (deleteTarget === id) {
      try {
        await invoke('remove_dictionary_entry', { id });
        await fetchDictionary();
      } catch (err) { console.error(err); }
      deleteTarget = null;
    } else {
      deleteTarget = id;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      if (modal) closeModal();
      else if (deleteTarget !== null) deleteTarget = null;
    }
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey) && modal) saveModal();
  }

  $effect(() => {
    if (modal && termInput) setTimeout(() => termInput?.focus(), 50);
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="content-inner">
  <h1 class="page-h">Dictionary</h1>
  <p class="page-sub">Your personal vocabulary. Add words or phrases the AI should know — names, brands, jargon, anything niche. They get injected into every transcription so the AI recognises them and uses your exact spelling.</p>

  <div class="toolbar">
    <div class="search">
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="11" cy="11" r="7"/><path d="m21 21-4.35-4.35"/>
      </svg>
      <input
        class="search-input"
        type="text"
        placeholder={`Search ${$dictionary.length} ${$dictionary.length === 1 ? 'term' : 'terms'}…`}
        bind:value={search}
        aria-label="Search dictionary"
      />
      {#if search}
        <button class="clear-btn" onclick={() => search = ''} aria-label="Clear">
          <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
        </button>
      {/if}
    </div>

    <button class="btn-primary" onclick={openAdd}>
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 5v14M5 12h14"/></svg>
      Add term
    </button>
  </div>

  {#if $dictionary.length === 0}
    <div class="empty-state" in:fade={{ duration: 220 }}>
      <p class="empty-h">No terms yet</p>
      <p class="empty-sub">Add words the AI should know — names, brands, jargon, anything a generic model is unlikely to get right.</p>
      <button class="btn-primary" onclick={openAdd}>
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 5v14M5 12h14"/></svg>
        Add term
      </button>
    </div>
  {:else if filtered.length === 0}
    <div class="empty-state" in:fade={{ duration: 200 }}>
      <p class="empty-h">No matches</p>
      <p class="empty-sub">Nothing matches "{search}".</p>
      <button class="btn-ghost" onclick={() => search = ''}>Clear search</button>
    </div>
  {:else}
    <div class="list-wrap">
      {#each filtered as e (e.id)}
        <div
          class="dict-row"
          in:fly={{ y: 6, duration: 220, easing: expoOut }}
          out:fade={{ duration: 120 }}
          animate:flip={{ duration: 220, easing: expoOut }}
        >
          <div class="dict-content">
            <div class="dict-term">{e.term}</div>
            {#if e.auto_learned}
              <svg class="auto-star" width="12" height="12" viewBox="0 0 24 24" fill="currentColor"
                aria-label="Auto-learned">
                <title>Added automatically by Auto-learn corrections</title>
                <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z"/>
              </svg>
            {/if}
            {#if e.mistake}
              <div class="dict-mistake-label" aria-hidden="true">often:</div>
              <div class="dict-mistake">"{e.mistake}"</div>
            {/if}
          </div>

          <div class="row-actions">
            <button class="icon-btn" onclick={() => openEdit(e)} aria-label="Edit">
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4z"/></svg>
            </button>
            <button
              class="icon-btn delete-btn"
              class:armed={deleteTarget === e.id}
              onclick={() => confirmDelete(e.id)}
              aria-label={deleteTarget === e.id ? 'Confirm delete' : 'Delete'}
            >
              {#if deleteTarget === e.id}
                <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>
              {:else}
                <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><path d="m19 6-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/></svg>
              {/if}
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

{#if modal}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div class="modal-backdrop" onclick={closeModal} in:fade={{ duration: 150 }} out:fade={{ duration: 100 }}></div>
  <div
    class="modal-card"
    role="dialog"
    aria-modal="true"
    in:fly={{ y: 14, duration: 260, easing: expoOut }}
    out:fly={{ y: 8, duration: 150, easing: expoOut }}
  >
    <div class="modal-header">
      <h2 class="modal-title">{modal.mode === 'add' ? 'Add term' : 'Edit term'}</h2>
      <button class="icon-btn" onclick={closeModal} aria-label="Close">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
      </button>
    </div>

    <div class="modal-body">
      <label class="field-label" for="dict-term">Term</label>
      <input
        id="dict-term"
        class="field-input"
        type="text"
        placeholder="e.g. Kubernetes, Björk, ChatGPT"
        bind:value={draftTerm}
        bind:this={termInput}
        maxlength={TERM_LIMIT}
        autocomplete="off"
        spellcheck="false"
      />
      <p class="field-hint">The exact word or phrase you want the AI to use.</p>

      <label class="field-label" for="dict-mistake">
        Often mistranscribed as <span class="optional">optional</span>
      </label>
      <input
        id="dict-mistake"
        class="field-input"
        type="text"
        placeholder="e.g. koobernetes, byork"
        bind:value={draftMistake}
        maxlength={MISTAKE_LIMIT}
        autocomplete="off"
        spellcheck="false"
      />
      <p class="field-hint">What the transcription model typically writes instead. Skip this if the term just needs to be in the AI's awareness.</p>
    </div>

    <div class="modal-footer">
      {#if saveError}
        <p class="save-error">{saveError}</p>
      {/if}
      <div class="footer-actions">
        <button class="btn-ghost" onclick={closeModal}>Cancel</button>
        <button
          class="btn-primary"
          onclick={saveModal}
          disabled={saving || !draftTerm.trim()}
        >
          {#if saving}<span class="spinner"></span>{/if}
          {modal.mode === 'add' ? 'Add term' : 'Save changes'}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .content-inner {
    padding: 18px 28px 36px;
    max-width: 920px;
  }

  .page-h {
    font-family: var(--serif);
    font-size: 26px;
    font-weight: 500;
    letter-spacing: -0.02em;
    margin: 0 0 4px;
    line-height: 1.1;
    color: var(--ink);
  }

  .page-sub {
    color: var(--ink-mute);
    font-size: 12.5px;
    margin: 0 0 22px;
    max-width: 560px;
    line-height: 1.5;
  }

  /* ── toolbar ── */

  .toolbar {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: 14px;
  }

  .search {
    flex: 1;
    min-width: 160px;
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 6px 10px;
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--ink-mute);
    transition: border-color 0.15s;
  }
  .search:focus-within { border-color: var(--arm-300); }

  .search-input {
    flex: 1;
    background: transparent;
    border: 0;
    outline: none;
    font-size: 13px;
    font-family: var(--sans);
    color: var(--ink-strong);
    min-width: 0;
  }
  .search-input::placeholder { color: var(--ink-mute); }

  .clear-btn {
    background: transparent;
    border: 0;
    padding: 0;
    color: var(--ink-mute);
    cursor: pointer;
    display: grid;
    place-items: center;
    width: 16px; height: 16px;
    border-radius: 4px;
  }
  .clear-btn:hover { color: var(--ink-strong); }

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
  .btn-ghost:hover { background: var(--amber-50); color: var(--ink-strong); }

  /* ── list ── */

  .list-wrap {
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    overflow: hidden;
    background: var(--bg-elev);
  }

  .dict-row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 16px;
    align-items: center;
    padding: 11px 14px;
    border-bottom: 1px solid var(--line);
    transition: background 0.12s;
  }
  .dict-row:last-child { border-bottom: 0; }
  .dict-row:hover { background: var(--amber-50); }

  .dict-content {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    flex-wrap: wrap;
  }

  .dict-term {
    font-size: 13.5px;
    font-weight: 500;
    color: var(--ink);
    letter-spacing: -0.005em;
  }

  .auto-star {
    color: #f97316;
    flex-shrink: 0;
    opacity: 0.85;
  }

  .dict-mistake-label {
    font-family: var(--mono);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--arm-300);
    line-height: 1;
  }

  .dict-mistake {
    font-size: 12.5px;
    color: var(--ink-mute);
    font-style: italic;
  }

  .row-actions {
    display: flex;
    gap: 2px;
    opacity: 0;
    transition: opacity 0.15s;
  }
  .dict-row:hover .row-actions,
  .dict-row:focus-within .row-actions { opacity: 1; }

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
  .icon-btn:hover { background: var(--amber-100); color: var(--ink-strong); }

  .delete-btn.armed {
    background: #fef2f2;
    color: #dc2626;
    box-shadow: inset 0 0 0 1px #fca5a5;
  }
  .delete-btn.armed:hover { background: #fee2e2; }

  /* ── empty states ── */

  .empty-state {
    padding: 56px 4px;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 6px;
  }

  .empty-h {
    font-family: var(--serif);
    font-style: italic;
    font-size: 17px;
    font-weight: 500;
    color: var(--ink-soft);
    margin: 0;
  }

  .empty-sub {
    font-size: 12.5px;
    color: var(--ink-mute);
    line-height: 1.5;
    margin: 0 0 10px;
    max-width: 360px;
  }

  /* ── modal ── */

  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(13, 10, 8, 0.28);
    z-index: 50;
    backdrop-filter: blur(2px);
  }

  .modal-card {
    position: fixed;
    top: 50%;
    left: 50%;
    translate: -50% -50%;
    z-index: 51;
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    width: min(460px, calc(100vw - 40px));
    box-shadow: 0 20px 60px -12px rgba(13, 10, 8, 0.16);
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
    font-family: var(--serif);
    font-size: 18px;
    font-weight: 500;
    letter-spacing: -0.015em;
    color: var(--ink);
    margin: 0;
  }

  .modal-body {
    padding: 18px 20px 14px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .modal-footer {
    padding: 12px 20px 16px;
    border-top: 1px solid var(--line-soft);
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

  .optional {
    font-family: var(--mono);
    font-size: 9.5px;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--ink-mute);
    font-weight: 400;
  }

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
  .field-input:focus { border-color: var(--arm-400); }

  .field-hint {
    font-size: 11px;
    color: var(--ink-mute);
    margin: 4px 0 0;
  }

  .save-error {
    font-size: 11.5px;
    color: #dc2626;
    margin: 0 0 8px;
    padding: 6px 10px;
    background: #fef2f2;
    border: 1px solid #fca5a5;
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
