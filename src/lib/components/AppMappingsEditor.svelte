<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { fly, slide, fade } from 'svelte/transition';
  import { flip } from 'svelte/animate';
  import { expoOut } from 'svelte/easing';
  import {
    cleanAppName,
    getAppDisplayName,
    getProfileLabel,
    normalizeExe,
    profileOptions,
    type AppMapping,
    type InstalledApp,
  } from '../appMappings';
  import { animateWidth, MOTION_MS, MOTION_PX, motionMs, motionPx } from '../motion';

  let {
    showHeading = true,
    intro = 'Use a different tone automatically when you type in specific apps.',
    emptyText = 'No app tones yet. Add one below.',
    addLabel = 'Add App Tone',
  }: {
    showHeading?: boolean;
    intro?: string;
    emptyText?: string;
    addLabel?: string;
  } = $props();

  let mappings = $state<AppMapping[]>([]);
  let installedApps = $state<InstalledApp[]>([]);
  let areAppsLoaded = $state(false);
  let addExe = $state('');
  let addName = $state('');
  let addProfile = $state('casual');
  let appSearch = $state('');
  let appPickerOpen = $state(false);
  let profileDropdownOpen = $state(false);
  let mappingError = $state('');

  const pendingExe = $derived(addExe || (appSearch.trim() ? customExeFromSearch(appSearch) : ''));
  const pendingName = $derived(cleanAppName(addName || appSearch || pendingExe));
  const mappedExes = $derived(new Set(mappings.map((mapping) => normalizeExe(mapping.exe))));
  const filteredApps = $derived(
    installedApps
      .filter((app) => !mappedExes.has(normalizeExe(app.exe)))
      .filter((app) => matchesAppSearch(app, appSearch))
      .slice(0, 40),
  );

  onMount(() => {
    loadMappings();
    loadInstalledApps();
  });

  async function loadMappings() {
    try {
      mappings = normalizeMappings(await invoke<AppMapping[]>('get_app_mappings'));
    } catch (err) {
      console.error('get_app_mappings failed:', err);
    }
  }

  async function loadInstalledApps() {
    if (areAppsLoaded) return;
    try {
      installedApps = (await invoke<InstalledApp[]>('get_installed_apps')).map((app) => ({
        exe: normalizeExe(app.exe),
        name: cleanAppName(app.name || app.exe),
      }));
      areAppsLoaded = true;
      mappings = normalizeMappings(mappings);
    } catch (err) {
      console.error('get_installed_apps failed:', err);
    }
  }

  async function saveMappings(updated: AppMapping[]) {
    mappings = normalizeMappings(updated);
    try {
      await invoke('save_app_mappings', { mappings });
      mappingError = '';
    } catch (err) {
      console.error('save_app_mappings failed:', err);
      mappingError = 'Could not save app tones.';
    }
  }

  async function deleteMapping(exe: string) {
    await saveMappings(mappings.filter((mapping) => normalizeExe(mapping.exe) !== normalizeExe(exe)));
  }

  function customExeFromSearch(s: string): string {
    return normalizeExe(s).replace(/\.exe$/, '') + '.exe';
  }

  async function addMapping() {
    const rawExe = pendingExe;
    if (!rawExe) return;
    const entry = normalizeMapping({
      exe: rawExe,
      profile: addProfile,
      name: addName || appSearch || rawExe,
    });
    await saveMappings([...mappings.filter((mapping) => mapping.exe !== entry.exe), entry]);
    addExe = '';
    addName = '';
    addProfile = 'casual';
    appSearch = '';
    appPickerOpen = false;
  }

  function pickApp(app: InstalledApp) {
    addExe = normalizeExe(app.exe);
    addName = cleanAppName(app.name || app.exe);
    appSearch = addName;
    appPickerOpen = false;
  }

  function closeAppPicker(e: MouseEvent | PointerEvent) {
    if (!(e.target as HTMLElement).closest('.app-picker-wrap')) appPickerOpen = false;
  }

  function closeProfileDropdown(e: MouseEvent | PointerEvent) {
    if (!(e.target as HTMLElement).closest('.profile-drop-wrap')) profileDropdownOpen = false;
  }

  function handleProfileButtonKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && profileDropdownOpen) {
      profileDropdownOpen = false;
      e.stopPropagation();
    }
  }

  function normalizeMappings(entries: AppMapping[]) {
    const seen = new Set<string>();
    return entries
      .map(normalizeMapping)
      .filter((entry) => {
        if (!entry.exe || seen.has(entry.exe)) return false;
        seen.add(entry.exe);
        return true;
      });
  }

  function normalizeMapping(entry: AppMapping): AppMapping {
    const exe = normalizeExe(entry.exe);
    return {
      exe,
      profile: entry.profile || 'casual',
      name: getAppDisplayName({ exe, name: entry.name }, installedApps),
    };
  }

  function matchesAppSearch(app: InstalledApp, search: string) {
    const query = search.trim().toLowerCase();
    if (!query) return true;

    const appName = cleanAppName(app.name || app.exe).toLowerCase();
    const appExe = normalizeExe(app.exe);
    const compactQuery = query.replace(/[^a-z0-9]/g, '');
    const compactName = appName.replace(/[^a-z0-9]/g, '');
    const compactExe = appExe.replace(/[^a-z0-9]/g, '');

    return appName.includes(query)
      || appExe.includes(query)
      || (compactQuery.length > 0 && (compactName.includes(compactQuery) || compactExe.includes(compactQuery)));
  }

  $effect(() => {
    if (!appPickerOpen) return;

    const timeout = window.setTimeout(() => {
      window.addEventListener('pointerdown', closeAppPicker);
    });

    return () => {
      window.clearTimeout(timeout);
      window.removeEventListener('pointerdown', closeAppPicker);
    };
  });

  $effect(() => {
    if (!profileDropdownOpen) return;

    const timeout = window.setTimeout(() => {
      window.addEventListener('pointerdown', closeProfileDropdown);
    });

    return () => {
      window.clearTimeout(timeout);
      window.removeEventListener('pointerdown', closeProfileDropdown);
    };
  });
