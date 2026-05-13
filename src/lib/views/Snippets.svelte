<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { fly, fade, slide } from 'svelte/transition';
  import { flip } from 'svelte/animate';
  import { expoOut, cubicOut } from 'svelte/easing';
  import { snippets, fetchSnippets, type Snippet } from '../stores';

  type SortKey = 'newest' | 'oldest' | 'alpha' | 'most_used';

  let search = $state('');
  let sort = $state<SortKey>('newest');
  let modal = $state<{ mode: 'add' | 'edit'; snippet?: Snippet } | null>(null);
  let saving = $state(false);
  let deleteTarget = $state<number | null>(null);
  let draftTrigger = $state('');
  let draftExpansion = $state('');
  let triggerInput = $state<HTMLInputElement | null>(null);
  let expansionInput = $state<HTMLTextAreaElement | null>(null);

  const TRIGGER_LIMIT = 300;

  // ---- derived: filtered + sorted list ----
  const filtered = $derived((() => {
    const q = search.toLowerCase();
    let list = q
      ? $snippets.filter(
          s => s.trigger.toLowerCase().includes(q) || s.expansion.toLowerCase().includes(q)
        )
      : [...$snippets];

    if (sort === 'newest')    list.sort((a, b) => b.created_at.localeCompare(a.created_at));
    if (sort === 'oldest')    list.sort((a, b) => a.created_at.localeCompare(b.created_at));
    if (sort === 'alpha')     list.sort((a, b) => a.trigger.localeCompare(b.trigger));
    if (sort === 'most_used') list.sort((a, b) => b.use_count - a.use_count);

    return list;
  })());

  onMount(() => { fetchSnippets(); });

  // ---- modal helpers ----
  function openAdd() {
    draftTrigger = '';
    draftExpansion = '';
    modal = { mode: 'add' };
  }

  function openEdit(s: Snippet) {
    draftTrigger = s.trigger;
    draftExpansion = s.expansion;
    modal = { mode: 'edit', snippet: s };
  }

  function closeModal() {
    modal = null;
    deleteTarget = null;
  }

  async function saveModal() {
    const trimmedTrigger = draftTrigger.trim();
    const trimmedExpansion = draftExpansion.trim();
    if (!trimmedTrigger || !trimmedExpansion) return;

    saving = true;
    try {
      if (modal?.mode === 'add') {
        await invoke('create_snippet', { trigger: trimmedTrigger, expansion: trimmedExpansion });
      } else if (modal?.mode === 'edit' && modal.snippet) {
        await invoke('edit_snippet', { id: modal.snippet.id, trigger: trimmedTrigger, expansion: trimmedExpansion });
      }
      await fetchSnippets();
      closeModal();
    } catch (e) {
      console.error(e);
    } finally {
      saving = false;
    }
  }

  async function confirmDelete(id: number) {
    if (deleteTarget === id) {
      try {
        await invoke('remove_snippet', { id });
        await fetchSnippets();
      } catch (e) { console.error(e); }
      deleteTarget = null;
    } else {
      deleteTarget = id;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') closeModal();
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) saveModal();
  }

  // Auto-grow textarea
  function autoGrow(node: HTMLTextAreaElement) {
    function resize() {
      node.style.height = 'auto';
      node.style.height = node.scrollHeight + 'px';
    }
    node.addEventListener('input', resize);
    resize();
    return { destroy() { node.removeEventListener('input', resize); } };
  }

  // Focus trigger input when modal opens
  $effect(() => {
    if (modal && triggerInput) {
      setTimeout(() => triggerInput?.focus(), 50);
    }
  });

  // Deselect delete target when clicking elsewhere
  function maybeDeselect(id: number) {
    if (deleteTarget !== null && deleteTarget !== id) deleteTarget = null;
  }

  const sortLabels: { key: SortKey; label: string }[] = [
    { key: 'newest',    label: 'Newest'    },
    { key: 'oldest',    label: 'Oldest'    },
    { key: 'alpha',     label: 'A → Z'     },
    { key: 'most_used', label: 'Most used' },
  ];
</script>

<svelte:window on:keydown={modal ? handleKeydown : undefined} />

