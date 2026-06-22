<script lang="ts">
  import { onMount } from 'svelte';
  import { MOTION_MS, motionMs } from '../../motion';
  import { sortLabels, type SortKey } from './helpers';

  let {
    search = $bindable(),
    sort = $bindable(),
    onNew,
  }: {
    search: string;
    sort: SortKey;
    onNew: () => void;
  } = $props();

  let sortWrapEl = $state<HTMLDivElement | null>(null);
  let sortButtonEls = $state<Record<SortKey, HTMLButtonElement | null>>({
    newest: null,
    oldest: null,
    alpha: null,
    most_used: null,
  });
  let sortIndicatorStyle = $state('opacity:0;');

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
    <span class="sort-indicator" style={sortIndicatorStyle}></span>
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

  <button class="btn-primary" onclick={onNew}>
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 5v14M5 12h14"/></svg>
    New snippet
  </button>
</div>

<style>
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

  @media (max-width: 720px) {
    .search {
      flex-basis: 100%;
    }

    .sort-pills {
      order: 3;
      width: 100%;
      overflow-x: auto;
    }
  }
</style>
