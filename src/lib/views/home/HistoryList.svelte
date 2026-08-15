<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { fly } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { icons } from '../../icons';
  import { appStore } from '../../stores';
  import { fmtDuration } from '../insights/helpers';
  import HistoryToolbar from './HistoryToolbar.svelte';
  import { fmtDate, fmtTime, formatAppLabel, type Entry, type RenderItem } from './helpers';

  export let recents: Entry[];
  export let failedEntry: { created_at: string } | null;
  export let cancelledEntry: { created_at: string } | null;
  export let loading: boolean;
  export let hasMoreHistory: boolean;
  export let loadingMore: boolean;
  export let retrying: boolean;
  export let resumingCancelled: boolean;
  export let copiedId: number | null;
  export let hk1: string;
  export let hk2: string;
  export let search: string;
  export let apps: string[];
  export let appFilter: string | null;
  export let onSearchChange: (value: string) => void;
  export let onAppFilterChange: (app: string | null) => void;
  export let onClearFilters: () => void;
  export let onRetry: () => void;
  export let onContinueCancelled: () => void;
  export let onDismissCancelled: () => void;
  export let onLoadOlder: () => void;
  export let onCopy: (entry: Entry) => void;

  $: hasBanner = !!failedEntry || !!cancelledEntry;
  $: filtersActive = (search ?? '').trim().length > 0 || appFilter !== null;
  $: firstLabel = recents.length > 0 ? fmtDate(recents[0].created_at) : '';

  function rowMeta(entry: Entry): string {
    const parts: string[] = [];
    if (entry.app_name) parts.push(formatAppLabel(entry.app_name));
    if (
      typeof entry.duration_ms === 'number' &&
      Number.isFinite(entry.duration_ms) &&
      entry.duration_ms >= 0
    ) {
      parts.push(fmtDuration(entry.duration_ms));
    }
    return parts.join(' · ');
  }

  let flatItems: RenderItem[] = [];
  $: {
    const seenHeaders = new Set<string>();
    const dateCache = new Map<string, string>();
    flatItems = recents.reduce<RenderItem[]>((acc, entry) => {
      const dayKey = entry.created_at.slice(0, 10);
      let label = dateCache.get(dayKey);
      if (!label) {
        label = fmtDate(entry.created_at);
        dateCache.set(dayKey, label);
      }
      if (!seenHeaders.has(dayKey)) {
        seenHeaders.add(dayKey);
        const isFirstHeader = acc.length === 0;
        if (!(hasBanner ? label === 'Today' : isFirstHeader)) {
          acc.push({ type: 'header', label, key: `header-${dayKey}` });
        }
      }
      acc.push({ type: 'row', entry, key: `row-${entry.id}` });
      return acc;
    }, []);

    // Prune cachedHeights to avoid memory leaks from old/deleted history items
    const keys = new Set(flatItems.map(item => item.key));
    for (const key of Object.keys(cachedHeights)) {
      if (!keys.has(key)) {
        delete cachedHeights[key];
      }
    }
  }

  const DEFAULT_HEADER_HEIGHT = 38;
  const DEFAULT_ROW_HEIGHT = 74;

  let container: HTMLElement | null = null;
  let listContainer: HTMLElement | null = null;
  let cachedHeights: Record<string, number> = {};

  let visibleItems: { item: RenderItem; index: number }[] = [];
  let topSpacerHeight = 0;
  let bottomSpacerHeight = 0;

  let tops: number[] = [];
  let totalHeight = 0;

  let listOffset = 0;
  let lastStart = -1;
  let lastEnd = -1;

  function updateListOffset() {
    if (!container || !listContainer) return;
    const listRect = listContainer.getBoundingClientRect();
    if (container === document.documentElement) {
      listOffset = listRect.top + window.scrollY;
    } else {
      const containerRect = container.getBoundingClientRect();
      listOffset = listRect.top - containerRect.top + container.scrollTop;
    }
  }

  function updateLayout() {
    tops = [];
    let currentTop = 0;
    for (let i = 0; i < flatItems.length; i++) {
      tops.push(currentTop);
      const item = flatItems[i];
      const h = cachedHeights[item.key] || (item.type === 'header' ? DEFAULT_HEADER_HEIGHT : DEFAULT_ROW_HEIGHT);
      currentTop += h;
    }
    totalHeight = currentTop;
    updateListOffset();
  }

  function updateVirtualList() {
    if (!container || flatItems.length === 0 || tops.length !== flatItems.length) {
      visibleItems = [];
      topSpacerHeight = 0;
      bottomSpacerHeight = 0;
      lastStart = -1;
      lastEnd = -1;
      return;
    }
    const scrollTop = container === document.documentElement ? window.scrollY : container.scrollTop;
    const clientHeight = container === document.documentElement ? window.innerHeight : container.clientHeight;

    const relativeScrollTop = Math.max(0, scrollTop - listOffset);
    const buffer = 400; // scroll buffer (px)
    const startY = Math.max(0, relativeScrollTop - buffer);
    const endY = relativeScrollTop + clientHeight + buffer;

    let start = flatItems.length;
    let end = flatItems.length;

    // Binary search for start index (first item ending after startY)
    let low = 0;
    let high = flatItems.length - 1;
    while (low <= high) {
      const mid = (low + high) >> 1;
      const top = tops[mid];
      const h = cachedHeights[flatItems[mid].key] || (flatItems[mid].type === 'header' ? DEFAULT_HEADER_HEIGHT : DEFAULT_ROW_HEIGHT);
      if (top + h >= startY) {
        start = mid;
        high = mid - 1;
      } else {
        low = mid + 1;
      }
    }

    // Binary search for end index (first item starting after endY)
    low = start;
    high = flatItems.length - 1;
    while (low <= high) {
      const mid = (low + high) >> 1;
      if (tops[mid] > endY) {
        end = mid;
        high = mid - 1;
      } else {
        low = mid + 1;
      }
    }

    start = Math.max(0, Math.min(start, flatItems.length));
    end = Math.max(start, Math.min(end, flatItems.length));

    if (start === lastStart && end === lastEnd) {
      return;
    }
    lastStart = start;
    lastEnd = end;

    visibleItems = flatItems.slice(start, end).map((item, idx) => ({
      item,
      index: start + idx
    }));

    topSpacerHeight = start < flatItems.length ? (tops[start] || 0) : totalHeight;
    bottomSpacerHeight = totalHeight - (end < flatItems.length ? tops[end] : totalHeight);
  }

  $: {
    flatItems;
    container;
    listContainer;
    appStore.updateInfo;
    // The toolbar's "Clear filters" button appears/disappears with filter
    // state, which changes the list's vertical offset — recompute when it
    // toggles, not just when history rows change.
    filtersActive;
    // Depend on the entries directly, not the derived `hasBanner` boolean —
    // swapping one banner for the other keeps the boolean true, so the block
    // would otherwise not re-run and `listOffset` would go stale.
    failedEntry;
    cancelledEntry;
    updateLayout();
    updateVirtualList();
    // Recalculate list offset after the DOM has updated to handle banner toggles
    tick().then(() => {
      const oldOffset = listOffset;
      updateListOffset();
      if (listOffset !== oldOffset) {
        updateVirtualList();
      }
    });
  }

  function handleScroll(event?: Event) {
    if (event?.type === 'resize') {
      updateListOffset();
    }
    updateVirtualList();
  }

  const nodeKeys = new WeakMap<HTMLElement, string>();
  const sharedObserver = new ResizeObserver((entries) => {
    let changed = false;
    for (const entry of entries) {
      const node = entry.target as HTMLElement;
      const key = nodeKeys.get(node);
      if (key) {
        const height = entry.borderBoxSize?.[0]?.blockSize ?? node.getBoundingClientRect().height;
        if (height > 0 && cachedHeights[key] !== height) {
          cachedHeights[key] = height;
          changed = true;
        }
      }
    }
    if (changed) {
      updateLayout();
      updateVirtualList();
    }
  });

  function measureItem(node: HTMLElement, key: string) {
    nodeKeys.set(node, key);
    sharedObserver.observe(node);

    return {
      update(newKey: string) {
        nodeKeys.delete(node);
        nodeKeys.set(node, newKey);
        const height = node.getBoundingClientRect().height;
        if (height > 0 && cachedHeights[newKey] !== height) {
          cachedHeights[newKey] = height;
          updateLayout();
          updateVirtualList();
        }
      },
      destroy() {
        sharedObserver.unobserve(node);
        nodeKeys.delete(node);
      }
    };
  }

  onMount(() => {
    container = document.querySelector('.content') || document.documentElement;
    const scrollTarget = container === document.documentElement ? window : container;
    scrollTarget.addEventListener('scroll', handleScroll, { passive: true });
    window.addEventListener('resize', handleScroll, { passive: true });

    return () => {
      if (container) {
        const target = container === document.documentElement ? window : container;
        target.removeEventListener('scroll', handleScroll);
        container = null;
      }
      listContainer = null;
      window.removeEventListener('resize', handleScroll);
      sharedObserver.disconnect();
    };
  });
