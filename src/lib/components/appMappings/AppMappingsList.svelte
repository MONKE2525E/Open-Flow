<script lang="ts">
  import { fly, fade } from 'svelte/transition';
  import { flip } from 'svelte/animate';
  import { expoOut } from 'svelte/easing';
  import {
    cleanupIntensityOptions,
    getAppDisplayName,
    getCleanupIntensityLabel,
    getProfileLabel,
    profileOptions,
    type AppMapping,
    type InstalledApp,
  } from '../../appMappings';
  import { listItemCollapse, MOTION_MS, MOTION_PX, motionMs, motionPx } from '../../motion';
  import { focusListboxOption, handleListboxOptionKeydown } from './listbox';

  let {
    mappings,
    installedApps,
    emptyText,
    leavingExes,
    onDelete,
    onUpdateField,
  }: {
    mappings: AppMapping[];
    installedApps: InstalledApp[];
    emptyText: string;
    leavingExes: Set<string>;
    onDelete: (exe: string) => void | Promise<void>;
    onUpdateField: (
      exe: string,
      patch: Partial<AppMapping>,
    ) => void | Promise<void>;
  } = $props();

  type ActiveRowDropdown = {
    exe: string;
    field: 'profile' | 'cleanup';
    mapping: AppMapping;
  };

  const cleanupIntensityChoices = [
    { id: '', label: 'Default' },
    ...cleanupIntensityOptions,
  ] as const;

  let openRowDropdown = $state<string | null>(null);
  let rowDropdownPos = $state<{ top: number; right: number } | null>(null);
  let lastRowDropdownTrigger = $state<HTMLButtonElement | null>(null);

  const visibleMappings = $derived(mappings.filter((mapping) => !leavingExes.has(mapping.exe)));
  const activeRowDropdown = $derived.by<ActiveRowDropdown | null>(() => {
    if (!openRowDropdown) return null;
    const sep = openRowDropdown.lastIndexOf(':');
    if (sep === -1) return null;
    const exe = openRowDropdown.slice(0, sep);
    const field = openRowDropdown.slice(sep + 1) as 'profile' | 'cleanup';
    const mapping = mappings.find((entry) => entry.exe === exe);
    if (!mapping) return null;
    return { exe, field, mapping };
  });
  const activeRowMenuId = $derived(activeRowDropdown ? menuIdFor(openRowDropdown) : '');

  function menuIdFor(key: string | null): string {
    return `app-mappings-row-menu-${(key || 'menu').replace(/[^a-z0-9_-]/gi, '-')}`;
  }

  function closeRowDropdownMenu() {
    openRowDropdown = null;
    rowDropdownPos = null;
  }

  async function openRowDropdownMenu(
    key: string,
    trigger: HTMLButtonElement,
    preferLast = false,
  ) {
    const rect = trigger.getBoundingClientRect();
    rowDropdownPos = { top: rect.bottom + 4, right: window.innerWidth - rect.right };
    openRowDropdown = key;
    lastRowDropdownTrigger = trigger;
    await focusListboxOption(menuIdFor(key), preferLast);
  }

  function toggleRowDropdown(key: string, event: MouseEvent) {
    if (openRowDropdown === key) {
      closeRowDropdownMenu();
      return;
    }

    void openRowDropdownMenu(key, event.currentTarget as HTMLButtonElement);
  }

  function closeRowDropdown(event: MouseEvent | PointerEvent) {
    const target = event.target;
    if (target instanceof Element && !target.closest('.row-drop-wrap') && !target.closest('.row-drop-menu')) {
      closeRowDropdownMenu();
    }
  }

  function handleRowDropdownKeydown(event: KeyboardEvent) {
    if ((event.key === 'Enter' || event.key === ' ') && !openRowDropdown) {
      const key = (event.currentTarget as HTMLElement | null)?.getAttribute('data-dropdown-key');
      if (key && event.currentTarget instanceof HTMLButtonElement) {
        event.preventDefault();
        void openRowDropdownMenu(key, event.currentTarget);
      }
      return;
    }
    if (event.key === 'ArrowDown') {
      const key = (event.currentTarget as HTMLElement | null)?.getAttribute('data-dropdown-key');
      if (key && event.currentTarget instanceof HTMLButtonElement) {
        event.preventDefault();
        void openRowDropdownMenu(key, event.currentTarget);
      }
      return;
    }
    if (event.key === 'ArrowUp') {
      const key = (event.currentTarget as HTMLElement | null)?.getAttribute('data-dropdown-key');
      if (key && event.currentTarget instanceof HTMLButtonElement) {
        event.preventDefault();
        void openRowDropdownMenu(key, event.currentTarget, true);
      }
      return;
    }
    if (event.key === 'Escape' && openRowDropdown) {
      closeRowDropdownMenu();
      lastRowDropdownTrigger?.focus();
      event.stopPropagation();
    }
  }

  async function selectProfile(exe: string, profile: string) {
    await onUpdateField(exe, { profile });
    closeRowDropdownMenu();
    lastRowDropdownTrigger?.focus();
  }

  async function selectCleanup(exe: string, cleanupIntensity: string) {
    await onUpdateField(exe, {
      cleanup_intensity: cleanupIntensity || undefined,
    });
    closeRowDropdownMenu();
    lastRowDropdownTrigger?.focus();
  }

  $effect(() => {
    if (!openRowDropdown) return;

    const handleScroll = () => {
      openRowDropdown = null;
      rowDropdownPos = null;
    };

    const timeout = window.setTimeout(() => {
      window.addEventListener('pointerdown', closeRowDropdown);
      window.addEventListener('scroll', handleScroll, { capture: true, passive: true });
    });

    return () => {
      window.clearTimeout(timeout);
      window.removeEventListener('pointerdown', closeRowDropdown);
      window.removeEventListener('scroll', handleScroll, { capture: true });
    };
  });
