<script lang="ts">
  import { onMount } from 'svelte';
  import { MOTION_MS, motionMs } from '../../motion';
  import { sortLabels, type SortKey } from './helpers';

  let {
    search = $bindable(),
    sort = $bindable(),
    count,
    onAdd,
  }: {
    search: string;
    sort: SortKey;
    count: number;
    onAdd: () => void;
  } = $props();

  let sortWrapEl = $state<HTMLDivElement | null>(null);
  let sortButtonEls = $state<Record<SortKey, HTMLButtonElement | null>>({
    newest: null,
    oldest: null,
    alpha: null,
    most_corrected: null,
  });
  let sortIndicatorStyle = $state('opacity:0;');

  function updateSortIndicator() {
    const wrap = sortWrapEl;
    const btn = sortButtonEls[sort];
    if (!wrap || !btn) return;
    const left = btn.offsetLeft;
    const width = btn.offsetWidth;
    sortIndicatorStyle = `opacity:1; left:${left}px; width:${width}px; transition: left ${motionMs(MOTION_MS.base)}ms cubic-bezier(0.22, 1, 0.36, 1), width ${motionMs(MOTION_MS.base)}ms cubic-bezier(0.22, 1, 0.36, 1), opacity ${motionMs(MOTION_MS.fast)}ms ease;`;
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
      class="ui-input ui-input--dense"
      type="text"
      placeholder={`Search ${count} ${count === 1 ? 'term' : 'terms'}…`}
      bind:value={search}
      aria-label="Search dictionary"
    />
    {#if search}
      <button class="clear-btn ui-focus-ring" onclick={() => search = ''} aria-label="Clear">
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
      </button>
    {/if}
  </div>

  <div class="sort-pills" bind:this={sortWrapEl}>
    <span class="sort-indicator" style={sortIndicatorStyle}></span>
    {#each sortLabels as { key, label }}
      <button
        class="sort-pill ui-focus-ring"
        class:active={sort === key}
        aria-pressed={sort === key}
        bind:this={sortButtonEls[key]}
        onclick={() => { sort = key; }}
      >{label}</button>
    {/each}
  </div>

  <button class="btn-primary" onclick={onAdd}>
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 5v14M5 12h14"/></svg>
    Add term
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
    border-radius: var(--r-sm);
    height: 32px;
    padding: 0 9px;
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--ink-mute);
  }
  .search .ui-input {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: 1px solid transparent;
  }
  .search .ui-input:focus-visible {
    border-color: var(--accent);
    box-shadow: var(--ui-focus-ring);
    outline: none;
  }

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
    border-radius: var(--r-sm);
    padding: 3px;
    position: relative;
    overflow: hidden;
    align-items: center;
  }

  .sort-indicator {
    position: absolute;
    top: 3px;
    left: 3px;
    height: calc(100% - 6px);
    border-radius: calc(var(--r-sm) - 3px);
    background: var(--accent-soft);
    z-index: 0;
    pointer-events: none;
    opacity: 0;
  }

  .sort-pill {
    background: transparent;
    border: 0;
    border-radius: calc(var(--r-sm) - 3px);
    box-sizing: border-box;
    height: 24px;
    padding: 0 9px;
    font-size: 11.5px;
    font-weight: 500;
    font-family: var(--sans);
    color: var(--ink-mute);
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.12s, color 0.12s;
    position: relative;
    z-index: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
  }
  .sort-pill:hover { color: var(--ink-strong); }
  .sort-pill:hover:not(.active) { background: var(--control-hover); }
  .sort-pill.active { color: var(--accent-ink); }
  .sort-pill.active:hover { background: transparent; }

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
    .search { flex-basis: 100%; }
    .sort-pills { order: 3; width: 100%; overflow-x: auto; }
  }
</style>