</script>

{#if showHeading}
  <h2 class="settings-h">App Mappings</h2>
{/if}
<p class="panel-note">{intro}</p>

{#if mappings.length > 0}
  <div class="mapping-list">
    {#each mappings as mapping (mapping.exe)}
      <div
        class="mapping-row"
        animate:flip={{ duration: motionMs(300), easing: expoOut }}
        in:fly={{ y: motionPx(10), duration: motionMs(300), easing: expoOut }}
        out:slide={{ duration: motionMs(200), easing: expoOut }}
      >
        <div class="mapping-app-info">
          <span class="mapping-app-name">{getAppDisplayName(mapping, installedApps)}</span>
          <span class="mapping-exe-pill" aria-hidden="true">{mapping.exe}</span>
        </div>
        <span class="mapping-profile-badge">{getProfileLabel(mapping.profile)}</span>
        <button class="mapping-delete-btn" onclick={() => deleteMapping(mapping.exe)} title="Remove {getAppDisplayName(mapping, installedApps)}">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
            <path d="M18 6 6 18M6 6l12 12"/>
          </svg>
        </button>
      </div>
    {/each}
  </div>
{:else}
  <div class="mapping-empty">{emptyText}</div>
{/if}

{#if mappingError}
  <div class="mapping-error">{mappingError}</div>
{/if}

<div class="add-mapping-section">
  <div class="add-mapping-label">{addLabel}</div>
  <div class="add-mapping-row">
    <div class="app-picker-wrap">
      <input
        class="app-search-input"
        placeholder={areAppsLoaded ? 'Search apps...' : 'Loading apps...'}
        bind:value={appSearch}
        onfocus={() => { appPickerOpen = true; }}
        oninput={() => { addExe = ''; addName = ''; appPickerOpen = true; }}
        onkeydown={(e) => {
          if (e.key === 'Enter') {
            addMapping();
          } else if (e.key === 'Escape' && appPickerOpen) {
            appPickerOpen = false;
            e.stopPropagation();
          }
        }}
      />
      {#if appPickerOpen && filteredApps.length > 0}
        <div
          class="app-picker-menu scroll-styled"
          role="presentation"
          onclick={(e) => e.stopPropagation()}
          in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.fast), easing: expoOut }}
          out:fade={{ duration: motionMs(100) }}
        >
          {#each filteredApps as app}
            <button class="app-picker-item" onclick={() => pickApp(app)}>
              <span class="app-picker-name">{cleanAppName(app.name || app.exe)}</span>
              <span class="mapping-exe-pill" aria-hidden="true">{app.exe}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
    <div
      class="profile-drop-wrap"
      role="presentation"
      onclick={(e) => e.stopPropagation()}
    >
      <select class="profile-select profile-select-hidden" bind:value={addProfile} tabindex="-1" aria-hidden="true">
        {#each profileOptions as profile}
          <option value={profile.id}>{profile.label}</option>
        {/each}
      </select>
      <button
        class="profile-drop-btn"
        use:animateWidth={{ text: getProfileLabel(addProfile) }}
        onclick={() => (profileDropdownOpen = !profileDropdownOpen)}
        onkeydown={handleProfileButtonKeydown}
      >
        <span>{getProfileLabel(addProfile)}</span>
        <svg class:open={profileDropdownOpen} width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="m6 9 6 6 6-6"/>
        </svg>
      </button>
      {#if profileDropdownOpen}
        <div
          class="profile-drop-menu scroll-styled"
          role="presentation"
          onclick={(e) => e.stopPropagation()}
          in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.fast), easing: expoOut }}
          out:fade={{ duration: motionMs(100) }}
        >
          {#each profileOptions as profile}
            <button
              class="profile-drop-item"
              class:active={addProfile === profile.id}
              onclick={() => { addProfile = profile.id; profileDropdownOpen = false; }}
              onkeydown={handleProfileButtonKeydown}
            >
              {profile.label}
            </button>
          {/each}
        </div>
      {/if}
    </div>
    <button class="btn-ghost add-btn" onclick={addMapping} disabled={!addExe && !appSearch.trim()}>Add</button>
  </div>
  {#if pendingExe}
    <div class="add-preview">
      <span>{pendingName}</span>
      <span class="preview-dot"></span>
      <span>{getProfileLabel(addProfile)}</span>
    </div>
  {/if}
</div>

<style>
  .panel-note {
    font-size: 12px;
    color: var(--ink-mute);
    margin: 0 0 16px;
    line-height: 1.5;
  }

  .btn-ghost {
    background: transparent;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    padding: 6px 12px;
    font-size: 12px;
    color: var(--ink-strong);
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
  }

  .btn-ghost:hover { background: var(--control-hover); }
  .btn-ghost:disabled { opacity: 0.4; cursor: default; }

  .mapping-list {
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    overflow: hidden;
    margin-bottom: 20px;
    max-width: 640px;
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

  .mapping-profile-badge {
    font-size: 12px;
    font-weight: 500;
    color: var(--accent-ink);
    background: var(--accent-soft);
    border: 1px solid color-mix(in oklab, var(--accent) 28%, transparent);
    border-radius: 4px;
    padding: 2px 8px;
    flex-shrink: 0;
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

  .mapping-error {
    font-size: 12px;
    color: var(--accent);
    padding: 4px 0 12px;
  }

  .add-mapping-section {
    border-top: 1px solid var(--line);
    padding-top: 16px;
    max-width: 640px;
  }

  .add-mapping-label {
    font-size: 11px;
    font-weight: 500;
    color: var(--ink-mute);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-family: var(--mono);
    margin-bottom: 10px;
  }

  .add-mapping-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }

  .app-picker-wrap {
    position: relative;
    flex: 1 1 260px;
    min-width: 0;
  }

  .app-search-input {
    width: 100%;
    box-sizing: border-box;
    font-family: var(--sans);
    font-size: 12.5px;
    background: transparent;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    padding: 7px 10px;
    color: var(--ink-strong);
    outline: none;
  }

  .app-search-input:focus { border-color: var(--accent); }

  .app-picker-menu {
    position: absolute;
    left: 0;
    top: calc(100% + 4px);
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    box-shadow: var(--shadow-popover);
    width: 100%;
    max-height: 180px;
    overflow-y: auto;
    z-index: 20;
  }

  .app-picker-item {
    display: block;
    width: 100%;
    padding: 8px 10px;
    font-family: var(--sans);
    background: none;
    border: none;
    border-bottom: 1px solid var(--line);
    cursor: pointer;
    text-align: left;
  }

  .app-picker-item:last-child { border-bottom: none; }
  .app-picker-item:hover { background: var(--control-hover); }

  .app-picker-name {
    display: block;
    font-size: 12.5px;
    color: var(--ink-strong);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .profile-drop-wrap {
    position: relative;
    flex-shrink: 0;
  }

  .profile-select-hidden {
    position: absolute;
    top: 0;
    left: 0;
    width: 1px;
    height: 1px;
    clip-path: inset(50%);
    border: 0;
    padding: 0;
    margin: 0;
    overflow: hidden;
    pointer-events: none;
  }

  .profile-drop-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    background: transparent;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    padding: 6px 12px;
    font-size: 12px;
    font-family: var(--sans);
    color: var(--ink-strong);
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
  }

  .profile-drop-btn:hover { background: var(--control-hover); }
  .profile-drop-btn svg { transition: transform 150ms; }
  .profile-drop-btn svg.open { transform: rotate(180deg); }

  .profile-drop-btn span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 140px;
  }

  .profile-drop-menu {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    box-shadow: var(--shadow-popover);
    min-width: 130px;
    max-height: 200px;
    overflow-y: auto;
    z-index: 20;
  }

  .profile-drop-item {
    display: block;
    width: 100%;
    text-align: left;
    padding: 8px 12px;
    font-size: 12px;
    font-family: var(--sans);
    color: var(--ink-strong);
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--line);
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .profile-drop-item:last-child { border-bottom: none; }
  .profile-drop-item:hover { background: var(--control-hover); }
  .profile-drop-item.active { background: var(--accent-soft); color: var(--ink); font-weight: 500; }

  .add-btn { flex-shrink: 0; }

  .add-preview {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-top: 8px;
    padding: 2px 1px;
    font-size: 12px;
    color: var(--ink-mute);
  }

  .preview-dot {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--ink-faint);
    flex-shrink: 0;
  }

  @media (max-width: 720px) {
    .app-picker-wrap {
      flex-basis: 100%;
    }
  }
</style>
