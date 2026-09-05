<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { emit, invoke, listen } from '../tauri';
  import { fade } from 'svelte/transition';
  import { motionMs } from '../motion';
  import { appStore, fetchDictionary, cancelDictionaryFetch, type DictionaryEntry } from '../stores';
  import type { SortKey } from './dictionary/helpers';
  import DictionaryToolbar from './dictionary/DictionaryToolbar.svelte';
  import DictionaryList from './dictionary/DictionaryList.svelte';
  import DictionaryInspector from './dictionary/DictionaryInspector.svelte';
  import DictionaryModal from './dictionary/DictionaryModal.svelte';

  let search = $state('');
  let debouncedSearch = $state('');
  let sort = $state<SortKey>('newest');
  let selected = $state<DictionaryEntry | null>(null);
  let modal = $state<{ mode: 'add' | 'edit'; entry?: DictionaryEntry } | null>(null);
  let deleteTarget = $state<number | null>(null);
  let inspectorDir = $state<1 | -1>(1);
  let leavingIds = $state<Set<number>>(new Set());

  $effect(() => {
    const currentSearch = search;
    const timer = window.setTimeout(() => { debouncedSearch = currentSearch; }, 120);
    return () => window.clearTimeout(timer);
  });

  const filtered = $derived.by(() => {
    const q = debouncedSearch.trim().toLowerCase();
    let list = q
      ? appStore.dictionary.filter(e =>
          e.term.toLowerCase().includes(q) ||
          (e.mistake ?? '').toLowerCase().includes(q)
        )
      : [...appStore.dictionary];

    if (sort === 'newest')         list.sort((a, b) => b.created_at.localeCompare(a.created_at));
    if (sort === 'oldest')         list.sort((a, b) => a.created_at.localeCompare(b.created_at));
    if (sort === 'alpha')          list.sort((a, b) => a.term.localeCompare(b.term));
    if (sort === 'most_corrected') list.sort((a, b) => b.correction_count - a.correction_count);

    return list;
  });
  const visibleFiltered = $derived(filtered.filter((entry) => !leavingIds.has(entry.id)));

  onMount(() => {
    let unlisten: (() => void) | undefined;
    fetchDictionary();
    listen('verenu:dictionary-updated', () => fetchDictionary())
      .then((cleanup) => { unlisten = cleanup; })
      .catch(() => {});
    return () => { unlisten?.(); };
  });

  function selectRow(e: DictionaryEntry) {
    if (selected?.id === e.id) {
      inspectorDir = -1;
      selected = null;
    } else {
      inspectorDir = 1;
      selected = e;
    }
    deleteTarget = null;
  }

  function openAdd() {
    modal = { mode: 'add' };
  }

  function openEdit(e: DictionaryEntry) {
    modal = { mode: 'edit', entry: e };
  }

  function closeModal() { modal = null; }

  function upsertDictionaryEntry(entry: DictionaryEntry) {
    cancelDictionaryFetch();
    const next = appStore.dictionary.filter((item) => item.id !== entry.id);
    appStore.dictionary = [entry, ...next];
  }

  function handleSaved(entry: DictionaryEntry) {
    upsertDictionaryEntry(entry);
    selected = entry;
  }

  async function confirmDelete(id: number) {
    if (deleteTarget === id) {
      try {
        if (leavingIds.has(id)) return;
        if (selected?.id === id) {
          inspectorDir = -1;
          selected = null;
          await tick();
        }
        leavingIds = new Set(leavingIds).add(id);
        window.setTimeout(async () => {
          try {
            await invoke('remove_dictionary_entry', { id });
            cancelDictionaryFetch();
            appStore.dictionary = appStore.dictionary.filter((entry) => entry.id !== id);
          } catch (err) {
            console.error(err);
            await emit('verenu:error', 'Could not delete dictionary term.');
          } finally {
            const nextLeaving = new Set(leavingIds);
            nextLeaving.delete(id);
            leavingIds = nextLeaving;
          }
        }, motionMs(200));
      } catch (err) { console.error(err); }
      deleteTarget = null;
    } else {
      deleteTarget = id;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && !modal) {
      selected = null;
      // Escape is the "back out" key: disarm a pending destructive delete too,
      // so re-selecting the same row doesn't resurrect the Confirm state.
      deleteTarget = null;
    }
  }

  function goToSnippets() {
    closeModal();
    appStore.currentPage = 'snippets';
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="content-inner">
  <h1 class="page-h">Dictionary</h1>
  <p class="page-sub">Your personal vocabulary. Add words or phrases the AI should know — names, brands, jargon, anything niche. They get injected into every transcription so the AI recognises them and uses your exact spelling.</p>
  {#if appStore.dictionaryFetchStatus === 'error' && appStore.dictionary.length > 0}
    <div class="load-warning" role="alert" aria-live="assertive">
      <span>{appStore.dictionaryFetchError || 'Unable to load dictionary terms.'} Check backend connection and retry.</span>
      <button type="button" class="load-warning-retry" onclick={() => fetchDictionary()}>Retry</button>
    </div>
  {/if}

  <DictionaryToolbar bind:search bind:sort count={appStore.dictionary.length} onAdd={openAdd} />

  {#if appStore.dictionaryFetchStatus === 'loading' && appStore.dictionary.length === 0}
    <div class="empty-state" role="status" aria-live="polite" in:fade={{ duration: 220 }}>
      <p class="empty-h">Loading terms…</p>
      <p class="empty-sub">Fetching your dictionary from the backend.</p>
    </div>
  {:else if appStore.dictionaryFetchStatus === 'error' && appStore.dictionary.length === 0}
    <div class="empty-state empty-state-error" role="alert" in:fade={{ duration: 220 }}>
      <p class="empty-h">Could not load dictionary</p>
      <p class="empty-sub">The backend is unavailable right now. {appStore.dictionaryFetchError}</p>
      <button type="button" class="btn-ghost" onclick={() => fetchDictionary()}>Try again</button>
    </div>
  {:else if appStore.dictionary.length === 0}
    <div class="empty-state" in:fade={{ duration: 220 }}>
      <p class="empty-h">No terms yet</p>
      <p class="empty-sub">Add words the AI should know — names, brands, jargon, anything a generic model is unlikely to get right.</p>
      <button class="btn-primary" onclick={openAdd}>
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 5v14M5 12h14"/></svg>
        Add term
      </button>
    </div>
  {:else}
    {#if appStore.dictionaryFetchStatus === 'loading'}
      <p class="fetch-status" role="status" aria-live="polite">Refreshing dictionary…</p>
    {:else if appStore.dictionaryFetchStatus === 'error'}
      <p class="fetch-status fetch-status-error" role="alert">Refresh failed: {appStore.dictionaryFetchError}</p>
    {/if}
    <div class="dict-layout">
      <DictionaryList
        {filtered}
        {visibleFiltered}
        {search}
        selectedId={selected?.id ?? null}
        onSelect={selectRow}
        onClearSearch={() => search = ''}
      />
      <DictionaryInspector
        {selected}
        {inspectorDir}
        {deleteTarget}
        onEdit={openEdit}
        onDelete={confirmDelete}
      />
    </div>
  {/if}
</div>

{#if modal}
  <DictionaryModal
    mode={modal.mode}
    entry={modal.entry}
    onClose={closeModal}
    onSaved={handleSaved}
    onGoToSnippets={goToSnippets}
  />
{/if}

<style>
  .content-inner {
    width: min(100%, var(--page-max));
    margin-inline: auto;
    padding: var(--page-pad-y) var(--page-pad-x) 36px;
    min-width: 0;
  }

  .page-h {
    font-family: var(--sans);
    font-size: 23px;
    font-weight: 600;
    letter-spacing: -0.025em;
    margin: 0 0 4px;
    line-height: 1.1;
    color: var(--ink);
  }

  .page-sub { color: var(--ink-mute); font-size: 12.5px; margin: 0 0 22px; max-width: 560px; line-height: 1.5; }

  .fetch-status {
    margin: 0 0 10px;
    font-size: 12px;
    color: var(--ink-mute);
  }

  .fetch-status-error {
    color: var(--danger);
  }

  .load-warning {
    margin: 0 0 16px;
    padding: 8px 10px;
    border-radius: var(--r-sm);
    border: 1px solid var(--danger-line);
    background: var(--danger-bg);
    color: var(--danger);
    font-size: 11.5px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .load-warning-retry {
    border: 1px solid var(--danger-line);
    background: transparent;
    color: var(--danger);
    font-family: var(--sans);
    font-size: 11px;
    border-radius: 6px;
    padding: 4px 8px;
    cursor: pointer;
  }

  .load-warning-retry:hover {
    background: color-mix(in oklab, var(--danger-bg) 60%, transparent);
  }

  .dict-layout {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 18px;
    align-items: start;
  }

  .empty-state {
    padding: 52px 4px;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 6px;
  }

  .empty-h {
    font-family: var(--sans);
    font-style: normal;
    font-size: 15px;
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

  @media (max-width: 1060px) {
    .dict-layout { grid-template-columns: 1fr; }
  }
</style>
