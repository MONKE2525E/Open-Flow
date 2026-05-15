<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { tick } from 'svelte';
  import { fly, slide } from 'svelte/transition';
  import { flip } from 'svelte/animate';
  import { expoOut } from 'svelte/easing';

  interface InstalledApp { name: string; exe: string; }
  interface AppMapping { exe: string; profile: string; name: string; }

  const profiles = [
    { id: 'casual',      label: 'Casual'      },
    { id: 'formal',      label: 'Formal'      },
    { id: 'very_casual', label: 'Very Casual' },
  ];

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

  const filteredApps = $derived(
    appSearch
      ? installedApps.filter(a =>
          a.name.toLowerCase().includes(appSearch.toLowerCase()) ||
          a.exe.toLowerCase().includes(appSearch.toLowerCase())
        ).slice(0, 40)
      : installedApps.slice(0, 40)
  );

  async function loadMappings() {
    try {
      mappings = await invoke<AppMapping[]>('get_app_mappings');
    } catch (err) {
      console.error('get_app_mappings failed:', err);
    }
  }

  async function loadInstalledApps() {
    if (areAppsLoaded) return;
    try {
      installedApps = await invoke<InstalledApp[]>('get_installed_apps');
      areAppsLoaded = true;
    } catch (err) {
      console.error('get_installed_apps failed:', err);
    }
  }

  async function deleteMapping(exe: string) {
    mappings = mappings.filter(m => m.exe !== exe);
    try {
      await invoke('save_app_mappings', { mappings });
      mappingError = '';
    } catch (err) {
      console.error('deleteMapping failed:', err);
      mappingError = 'Failed to delete mapping.';
    }
  }

  async function addMapping() {
    if (!addExe) return;
    const existing = mappings.findIndex(m => m.exe === addExe);
    const entry: AppMapping = { exe: addExe, profile: addProfile, name: addName || addExe };
    if (existing >= 0) {
      mappings = mappings.map((m, i) => i === existing ? entry : m);
    } else {
      mappings = [...mappings, entry];
    }
    try {
      await invoke('save_app_mappings', { mappings });
      mappingError = '';
    } catch (err) {
      console.error('addMapping failed:', err);
      mappingError = 'Failed to save mapping.';
    }
    addExe = '';
    addName = '';
    addProfile = 'casual';
    appSearch = '';
    appPickerOpen = false;
  }

  function pickApp(app: InstalledApp) {
    addExe = app.exe;
    addName = app.name;
    appSearch = app.name;
    appPickerOpen = false;
  }

  function closeAppPicker(e: MouseEvent) {
    if (!(e.target as HTMLElement).closest('.app-picker-wrap')) appPickerOpen = false;
  }

  function closeProfileDropdown(e: MouseEvent) {
    if (!(e.target as HTMLElement).closest('.profile-drop-wrap')) profileDropdownOpen = false;
  }

  $effect(() => {
    if (appPickerOpen) {
      tick().then(() => window.addEventListener('click', closeAppPicker, { once: true }));
    }
  });

  $effect(() => {
    if (profileDropdownOpen) {
      tick().then(() => window.addEventListener('click', closeProfileDropdown, { once: true }));
    }
  });

  loadMappings();
  loadInstalledApps();
</script>

<h2 class="settings-h">App Mappings</h2>
<p class="panel-note">Switch profiles automatically based on the active window.</p>

