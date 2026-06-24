<script lang="ts">
  import { fade } from 'svelte/transition';
  import { flip } from 'svelte/animate';
  import { expoOut } from 'svelte/easing';
  import type { DictionaryEntry } from '../../stores';
  import { listItemCollapse, MOTION_MS, motionMs } from '../../motion';
  import { confidenceLabel, fmtDate } from './helpers';

  let {
    filtered,
    visibleFiltered,
    search,
    selectedId,
    onSelect,
    onClearSearch,
  }: {
    filtered: DictionaryEntry[];
    visibleFiltered: DictionaryEntry[];
    search: string;
    selectedId: number | null;
    onSelect: (e: DictionaryEntry) => void;
    onClearSearch: () => void;
  } = $props();
</script>

<div class="dict-list-col">
  {#if filtered.length === 0}
    <div class="empty-state" in:fade={{ duration: 200 }}>
      <p class="empty-h">No matches</p>
      <p class="empty-sub">Nothing matches "{search}".</p>
      <button class="btn-ghost" onclick={onClearSearch}>Clear search</button>
    </div>
  {:else if visibleFiltered.length === 0}
    <div class="dict-list" aria-hidden="true"></div>
  {:else}
    <div class="dict-list">
      {#each visibleFiltered as e (e.id)}
        <button
          type="button"
          class="dict-row"
          class:is-selected={selectedId === e.id}
          aria-pressed={selectedId === e.id}
          animate:flip={{ duration: motionMs(MOTION_MS.panel), easing: expoOut }}
          out:listItemCollapse={{ duration: 200 }}
          onclick={() => onSelect(e)}
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

<style>
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
</style>
