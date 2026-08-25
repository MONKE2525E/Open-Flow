<script lang="ts">
  import { onMount } from 'svelte';
  import { fade, fly, slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { modalFocusTrap } from '../../modalFocus';
  import { portal } from '../../portal';
  import { isTrustworthy } from '../../modelCatalogStore.svelte';
  import { expandFromOrigin, modalBackdrop, MOTION_MS, motionMs } from '../../motion';
  import { getProviderLogo, getProviderPlate } from '../../setup/ProviderLogos';
  import LocalDownloadProgress from './LocalDownloadProgress.svelte';
  import type { ProviderId } from '../../settings';
  import { modelId, providerDisplayLabel, splitModelId, taskLabel, type TaskType } from './models';
  import {
    curatedRows,
    rowForSelection,
    unverifiedRows,
    type LocalControls,
    type ModelRow,
    type PickerContext,
  } from './modelStates';

  let {
    mode,
    task,
    context,
    defaultModel,
    fallbackModels,
    advancedModelUi,
    customDraft,
    onSelect,
    onAddFallback,
    onCustomDraftChange,
    onAddCustomModel,
    onOpenApiKeys,
    onClose,
    local,
    origin = null,
  }: {
    /**
     * `select` replaces the active model; `fallback` only appends to the chain.
     * Someone who opened "Add fallback" must not be able to clobber their
     * default by clicking a row, so the row gesture changes with the mode.
     */
    mode: 'select' | 'fallback';
    task: TaskType;
    context: PickerContext;
    defaultModel: string;
    fallbackModels: string[];
    advancedModelUi: boolean;
    customDraft: string;
    onSelect: (id: string) => void;
    onAddFallback: (id: string) => void;
    onCustomDraftChange: (value: string) => void;
    onAddCustomModel: (id: string) => void;
    onOpenApiKeys: () => void;
    onClose: () => void;
    /**
     * Downloading, deleting and prompt-editing local models happens here now,
     * on the row for the model itself, rather than in a separate section that
     * listed the same models a second time under different UI.
     */
    local: LocalControls;
    /** Centre of the button that opened this, so the dialog grows out of it. */
    origin?: { x: number; y: number } | null;
  } = $props();

  const localModel = (id: string) => local.models.find((m) => m.id === id);
  const isDownloading = (id: string) => localModel(id)?.is_downloading === true;
  const isDownloaded = (id: string) => localModel(id)?.is_downloaded === true;

  /** The cleanup runtime is a one-time prerequisite shared by every local LLM. */
  const runtimePending = $derived(
    !!local.runtime && !local.runtime.info?.installed && !!local.runtime.info,
  );

  let modalEl = $state<HTMLElement | null>(null);
  let query = $state('');
  let providerFilter = $state<ProviderId | 'all'>('all');
  /**
   * Settings is a full-screen page beside the rail, so centring on the whole
   * window puts the dialog visibly left of the content it belongs to. Measure
   * the panel and centre inside that instead.
   */
  let panelLeft = $state(0);

  onMount(() => {
    const updatePanelLeft = () => {
      const panel = document.querySelector('.settings-page');
      if (panel) panelLeft = panel.getBoundingClientRect().left;
    };
    updatePanelLeft();
    window.addEventListener('resize', updatePanelLeft);
    requestAnimationFrame(() => {
      if (modalEl?.isConnected) {
        modalEl.querySelector<HTMLElement>('.picker-search')?.focus();
      }
    });
    return () => window.removeEventListener('resize', updatePanelLeft);
  });

  const RAIL_ORDER: ProviderId[] = ['groq', 'openai', 'google', 'assemblyai', 'local'];

  const current = $derived(rowForSelection(defaultModel, context));
  // Selections stay listed even after a provider drops them, so a dead choice
  // still has somewhere to show its state and be swapped out.
  const pinned = $derived([defaultModel, ...fallbackModels].filter(Boolean));
  const allRows = $derived(
    [...curatedRows(context, pinned), ...(advancedModelUi ? unverifiedRows(context) : [])].filter(
      (row) => local.supported || row.provider !== 'local',
    ),
  );

  function matches(row: ModelRow): boolean {
    const needle = query.trim().toLowerCase();
    if (!needle) return true;
    return (
      row.id.toLowerCase().includes(needle) ||
      row.label.toLowerCase().includes(needle) ||
      providerDisplayLabel(row.provider).toLowerCase().includes(needle)
    );
  }

  const searched = $derived(allRows.filter(matches));
  const shown = $derived(
    providerFilter === 'all'
      ? searched
      : searched.filter((row) => row.provider === providerFilter),
  );

  /** Usable first, then anything needing setup, then anything gone. */
  const RANK: Record<ModelRow['state'], number> = {
    ready: 0,
    unverified: 1,
    'needs-setup': 2,
    unavailable: 3,
    'not-found': 4,
  };

  const grouped = $derived(
    RAIL_ORDER.map((provider) => ({
      provider,
      rows: shown
        .filter((row) => row.provider === provider)
        .sort((a, b) => RANK[a.state] - RANK[b.state] || a.label.localeCompare(b.label)),
    })).filter((group) => group.rows.length > 0),
  );

  const counts = $derived(
    Object.fromEntries(
      RAIL_ORDER.map((provider) => [
        provider,
        searched.filter((row) => row.provider === provider).length,
      ]),
    ) as Record<ProviderId, number>,
  );

  function isActive(key: string) {
    return key === defaultModel;
  }
  function isFallback(key: string) {
    return fallbackModels.includes(key);
  }

  function activate(row: ModelRow) {
    if (row.remedy === 'add-key') return onOpenApiKeys();
    // A model that isn't on disk yet can't be chosen, so the row's job is to
    // fetch it. Selecting it afterwards is a second, deliberate click.
    if (row.remedy === 'download') return local.onDownload(row.id);
    if (mode === 'fallback') {
      if (isActive(row.key) || isFallback(row.key)) return;
      onAddFallback(row.key);
    } else {
      onSelect(row.key);
    }
    onClose();
  }

  function addFallback(row: ModelRow) {
    if (isActive(row.key) || isFallback(row.key)) return;
    onAddFallback(row.key);
  }

  /**
   * Which provider a hand-typed id belongs to.
   *
   * Never parsed out of the id itself: plenty of ids contain a slash that is
   * part of the name, not a provider — `openai/gpt-oss-20b` and
   * `qwen/qwen3.6-27b` are both *Groq* models. Splitting on `/` would file
   * them under OpenAI. The rail's current filter is an explicit answer, and
   * the active model's provider is the sensible default.
   */
  const customProvider = $derived<ProviderId>(
    providerFilter !== 'all'
      ? providerFilter
      : (splitModelId(defaultModel)?.provider ?? 'groq'),
  );

  let customError = $state('');

  function submitCustom() {
    const value = customDraft.trim();
    if (!value) return;

    const cache = context.cache[customProvider];
    // Only reject when the provider has actually told us what it offers.
    // Offline, or before the first fetch, a typo is indistinguishable from a
    // brand-new model, so it goes through with an "unverified" note instead.
    if (customProvider !== 'local' && isTrustworthy(cache) && !cache!.ids.includes(value)) {
      customError = `${providerDisplayLabel(customProvider)} doesn't list a model called “${value}”.`;
      return;
    }

    customError = '';
    const selected = modelId(customProvider, value);
    if (mode === 'fallback') {
      onAddFallback(selected);
    } else {
      onAddCustomModel(selected);
    }
    onClose();
  }

  $effect(() => {
    // Clear a stale complaint as soon as the text changes.
    customDraft;
    customError = '';
  });

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      onClose();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<div class="picker-wrap" use:portal style="padding-left: {panelLeft}px">
  <div
    class="picker-backdrop"
    onclick={onClose}
    onkeydown={(event) => {
      if (event.key === 'Enter' || event.key === ' ') {
        event.preventDefault();
        onClose();
      }
    }}
    role="button"
    tabindex="0"
    aria-label="Close model picker"
    in:modalBackdrop={{ duration: MOTION_MS.fast }}
    out:modalBackdrop={{ duration: MOTION_MS.fast }}
  ></div>

  <div
    bind:this={modalEl}
    class="picker-card"
    use:modalFocusTrap={{
      active: true,
      initialFocus: () => modalEl?.querySelector<HTMLElement>('.picker-search') ?? modalEl,
    }}
    role="dialog"
    aria-modal="true"
    aria-label={mode === 'fallback'
      ? `Add a ${taskLabel(task).toLowerCase()} fallback`
      : `Choose a ${taskLabel(task).toLowerCase()} model`}
    tabindex="-1"
    onkeydown={onKeydown}
    in:expandFromOrigin={{ origin: origin ?? undefined, duration: MOTION_MS.base }}
    out:expandFromOrigin={{ origin: origin ?? undefined, duration: MOTION_MS.fast }}
  >
    <header class="picker-head">
      <div class="picker-title">
        <h3>
          {mode === 'fallback'
            ? `Add a ${taskLabel(task).toLowerCase()} fallback`
            : `Choose a ${taskLabel(task).toLowerCase()} model`}
        </h3>
        <p>
          {mode === 'fallback'
            ? 'Used in order when the model above cannot run.'
            : 'The model Verenu reaches for first.'}
        </p>
      </div>
      <button type="button" class="picker-close ui-focus-ring" aria-label="Close" onclick={onClose}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
          <path d="M6 6l12 12M18 6L6 18" />
        </svg>
      </button>
    </header>

    <div class="picker-search-row">
      <div class="picker-search-field">
        <svg class="search-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
          <circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" />
        </svg>
        <input
          class="picker-search"
          type="search"
          placeholder="Search {allRows.length} models…"
          bind:value={query}
          autocomplete="off"
          autocapitalize="off"
          autocorrect="off"
          spellcheck="false"
          aria-label="Search models"
        />
      </div>
    </div>

    <div class="picker-body">
      <nav class="picker-rail" aria-label="Filter by provider">
        <button
          type="button"
          class="rail-item ui-focus-ring"
          class:rail-active={providerFilter === 'all'}
          onclick={() => (providerFilter = 'all')}
        >
          <span class="rail-logo" aria-hidden="true">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round">
              <path d="M4 7h16M4 12h16M4 17h16" />
            </svg>
          </span>
          <span class="rail-name">All providers</span>
          {#key searched.length}
            <span class="rail-count" in:fade={{ duration: motionMs(MOTION_MS.fast) }}>
              {searched.length}
            </span>
          {/key}
        </button>
        {#each RAIL_ORDER.filter((p) => counts[p] > 0) as provider (provider)}
          <button
            type="button"
            class="rail-item ui-focus-ring"
            class:rail-active={providerFilter === provider}
            onclick={() => (providerFilter = provider)}
          >
            <span class="rail-logo" class:plate-bleed={getProviderPlate(provider) === 'bleed'}>
              {@html getProviderLogo(provider)}
            </span>
            <span class="rail-name">{providerDisplayLabel(provider)}</span>
            {#key counts[provider]}
            <span class="rail-count" in:fade={{ duration: motionMs(MOTION_MS.fast) }}>
              {counts[provider]}
            </span>
          {/key}
          </button>
        {/each}
      </nav>

      <div class="picker-list scroll-styled scroll-thumb-elev">
        {#if grouped.length === 0}
          <div class="picker-empty" in:fade={{ duration: motionMs(MOTION_MS.fast) }}>
            <p>No models match “{query.trim()}”.</p>
            <button class="row-tool ui-focus-ring" type="button" onclick={() => (query = '')}>
              Clear search
            </button>
          </div>
        {/if}

        {#if !local.supported}
          <p class="picker-note">
            On-device models aren’t available on Intel Macs yet — they haven’t been validated on
            that hardware, and older Intel machines struggle to run a local model well. A cloud
            provider above works with no download.
          </p>
        {/if}

        {#each grouped as group (group.provider)}
          <p class="group-label" in:fade={{ duration: motionMs(MOTION_MS.fast) }}>
            {providerDisplayLabel(group.provider)}
          </p>

          {#if group.provider === 'local' && local.runtime?.info && runtimePending}
            <div class="runtime-note">
              {#if local.runtime.info.is_downloading}
                <span>Fetching the on-device runtime…</span>
                <button class="row-tool ui-focus-ring" type="button" onclick={() => local.runtime?.onCancel()}>Cancel</button>
              {:else}
                <span>
                  On-device cleanup needs a one-time runtime (~{local.runtime.info.approx_download_mb} MB).
                  Downloading any model below fetches it too.
                </span>
              {/if}
            </div>
          {:else if group.provider === 'local' && local.runtime?.info?.installed}
            <div class="runtime-note">
              <span>On-device runtime installed{local.runtime.info.backend ? ` (${local.runtime.info.backend})` : ''}.</span>
              <button class="row-tool ui-focus-ring" type="button" onclick={() => local.runtime?.onDelete()}>Remove</button>
            </div>
          {/if}
          {#each group.rows as row, index (row.key)}
            {@const active = isActive(row.key)}
            {@const fallback = isFallback(row.key)}
            <div
              class="model-row"
              class:row-active={active}
              class:row-dim={row.state === 'needs-setup'}
              in:fly|global={{
                y: 6,
                duration: motionMs(MOTION_MS.base),
                delay: motionMs(Math.min(index, 8) * 12),
                easing: cubicOut,
              }}
              out:fade|global={{ duration: motionMs(MOTION_MS.fast) }}
            >
              <button
                class="row-main"
                type="button"
                aria-current={active ? 'true' : undefined}
                onclick={() => activate(row)}
              >
                <span class="row-logo" class:plate-bleed={getProviderPlate(row.provider, row.id) === 'bleed'}>
                  {@html getProviderLogo(row.provider, row.id)}
                </span>
                <span class="row-text">
                  <span class="row-name">{row.label}</span>
                  <span class="row-sub">
                    <code>{row.id}</code>
                    {#each row.tags as tag (tag)}
                      <span class="row-tag">{tag}</span>
                    {/each}
                    {#if row.note}<span class="row-note">{row.note}</span>{/if}
                  </span>
                </span>
                {#if active}
                  <span class="row-state row-state-active">Active</span>
                {:else if fallback}
                  <span class="row-state">Fallback {fallbackModels.indexOf(row.key) + 1}</span>
                {:else if row.remedy === 'add-key'}
                  <span class="row-state row-state-cta">Add key →</span>
                {/if}
              </button>

              {#if row.provider === 'local'}
                {@const phase = isDownloading(row.id)
                  ? 'busy'
                  : isDownloaded(row.id)
                    ? 'ready'
                    : 'absent'}
                {#key phase}
                  <span class="row-tools" in:fade={{ duration: motionMs(MOTION_MS.fast) }}>
                  {#if isDownloading(row.id)}
                    <button
                      class="row-tool ui-focus-ring"
                      data-testid="cancel-model-download"
                      type="button"
                      onclick={() => local.onCancel(row.id)}>Cancel</button
                    >
                  {:else if !isDownloaded(row.id)}
                    <button
                      class="row-tool row-tool-accent ui-focus-ring"
                      data-testid="download-model"
                      type="button"
                      onclick={() => local.onDownload(row.id)}>Download</button
                    >
                  {:else}
                    {#if task === 'cleanup' && advancedModelUi && local.onEditPrompt}
                      <button
                        class="row-tool ui-focus-ring"
                        data-testid="edit-prompt"
                        type="button"
                        onclick={(event) =>
                          local.onEditPrompt?.(
                            row.id,
                            (event.currentTarget as HTMLButtonElement).getBoundingClientRect(),
                          )}
                        >Prompt{local.promptCustomized?.(row.id) ? ' •' : ''}</button
                      >
                    {/if}
                    <button
                      class="row-tool row-tool-danger ui-focus-ring"
                      data-testid="delete-model"
                      type="button"
                      onclick={() => local.onDelete(row.id)}>Delete</button
                    >
                  {/if}
                  </span>
                {/key}
              {:else if mode === 'select' && row.remedy === 'none' && !active && !fallback}
                <button
                  class="row-add ui-focus-ring"
                  type="button"
                  aria-label="Add {row.label} as a fallback"
                  title="Add as fallback"
                  onclick={() => addFallback(row)}
                >+</button>
              {/if}
            </div>

            {#if row.provider === 'local' && isDownloading(row.id)}
              <div class="row-progress" transition:slide={{ duration: motionMs(MOTION_MS.fast), easing: cubicOut }}>
                <LocalDownloadProgress
                  stage={local.downloadStage[row.id] === 'verifying' ? 'verifying' : 'downloading'}
                  percent={(local.downloadProgress[row.id]?.progress ?? 0) * 100}
                  label={local.downloadStage[row.id] === 'verifying' ? 'Verifying…' : 'Downloading…'}
                  indeterminate={local.downloadProgress[row.id] == null}
                />
              </div>
            {/if}
          {/each}
        {/each}
      </div>
    </div>

    {#if advancedModelUi && customProvider !== 'local'}
      <footer class="picker-foot" transition:slide={{ duration: motionMs(MOTION_MS.fast) }}>
        <div class="custom-row">
          <label class="custom-label" for="picker-custom-id">
            Add to {providerDisplayLabel(customProvider)}
          </label>
          <input
            id="picker-custom-id"
            class="custom-input"
            class:custom-invalid={!!customError}
            placeholder="Exact model id"
            value={customDraft}
            autocomplete="off"
            autocapitalize="off"
            autocorrect="off"
            spellcheck="false"
            aria-invalid={!!customError}
            aria-describedby={customError ? 'picker-custom-error' : undefined}
            oninput={(event) => onCustomDraftChange((event.currentTarget as HTMLInputElement).value)}
            onkeydown={(event) => {
              if (event.key === 'Enter') {
                event.stopPropagation();
                submitCustom();
              }
            }}
          />
          <button class="custom-add ui-focus-ring" type="button" disabled={!customDraft.trim()} onclick={submitCustom}>
            Add
          </button>
        </div>
        {#if customError}
          <p class="custom-error" id="picker-custom-error" transition:slide={{ duration: motionMs(MOTION_MS.fast) }}>
            {customError}
          </p>
        {/if}
      </footer>
    {/if}
  </div>
</div>

<style>
  .picker-wrap {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    /* The inline style overrides padding-left only, to clear the settings
       rail; the rest keeps the card off the window edges. */
    padding: 24px;
    box-sizing: border-box;
  }

  .picker-backdrop {
    position: absolute;
    inset: 0;
    border: none;
    background: var(--overlay);
    backdrop-filter: blur(2px);
  }

  .picker-card {
    position: relative;
    z-index: 1;
    width: min(720px, 100%);
    height: min(600px, 100%);
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    box-shadow: var(--shadow-elev);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    container-type: inline-size;
    container-name: model-picker;
  }

  /* ── Header ─────────────────────────────── */
  .picker-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding: 18px 20px 10px;
  }

  .picker-title h3 {
    margin: 0;
    font-family: var(--serif);
    font-size: 17px;
    font-weight: 500;
    color: var(--ink);
  }

  .picker-title p {
    margin: 4px 0 0;
    font-family: var(--sans);
    font-size: 12px;
    color: var(--ink-mute);
  }

  .picker-close {
    flex-shrink: 0;
    width: 26px;
    height: 26px;
    display: grid;
    place-items: center;
    /* Inherited button padding left less content box than the glyph needs, so
       it overflowed the grid cell and drifted off centre. */
    padding: 0;
    box-sizing: border-box;
    border: 1px solid var(--line);
    border-radius: 7px;
    background: transparent;
    color: var(--ink-mute);
    cursor: pointer;
    transition: background-color var(--ui-duration-fast) var(--ui-ease-out), color var(--ui-duration-fast) var(--ui-ease-out), border-color var(--ui-duration-fast) var(--ui-ease-out);
  }

  .picker-close svg {
    display: block;
  }

  .picker-close:hover {
    background: var(--control-hover);
    color: var(--ink);
  }

  /* ── Search ─────────────────────────────── */
  .picker-search-row {
    padding: 4px 20px 14px;
  }

  /* The row's padding is deliberately lopsided (4 top / 14 bottom), so an icon
     centred on the row sat below the input it belongs to. Anchor it to the
     field instead. */
  .picker-search-field {
    position: relative;
    display: flex;
    align-items: center;
  }

  .search-icon {
    position: absolute;
    left: 10px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--ink-faint);
    pointer-events: none;
  }

  .picker-search {
    width: 100%;
    font-family: var(--sans);
    font-size: 13px;
    padding: 8px 10px 8px 32px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--paper);
    color: var(--ink);
    transition: border-color var(--ui-duration-fast) var(--ui-ease-out), background-color var(--ui-duration-fast) var(--ui-ease-out);
  }

  .picker-search:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }

  /* ── Body: provider rail + list ─────────── */
  .picker-body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 178px 1fr;
  }

  .picker-rail {
    border-right: 1px solid var(--line);
    padding: 10px 8px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    overflow-y: auto;
  }

  .rail-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 9px;
    border: none;
    border-radius: 7px;
    background: transparent;
    font-family: var(--sans);
    font-size: 12.5px;
    color: var(--ink-soft);
    cursor: pointer;
    text-align: left;
    transition: background-color var(--ui-duration-fast) var(--ui-ease-out), color var(--ui-duration-fast) var(--ui-ease-out);
  }

  .rail-item:hover:not(:disabled) {
    background: var(--control-hover);
    color: var(--ink);
  }

  .rail-item:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .rail-active,
  .rail-active:hover:not(:disabled) {
    background: var(--control-hover);
    color: var(--ink);
    font-weight: 500;
  }

  .rail-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rail-count {
    font-variant-numeric: tabular-nums;
    font-size: 11px;
    color: var(--ink-faint);
  }

  /* Marks are shown exactly as each brand draws them. A logo that already
     carries its own background (Groq's orange square) fills the tile; the
     rest sit bare, so nothing reads as an outline or a recolour. */
  .rail-logo,
  .row-logo {
    flex-shrink: 0;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 6px;
    overflow: hidden;
    color: var(--ink-soft);
  }

  .rail-logo :global(svg),
  .row-logo :global(svg) {
    width: 15px;
    height: 15px;
  }

  .plate-bleed :global(svg) {
    width: 100%;
    height: 100%;
  }

  .row-logo {
    width: 26px;
    height: 26px;
    border-radius: 7px;
  }

  .row-logo :global(svg) {
    width: 18px;
    height: 18px;
  }


  /* ── Model list ─────────────────────────── */
  .picker-list {
    overflow-y: auto;
    padding: 10px 12px 14px;
  }

  .group-label {
    margin: 10px 0 4px 6px;
    font-family: var(--sans);
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.09em;
    color: var(--ink-faint);
  }

  .picker-list > .group-label:first-child {
    margin-top: 0;
  }

  .model-row {
    display: flex;
    align-items: center;
    gap: 4px;
    /* Trailing room so the "+" and the local tools sit inside the highlight
       rather than flush against its edge. */
    padding-right: 6px;
    border: 1px solid transparent;
    border-radius: 9px;
    transition: background-color var(--ui-duration-fast) var(--ui-ease-out), border-color var(--ui-duration-fast) var(--ui-ease-out);
  }

  .model-row:hover {
    background: var(--control-hover);
  }

  .row-active {
    border-color: color-mix(in srgb, var(--accent) 40%, var(--line));
    background: var(--accent-soft);
  }

  .row-main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--ink);
    font-family: var(--sans);
    text-align: left;
    cursor: pointer;
  }

  .row-main:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }

  .row-dim .row-main {
    opacity: 0.62;
  }

  .row-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .row-name {
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-sub {
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
    font-size: 10.5px;
    color: var(--ink-faint);
  }

  .row-sub code {
    font-family: var(--mono);
    font-size: 10px;
    /* Shrink last, and never past a legible stub. */
    min-width: 10ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-sub > :not(:first-child)::before {
    content: '·';
    margin-right: 6px;
  }

  .row-tag {
    text-transform: capitalize;
    white-space: nowrap;
  }

  .row-note {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-state {
    flex-shrink: 0;
    font-size: 10.5px;
    color: var(--ink-faint);
    white-space: nowrap;
  }

  .row-state-active {
    color: var(--accent-ink);
    font-weight: 500;
  }

  .row-state-cta {
    color: var(--accent-ink);
  }

  .row-tools {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }

  .row-tool {
    font-family: var(--sans);
    font-size: 11px;
    font-weight: 500;
    padding: 3px 9px;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    background: transparent;
    color: var(--ink-soft);
    cursor: pointer;
    white-space: nowrap;
    transition: background-color var(--ui-duration-fast) var(--ui-ease-out), color var(--ui-duration-fast) var(--ui-ease-out), border-color var(--ui-duration-fast) var(--ui-ease-out);
  }

  .row-tool:hover {
    background: var(--control-hover);
    color: var(--ink);
  }

  .row-tool-accent {
    border-color: color-mix(in srgb, var(--accent) 45%, var(--line));
    color: var(--accent-ink);
  }

  .row-tool-danger:hover {
    border-color: var(--danger-line);
    color: var(--danger);
  }

  .row-progress {
    padding: 2px 10px 8px 46px;
  }

  /* One line for the shared on-device runtime, not one per model. */
  .runtime-note {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin: 2px 6px 6px;
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    font-family: var(--sans);
    font-size: 11.5px;
    line-height: 1.45;
    color: var(--ink-mute);
  }

  .row-add {
    flex-shrink: 0;
    width: 24px;
    height: 24px;
    display: grid;
    place-items: center;
    padding: 0;
    box-sizing: border-box;
    border: none;
    border-radius: 7px;
    background: transparent;
    color: var(--ink-faint);
    font-size: 15px;
    line-height: 1;
    cursor: pointer;
    opacity: 0;
    transition: opacity var(--ui-duration-fast) var(--ui-ease-out), color var(--ui-duration-fast) var(--ui-ease-out), background-color var(--ui-duration-fast) var(--ui-ease-out);
  }

  .model-row:hover .row-add,
  .row-add:focus-visible {
    opacity: 1;
  }

  .row-add:hover {
    background: var(--control-hover);
    color: var(--ink);
  }

  .picker-note {
    margin: 10px 6px 4px;
    padding: 9px 11px;
    border: 1px solid var(--line);
    border-radius: 8px;
    font-family: var(--sans);
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--ink-mute);
  }

  .picker-empty {
    min-height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    font-family: var(--sans);
    font-size: 13px;
    color: var(--ink-mute);
    text-align: center;
  }

  .picker-empty p {
    margin: 0;
  }

  /* ── Footer ─────────────────────────────── */
  .picker-foot {
    padding: 12px 20px;
    border-top: 1px solid var(--line);
    background: var(--paper);
  }

  .custom-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .custom-invalid {
    border-color: var(--danger-line);
  }

  .custom-error {
    margin: 7px 0 0;
    font-family: var(--sans);
    font-size: 11.5px;
    color: var(--danger);
  }

  .custom-label {
    font-family: var(--sans);
    font-size: 12px;
    color: var(--ink-mute);
    white-space: nowrap;
  }

  .custom-input {
    flex: 1;
    min-width: 0;
    font-family: var(--mono);
    font-size: 11.5px;
    padding: 6px 9px;
    border: 1px solid var(--line);
    border-radius: 7px;
    background: var(--bg-elev);
    color: var(--ink);
    transition: border-color var(--ui-duration-fast) var(--ui-ease-out);
  }

  .custom-input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }

  .custom-add {
    flex-shrink: 0;
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 500;
    padding: 6px 12px;
    border: 1px solid var(--line-strong);
    border-radius: 7px;
    background: transparent;
    color: var(--ink-soft);
    cursor: pointer;
    transition: background-color var(--ui-duration-fast) var(--ui-ease-out), color var(--ui-duration-fast) var(--ui-ease-out), border-color var(--ui-duration-fast) var(--ui-ease-out), opacity var(--ui-duration-fast) var(--ui-ease-out);
  }

  .custom-add:hover:not(:disabled) {
    background: var(--control-hover);
    color: var(--ink);
  }

  .custom-add:disabled {
    opacity: 0.4;
    cursor: default;
  }

  @container model-picker (max-width: 600px) {
    .picker-body {
      grid-template-columns: 1fr;
    }

    .picker-rail {
      flex-direction: row;
      overflow-x: auto;
      border-right: none;
      border-bottom: 1px solid var(--line);
    }

    .rail-item {
      width: auto;
      flex: 0 0 auto;
    }

    .rail-name {
      flex: 0 0 auto;
    }

    .rail-count,
    .rail-name {
      white-space: nowrap;
    }
  }
</style>
