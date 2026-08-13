<script lang="ts">
  import Dropdown from '../../components/Dropdown.svelte';
  import { cleanAppName } from '../../appMappings';
  import { formatAppLabel } from './helpers';

  export let search: string;
  export let apps: string[];
  export let appFilter: string | null;
  export let onSearchChange: (value: string) => void;
  export let onAppFilterChange: (app: string | null) => void;
  export let onClearFilters: () => void;

  let appDropdownOpen = false;

  $: filtersActive = (search ?? '').trim().length > 0 || appFilter !== null;
</script>

<div class="history-toolbar">
  <div class="history-search-group">
    <div class="history-search">
      <svg
        width="13"
        height="13"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.6"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <circle cx="11" cy="11" r="7" /><path d="m21 21-4.35-4.35" />
      </svg>
      <input
        class="ui-input ui-input--dense"
        type="text"
        placeholder="Search history or app…"
        value={search}
        oninput={(e) => onSearchChange(e.currentTarget.value)}
        aria-label="Search history"
      />
      {#if search}
        <button
          class="clear-btn ui-focus-ring"
          onclick={() => onSearchChange('')}
          aria-label="Clear search"
        >
          <svg
            width="10"
            height="10"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2.5"
            stroke-linecap="round"
            aria-hidden="true"
          >
            <path d="M18 6 6 18M6 6l12 12" />
          </svg>
        </button>
      {/if}
    </div>

    <div class="history-search-divider"></div>

    <Dropdown bind:open={appDropdownOpen} closeSelector=".history-app-dropdown">
      <div class="ui-dropdown history-app-dropdown">
        <button
          class="btn-ghost ui-dropdown-trigger history-app-trigger"
          aria-haspopup="listbox"
          aria-expanded={appDropdownOpen}
          aria-controls="history-app-menu"
          onclick={() => (appDropdownOpen = !appDropdownOpen)}
        >
          <span>{appFilter ? cleanAppName(appFilter) : 'All apps'}</span>
          <svg
            class="ui-chevron"
            class:open={appDropdownOpen}
            width="10"
            height="10"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="m6 9 6 6 6-6" />
          </svg>
        </button>
        {#if appDropdownOpen}
          <div
            id="history-app-menu"
            class="ui-dropdown-menu history-app-menu scroll-styled"
            role="listbox"
            aria-label="Filter history by app"
          >
            <button
              class="ui-dropdown-option"
              class:active={!appFilter}
              role="option"
              aria-selected={!appFilter}
              onclick={() => { onAppFilterChange(null); appDropdownOpen = false; }}
            >
              All apps
            </button>
            {#each apps as app}
              <button
                class="ui-dropdown-option"
                class:active={appFilter === app}
                role="option"
                aria-selected={appFilter === app}
                onclick={() => { onAppFilterChange(app); appDropdownOpen = false; }}
              >
                {formatAppLabel(app)}
              </button>
            {/each}
          </div>
        {/if}
      </div>
    </Dropdown>
  </div>

  {#if filtersActive}
    <button class="btn-ghost clear-filters-btn ui-focus-ring" onclick={onClearFilters}>
      Clear filters
    </button>
  {/if}
</div>

<style>
  .history-toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 16px;
  }

  /* The search input and the app dropdown are one visual unit: a single
     bordered field whose orange focus ring wraps both, so focusing either
     control highlights the whole bar instead of just the text box. */
  .history-search-group {
    flex: 1 1 260px;
    min-width: 160px;
    display: flex;
    align-items: center;
    height: 32px;
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    color: var(--ink-mute);
    transition:
      border-color var(--ui-duration-fast) var(--ui-ease-out),
      box-shadow var(--ui-duration-fast) var(--ui-ease-out);
  }
  .history-search-group:focus-within {
    border-color: var(--accent);
    box-shadow: var(--ui-focus-ring);
  }

  .history-search {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0 9px;
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .history-search .ui-input {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: 1px solid transparent;
  }
  .history-search .ui-input:focus-visible {
    outline: none;
  }

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
  .clear-btn:hover {
    color: var(--ink-strong);
  }

  .history-app-trigger {
    height: 100%;
    border: 0;
    border-radius: 0 var(--r-sm) var(--r-sm) 0;
    background: transparent;
  }
  .history-app-trigger:hover,
  .history-app-trigger[aria-expanded='true'] {
    background: var(--control-hover);
  }

  .history-app-menu {
    width: max-content;
    min-width: 180px;
    max-width: 280px;
  }

  .clear-filters-btn {
    height: 32px;
    white-space: nowrap;
    flex-shrink: 0;
  }

  @media (max-width: 560px) {
    .history-toolbar {
      flex-wrap: wrap;
    }
    .history-search-group {
      flex-basis: 100%;
    }
  }
</style>
