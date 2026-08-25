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

  $effect(() => {
    const handleAway = (event: Event) => {
      if (!uiExpanded || filtersActive || !groupEl) return;
      const target = event.target;
      if (target instanceof Node && !groupEl.contains(target)) uiExpanded = false;
    };
    document.addEventListener('pointerdown', handleAway);
    document.addEventListener('focusin', handleAway);
    return () => {
      document.removeEventListener('pointerdown', handleAway);
      document.removeEventListener('focusin', handleAway);
    };
  });

</script>

<div class="history-toolbar">
  <div class="history-search-group" class:expanded bind:this={groupEl}>
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
<<<<<<< New base: Fix Insights chart and macOS animations
  /* WebView2 reserves a native scrollbar gutter even while the custom thumb is
     transparent. That leaves the selected row looking clipped on Windows.
     The menu remains wheel/trackpad-scrollable without the gutter. */
  :global(.app-windows) .history-app-menu {
    scrollbar-width: none;
  }
  :global(.app-windows) .history-app-menu::-webkit-scrollbar {
    width: 0;
    height: 0;
  }
  .history-app-menu.opens-up { bottom: calc(100% + 4px); top: auto; }

  .clear-filters-wrap { display: flex; align-items: center; height: 100%; flex-shrink: 0; }

||||||| Common ancestor
  .history-app-menu.opens-up { bottom: calc(100% + 4px); top: auto; }

  .clear-filters-wrap { display: flex; align-items: center; height: 100%; flex-shrink: 0; }

=======

  .clear-filters-wrap { display: flex; align-items: center; height: 100%; flex-shrink: 0; }

>>>>>>> Current commit: Address PR review findings
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

  /* A compositor-backed reveal keeps the menu animation visible in WebKit,
     including the macOS Tauri window. The Svelte transition still provides
     the matching exit animation. */
  .history-app-menu { animation: history-app-menu-enter var(--ui-duration-fast) var(--ui-ease-out); transform-origin: top right; will-change: opacity, transform; }

  @keyframes history-app-menu-enter {
    from { opacity: 0; transform: translate3d(0, -6px, 0) scale(0.98); }
    to { opacity: 1; transform: translate3d(0, 0, 0) scale(1); }
  }

  @media (prefers-reduced-motion: reduce) {
    .history-app-menu { animation: none; will-change: auto; }
  }
</style>
