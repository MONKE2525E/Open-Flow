<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { emit, invoke } from '../tauri';
  import { fade } from 'svelte/transition';
  import { appStore, fetchSnippets, cancelSnippetsFetch, type Snippet } from '../stores';
  import { motionMs } from '../motion';
  import type { SortKey } from './snippets/helpers';
  import SnippetToolbar from './snippets/SnippetToolbar.svelte';
  import SnippetList from './snippets/SnippetList.svelte';
  import SnippetInspector from './snippets/SnippetInspector.svelte';
  import SnippetModal from './snippets/SnippetModal.svelte';

  let search = $state('');
  let debouncedSearch = $state('');
  let sort = $state<SortKey>('newest');
  let selected = $state<Snippet | null>(null);
  let modal = $state<{ mode: 'add' | 'edit'; snippet?: Snippet } | null>(null);
  let deleteTarget = $state<number | null>(null);
  let inspectorDir = $state<1 | -1>(1);
  let leavingIds = $state<Set<number>>(new Set());

  $effect(() => {
    const currentSearch = search;
    const timer = window.setTimeout(() => { debouncedSearch = currentSearch; }, 120);
    return () => window.clearTimeout(timer);
  });
  const filtered = $derived.by(() => {
    const q = debouncedSearch.toLowerCase();
    let list = q
      ? appStore.snippets.filter(s =>
          s.trigger.toLowerCase().includes(q) || s.expansion.toLowerCase().includes(q)
        )
      : [...appStore.snippets];

    if (sort === 'newest')    list.sort((a, b) => b.created_at.localeCompare(a.created_at));
    if (sort === 'oldest')    list.sort((a, b) => a.created_at.localeCompare(b.created_at));
    if (sort === 'alpha')     list.sort((a, b) => a.trigger.localeCompare(b.trigger));
    if (sort === 'most_used') list.sort((a, b) => b.use_count - a.use_count);

    return list;
  });
  const visibleFiltered = $derived(filtered.filter((snippet) => !leavingIds.has(snippet.id)));

  onMount(() => { fetchSnippets(); });

  function selectRow(s: Snippet) {
    if (selected?.id === s.id) {
      inspectorDir = -1;
      selected = null;
    } else {
      inspectorDir = 1;
      selected = s;
    }
    deleteTarget = null;
  }

  function openAdd() {
    modal = { mode: 'add' };
  }

  function openEdit(s: Snippet) {
    modal = { mode: 'edit', snippet: s };
  }

  function closeModal() { modal = null; }

  function upsertSnippet(snippet: Snippet) {
    cancelSnippetsFetch();
    const next = appStore.snippets.filter((entry) => entry.id !== snippet.id);
    appStore.snippets = [snippet, ...next];
  }

  function handleSaved(snippet: Snippet) {
    upsertSnippet(snippet);
    selected = snippet;
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
            await invoke('remove_snippet', { id });
            cancelSnippetsFetch();
            appStore.snippets = appStore.snippets.filter((entry) => entry.id !== id);
          } catch (err) {
            console.error(err);
            await emit('verenu:error', 'Could not delete snippet.');
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
    if (e.key === 'Escape' && !modal) selected = null;
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="content-inner">
  <h1 class="page-h">Snippets</h1>
  <p class="page-sub">Speak a trigger and Verenu expands it during dictation.</p>
  {#if appStore.snippetsFetchStatus === 'error' && appStore.snippets.length > 0}
    <div class="load-warning" role="alert" aria-live="assertive">
      <span>{appStore.snippetsFetchError || 'Unable to load snippets.'} Check backend connection and retry.</span>
      <button type="button" class="load-warning-retry" onclick={() => fetchSnippets()}>Retry</button>
    </div>
  {/if}

  <SnippetToolbar bind:search bind:sort onNew={openAdd} />

  {#if appStore.snippetsFetchStatus === 'loading' && appStore.snippets.length === 0}
    <div class="empty-state" role="status" aria-live="polite" in:fade={{ duration: 250 }}>
      <p class="empty-h">Loading snippets…</p>
      <p class="empty-sub">Fetching snippets from the backend.</p>
    </div>
  {:else if appStore.snippetsFetchStatus === 'error' && appStore.snippets.length === 0}
    <div class="empty-state empty-state-error" role="alert" in:fade={{ duration: 250 }}>
      <p class="empty-h">Could not load snippets</p>
      <p class="empty-sub">The backend is unavailable right now. {appStore.snippetsFetchError}</p>
      <button type="button" class="btn-ghost" onclick={() => fetchSnippets()}>Try again</button>
    </div>
  {:else if appStore.snippets.length === 0}
    <div class="empty-state" in:fade={{ duration: 250 }}>
      <p class="empty-h">No snippets yet</p>
      <p class="empty-sub">Add a trigger phrase and Verenu will expand it automatically during dictation.</p>
      <button class="btn-primary" onclick={openAdd}>
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 5v14M5 12h14"/></svg>
        New snippet
      </button>
    </div>
  {:else}
    {#if appStore.snippetsFetchStatus === 'loading'}
      <p class="fetch-status" role="status" aria-live="polite">Refreshing snippets…</p>
    {:else if appStore.snippetsFetchStatus === 'error'}
      <p class="fetch-status fetch-status-error" role="alert">Refresh failed: {appStore.snippetsFetchError}</p>
    {/if}
    <div class="snip-layout">
      <SnippetList
        {filtered}
        {visibleFiltered}
        {search}
        selectedId={selected?.id ?? null}
        onSelect={selectRow}
        onClearSearch={() => search = ''}
      />
      <SnippetInspector
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
  <SnippetModal
    mode={modal.mode}
    snippet={modal.snippet}
    onClose={closeModal}
    onSaved={handleSaved}
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
    font-family: var(--serif);
    font-size: 26px;
    font-weight: 500;
    letter-spacing: -0.02em;
    margin: 0 0 4px;
    line-height: 1.1;
    color: var(--ink);
  }

  .page-sub { color: var(--ink-mute); font-size: 12.5px; margin: 0 0 22px; }

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

  .snip-layout {
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

  @media (max-width: 1060px) {
    .snip-layout {
      grid-template-columns: 1fr;
    }
  }
</style>