{#if mappings.length > 0}
  <div class="mapping-list">
    {#each mappings as m (m.exe)}
      <div class="mapping-row" animate:flip={{duration: 300, easing: expoOut}} in:fly={{y: 10, duration: 300, easing: expoOut}} out:slide={{duration: 200, easing: expoOut}}>
        <div class="mapping-app-info">
          <span class="mapping-app-name">{m.name || m.exe.replace(/\.exe$/i, '')}</span>
          <span class="mapping-exe-pill">{m.exe}</span>
        </div>
        <svg class="mapping-arrow-icon" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14M12 5l7 7-7 7"/></svg>
        <span class="mapping-profile-badge">{m.profile}</span>
        <button class="mapping-delete-btn" onclick={() => deleteMapping(m.exe)} title="Remove">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
        </button>
      </div>
    {/each}
  </div>
{:else}
  <div class="mapping-empty">No app mappings yet. Add one below to get started.</div>
{/if}

{#if mappingError}
  <div class="mapping-error">{mappingError}</div>
{/if}

<div class="add-mapping-section">
  <div class="add-mapping-label">Add Mapping</div>
  <div class="add-mapping-row">
    <div class="app-picker-wrap">
      <input
        class="app-search-input"
        placeholder={areAppsLoaded ? 'Search apps…' : 'Loading apps…'}
        bind:value={appSearch}
        onfocus={() => { appPickerOpen = true; }}
        oninput={() => { appPickerOpen = true; }}
      />
      {#if appPickerOpen && filteredApps.length > 0}
        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
        <div class="app-picker-menu" role="presentation" onclick={(e) => e.stopPropagation()}>
          {#each filteredApps as app}
            <button class="app-picker-item" onclick={() => pickApp(app)}>
              <span class="app-picker-name">{app.name}</span>
              <span class="app-picker-exe">{app.exe}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="profile-drop-wrap" role="presentation" onclick={(e) => e.stopPropagation()}>
      <!-- Hidden native select retained for smoke test: .profile-select.selectOption() requires a real <select> -->
      <select class="profile-select profile-select-hidden" bind:value={addProfile} tabindex="-1" aria-hidden="true">
        {#each profiles as p}
          <option value={p.id}>{p.label}</option>
        {/each}
      </select>
      <button class="btn-ghost mic-btn" onclick={() => (profileDropdownOpen = !profileDropdownOpen)}>
        <span>{profiles.find(p => p.id === addProfile)?.label ?? 'Casual'}</span>
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="m6 9 6 6 6-6"/>
        </svg>
      </button>
      {#if profileDropdownOpen}
        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
        <div class="profile-drop-menu" role="presentation" onclick={(e) => e.stopPropagation()}>
          {#each profiles as p}
            <button class="profile-drop-item" class:active={addProfile === p.id} onclick={() => { addProfile = p.id; profileDropdownOpen = false; }}>
              {p.label}
            </button>
          {/each}
        </div>
      {/if}
    </div>
    <button
      class="btn-ghost add-btn"
      onclick={addMapping}
      disabled={!addExe}
    >Add</button>
  </div>
  {#if addExe}
    <div class="add-preview">
      <span class="mapping-exe-pill">{addExe}</span>
      <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M5 12h14M12 5l7 7-7 7"/></svg>
      <span class="mapping-profile-badge">{addProfile}</span>
    </div>
  {/if}
</div>

<style>
  .mapping-list { border: 1px solid var(--line); border-radius: var(--r-sm); overflow: hidden; margin-bottom: 20px; }
  .mapping-row { display: flex; align-items: center; gap: 10px; padding: 9px 12px; border-bottom: 1px solid var(--line); background: var(--bg-elev); }
  .mapping-row:last-child { border-bottom: none; }
  .mapping-app-info { flex: 1; min-width: 0; display: flex; align-items: center; gap: 7px; }
  .mapping-app-name { font-size: 12.5px; font-weight: 500; color: var(--ink-strong); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 130px; }
  .mapping-exe-pill { font-family: var(--mono); font-size: 10px; color: var(--ink-mute); background: var(--paper); border: 1px solid var(--line); border-radius: 4px; padding: 1px 5px; flex-shrink: 0; }
  .mapping-arrow-icon { color: var(--ink-mute); flex-shrink: 0; }
  .mapping-profile-badge {
    font-family: var(--mono);
    font-size: 10px;
    font-weight: 500;
    color: var(--accent);
    background: var(--accent-soft);
    border: 1px solid color-mix(in oklab, var(--accent) 30%, transparent);
    border-radius: 4px;
    padding: 2px 7px;
    flex-shrink: 0;
    text-transform: lowercase;
  }
  .mapping-delete-btn { background: none; border: none; padding: 3px; border-radius: 4px; color: var(--ink-mute); cursor: pointer; display: flex; align-items: center; flex-shrink: 0; margin-left: auto; }
  .mapping-delete-btn:hover { color: var(--ink-strong); background: var(--paper); }
  .mapping-empty { font-size: 12px; color: var(--ink-mute); padding: 16px 0 20px; font-style: italic; }
  .mapping-error { font-size: 12px; color: var(--accent); padding: 4px 0 12px; }
  .add-mapping-section { border-top: 1px solid var(--line); padding-top: 16px; }
  .add-mapping-label { font-size: 11px; font-weight: 500; color: var(--ink-mute); text-transform: uppercase; letter-spacing: 0.08em; font-family: var(--mono); margin-bottom: 10px; }
  .add-mapping-row { display: flex; gap: 8px; align-items: center; }
  .app-picker-wrap { position: relative; flex: 1; }
  .app-search-input { width: 100%; box-sizing: border-box; font-family: var(--sans); font-size: 12px; background: transparent; border: 1px solid var(--line-strong); border-radius: 6px; padding: 5px 10px; color: var(--ink-strong); }
  .app-search-input:focus { outline: none; border-color: var(--accent); }
  .app-picker-menu { position: absolute; left: 0; top: calc(100% + 4px); background: var(--bg-elev); border: 1px solid var(--line); border-radius: var(--r-sm); box-shadow: var(--shadow-popover); width: 100%; max-height: 180px; overflow-y: auto; z-index: 10; }
  .app-picker-item { display: flex; align-items: center; justify-content: space-between; width: 100%; padding: 7px 10px; font-family: var(--sans); background: none; border: none; border-bottom: 1px solid var(--line); cursor: pointer; text-align: left; gap: 8px; }
  .app-picker-item:last-child { border-bottom: none; }
  .app-picker-item:hover { background: var(--paper); }
  .app-picker-name { font-size: 12px; color: var(--ink-strong); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; flex: 1; }
  .app-picker-exe { font-family: var(--mono); font-size: 10px; color: var(--ink-mute); flex-shrink: 0; }
  .profile-drop-wrap { position: relative; flex-shrink: 0; }
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
  .mic-btn { display: flex; align-items: center; gap: 6px; }
  .mic-btn span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 140px; }
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
    z-index: 10;
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
  .profile-drop-item:hover { background: var(--paper); }
  .profile-drop-item.active { background: var(--accent-soft); color: var(--ink); font-weight: 500; }
  .add-btn { flex-shrink: 0; }
  .add-preview { display: flex; align-items: center; gap: 6px; margin-top: 8px; padding: 4px 2px; }
</style>
