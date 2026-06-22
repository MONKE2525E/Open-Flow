<script lang="ts">
  import { animateWidth, MOTION_MS, MOTION_PX, motionMs, motionPx } from '../../motion';
  import { fly, fade } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import {
    cleanAppName,
    cleanupIntensityOptions,
    getCleanupIntensityLabel,
    getProfileLabel,
    profileOptions,
    type AppMapping,
    type InstalledApp,
  } from '../../appMappings';
  import { customExeFromSearch, matchesAppSearch } from './helpers';
  import { focusListboxOption, handleListboxOptionKeydown } from './listbox';

  let {
    installedApps,
    areAppsLoaded,
    mappedExes,
    addLabel,
    onAdd,
  }: {
    installedApps: InstalledApp[];
    areAppsLoaded: boolean;
    mappedExes: Set<string>;
    addLabel: string;
    onAdd: (mapping: AppMapping) => boolean | void | Promise<boolean | void>;
  } = $props();

  const APP_PICKER_MENU_ID = 'app-mappings-app-picker-menu';
  const ADD_PROFILE_MENU_ID = 'app-mappings-add-profile-menu';
  const ADD_CLEANUP_MENU_ID = 'app-mappings-add-cleanup-menu';

  const cleanupIntensityChoices = [
    { id: '', label: 'Default' },
    ...cleanupIntensityOptions,
  ] as const;

  let addExe = $state('');
  let addName = $state('');
  let addProfile = $state('casual');
  let addCleanupIntensity = $state('');
  let appSearch = $state('');
  let appPickerOpen = $state(false);
  let profileDropdownOpen = $state(false);
  let cleanupDropdownOpen = $state(false);
  let profileDropdownButton = $state<HTMLButtonElement | null>(null);
  let cleanupDropdownButton = $state<HTMLButtonElement | null>(null);

  const pendingExe = $derived(addExe || (appSearch.trim() ? customExeFromSearch(appSearch) : ''));
  const pendingName = $derived(cleanAppName(addName || appSearch || pendingExe));
  const filteredApps = $derived(
    installedApps
      .filter((app) => !mappedExes.has(app.exe))
      .filter((app) => matchesAppSearch(app, appSearch))
      .slice(0, 40),
  );

  function resetForm() {
    addExe = '';
    addName = '';
    addProfile = 'casual';
    addCleanupIntensity = '';
    appSearch = '';
    appPickerOpen = false;
    profileDropdownOpen = false;
    cleanupDropdownOpen = false;
  }

  function pickApp(app: InstalledApp) {
    addExe = app.exe;
    addName = cleanAppName(app.name || app.exe);
    appSearch = addName;
    appPickerOpen = false;
  }

  async function openProfileDropdown(preferLast = false) {
    profileDropdownOpen = true;
    await focusListboxOption(ADD_PROFILE_MENU_ID, preferLast);
  }

  async function openCleanupDropdown(preferLast = false) {
    cleanupDropdownOpen = true;
    await focusListboxOption(ADD_CLEANUP_MENU_ID, preferLast);
  }

  async function submit() {
    const exe = pendingExe;
    if (!exe) return;

    const saved = await onAdd({
      exe,
      profile: addProfile,
      name: addName || appSearch || exe,
      cleanup_intensity: addCleanupIntensity || undefined,
    });

    if (saved !== false) {
      resetForm();
    }
  }

  function closeAppPicker(event: MouseEvent | PointerEvent) {
    const target = event.target;
    if (target instanceof Element && !target.closest('.app-picker-wrap')) {
      appPickerOpen = false;
    }
  }

  function closeProfileDropdown(event: MouseEvent | PointerEvent) {
    const target = event.target;
    if (target instanceof Element && !target.closest('.profile-drop-wrap')) {
      profileDropdownOpen = false;
    }
  }

  function closeCleanupDropdown(event: MouseEvent | PointerEvent) {
    const target = event.target;
    if (target instanceof Element && !target.closest('.cleanup-drop-wrap')) {
      cleanupDropdownOpen = false;
    }
  }

  function handleProfileButtonKeydown(event: KeyboardEvent) {
    if ((event.key === 'Enter' || event.key === ' ') && !profileDropdownOpen) {
      event.preventDefault();
      void openProfileDropdown();
      return;
    }
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      void openProfileDropdown();
      return;
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault();
      void openProfileDropdown(true);
      return;
    }
    if (event.key === 'Escape' && profileDropdownOpen) {
      profileDropdownOpen = false;
      profileDropdownButton?.focus();
      event.stopPropagation();
    }
  }

  function handleCleanupButtonKeydown(event: KeyboardEvent) {
    if ((event.key === 'Enter' || event.key === ' ') && !cleanupDropdownOpen) {
      event.preventDefault();
      void openCleanupDropdown();
      return;
    }
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      void openCleanupDropdown();
      return;
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault();
      void openCleanupDropdown(true);
      return;
    }
    if (event.key === 'Escape' && cleanupDropdownOpen) {
      cleanupDropdownOpen = false;
      cleanupDropdownButton?.focus();
      event.stopPropagation();
    }
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

  $effect(() => {
    if (!cleanupDropdownOpen) return;

    const timeout = window.setTimeout(() => {
      window.addEventListener('pointerdown', closeCleanupDropdown);
    });

    return () => {
      window.clearTimeout(timeout);
      window.removeEventListener('pointerdown', closeCleanupDropdown);
    };
  });