</script>

{#if loading}
  <div class="empty-state">Loading history…</div>
{:else}
  {#if hasBanner}
    <div class="day-head">Today</div>
    <div class="day-table">
      {#if cancelledEntry}
        <div
          class="day-row"
          transition:fly={{ y: -10, duration: 400, easing: expoOut }}
        >
          <div class="day-time">{fmtTime(cancelledEntry.created_at)}</div>
          <div class="day-text error-msg">You cancelled a recording — pick it back up?</div>
          <div class="row-actions">
            <button
              class="dismiss-btn"
              onclick={onDismissCancelled}
              disabled={resumingCancelled}
              title="Discard"
              aria-label="Discard"
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
                <path d="M6 6l12 12M6 18 18 6"/>
              </svg>
            </button>
            <button
              class="retry-btn"
              onclick={onContinueCancelled}
              disabled={resumingCancelled}
            >
              {resumingCancelled ? '…' : 'Continue'}
            </button>
          </div>
        </div>
      {/if}
      {#if failedEntry}
        <div
          class="day-row"
          transition:fly={{ y: -10, duration: 400, easing: expoOut }}
        >
          <div class="day-time">{fmtTime(failedEntry.created_at)}</div>
          <div class="day-text error-msg">Looks like your last transcription failed.</div>
          <button
            class="retry-btn"
            onclick={onRetry}
            disabled={retrying}
          >
            {retrying ? '…' : 'Retry'}
          </button>
        </div>
      {/if}
    </div>
  {/if}

  <div class="day-head day-head-row">
    <span>{hasBanner ? 'Today' : firstLabel}</span>
    <HistoryToolbar
      {search}
      {apps}
      {appFilter}
      {onSearchChange}
      {onAppFilterChange}
      {onClearFilters}
    />
  </div>

  {#if recents.length === 0 && !hasBanner}
    {#if filtersActive}
      <div class="empty-state">
        <p class="empty-h">No matches</p>
        <p class="empty-sub">Nothing matches your current search and filters.</p>
      </div>
    {:else}
      <div class="empty-state">
        No dictations yet. Hold <kbd>{hk1}</kbd> <kbd>{hk2}</kbd> to get started.
      </div>
    {/if}
  {:else}
    <div bind:this={listContainer}>
      <div style="height: {topSpacerHeight}px;"></div>
      {#each visibleItems as { item, index } (item.key)}
        {#if item.type === 'header'}
          <div use:measureItem={item.key} class="day-head" class:muted={index > 0 || hasBanner}>
            {item.label}
          </div>
        {:else if item.type === 'row'}
          <div use:measureItem={item.key} class="day-row" class:first-in-table={(index === 0 && !hasBanner) || flatItems[index - 1]?.type === 'header'}>
            <div class="day-time">{fmtTime(item.entry.created_at)}</div>
            <div class="day-main">
              <div class="day-text">{item.entry.clean_text}</div>
              {#if rowMeta(item.entry)}
                <div class="day-meta">{rowMeta(item.entry)}</div>
              {/if}
            </div>
            <button
              class="copy-btn"
              class:copied={copiedId === item.entry.id}
              onclick={() => onCopy(item.entry)}
              title="Copy to clipboard"
              aria-label="Copy"
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                {#if copiedId === item.entry.id}
                  {@html icons.check}
                {:else}
                  {@html icons.copy}
                {/if}
              </svg>
            </button>
          </div>
        {/if}
      {/each}
      <div style="height: {bottomSpacerHeight}px;"></div>
    </div>
    {#if hasMoreHistory}
      <div class="load-older-wrap">
        <button class="btn-ghost load-older-btn" onclick={onLoadOlder} disabled={loadingMore}>
          {loadingMore ? 'Loading…' : 'Load older'}
        </button>
      </div>
    {/if}
  {/if}
{/if}

<style>
  .load-older-wrap {
    display: flex;
    justify-content: center;
    padding-top: 12px;
  }

  .day-head {
    font-family: var(--serif);
    font-style: italic;
    font-size: 14px;
    color: var(--ink-soft);
    margin: 4px 4px 10px;
  }
  .day-head.muted { margin-top: 22px; color: var(--ink-mute); }
  .day-head-row { display: flex; align-items: center; gap: 12px; }
  .day-head-row > span { flex: 0 0 auto; }

  .day-table { border-top: 1px solid var(--line); }

  .day-row {
    display: grid;
    grid-template-columns: 84px 1fr auto;
    align-items: start;
    padding: 11px 4px;
    border-bottom: 1px solid var(--line);
    gap: 14px;
    cursor: default;
  }
  .day-row:hover { background: var(--control-active); }
  .day-row:not(:hover) .copy-btn:not(:focus-visible) { opacity: 0.25; }
  .day-row:hover .copy-btn { opacity: 0.9; }

  .copy-btn {
    all: unset;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: 4px;
    color: var(--ink-mute);
    opacity: 0.25;
    transition: color 0.12s, opacity 0.12s;
    flex-shrink: 0;
    margin-top: 2px;
  }
  .copy-btn:hover { opacity: 0.9; }
  .copy-btn:focus-visible {
    opacity: 0.9;
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .copy-btn.copied { color: var(--jap-500, #d97757); opacity: 1; }
  .copy-btn svg { width: 10px; height: 10px; }

  .day-time {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--ink-mute);
    font-variant-numeric: tabular-nums;
    padding-top: 2px;
    font-weight: 500;
  }

  .day-text {
    font-size: 13px;
    line-height: 1.55;
    color: var(--ink-strong);
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .day-main {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .day-meta {
    font-size: 10.5px;
    color: var(--ink-faint);
    letter-spacing: 0.01em;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .error-msg {
    color: var(--ink-mute);
    font-style: italic;
  }

  .retry-btn {
    all: unset;
    cursor: pointer;
    font-size: 11px;
    font-weight: 500;
    color: var(--accent);
    padding: 2px 8px;
    border: 1px solid currentColor;
    border-radius: 4px;
    transition: background 0.12s, color 0.12s;
    flex-shrink: 0;
    white-space: nowrap;
    line-height: 1.6;
  }
  .retry-btn:hover:not(:disabled) {
    background: var(--accent);
    color: var(--on-accent, #fff);
  }
  .retry-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .row-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .dismiss-btn {
    all: unset;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border-radius: 4px;
    color: var(--ink-mute);
    transition: color 0.12s, background 0.12s;
  }
  .dismiss-btn:hover { color: var(--ink-strong); background: var(--control-active); }
  .dismiss-btn svg { width: 11px; height: 11px; }

  .empty-state {
    padding: 32px 4px;
    font-size: 13px;
    color: var(--ink-mute);
    font-style: italic;
  }

  .empty-state .empty-h,
  .empty-state .empty-sub {
    font-style: normal;
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

  .empty-state :global(kbd) {
    font-style: normal;
    background: var(--paper-2);
    border: 1px solid var(--line-strong);
    border-radius: 4px;
    font-family: var(--mono);
    font-size: 11px;
    padding: 1px 5px;
    color: var(--ink);
  }

  @media (max-width: 720px) {
    .day-row {
      grid-template-columns: 68px 1fr auto;
      gap: 10px;
    }
  }

  .day-row.first-in-table {
    border-top: 1px solid var(--line);
  }
</style>
