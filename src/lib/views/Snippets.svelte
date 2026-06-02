<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '../tauri';
  import { fly, fade } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { appStore, fetchSnippets, type Snippet } from '../stores';
  import MicInputButton from '../components/MicInputButton.svelte';
  import { MOTION_MS, MOTION_PX, motionMs, motionPx } from '../motion';

  type SortKey = 'newest' | 'oldest' | 'alpha' | 'most_used';

  function fmtDate(iso: string): string {
    try {
      const MS_PER_DAY = 86_400_000;
      const d = new Date(/[Z+]/.test(iso) ? iso : iso + 'Z');
      const diffDays = Math.floor((Date.now() - d.getTime()) / MS_PER_DAY);
      if (diffDays === 0) return 'Today';
      if (diffDays === 1) return 'Yesterday';
      if (diffDays < 7)  return `${diffDays}d ago`;
      return d.toLocaleDateString([], { month: 'short', day: 'numeric' });
    } catch { return iso.slice(0, 10); }
  }

  let search    = $state('');
  let debouncedSearch = $state('');
  let sort      = $state<SortKey>('newest');
  let selected  = $state<Snippet | null>(null);
  let modal     = $state<{ mode: 'add' | 'edit'; snippet?: Snippet } | null>(null);
  let saving    = $state(false);
  let saveError = $state('');
  let deleteTarget = $state<number | null>(null);
  let draftTrigger       = $state('');
  let draftExpansion     = $state('');
  let draftInstructions  = $state('');
  let triggerInput   = $state<HTMLInputElement | null>(null);
  let inspectorDir = $state<1 | -1>(1);
  let sortWrapEl = $state<HTMLDivElement | null>(null);
  let sortButtonEls = $state<Record<SortKey, HTMLButtonElement | null>>({
    newest: null,
    oldest: null,
    alpha: null,
    most_used: null,
  });
  let sortIndicatorStyle = $state('opacity:0;');

  const TRIGGER_LIMIT = 300;

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
    draftTrigger       = '';
    draftExpansion     = '';
    draftInstructions  = '';
    modal = { mode: 'add' };
  }

  function openEdit(s: Snippet) {
    draftTrigger       = s.trigger;
    draftExpansion     = s.expansion;
    draftInstructions  = s.instructions ?? '';
    modal = { mode: 'edit', snippet: s };
  }

  function closeModal() { modal = null; saveError = ''; }

  async function saveModal() {
    const t = draftTrigger.trim();
    const e = draftExpansion.trim();
    if (!t || !e) return;
    saving = true;
    saveError = '';
    try {
      const editedId = modal?.mode === 'edit' ? modal.snippet?.id : undefined;
      const i = draftInstructions.trim();
      if (modal?.mode === 'add') {
        await invoke('create_snippet', { trigger: t, expansion: e, instructions: i });
      } else if (modal?.mode === 'edit' && modal.snippet) {
        await invoke('edit_snippet', { id: modal.snippet.id, trigger: t, expansion: e, instructions: i });
      }
      await fetchSnippets();
      // Re-sync selected manually — no $effect to avoid reactivity loops
      if (editedId !== undefined) {
        selected = appStore.snippets.find(s => s.id === editedId) ?? null;
      }
      closeModal();
    } catch (err) {
      const msg = String(err);
      saveError = msg.includes('UNIQUE') ? 'A snippet with that trigger already exists.' : 'Failed to save snippet.';
    }
    finally { saving = false; }
  }

  async function confirmDelete(id: number) {
    if (deleteTarget === id) {
      try {
        await invoke('remove_snippet', { id });
        if (selected?.id === id) selected = null;
        await fetchSnippets();
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

  function autoGrow(node: HTMLTextAreaElement, value: string) {
    let last = 0;
    let hadSelection = false;
    function resize(couldShrink = true) {
      if (couldShrink) node.style.height = 'auto';
      const borderDiff = node.offsetHeight - node.clientHeight;
      const h = node.scrollHeight + borderDiff;
      if (h === last) { if (couldShrink) node.style.height = last + 'px'; return; }
      last = h;
      node.style.height = h + 'px';
    }
    function onBeforeInput() {
      hadSelection = node.selectionStart !== node.selectionEnd;
    }
    function onInput(e: Event) {
      value = node.value;
      const type = (e as InputEvent).inputType ?? '';
      const insertOnly = type === 'insertText' || type === 'insertLineBreak' || type === 'insertParagraph' || type === 'insertCompositionText';
      resize(!(insertOnly && !hadSelection));
    }
    node.addEventListener('beforeinput', onBeforeInput);
    node.addEventListener('input', onInput);
    resize();
    return {
      update(nextValue: string) {
        if (nextValue !== value) {
          value = nextValue;
          if (node.value !== nextValue) node.value = nextValue;
          resize(true);
        }
      },
      destroy() {
        node.removeEventListener('beforeinput', onBeforeInput);
        node.removeEventListener('input', onInput);
      }
    };
  }

  $effect(() => {
    if (modal && triggerInput) setTimeout(() => triggerInput?.focus(), 50);
  });

  const sortLabels: { key: SortKey; label: string }[] = [
    { key: 'newest',    label: 'Newest'    },
    { key: 'oldest',    label: 'Oldest'    },
    { key: 'alpha',     label: 'A → Z'     },
    { key: 'most_used', label: 'Most used' },
  ];

  function setSort(next: SortKey) {
    if (next === sort) return;
    sort = next;
  }

  function updateSortIndicator() {
    const wrap = sortWrapEl;
    const btn = sortButtonEls[sort];
    if (!wrap || !btn) return;
    const wrapRect = wrap.getBoundingClientRect();
    const btnRect = btn.getBoundingClientRect();
    const left = Math.round(btnRect.left - wrapRect.left);
    const width = Math.round(btnRect.width);
    sortIndicatorStyle = `opacity:1; transform:translateX(${left}px); width:${width}px; transition: transform ${motionMs(MOTION_MS.base)}ms cubic-bezier(0.22, 1, 0.36, 1), width ${motionMs(MOTION_MS.base)}ms cubic-bezier(0.22, 1, 0.36, 1), opacity ${motionMs(MOTION_MS.fast)}ms ease;`;
  }

  $effect(() => {
    sort;
    setTimeout(updateSortIndicator, 0);
  });

  onMount(() => {
    updateSortIndicator();
    const onResize = () => updateSortIndicator();
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="content-inner">
  <h1 class="page-h">Snippets</h1>
  <p class="page-sub">Speak a trigger and Open Flow expands it during dictation.</p>
  {#if appStore.snippetsFetchStatus === 'error' && appStore.snippets.length > 0}
    <div class="load-warning" role="alert" aria-live="assertive">
      <span>{appStore.snippetsFetchError || 'Unable to load snippets.'} Check backend connection and retry.</span>
      <button type="button" class="load-warning-retry" onclick={() => fetchSnippets()}>Retry</button>
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
        placeholder="Search snippets…"
        bind:value={search}
        aria-label="Search snippets"
      />
      {#if search}
        <button class="clear-btn" onclick={() => search = ''} aria-label="Clear">
          <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
        </button>
      {/if}
    </div>

    <div class="sort-pills" bind:this={sortWrapEl}>
      <span
        class="sort-indicator"
        style={sortIndicatorStyle}
      ></span>
      {#each sortLabels as { key, label }}
        <button
          class="sort-pill"
          class:active={sort === key}
          aria-pressed={sort === key}
          bind:this={sortButtonEls[key]}
          onclick={() => setSort(key)}
        >{label}</button>
      {/each}
    </div>

    <button class="btn-primary" onclick={openAdd}>
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 5v14M5 12h14"/></svg>
      New snippet
    </button>
  </div>

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
      <p class="empty-sub">Add a trigger phrase and Open Flow will expand it automatically during dictation.</p>
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

      <!-- Left: list -->
      <div class="snip-list-col">
        {#if filtered.length === 0}
          <div class="empty-state" in:fade={{ duration: 200 }}>
            <p class="empty-h">No matches</p>
            <p class="empty-sub">Nothing matches "{search}".</p>
            <button class="btn-ghost" onclick={() => search = ''}>Clear search</button>
          </div>
        {:else}
          <div class="snip-list">
            {#each filtered as s (s.id)}
              <button
                type="button"
                class="snip-row"
                class:is-selected={selected?.id === s.id}
                aria-pressed={selected?.id === s.id}
                onclick={() => selectRow(s)}
              >
                <span class="snip-left">
                  <span class="snip-trigger">{s.trigger}</span>
                  <span class="snip-arrow" aria-hidden="true">
                    <svg width="9" height="13" viewBox="0 0 9 13" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
                      <line x1="4.5" y1="0" x2="4.5" y2="9"/>
                      <polyline points="1.5,6.5 4.5,10 7.5,6.5"/>
                    </svg>
                  </span>
                  <span class="snip-expansion">{s.expansion}</span>
                </span>
                <span class="snip-meta">
                  <span>{s.use_count} {s.use_count === 1 ? 'use' : 'uses'}</span>
                  <span class="meta-dot">·</span>
                  <span>{fmtDate(s.created_at)}</span>
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
              <div class="insp-trigger">{selected.trigger}</div>
              <div class="insp-arrow" aria-hidden="true">
              <svg width="11" height="16" viewBox="0 0 11 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                <line x1="5.5" y1="0" x2="5.5" y2="12"/>
                <polyline points="2,9 5.5,13.5 9,9"/>
              </svg>
              </div>
              <div class="insp-expansion scroll-styled">{selected.expansion}</div>

              {#if selected.instructions}
                <div class="insp-instructions" in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.base), easing: expoOut }}>
                  <div class="insp-instr-label">
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                    <circle cx="12" cy="12" r="10"/><path d="M12 16v-4M12 8h.01"/>
                  </svg>
                  Cleanup instructions
                  </div>
                  <p class="insp-instr-text">{selected.instructions}</p>
                </div>
              {/if}

              <div class="insp-divider"></div>

              <div class="insp-stats">
                <div class="insp-stat-row">
                <span class="insp-stat-num">{selected.use_count}</span>
                <span class="insp-stat-label">{selected.use_count === 1 ? 'use' : 'uses'}</span>
                </div>
                <div class="insp-stat-row">
                <span class="insp-stat-label">Added</span>
                <span class="insp-stat-date">{fmtDate(selected.created_at)}</span>
                </div>
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
              <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/>
              <polyline points="14 2 14 8 20 8"/>
              <line x1="8" y1="13" x2="16" y2="13"/>
              <line x1="8" y1="17" x2="16" y2="17"/>
            </svg>
            <p>Select a snippet<br>to inspect it</p>
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
      <h2 class="modal-title">{modal.mode === 'add' ? 'New snippet' : 'Edit snippet'}</h2>
      <button class="icon-btn" onclick={closeModal} aria-label="Close">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
      </button>
    </div>

    <div class="modal-body scrollbar-standard">
      <label class="field-label" for="trigger-input">
        Trigger
        <span class="char-count" class:over={draftTrigger.length > TRIGGER_LIMIT}>{draftTrigger.length}/{TRIGGER_LIMIT}</span>
      </label>
      <div class="input-row">
        <input
          id="trigger-input"
          class="field-input"
          type="text"
          placeholder="e.g. my email"
          bind:value={draftTrigger}
          bind:this={triggerInput}
          maxlength={TRIGGER_LIMIT}
          autocomplete="off"
          spellcheck="false"
        />
        <MicInputButton onResult={(t) => draftTrigger = t} />
      </div>
      <p class="field-hint">Speak this phrase to trigger the expansion.</p>

      <label class="field-label" for="expansion-input">Expansion</label>
      <div class="input-row input-row--top">
        <textarea
          id="expansion-input"
          class="field-input scrollbar-standard"
          placeholder="e.g. hello@example.com"
          bind:value={draftExpansion}
          use:autoGrow={draftExpansion}
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
        use:autoGrow={draftInstructions}
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
        <button class="btn-ghost" onclick={closeModal}>Cancel</button>
        <button
          class="btn-primary"
          onclick={saveModal}
          disabled={saving || !draftTrigger.trim() || !draftExpansion.trim() || draftTrigger.length > TRIGGER_LIMIT}
        >
          {#if saving}<span class="spinner"></span>{/if}
          {modal.mode === 'add' ? 'Add snippet' : 'Save changes'}
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

  .snip-layout {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 18px;
    align-items: start;
  }

  /* ── list column ── */

  .snip-list-col { min-width: 0; }

  .snip-list {
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    overflow: hidden;
    background: var(--bg-elev);
  }

  .snip-row {
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
  .snip-row:last-child { border-bottom: 0; }
  .snip-row:hover { background: var(--control-hover); }
  .snip-row.is-selected { background: var(--control-active); }
  .snip-row:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .snip-left { display: block; min-width: 0; }

  .snip-trigger {
    display: block;
    font-size: 13px;
    font-weight: 500;
    color: var(--ink);
  }

  .snip-arrow {
    color: var(--arm-300);
    margin: 3px 0 2px 0;
    line-height: 0;
    display: block;
  }

  .snip-expansion {
    display: block;
    font-size: 12.5px;
    color: var(--ink-mute);
    line-height: 1.45;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .snip-meta {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
    font-size: 11px;
    color: var(--ink-faint);
    white-space: nowrap;
  }

  .meta-dot { display: none; }

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

  .insp-arrow {
    color: var(--arm-300);
    margin: 6px 0 5px 1px;
    line-height: 0;
    display: block;
  }

  .insp-expansion {
    font-size: 13px;
    color: var(--ink-strong);
    line-height: 1.6;
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 120px;
    overflow-y: auto;
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

  .insp-stat-label {
    font-size: 11.5px;
    color: var(--ink-mute);
  }

  .insp-stat-date {
    font-size: 12.5px;
    color: var(--ink-soft);
    font-weight: 500;
    margin-left: auto;
  }

  .insp-actions {
    display: flex;
    gap: 8px;
  }

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
    backdrop-filter: blur(2px);
    outline: none;
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
    width: min(500px, calc(100vw - 40px));
    max-height: calc(100dvh - 80px);
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

  /* ── instructions label ── */

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

  @media (max-width: 1060px) {
    .snip-layout {
      grid-template-columns: 1fr;
    }

    .inspector-col {
      position: static;
    }
  }

  @media (max-width: 720px) {
    .search {
      flex-basis: 100%;
    }

    .sort-pills {
      order: 3;
      width: 100%;
      overflow-x: auto;
    }

    .snip-row {
      grid-template-columns: 1fr;
      gap: 8px;
    }

    .snip-meta {
      flex-direction: row;
      align-items: center;
    }

    .meta-dot {
      display: inline;
    }
  }

  /* ── inspector instructions ── */

  .insp-instructions {
    margin-top: 14px;
    background: var(--paper);
    border: 1px dashed var(--line);
    border-radius: var(--r-sm);
    padding: 10px 12px;
  }

  .insp-instr-label {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 10.5px;
    font-weight: 500;
    color: var(--ink-mute);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-bottom: 6px;
  }

  .insp-instr-text {
    font-size: 12.5px;
    color: var(--ink-soft);
    line-height: 1.55;
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }
</style>

