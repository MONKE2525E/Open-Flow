<script lang="ts">
  import { fade } from 'svelte/transition';
  import { flip } from 'svelte/animate';
  import { expoOut } from 'svelte/easing';
  import type { Snippet } from '../../stores';
  import { listItemCollapse, MOTION_MS, motionMs } from '../../motion';
  import { fmtDate } from './helpers';

  let {
    filtered,
    visibleFiltered,
    search,
    selectedId,
    onSelect,
    onClearSearch,
  }: {
    filtered: Snippet[];
    visibleFiltered: Snippet[];
    search: string;
    selectedId: number | null;
    onSelect: (s: Snippet) => void;
    onClearSearch: () => void;
  } = $props();
</script>

<div class="snip-list-col">
  {#if filtered.length === 0}
    <div class="empty-state" in:fade={{ duration: 200 }}>
      <p class="empty-h">No matches</p>
      <p class="empty-sub">Nothing matches "{search}".</p>
      <button class="btn-ghost" onclick={onClearSearch}>Clear search</button>
    </div>
  {:else if visibleFiltered.length === 0}
    <div class="snip-list" aria-hidden="true"></div>
  {:else}
    <div class="snip-list">
      {#each visibleFiltered as s (s.id)}
        <button
          type="button"
          class="snip-row"
          class:is-selected={selectedId === s.id}
          aria-pressed={selectedId === s.id}
          animate:flip={{ duration: motionMs(MOTION_MS.panel), easing: expoOut }}
          out:listItemCollapse={{ duration: 200 }}
          onclick={() => onSelect(s)}
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

<style>
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

  @media (max-width: 720px) {
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
</style>
