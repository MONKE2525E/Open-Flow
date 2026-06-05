<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke, listen } from '../tauri';
  import { fly, fade } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { MOTION_MS, MOTION_PX, motionMs, motionPx } from '../motion';
  import { appStore, fetchDictionary, type DictionaryEntry } from '../stores';
  import MicInputButton from '../components/MicInputButton.svelte';

  type SortKey = 'newest' | 'oldest' | 'alpha' | 'most_corrected';
  type CreatedRecordMeta = { id: number; created_at: string };

  function fmtDate(iso: string): string {
    try {
      const MS_PER_DAY = 86_400_000;
      const d = new Date(/[Z+]/.test(iso) ? iso : iso + 'Z');
      const diffDays = Math.floor((Date.now() - d.getTime()) / MS_PER_DAY);
      if (diffDays === 0) return 'Today';
      if (diffDays === 1) return 'Yesterday';
      if (diffDays < 7) return `${diffDays}d ago`;
      return d.toLocaleDateString([], { month: 'short', day: 'numeric' });
    } catch { return iso.slice(0, 10); }
  }

  function confidenceLabel(tier?: string | null): string {
    if (tier === 'high') return 'High confidence';
    if (tier === 'medium') return 'Medium confidence';
    if (tier === 'low') return 'Low confidence';
    if (tier === 'manual') return 'Manual';
    return 'Unknown confidence';
  }

  let search        = $state('');
  let debouncedSearch = $state('');
  let sort          = $state<SortKey>('newest');
  let selected      = $state<DictionaryEntry | null>(null);
  let modal         = $state<{ mode: 'add' | 'edit'; entry?: DictionaryEntry } | null>(null);
  let saving        = $state(false);
  let saveError     = $state('');
  let deleteTarget  = $state<number | null>(null);
  let draftTerm     = $state('');
  let draftMistake  = $state('');
  let termInput     = $state<HTMLInputElement | null>(null);
  let mistakeInput  = $state<HTMLInputElement | null>(null);
  let inspectorDir  = $state<1 | -1>(1);
  let sortWrapEl    = $state<HTMLDivElement | null>(null);
  let sortButtonEls = $state<Record<SortKey, HTMLButtonElement | null>>({
    newest: null, oldest: null, alpha: null, most_corrected: null,
  });
  let sortIndicatorStyle = $state('opacity:0;');

  const TERM_LIMIT    = 120;
  const MISTAKE_LIMIT = 120;

  const countCodePoints = (value: string): number => [...value].length;

  function requireCreatedRecordMeta(value: unknown, command: string): CreatedRecordMeta {
    console.info(`${command} result:`, value);
    if (typeof value !== 'object' || value === null) {
      throw new Error('Save returned no record metadata. Relaunch the Tauri app and try again.');
    }
    const meta = value as Partial<CreatedRecordMeta>;
    if (typeof meta.id !== 'number' || !Number.isFinite(meta.id) || typeof meta.created_at !== 'string' || !meta.created_at.trim()) {
      throw new Error('Save returned invalid record metadata. Check the app logs before retrying.');
    }
    return { id: meta.id, created_at: meta.created_at };
  }

  const sortLabels: { key: SortKey; label: string }[] = [
    { key: 'newest',         label: 'Newest'         },
    { key: 'oldest',         label: 'Oldest'         },
    { key: 'alpha',          label: 'A → Z'          },
    { key: 'most_corrected', label: 'Most corrected' },
  ];

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

  onMount(() => {
    let unlisten: (() => void) | undefined;
    fetchDictionary();
    listen('open-flow:dictionary-updated', () => fetchDictionary())
      .then((cleanup) => { unlisten = cleanup; })
      .catch(() => {});
    updateSortIndicator();
    const onResize = () => updateSortIndicator();
    window.addEventListener('resize', onResize);
    return () => { unlisten?.(); window.removeEventListener('resize', onResize); };
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
    draftTerm = ''; draftMistake = '';
    modal = { mode: 'add' };
  }

  function openEdit(e: DictionaryEntry) {
    draftTerm    = e.term;
    draftMistake = e.mistake ?? '';
    modal = { mode: 'edit', entry: e };
  }

  function closeModal() { modal = null; saveError = ''; }

  function upsertDictionaryEntry(entry: DictionaryEntry) {
    const next = appStore.dictionary.filter((item) => item.id !== entry.id);
    appStore.dictionary = [entry, ...next];
  }

  async function saveModal() {
    // Read directly from DOM elements at click time to bypass WKWebView
    // bind:value paste-sync lag, matching the pattern in Snippets.svelte.
    if (termInput) draftTerm = termInput.value;
    if (mistakeInput) draftMistake = mistakeInput.value;

    const term = draftTerm.trim();
    const mistakeValue = draftMistake.trim();
    const mistake = mistakeValue || null;
    if (!term) return;
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
      const editedId = modal?.mode === 'edit' ? modal.entry?.id : undefined;
      if (modal?.mode === 'add') {
        const created = requireCreatedRecordMeta(
          await invoke<unknown>('create_dictionary_entry', { term, mistake }),
          'create_dictionary_entry',
        );
        const entry: DictionaryEntry = {
          id: created.id,
          term,
          mistake,
          auto_learned: false,
          correction_count: 0,
          confidence_tier: 'manual',
          last_seen_at: null,
          created_at: created.created_at,
        };
        upsertDictionaryEntry(entry);
        selected = entry;
      } else if (modal?.mode === 'edit' && modal.entry) {
        await invoke('edit_dictionary_entry', { id: modal.entry.id, term, mistake });
        const entry: DictionaryEntry = {
          ...modal.entry,
          term,
          mistake,
        };
        upsertDictionaryEntry(entry);
        selected = entry;
      }
      if (editedId !== undefined) {
        selected = appStore.dictionary.find(e => e.id === editedId) ?? null;
      }
      closeModal();
    } catch (err) {
      const msg = String(err);
      saveError = msg.includes('UNIQUE') ? 'That term already exists.' : (msg || 'Failed to save.');
    } finally { saving = false; }
  }

  async function confirmDelete(id: number) {
    if (deleteTarget === id) {
      try {
        await invoke('remove_dictionary_entry', { id });
        appStore.dictionary = appStore.dictionary.filter((entry) => entry.id !== id);
        if (selected?.id === id) selected = null;
      } catch (err) { console.error(err); }
      deleteTarget = null;
    } else {
      deleteTarget = id;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      if (modal) closeModal();
      else selected = null;
    }
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey) && modal) saveModal();
  }

  $effect(() => {
    if (modal && termInput) setTimeout(() => termInput?.focus(), 50);
  });

  function updateSortIndicator() {
    const wrap = sortWrapEl;
    const btn  = sortButtonEls[sort];
    if (!wrap || !btn) return;
    const wrapRect = wrap.getBoundingClientRect();
    const btnRect  = btn.getBoundingClientRect();
    const left  = Math.round(btnRect.left - wrapRect.left);
    const width = Math.round(btnRect.width);
    sortIndicatorStyle = `opacity:1; transform:translateX(${left}px); width:${width}px; transition: transform ${motionMs(MOTION_MS.base)}ms cubic-bezier(0.22, 1, 0.36, 1), width ${motionMs(MOTION_MS.base)}ms cubic-bezier(0.22, 1, 0.36, 1), opacity ${motionMs(MOTION_MS.fast)}ms ease;`;
  }

  $effect(() => {
    sort;
    setTimeout(updateSortIndicator, 0);
  });
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

  <div class="toolbar">
    <div class="search">
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="11" cy="11" r="7"/><path d="m21 21-4.35-4.35"/>
      </svg>
      <input
        class="search-input"
        type="text"
        placeholder={`Search ${appStore.dictionary.length} ${appStore.dictionary.length === 1 ? 'term' : 'terms'}…`}
        bind:value={search}
        aria-label="Search dictionary"
      />
      {#if search}
        <button class="clear-btn" onclick={() => search = ''} aria-label="Clear">
          <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
        </button>
      {/if}
    </div>

    <div class="sort-pills" bind:this={sortWrapEl}>
      <span class="sort-indicator" style={sortIndicatorStyle}></span>
      {#each sortLabels as { key, label }}
        <button
          class="sort-pill"
          class:active={sort === key}
          aria-pressed={sort === key}
          bind:this={sortButtonEls[key]}
          onclick={() => { sort = key; }}
        >{label}</button>
      {/each}
    </div>

    <button class="btn-primary" onclick={openAdd}>
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 5v14M5 12h14"/></svg>
      Add term
    </button>
  </div>

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

      <!-- Left: list -->
      <div class="dict-list-col">
        {#if filtered.length === 0}
          <div class="empty-state" in:fade={{ duration: 200 }}>
            <p class="empty-h">No matches</p>
            <p class="empty-sub">Nothing matches "{search}".</p>
            <button class="btn-ghost" onclick={() => search = ''}>Clear search</button>
          </div>
        {:else}
          <div class="dict-list">
            {#each filtered as e (e.id)}
              <button
                type="button"
                class="dict-row"
                class:is-selected={selected?.id === e.id}
                aria-pressed={selected?.id === e.id}
                onclick={() => selectRow(e)}
              >
                <span class="dict-left">
                  <span class="dict-main">
                    <span class="dict-term">{e.term}</span>
                    {#if e.auto_learned}
                      <svg class="dict-auto-star" width="11" height="11" viewBox="0 0 24 24" fill="currentColor" aria-label="Auto-learned">
                        <title>Added automatically by Auto-learn</title>
                        <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z"/>
                      </svg>
                    {/if}
                    {#if e.mistake}
                      <span class="dict-often-label">often:</span>
                      <span class="dict-mistake">"{e.mistake}"</span>
                    {/if}
                  </span>
                </span>
                <span class="dict-meta">
                  {#if e.correction_count > 0}
                    <span>{e.correction_count} {e.correction_count === 1 ? 'correction' : 'corrections'}</span>
                  {/if}
                  {#if e.auto_learned}
                    <span>{confidenceLabel(e.confidence_tier)}</span>
                  {/if}
                  <span>{fmtDate(e.created_at)}</span>
                </span>
              </button>
            {/each}
          </div>
        {/if}
      </div>

      <!-- Right: inspector -->
      <div class="inspector-col">
        {#if selected}
          {#key selected.id}
            <div
              class="inspector"
              in:fly={{ x: inspectorDir * motionPx(MOTION_PX.panel), duration: motionMs(MOTION_MS.panel), easing: expoOut }}
              out:fade={{ duration: 0 }}
            >
              <div class="insp-trigger">{selected.term}</div>

              {#if selected.mistake}
                <div class="insp-often">
                  <span class="insp-often-label">often:</span>
                  <span class="insp-often-text">"{selected.mistake}"</span>
                </div>
              {/if}

              {#if selected.auto_learned}
                <div class="insp-auto-badge">
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="currentColor">
                    <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z"/>
                  </svg>
                  Auto-learned
                </div>
                <div class="insp-often">
                  <span class="insp-often-label">Confidence:</span>
                  <span class="insp-often-text">{confidenceLabel(selected.confidence_tier)}</span>
                </div>
              {/if}

              <div class="insp-divider"></div>

              <div class="insp-stats">
                {#if selected.correction_count > 0}
                  <div class="insp-stat-row">
                    <span class="insp-stat-num">{selected.correction_count}</span>
                    <span class="insp-stat-label">{selected.correction_count === 1 ? 'correction' : 'corrections'}</span>
                  </div>
                {/if}
                <div class="insp-stat-row">
                  <span class="insp-stat-label">Added</span>
                  <span class="insp-stat-date">{fmtDate(selected.created_at)}</span>
                </div>
                {#if selected.last_seen_at}
                  <div class="insp-stat-row">
                    <span class="insp-stat-label">Last seen</span>
                    <span class="insp-stat-date">{fmtDate(selected.last_seen_at)}</span>
                  </div>
                {/if}
              </div>

              <div class="insp-actions">
                <button class="btn-insp-edit" onclick={() => openEdit(selected!)}>
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M12 20h9M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4z"/></svg>
                  Edit
                </button>
                <button
                  class="btn-insp-delete"
                  class:armed={deleteTarget === selected.id}
                  onclick={() => confirmDelete(selected!.id)}
                >
                  {#if deleteTarget === selected.id}
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M20 6 9 17l-5-5"/></svg>
                    Confirm
                  {:else}
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M3 6h18"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><path d="m19 6-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/></svg>
                    Delete
                  {/if}
                </button>
              </div>
            </div>
          {/key}
        {:else}
          <div class="inspector-empty" in:fade={{ duration: motionMs(MOTION_MS.base) }}>
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" style="color:var(--arm-300)">
              <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"/>
            </svg>
            <p>Select a term<br>to inspect it</p>
          </div>
        {/if}
      </div>

    </div>
  {/if}
</div>

{#if modal}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <button class="modal-backdrop" aria-label="Close dialog" onclick={closeModal} in:fade={{ duration: 150 }} out:fade={{ duration: 100 }}></button>
  <div
    class="modal-card"
    role="dialog"
    aria-modal="true"
    in:fly={{ y: 14, duration: 260, easing: expoOut }}
    out:fly={{ y: 8, duration: 150, easing: expoOut }}
  >
    <div class="modal-header">
      <h2 class="modal-title">{modal?.mode === 'add' ? 'Add term' : 'Edit term'}</h2>
      <button class="icon-btn" onclick={closeModal} aria-label="Close">
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
          placeholder="e.g. koobernetes, byork"
          bind:value={draftMistake}
          bind:this={mistakeInput}
          autocomplete="off"
          spellcheck="false"
        />
        <MicInputButton onResult={(t) => draftMistake = t} />
      </div>
      <p class="field-hint">What the transcription model typically writes instead. Skip if the term just needs to be in the AI's awareness.</p>
    </div>

    <div class="modal-footer">
      {#if saveError}
        <p class="save-error">{saveError}</p>
      {/if}
      {#if draftTerm.length >= TERM_LIMIT}
        <button
          class="snippet-nudge"
          onclick={() => { closeModal(); appStore.currentPage = 'snippets'; }}
          in:fly={{ y: 5, duration: 220, easing: expoOut }}
          out:fade={{ duration: 100 }}
        >Maybe this would be better as a snippet.</button>
      {/if}
      <div class="footer-actions">
        <button class="btn-ghost" onclick={closeModal}>Cancel</button>
        <button
          class="btn-primary"
          onclick={saveModal}
          disabled={saving || !draftTerm.trim()}
        >
          {#if saving}<span class="spinner"></span>{/if}
          {modal?.mode === 'add' ? 'Add term' : 'Save changes'}
        </button>
      </div>
    </div>
  </div>
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

  /* ── toolbar ── */

  .toolbar {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: 18px;
    flex-wrap: wrap;
  }

  .search {
    flex: 1 1 260px;
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

  .sort-pills {
    display: flex;
    gap: 2px;
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 3px;
    position: relative;
    overflow: hidden;
  }

  .sort-indicator {
    position: absolute;
    top: 3px;
    left: 3px;
    height: calc(100% - 6px);
    border-radius: 5px;
    background: var(--accent-soft);
    z-index: 0;
    pointer-events: none;
    opacity: 0;
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
    position: relative;
    z-index: 1;
  }
  .sort-pill:hover { color: var(--ink-strong); background: var(--control-hover); }
  .sort-pill.active { color: var(--accent-ink); font-weight: 500; }

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
  .btn-ghost:hover { background: var(--control-hover); color: var(--ink-strong); }

  /* ── two-column layout ── */

  .dict-layout {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 18px;
    align-items: start;
  }

  /* ── list column ── */

  .dict-list-col { min-width: 0; }

  .dict-list {
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    overflow: hidden;
    background: var(--bg-elev);
  }

  .dict-row {
    border: 0;
    background: transparent;
    width: 100%;
    text-align: left;
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: center;
    gap: 16px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--line);
    cursor: pointer;
    transition: background 0.12s;
  }
  .dict-row:last-child { border-bottom: 0; }
  .dict-row:hover { background: var(--control-hover); }
  .dict-row.is-selected { background: var(--control-active); }
  .dict-row:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .dict-left { display: block; min-width: 0; overflow: hidden; }

  .dict-main {
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
  }

  .dict-term {
    font-size: 13px;
    font-weight: 500;
    color: var(--ink);
    flex-shrink: 0;
  }

  .dict-auto-star {
    color: var(--accent);
    flex-shrink: 0;
    position: relative;
    top: -1px;
  }

  .dict-often-label {
    font-family: var(--mono);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--ink-faint);
    flex-shrink: 0;
  }

  .dict-mistake {
    font-size: 12.5px;
    color: var(--ink-mute);
    font-style: italic;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .dict-meta {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
    font-size: 11px;
    color: var(--ink-faint);
    white-space: nowrap;
  }

  /* ── inspector column ── */

  .inspector-col {
    position: sticky;
    top: 0;
  }

  .inspector {
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    padding: 20px 22px;
    display: flex;
    flex-direction: column;
  }

  .insp-trigger {
    font-family: var(--serif);
    font-size: 19px;
    font-weight: 500;
    letter-spacing: -0.015em;
    color: var(--ink);
    line-height: 1.2;
  }

  .insp-often {
    display: flex;
    align-items: baseline;
    gap: 6px;
    margin-top: 8px;
  }

  .insp-often-label {
    font-family: var(--mono);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--ink-faint);
    flex-shrink: 0;
  }

  .insp-often-text {
    font-size: 13px;
    color: var(--ink-soft);
    font-style: italic;
    line-height: 1.5;
    word-break: break-word;
  }

  .insp-auto-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin-top: 12px;
    padding: 4px 9px;
    background: var(--accent-soft);
    color: var(--accent-ink);
    border-radius: 99px;
    font-size: 11px;
    font-weight: 500;
    align-self: flex-start;
  }

  .insp-divider {
    height: 1px;
    background: var(--line-soft);
    margin: 18px 0 14px;
  }

  .insp-stats {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 18px;
  }

  .insp-stat-row {
    display: flex;
    align-items: baseline;
    gap: 7px;
  }

  .insp-stat-num {
    font-family: var(--serif);
    font-size: 22px;
    font-weight: 500;
    letter-spacing: -0.02em;
    color: var(--accent-ink);
    line-height: 1;
  }

  .insp-stat-label { font-size: 11.5px; color: var(--ink-mute); }

  .insp-stat-date {
    font-size: 12.5px;
    color: var(--ink-soft);
    font-weight: 500;
    margin-left: auto;
  }

  .insp-actions { display: flex; gap: 8px; }

  .btn-insp-edit {
    flex: 1;
    background: var(--ink);
    color: var(--amber-50);
    border: 0;
    border-radius: 8px;
    padding: 7px 14px;
    font-size: 12.5px;
    font-weight: 500;
    font-family: var(--sans);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    cursor: pointer;
    transition: opacity 0.15s;
  }
  .btn-insp-edit:hover { opacity: 0.82; }

  .btn-insp-delete {
    background: transparent;
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 7px 14px;
    font-size: 12.5px;
    font-family: var(--sans);
    color: var(--ink-soft);
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    transition: background 0.12s, color 0.12s, border-color 0.12s;
  }
  .btn-insp-delete:hover { background: var(--control-hover); color: var(--ink-strong); }
  .btn-insp-delete.armed { background: var(--danger-bg); color: var(--danger); border-color: var(--danger-line); }
  .btn-insp-delete.armed:hover { background: var(--danger-bg); }

  .inspector-empty {
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    padding: 40px 22px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    text-align: center;
  }

  .inspector-empty p {
    font-family: var(--serif);
    font-style: italic;
    font-size: 14px;
    color: var(--ink-mute);
    margin: 0;
    line-height: 1.6;
  }

  /* ── empty states ── */

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

  /* ── icon button (modal close) ── */

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

  /* ── modal ── */

  .modal-backdrop {
    position: fixed;
    inset: 0;
    border: 0;
    padding: 0;
    appearance: none;
    background: var(--overlay);
    z-index: 50;
    outline: none;
    /* NOTE: no backdrop-filter — on WKWebView it repaints on layout changes and
       captures pointer events over the modal card, killing the dialog controls. */
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

  /* ── responsive ── */

  @media (max-width: 1060px) {
    .dict-layout { grid-template-columns: 1fr; }
    .inspector-col { position: static; }
  }

  @media (max-width: 720px) {
    .search { flex-basis: 100%; }
    .sort-pills { order: 3; width: 100%; overflow-x: auto; }
  }
</style>