<div class="content-inner">
  <h1 class="page-h">Snippets</h1>
  <p class="page-sub">Speak a trigger and Open Flow expands it during dictation.</p>

  <div class="toolbar">
    <div class="search">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="11" cy="11" r="7"/><path d="m21 21-4.35-4.35"/>
      </svg>
      <input
        class="search-input"
        type="text"
        placeholder="Search snippets…"
        bind:value={search}
        aria-label="Search snippets"
      />
      {#if search}
        <button class="search-clear" onclick={() => search = ''} aria-label="Clear search">
          <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
        </button>
      {/if}
    </div>

    <div class="sort-pills">
      {#each sortLabels as { key, label }}
        <button
          class="sort-pill"
          class:active={sort === key}
          onclick={() => sort = key}
        >{label}</button>
      {/each}
    </div>

    <button class="btn-primary" onclick={openAdd}>
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 5v14M5 12h14"/></svg>
      <span>New snippet</span>
    </button>
  </div>

  {#if $snippets.length === 0}
    <div class="empty-state" in:fade={{ duration: 300 }}>
      <p class="empty-h">No snippets yet</p>
      <p class="empty-sub">Add a trigger phrase and Open Flow will expand it automatically during dictation.</p>
      <button class="btn-primary" onclick={openAdd}>
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 5v14M5 12h14"/></svg>
        New snippet
      </button>
    </div>
  {:else if filtered.length === 0}
    <div class="empty-state" in:fade={{ duration: 200 }}>
      <div class="empty-glyph">◌</div>
      <p class="empty-h">No matches</p>
      <p class="empty-sub">No snippets match <span class="empty-query">"{search}"</span>. Try a different search.</p>
      <button class="btn-ghost" onclick={() => search = ''}>Clear search</button>
    </div>
  {:else}
    <div class="list-wrap">
      <div class="list-row snip-row header">
        <div class="head">Trigger</div>
        <div class="head">Expansion</div>
        <div class="head"></div>
      </div>

      {#each filtered as s (s.id)}
        <div
          class="list-row snip-row"
          animate:flip={{ duration: 240, easing: cubicOut }}
          in:fly={{ y: 6, duration: 200, easing: expoOut }}
          out:fade={{ duration: 120 }}
          onclick={() => maybeDeselect(s.id)}
          role="row"
        >
          <div>
            <div class="snip-trigger">{s.trigger}</div>
            <div class="snip-meta">{s.use_count} {s.use_count === 1 ? 'use' : 'uses'}</div>
          </div>
          <div class="snip-text">{s.expansion}</div>
          <div class="row-actions">
            <button
              class="icon-btn"
              onclick={(e) => { e.stopPropagation(); deleteTarget = null; openEdit(s); }}
              aria-label="Edit snippet"
              title="Edit"
            >
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M12 20h9M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4z"/></svg>
            </button>
            <button
              class="icon-btn"
              class:delete-armed={deleteTarget === s.id}
              onclick={(e) => { e.stopPropagation(); confirmDelete(s.id); }}
              aria-label={deleteTarget === s.id ? 'Confirm delete' : 'Delete snippet'}
              title={deleteTarget === s.id ? 'Click again to confirm' : 'Delete'}
            >
              {#if deleteTarget === s.id}
                <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M20 6 9 17l-5-5"/></svg>
              {:else}
                <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M3 6h18"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><path d="m19 6-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/></svg>
              {/if}
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<!-- Modal -->
{#if modal}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div class="modal-backdrop" onclick={closeModal} in:fade={{ duration: 150 }} out:fade={{ duration: 100 }}></div>

  <div
    class="modal-card"
    role="dialog"
    aria-modal="true"
    aria-label={modal.mode === 'add' ? 'New snippet' : 'Edit snippet'}
    in:fly={{ y: 16, duration: 280, easing: expoOut }}
    out:fly={{ y: 8, duration: 160, easing: cubicOut }}
  >
    <div class="modal-header">
      <h2 class="modal-title">{modal.mode === 'add' ? 'New snippet' : 'Edit snippet'}</h2>
      <button class="icon-btn" onclick={closeModal} aria-label="Close">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
      </button>
    </div>

    <div class="modal-body">
      <label class="field-label" for="trigger-input">
        Trigger
        <span class="char-counter" class:over={draftTrigger.length > TRIGGER_LIMIT}>
          {draftTrigger.length}/{TRIGGER_LIMIT}
        </span>
      </label>
      <input
        id="trigger-input"
        class="field-input trigger-field"
        type="text"
        placeholder="e.g. my email"
        bind:value={draftTrigger}
        bind:this={triggerInput}
        maxlength={TRIGGER_LIMIT}
        autocomplete="off"
        spellcheck="false"
      />
      <p class="field-hint">Speak this phrase — Open Flow replaces it instantly.</p>

      <label class="field-label" for="expansion-input">Expansion</label>
      <textarea
        id="expansion-input"
        class="field-input expansion-field"
        placeholder="e.g. hello@example.com"
        bind:value={draftExpansion}
        bind:this={expansionInput}
        use:autoGrow
        rows="3"
        spellcheck="false"
      ></textarea>
      <p class="field-hint">The text that replaces the trigger. No length limit.</p>
    </div>

    <div class="modal-footer">
      <button class="btn-ghost" onclick={closeModal}>Cancel</button>
      <button
        class="btn-primary"
        onclick={saveModal}
        disabled={saving || !draftTrigger.trim() || !draftExpansion.trim() || draftTrigger.length > TRIGGER_LIMIT}
      >
        {#if saving}
          <span class="spinner"></span>
          Saving…
        {:else}
          {modal.mode === 'add' ? 'Add snippet' : 'Save changes'}
        {/if}
      </button>
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

  .page-sub { color: var(--ink-mute); font-size: 12.5px; margin: 0 0 22px; }

  /* ---- toolbar ---- */

  .toolbar {
    display: flex;
    gap: 8px;
    margin-bottom: 14px;
    align-items: center;
    flex-wrap: wrap;
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

  .search-clear {
    background: transparent;
    border: 0;
    padding: 0;
    color: var(--ink-mute);
    cursor: pointer;
    display: grid;
    place-items: center;
    border-radius: 4px;
    width: 18px; height: 18px;
  }

  .search-clear:hover { color: var(--ink-strong); background: var(--amber-100); }

  .sort-pills {
    display: flex;
    gap: 2px;
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 3px;
  }

  .sort-pill {
    background: transparent;
    border: 0;
    border-radius: 5px;
    padding: 3px 9px;
    font-size: 11.5px;
    font-family: var(--sans);
    color: var(--ink-mute);
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.12s, color 0.12s;
  }

  .sort-pill:hover { color: var(--ink-strong); background: var(--amber-50); }
  .sort-pill.active { background: var(--amber-100); color: var(--ink); font-weight: 500; }

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

  .btn-primary:disabled { opacity: 0.45; cursor: default; }
  .btn-primary:not(:disabled):hover { opacity: 0.85; }

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

  /* ---- list ---- */

  .list-wrap {
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    overflow: hidden;
    background: var(--bg-elev);
  }

  .list-row {
    display: grid;
    gap: 16px;
    padding: 11px 16px;
    border-bottom: 1px solid var(--line);
    align-items: center;
    font-size: 13px;
    color: var(--ink-strong);
  }

  .list-row:last-child { border-bottom: 0; }

  .list-row .head {
    font-family: var(--mono);
    font-size: 9.5px;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--ink-mute);
  }

  .list-row.header { padding: 8px 16px; background: var(--paper); }
  .snip-row { grid-template-columns: 1fr 1fr 64px; }

  .snip-trigger {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--accent-ink);
    font-weight: 500;
  }

  .snip-meta {
    font-size: 11px;
    color: var(--ink-mute);
    margin-top: 2px;
    font-family: var(--mono);
  }

  .snip-text {
    font-size: 13px;
    line-height: 1.45;
    color: var(--ink-strong);
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
  }

  .row-actions {
    display: flex;
    gap: 2px;
    justify-content: flex-end;
    opacity: 0;
    transition: opacity 0.15s;
  }

  .list-row:hover .row-actions { opacity: 1; }

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

  .icon-btn.delete-armed {
    background: #fef2f2;
    color: #dc2626;
  }

  .icon-btn.delete-armed:hover { background: #fee2e2; }

  /* ---- empty states ---- */

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 60px 24px;
    text-align: center;
    gap: 8px;
  }

  .empty-h {
    font-family: var(--serif);
    font-size: 18px;
    font-weight: 500;
    letter-spacing: -0.015em;
    color: var(--ink);
    margin: 0;
  }

  .empty-sub {
    font-size: 12.5px;
    color: var(--ink-mute);
    line-height: 1.5;
    margin: 0 0 10px;
    max-width: 320px;
  }

  .empty-query { font-family: var(--mono); color: var(--accent-ink); font-size: 12px; }

  /* ---- modal ---- */

  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(13, 10, 8, 0.3);
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
    width: min(520px, calc(100vw - 40px));
    box-shadow: 0 20px 60px -12px rgba(13, 10, 8, 0.18);
    display: flex;
    flex-direction: column;
    gap: 0;
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
    padding: 20px 20px 16px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .modal-footer {
    padding: 14px 20px 18px;
    border-top: 1px solid var(--line-soft);
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
  }

  .field-label {
    font-size: 11.5px;
    font-weight: 500;
    color: var(--ink-soft);
    letter-spacing: 0.01em;
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 5px;
    margin-top: 10px;
  }

  .field-label:first-child { margin-top: 0; }

  .char-counter {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--ink-mute);
    font-weight: 400;
  }

  .char-counter.over { color: #dc2626; }

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
  }

  .field-input:focus { border-color: var(--arm-400); }

  .trigger-field {
    font-family: var(--mono);
    font-size: 12.5px;
    color: var(--accent-ink);
  }

  .expansion-field {
    min-height: 72px;
    line-height: 1.5;
    overflow-y: hidden;
  }

  .field-hint {
    font-size: 11px;
    color: var(--ink-mute);
    margin: 3px 0 0;
    line-height: 1.4;
  }

  .spinner {
    display: inline-block;
    width: 12px; height: 12px;
    border: 1.5px solid rgba(249,247,243,0.35);
    border-top-color: var(--amber-50);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
    flex-shrink: 0;
  }

  @keyframes spin { to { transform: rotate(360deg); } }
</style>
