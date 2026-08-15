<script lang="ts">
  import { tick } from 'svelte';
  import { scale, slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import Dropdown from '../../components/Dropdown.svelte';
  import { formatAppLabel } from './helpers';
  import { motionMs, MOTION_MS } from '../../motion';

  type Props = {
    search: string;
    apps?: string[];
    appFilter: string | null;
    onSearchChange: (value: string) => void;
    onAppFilterChange: (app: string | null) => void;
    onClearFilters: () => void;
  };

  let {
    search,
    apps = [],
    appFilter,
    onSearchChange,
    onAppFilterChange,
    onClearFilters,
  }: Props = $props();

  let appDropdownOpen = $state(false);
  let uiExpanded = $state(false);
  let preserveExpanded = $state(false);
  let expandLockUntil = 0;
  let groupEl = $state<HTMLElement | null>(null);
  let inputEl = $state<HTMLInputElement | null>(null);
  let appTriggerEl = $state<HTMLButtonElement | null>(null);

  let filtersActive = $derived((search ?? '').trim().length > 0 || appFilter !== null);
  let expanded = $derived(uiExpanded || filtersActive);

  async function expandSearch() {
    // Replacing the focused icon button with the input emits focusout before
    // the new input can receive focus. Keep the group open through that one
    // DOM transition so keyboard and automation users can type immediately.
    preserveExpanded = true;
    // The icon is replaced during the click/focus transition. Keep the
    // expanded control alive long enough for the replacement input to receive
    // focus, even when the browser reports the intermediate focusout late.
    expandLockUntil = Date.now() + 5000;
    uiExpanded = true;
    await tick();
    inputEl?.focus();
    requestAnimationFrame(() => {
      preserveExpanded = false;
    });
  }

  async function selectAppFilter(app: string | null) {
    preserveExpanded = true;
    appDropdownOpen = false;
    onAppFilterChange(app);
    await tick();
    appTriggerEl?.focus();
    requestAnimationFrame(() => {
      preserveExpanded = false;
    });
  }

  function handleGroupFocusOut() {
    requestAnimationFrame(() => {
      if (preserveExpanded) {
        // Some browsers finish the pointer transition after the replacement
        // frame, briefly leaving body as the active element. Give the input a
        // short settling window before deciding the group was really exited.
        setTimeout(collapseIfFocusLeft, 300);
        return;
      }
      collapseIfFocusLeft();
    });
  }

  function collapseIfFocusLeft() {
    if (Date.now() < expandLockUntil) {
      setTimeout(collapseIfFocusLeft, expandLockUntil - Date.now());
      return;
    }
    if (filtersActive) return;
    if (groupEl && document.activeElement && groupEl.contains(document.activeElement)) return;
    uiExpanded = false;
  }
</script>

<div class="history-toolbar">
  <div class="history-search-group" class:expanded bind:this={groupEl} onfocusout={handleGroupFocusOut}>
    {#if !expanded}
      <button class="search-icon-btn ui-focus-ring" onclick={expandSearch} aria-label="Search history">
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="11" cy="11" r="7" /><path d="m21 21-4.35-4.35" /></svg>
      </button>
    {:else}
      <div class="history-search">
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="11" cy="11" r="7" /><path d="m21 21-4.35-4.35" /></svg>
        <input
          bind:this={inputEl}
          class="ui-input ui-input--dense"
          type="text"
          placeholder="Search history or app..."
          value={search}
          onfocus={() => {
            expandLockUntil = 0;
            preserveExpanded = false;
          }}
          oninput={(event) => onSearchChange(event.currentTarget.value)}
          aria-label="Search history"
        />
        {#if search}
          <button class="clear-btn ui-focus-ring" onmousedown={(event) => event.preventDefault()} onclick={() => onSearchChange('')} aria-label="Clear search">
            <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" aria-hidden="true"><path d="M18 6 6 18M6 6l12 12" /></svg>
          </button>
        {/if}
      </div>

      <div class="history-search-divider"></div>

      <Dropdown bind:open={appDropdownOpen} closeSelector=".history-app-dropdown">
        <div class="ui-dropdown history-app-dropdown">
          <button
            bind:this={appTriggerEl}
            class="ui-dropdown-trigger ui-dropdown-trigger--compact history-app-trigger"
            aria-haspopup="listbox"
            aria-expanded={appDropdownOpen}
            aria-controls="history-app-menu"
            onclick={() => (appDropdownOpen = !appDropdownOpen)}
          >
            <span>{appFilter ? formatAppLabel(appFilter) : 'All apps'}</span>
            <svg class="ui-chevron" class:open={appDropdownOpen} width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m6 9 6 6 6-6" /></svg>
          </button>
          {#if appDropdownOpen}
            <div id="history-app-menu" class="ui-dropdown-menu history-app-menu scroll-styled" role="listbox" aria-label="Filter history by app" transition:scale={{ duration: motionMs(MOTION_MS.fast), start: 0.96, opacity: 0 }}>
              <button class="ui-dropdown-option" class:active={!appFilter} role="option" aria-selected={!appFilter} onclick={() => selectAppFilter(null)}>All apps</button>
              {#each apps as app}
                <button class="ui-dropdown-option" class:active={appFilter === app} role="option" aria-selected={appFilter === app} onclick={() => selectAppFilter(app)}>{formatAppLabel(app)}</button>
              {/each}
            </div>
          {/if}
        </div>
      </Dropdown>

      {#if filtersActive}
        <div class="clear-filters-wrap" transition:slide={{ axis: 'x', duration: motionMs(MOTION_MS.base), easing: cubicOut }}>
          <div class="history-search-divider"></div>
          <button class="btn-ghost clear-filters-btn ui-focus-ring" onclick={onClearFilters}>Clear filters</button>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .history-toolbar { display: flex; align-items: center; justify-content: flex-end; flex: 1 1 auto; min-width: 0; }

  .history-search-group {
    flex: 0 0 32px;
    min-width: 32px;
    display: flex;
    align-items: center;
    height: 32px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--r-sm);
    color: var(--ink-mute);
    margin-left: auto;
    overflow: hidden;
    transition:
      flex var(--ui-duration-base) var(--ui-ease-out),
      border-color var(--ui-duration-fast) var(--ui-ease-out),
      background-color var(--ui-duration-fast) var(--ui-ease-out);
  }

  .history-search-group.expanded {
    flex: 1 1 260px;
    min-width: 160px;
    max-width: 460px;
    background: var(--bg-elev);
    border-color: var(--line);
    overflow: visible;
  }

  .search-icon-btn {
    all: unset;
    box-sizing: border-box;
    width: 32px;
    height: 32px;
    flex-shrink: 0;
    display: grid;
    place-items: center;
    cursor: pointer;
    color: var(--ink-mute);
    border-radius: var(--r-sm);
    transition: color var(--ui-duration-fast) var(--ui-ease-out);
  }

  .search-icon-btn:hover { color: var(--ink-strong); }

  .history-search {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0 9px;
    display: flex;
    align-items: center;
    gap: 7px;
  }

  .history-search .ui-input { flex: 1; min-width: 0; background: transparent; border: 1px solid transparent; }
  .history-search .ui-input:focus-visible { outline: none; }

  .history-search-divider {
    width: 1px;
    height: 18px;
    background: var(--line);
    flex-shrink: 0;
  }

  .clear-btn {
    background: transparent;
    border: 0;
    padding: 0;
    color: var(--ink-mute);
    cursor: pointer;
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: 4px;
    flex-shrink: 0;
  }

  .clear-btn:hover { color: var(--ink-strong); }

  .history-app-trigger { border-color: transparent; background: transparent; flex-shrink: 0; }
  .history-app-trigger:hover,
  .history-app-trigger[aria-expanded='true'] { background: var(--control-hover); border-color: transparent; }

  .history-app-menu { width: max-content; min-width: 180px; max-width: 280px; }

  .clear-filters-wrap { display: flex; align-items: center; height: 100%; flex-shrink: 0; }

  .clear-filters-btn {
    height: 100%;
    padding: 0 12px;
    border-color: transparent;
    border-radius: 0 var(--r-sm) var(--r-sm) 0;
  }

  .clear-filters-btn:hover { background: var(--control-hover); border-color: transparent; }

  @media (max-width: 560px) {
    .history-search-group.expanded { flex-basis: 100%; max-width: none; }
  }
</style>
