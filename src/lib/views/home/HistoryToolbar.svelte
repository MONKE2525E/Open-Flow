<script lang="ts">
	import { tick } from 'svelte';
	import { scale, slide } from 'svelte/transition';
	import { cubicOut } from 'svelte/easing';
	import Dropdown from '../../components/Dropdown.svelte';
	import { cleanAppName } from '../../appMappings';
	import { formatAppLabel } from './helpers';
	import { motionMs, MOTION_MS } from '../../motion';

  export let search: string;
  export let apps: string[] = [];
  export let appFilter: string | null;
	export let onSearchChange: (value: string) => void;
	export let onAppFilterChange: (app: string | null) => void;
	export let onClearFilters: () => void;

	let appDropdownOpen = false;
	let uiExpanded = false;
	let groupEl: HTMLElement | null = null;
	let inputEl: HTMLInputElement | null = null;

	$: filtersActive = (search ?? '').trim().length > 0 || appFilter !== null;
	$: expanded = uiExpanded || filtersActive;

	async function expandSearch() {
		uiExpanded = true;
		await tick();
		inputEl?.focus();
	}

	function handleGroupFocusOut() {
		requestAnimationFrame(() => {
			if (groupEl && document.activeElement && groupEl.contains(document.activeElement)) return;
			if (!filtersActive) uiExpanded = false;
		});
	}
</script>

<div class="history-toolbar">
	<div class="history-app-anchor">
		<Dropdown bind:open={appDropdownOpen} closeSelector=".history-app-dropdown">
			<div class="ui-dropdown history-app-dropdown">
				<button
					class="ui-dropdown-trigger ui-dropdown-trigger--compact"
					aria-haspopup="listbox"
					aria-expanded={appDropdownOpen}
					aria-controls="history-app-menu"
					onclick={() => (appDropdownOpen = !appDropdownOpen)}
				>
					<span>{appFilter ? cleanAppName(appFilter) : 'All apps'}</span>
					<svg class="ui-chevron" class:open={appDropdownOpen} width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m6 9 6 6 6-6" /></svg>
				</button>
				{#if appDropdownOpen}
					<div id="history-app-menu" class="ui-dropdown-menu history-app-menu scroll-styled" role="listbox" aria-label="Filter history by app" transition:scale={{ duration: motionMs(MOTION_MS.fast), start: 0.96, opacity: 0 }}>
						<button class="ui-dropdown-option" class:active={!appFilter} role="option" aria-selected={!appFilter} onclick={() => { onAppFilterChange(null); appDropdownOpen = false; }}>All apps</button>
						{#each apps as app}
							<button class="ui-dropdown-option" class:active={appFilter === app} role="option" aria-selected={appFilter === app} onclick={() => { onAppFilterChange(app); appDropdownOpen = false; }}>{formatAppLabel(app)}</button>
						{/each}
					</div>
				{/if}
			</div>
		</Dropdown>
	</div>

	<div class="history-search-group" class:expanded bind:this={groupEl} onfocusout={handleGroupFocusOut}>
		{#if !expanded}
			<button class="search-icon-btn ui-focus-ring" onclick={expandSearch} aria-label="Search history">
				<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="11" cy="11" r="7" /><path d="m21 21-4.35-4.35" /></svg>
			</button>
		{:else}
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
		<input bind:this={inputEl}
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
		{/if}
	</div>

	{#if filtersActive}
		<button class="btn-ghost clear-filters-btn ui-focus-ring" onclick={onClearFilters} transition:slide={{ axis: 'x', duration: motionMs(MOTION_MS.base), easing: cubicOut }}>Clear filters</button>
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
	.history-app-anchor { flex-shrink: 0; }
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
		transition: flex-basis var(--ui-duration-base) var(--ui-ease-out), border-color var(--ui-duration-fast) var(--ui-ease-out), background-color var(--ui-duration-fast) var(--ui-ease-out);
	}
	.history-search-group.expanded { flex: 1 1 260px; min-width: 160px; max-width: 320px; background: var(--bg-elev); border-color: var(--line); overflow: visible; }
	.search-icon-btn { all: unset; box-sizing: border-box; width: 32px; height: 32px; flex-shrink: 0; display: grid; place-items: center; cursor: pointer; color: var(--ink-mute); border-radius: var(--r-sm); }
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
  .history-search .ui-input {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: 1px solid transparent;
  }
  .history-search .ui-input:focus-visible {
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
    width: 16px;
    height: 16px;
    border-radius: 4px;
    flex-shrink: 0;
  }
  .clear-btn:hover {
    color: var(--ink-strong);
  }

	.history-app-menu {
    width: max-content;
    min-width: 180px;
		max-width: 280px;
		left: 0;
		right: auto;
	}
	.clear-filters-btn { height: 32px; white-space: nowrap; flex-shrink: 0; }

  @media (max-width: 560px) {
    .history-toolbar {
      flex-wrap: wrap;
    }
		.history-search-group.expanded { flex-basis: 100%; max-width: none; }
  }
</style>