</script>

<div class="mapping-region">
  {#if mappings.length > 0}
    <div class="mapping-list">
      {#each visibleMappings as mapping (mapping.exe)}
        <div
          class="mapping-row"
          animate:flip={{ duration: motionMs(300), easing: expoOut }}
          in:fly={{ y: motionPx(10), duration: motionMs(300), easing: expoOut }}
          out:listItemCollapse={{ duration: 200 }}
        >
          <div class="mapping-app-info">
            <span class="mapping-app-name">{getAppDisplayName(mapping, installedApps)}</span>
            <span class="mapping-exe-pill" aria-hidden="true">{mapping.exe}</span>
          </div>
          <div class="row-drop-wrap" role="presentation" onclick={(event) => event.stopPropagation()}>
            <button
              type="button"
              class="mapping-badge-btn"
              onclick={(event) => toggleRowDropdown(`${mapping.exe}:profile`, event)}
              onkeydown={handleRowDropdownKeydown}
              data-dropdown-key={`${mapping.exe}:profile`}
              aria-haspopup="listbox"
              aria-expanded={openRowDropdown === `${mapping.exe}:profile`}
              aria-controls={menuIdFor(`${mapping.exe}:profile`)}
              title="Tone for {getAppDisplayName(mapping, installedApps)}"
            >
              {getProfileLabel(mapping.profile)}
              <svg class:open={openRowDropdown === `${mapping.exe}:profile`} width="9" height="9" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                <path d="m6 9 6 6 6-6"/>
              </svg>
            </button>
          </div>
          <div class="row-drop-wrap" role="presentation" onclick={(event) => event.stopPropagation()}>
            <button
              type="button"
              class="mapping-badge-btn"
              class:is-default={!mapping.cleanup_intensity}
              onclick={(event) => toggleRowDropdown(`${mapping.exe}:cleanup`, event)}
              onkeydown={handleRowDropdownKeydown}
              data-dropdown-key={`${mapping.exe}:cleanup`}
              aria-haspopup="listbox"
              aria-expanded={openRowDropdown === `${mapping.exe}:cleanup`}
              aria-controls={menuIdFor(`${mapping.exe}:cleanup`)}
              title="Cleanup style for {getAppDisplayName(mapping, installedApps)}"
            >
              {getCleanupIntensityLabel(mapping.cleanup_intensity)}
              <svg class:open={openRowDropdown === `${mapping.exe}:cleanup`} width="9" height="9" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                <path d="m6 9 6 6 6-6"/>
              </svg>
            </button>
          </div>
          <button class="mapping-delete-btn" onclick={() => onDelete(mapping.exe)} title="Remove {getAppDisplayName(mapping, installedApps)}" aria-label="Remove {getAppDisplayName(mapping, installedApps)}">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" aria-hidden="true">
              <path d="M18 6 6 18M6 6l12 12"/>
            </svg>
          </button>
        </div>
      {/each}
    </div>
  {:else}
    <div class="mapping-empty" in:fade={{ duration: motionMs(MOTION_MS.fast) }} out:fade={{ duration: motionMs(MOTION_MS.fast) }}>{emptyText}</div>
  {/if}
</div>

{#if activeRowDropdown && rowDropdownPos}
  <div
    id={activeRowMenuId}
    class="row-drop-menu scroll-styled"
    role="listbox"
    tabindex="-1"
    aria-label={activeRowDropdown.field === 'profile' ? 'Tone options' : 'Cleanup intensity options'}
    style="top: {rowDropdownPos.top}px; right: {rowDropdownPos.right}px;"
    onpointerdown={(event) => event.stopPropagation()}
    in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.fast), easing: expoOut }}
    out:fade={{ duration: motionMs(100) }}
  >
    {#if activeRowDropdown.field === 'profile'}
      {#each profileOptions as profile}
        <button
          type="button"
          class="row-drop-item"
          class:active={activeRowDropdown.mapping.profile === profile.id}
          onclick={() => selectProfile(activeRowDropdown.exe, profile.id)}
          onkeydown={(event) =>
            handleListboxOptionKeydown(event, activeRowMenuId, () => {
              closeRowDropdownMenu();
              lastRowDropdownTrigger?.focus();
            })}
          role="option"
          aria-selected={activeRowDropdown.mapping.profile === profile.id}
          tabindex="-1"
        >
          {profile.label}
        </button>
      {/each}
    {:else}
      {#each cleanupIntensityChoices as choice}
        <button
          type="button"
          class="row-drop-item"
          class:active={(activeRowDropdown.mapping.cleanup_intensity || '') === choice.id}
          onclick={() => selectCleanup(activeRowDropdown.exe, choice.id)}
          onkeydown={(event) =>
            handleListboxOptionKeydown(event, activeRowMenuId, () => {
              closeRowDropdownMenu();
              lastRowDropdownTrigger?.focus();
            })}
          role="option"
          aria-selected={(activeRowDropdown.mapping.cleanup_intensity || '') === choice.id}
          tabindex="-1"
        >
          {choice.label}
        </button>
      {/each}
    {/if}
  </div>
{/if}

<style>
  .mapping-region {
    max-width: var(--mappings-measure, 640px);
    margin-bottom: 20px;
  }

  .mapping-list {
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    overflow: hidden;
    max-width: var(--mappings-measure, 640px);
  }

  .mapping-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--line);
    background: var(--bg-elev);
  }

  .mapping-row:last-child { border-bottom: none; }

  .mapping-app-info {
    flex: 1;
    min-width: 0;
  }

  .mapping-app-name {
    display: block;
    font-size: 13px;
    font-weight: 500;
    color: var(--ink-strong);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .mapping-badge-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    font-weight: 500;
    font-family: var(--sans);
    color: var(--accent-ink);
    background: var(--accent-soft);
    border: 1px solid color-mix(in oklab, var(--accent) 28%, transparent);
    border-radius: 4px;
    padding: 2px 8px;
    cursor: pointer;
    white-space: nowrap;
  }

  .mapping-badge-btn:hover { background: color-mix(in oklab, var(--accent-soft) 80%, var(--accent) 20%); }

  .mapping-badge-btn.is-default {
    color: var(--ink-mute);
    background: transparent;
    border-color: var(--line-strong);
  }

  .mapping-badge-btn.is-default:hover { background: var(--control-hover); }

  .mapping-badge-btn svg { transition: transform 150ms; flex-shrink: 0; }
  .mapping-badge-btn svg.open { transform: rotate(180deg); }

  .row-drop-wrap {
    position: relative;
    flex-shrink: 0;
  }

  .row-drop-menu {
    position: fixed;
    z-index: 50;
    min-width: 120px;
    max-height: 220px;
    overflow-y: auto;
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    box-shadow: var(--shadow-popover);
  }

  .row-drop-item {
    display: block;
    width: 100%;
    text-align: left;
    padding: 7px 10px;
    font-size: 12px;
    font-family: var(--sans);
    color: var(--ink-strong);
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--line);
    cursor: pointer;
    white-space: nowrap;
  }

  .row-drop-item:last-child {
    border-bottom: none;
  }

  .row-drop-item:hover {
    background: var(--control-hover);
  }

  .row-drop-item.active {
    background: var(--accent-soft);
    color: var(--ink);
    font-weight: 500;
  }

  .mapping-exe-pill {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
    overflow: hidden;
    pointer-events: none;
  }

  .mapping-delete-btn {
    background: none;
    border: none;
    padding: 3px;
    border-radius: 4px;
    color: var(--ink-mute);
    cursor: pointer;
    display: flex;
    align-items: center;
    flex-shrink: 0;
  }

  .mapping-delete-btn:hover {
    color: var(--ink-strong);
    background: var(--control-hover);
  }

  .mapping-empty {
    font-size: 12px;
    color: var(--ink-mute);
    padding: 8px 0 20px;
    font-style: italic;
  }
</style>