</script>

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
        aria-expanded={appPickerOpen}
        aria-controls={APP_PICKER_MENU_ID}
        onkeydown={(event) => {
          if (event.key === 'Enter') {
            submit();
          } else if (event.key === 'Escape' && appPickerOpen) {
            appPickerOpen = false;
            event.stopPropagation();
          }
        }}
      />
      {#if appPickerOpen && filteredApps.length > 0}
        <div
          id={APP_PICKER_MENU_ID}
          class="app-picker-menu scroll-styled"
          role="listbox"
          tabindex="-1"
          aria-label="Installed apps"
          onpointerdown={(event) => event.stopPropagation()}
          in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.fast), easing: expoOut }}
          out:fade={{ duration: motionMs(100) }}
        >
          {#each filteredApps as app}
            <button
              type="button"
              class="app-picker-item"
              onclick={() => pickApp(app)}
              role="option"
              aria-selected={false}
            >
              <span class="app-picker-name">{cleanAppName(app.name || app.exe)}</span>
              <span class="app-picker-exe-pill" aria-hidden="true">{app.exe}</span>
            </button>
          {/each}
        </div>
      {:else if appPickerOpen && appSearch.trim()}
        <div
          class="app-picker-menu app-picker-empty"
          role="presentation"
          in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.fast), easing: expoOut }}
          out:fade={{ duration: motionMs(100) }}
        >
          <span>
            No matching apps found. Press Enter to map custom executable:
            <b>{customExeFromSearch(appSearch)}</b>
          </span>
        </div>
      {/if}
    </div>

    <div class="profile-drop-wrap" role="presentation" onclick={(event) => event.stopPropagation()}>
      <select class="profile-select profile-select-hidden" bind:value={addProfile} tabindex="-1" aria-hidden="true">
        {#each profileOptions as profile}
          <option value={profile.id}>{profile.label}</option>
        {/each}
      </select>
      <button
        bind:this={profileDropdownButton}
        type="button"
        class="profile-drop-btn"
        use:animateWidth={{ text: getProfileLabel(addProfile) }}
        onclick={() => (profileDropdownOpen ? profileDropdownOpen = false : openProfileDropdown())}
        onkeydown={handleProfileButtonKeydown}
        aria-haspopup="listbox"
        aria-expanded={profileDropdownOpen}
        aria-controls={ADD_PROFILE_MENU_ID}
      >
        <span>{getProfileLabel(addProfile)}</span>
        <svg class:open={profileDropdownOpen} width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="m6 9 6 6 6-6"/>
        </svg>
      </button>
      {#if profileDropdownOpen}
        <div
          id={ADD_PROFILE_MENU_ID}
          class="profile-drop-menu scroll-styled"
          role="listbox"
          tabindex="-1"
          aria-label="Tone options"
          onpointerdown={(event) => event.stopPropagation()}
          in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.fast), easing: expoOut }}
          out:fade={{ duration: motionMs(100) }}
        >
          {#each profileOptions as profile}
            <button
              type="button"
              class="profile-drop-item"
              class:active={addProfile === profile.id}
              onclick={() => {
                addProfile = profile.id;
                profileDropdownOpen = false;
                profileDropdownButton?.focus();
              }}
              onkeydown={(event) =>
                handleListboxOptionKeydown(event, ADD_PROFILE_MENU_ID, () => {
                  profileDropdownOpen = false;
                  profileDropdownButton?.focus();
                })}
              role="option"
              aria-selected={addProfile === profile.id}
              tabindex="-1"
            >
              {profile.label}
            </button>
          {/each}
        </div>
      {/if}
    </div>

    <div class="cleanup-drop-wrap" role="presentation" onclick={(event) => event.stopPropagation()}>
      <select class="cleanup-select cleanup-select-hidden" bind:value={addCleanupIntensity} tabindex="-1" aria-hidden="true">
        {#each cleanupIntensityChoices as choice}
          <option value={choice.id}>{choice.label}</option>
        {/each}
      </select>
      <button
        bind:this={cleanupDropdownButton}
        type="button"
        class="cleanup-drop-btn"
        use:animateWidth={{ text: getCleanupIntensityLabel(addCleanupIntensity) }}
        onclick={() => (cleanupDropdownOpen ? cleanupDropdownOpen = false : openCleanupDropdown())}
        onkeydown={handleCleanupButtonKeydown}
        aria-haspopup="listbox"
        aria-expanded={cleanupDropdownOpen}
        aria-controls={ADD_CLEANUP_MENU_ID}
      >
        <span>{getCleanupIntensityLabel(addCleanupIntensity)}</span>
        <svg class:open={cleanupDropdownOpen} width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="m6 9 6 6 6-6"/>
        </svg>
      </button>
      {#if cleanupDropdownOpen}
        <div
          id={ADD_CLEANUP_MENU_ID}
          class="cleanup-drop-menu scroll-styled"
          role="listbox"
          tabindex="-1"
          aria-label="Cleanup intensity options"
          onpointerdown={(event) => event.stopPropagation()}
          in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.fast), easing: expoOut }}
          out:fade={{ duration: motionMs(100) }}
        >
          {#each cleanupIntensityChoices as choice}
            <button
              type="button"
              class="cleanup-drop-item"
              class:active={addCleanupIntensity === choice.id}
              onclick={() => {
                addCleanupIntensity = choice.id;
                cleanupDropdownOpen = false;
                cleanupDropdownButton?.focus();
              }}
              onkeydown={(event) =>
                handleListboxOptionKeydown(event, ADD_CLEANUP_MENU_ID, () => {
                  cleanupDropdownOpen = false;
                  cleanupDropdownButton?.focus();
                })}
              role="option"
              aria-selected={addCleanupIntensity === choice.id}
              tabindex="-1"
            >
              {choice.label}
            </button>
          {/each}
        </div>
      {/if}
    </div>

    <button type="button" class="btn-primary add-btn" onclick={submit} disabled={!addExe && !appSearch.trim()}>Add</button>
  </div>

  {#if pendingExe}
    <div class="add-preview">
      <span>{pendingName}</span>
      <span class="preview-dot"></span>
      <span>{getProfileLabel(addProfile)}</span>
      <span class="preview-dot"></span>
      <span>{getCleanupIntensityLabel(addCleanupIntensity)} cleanup</span>
    </div>
  {/if}
</div>

<style>
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

  .app-search-input:focus {
    border-color: var(--accent);
  }

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

  .app-picker-empty {
    padding: 10px 12px;
    font-size: 12px;
    color: var(--ink-mute);
    line-height: 1.5;
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

  .app-picker-item:last-child {
    border-bottom: none;
  }

  .app-picker-item:hover {
    background: var(--control-hover);
  }

  .app-picker-name {
    display: block;
    font-size: 12.5px;
    color: var(--ink-strong);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .app-picker-exe-pill {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
    overflow: hidden;
    pointer-events: none;
  }

  .profile-drop-wrap,
  .cleanup-drop-wrap {
    position: relative;
    flex-shrink: 0;
  }

  .profile-select-hidden,
  .cleanup-select-hidden {
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

  .profile-drop-btn,
  .cleanup-drop-btn {
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

  .profile-drop-btn:hover,
  .cleanup-drop-btn:hover {
    background: var(--control-hover);
  }

  .profile-drop-btn svg,
  .cleanup-drop-btn svg {
    transition: transform 150ms;
  }

  .profile-drop-btn svg.open,
  .cleanup-drop-btn svg.open {
    transform: rotate(180deg);
  }

  .profile-drop-btn span,
  .cleanup-drop-btn span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 140px;
  }

  .profile-drop-menu,
  .cleanup-drop-menu {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    box-shadow: var(--shadow-popover);
    max-height: 200px;
    overflow-y: auto;
    z-index: 20;
  }

  .profile-drop-menu {
    min-width: 130px;
  }

  .cleanup-drop-menu {
    min-width: 120px;
  }

  .profile-drop-item,
  .cleanup-drop-item {
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

  .profile-drop-item:last-child,
  .cleanup-drop-item:last-child {
    border-bottom: none;
  }

  .profile-drop-item:hover,
  .cleanup-drop-item:hover {
    background: var(--control-hover);
  }

  .profile-drop-item.active,
  .cleanup-drop-item.active {
    background: var(--accent-soft);
    color: var(--ink);
    font-weight: 500;
  }

  .btn-primary {
    background: var(--ink);
    color: var(--amber-50);
    border: 1px solid transparent;
    border-radius: 6px;
    padding: 6px 12px;
    font-size: 12px;
    font-family: var(--sans);
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
    transition: opacity 0.15s;
  }

  .btn-primary:disabled { opacity: 0.4; cursor: default; }
  .btn-primary:not(:disabled):hover { opacity: 0.82; }

  .add-btn {
    flex-shrink: 0;
  }

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
