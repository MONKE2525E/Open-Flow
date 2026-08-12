<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '../../tauri';
  import { appStore } from '../../stores';
  import { icons } from '../../icons';
  import { isMac, isWindows } from '../../platform';
  import { MOTION_MS, SETTINGS_SECTION_ORDER, directionFromOrder, motionMs, motionPx } from '../../motion';
  import { visibleSettingsSections, type SettingsSectionId } from '../../settingsSections';
  import Brand from './Brand.svelte';
  import LocalDownloadProgress from '../settings/LocalDownloadProgress.svelte';
  import {
    getActiveDownloads,
    downloadUi,
    cancelDownload,
    acknowledgeDownloads,
  } from '../../downloadManager.svelte';
  import { tweened } from 'svelte/motion';
  import { cubicOut, expoOut } from 'svelte/easing';
  import { fly, slide } from 'svelte/transition';

  let rawMemoryMb = $state(0);
  let memoryDir = $state(1);
  let memoryMb = tweened(0, { duration: 800, easing: expoOut });

  onMount(() => {
    const refresh = async () => {
      try {
        const next = await invoke<number>('get_memory_mb');
        if (next !== rawMemoryMb) {
          memoryDir = next > rawMemoryMb ? 1 : -1;
          rawMemoryMb = next;
          memoryMb.set(next);
        }
      } catch { /* dev mode */ }
    };
    refresh();
    const id = setInterval(refresh, 2000);
    return () => clearInterval(id);
  });

  const navItems = [
    { id: 'home',       label: 'Home',       icon: 'home',     locked: false },
    { id: 'insights',   label: 'Insights',   icon: 'chart',    locked: false },
    { id: 'dictionary', label: 'Dictionary', icon: 'book',     locked: false },
    { id: 'snippets',   label: 'Snippets',   icon: 'scissors', locked: false },
    { id: 'style',      label: 'Style',      icon: 'type',     locked: false },
  ] as const;

  // The rail is shared: it shows app navigation normally and swaps to the
  // settings sections while settings is open, so the sidebar never unmounts.
  const settingsGroups = $derived(
    visibleSettingsSections({ isMac, devMode: appStore.devModeEnabled })
  );

  type RailEntry =
    | { kind: 'label'; key: string; label: string }
    | { kind: 'section'; key: string; id: SettingsSectionId; label: string; icon: keyof typeof icons };

  /*
   * Settings and app navigation use separate lists so outgoing and incoming
   * entries can overlap in the same grid cell during the mode transition.
   */
  const settingsEntries = $derived<RailEntry[]>(
    settingsGroups.flatMap((group) => [
      { kind: 'label' as const, key: `label:${group.group}`, label: group.group },
      ...group.items.map((item) => ({
        kind: 'section' as const,
        key: `section:${item.id}`,
        id: item.id,
        label: item.label,
        icon: item.icon,
      })),
    ])
  );

  /*
   * Purely horizontal: entries slide in from the rail's left edge and leave the
   * same way, so the sidebar reads as one axis of movement while the content
   * area moves on the other. The stagger (not the per-item duration) is what
   * makes the cascade legible; it's capped so the 9-entry settings rail doesn't
   * take much longer to resolve than the 4-entry app rail.
   *
   * Everything here shares cubicOut with pageSwap and the pill so the whole
   * morph settles on one curve — mixing easings was what made it feel uneven.
   */
  const RAIL_IN_MS = 260;
  const RAIL_OUT_MS = 180;
  const RAIL_IN_DELAY_MS = 40;
  const RAIL_TRAVEL_PX = 10;
  const RAIL_STAGGER_MS = 20;
  const RAIL_OUT_STAGGER_MS = 9;
  const RAIL_STAGGER_CAP = 6;

  function railDelay(index: number, base: number, step = RAIL_STAGGER_MS): number {
    return motionMs(base + Math.min(index, RAIL_STAGGER_CAP) * step);
  }

  function nav(id: string) {
    if (id === 'settings') { appStore.settingsOpen = true; return; }
    appStore.currentPage = id as typeof appStore.currentPage;
  }

  function goToSection(id: SettingsSectionId) {
    if (id === appStore.settingsSection) return;
    appStore.settingsAnimDir = directionFromOrder(
      appStore.settingsSection,
      id,
      SETTINGS_SECTION_ORDER
    );
    appStore.settingsSection = id;
  }

  function backToApp() { appStore.settingsOpen = false; }

  // ── Download panel ──────────────────────────────────────────────────────
  const activeDownloads = $derived(getActiveDownloads());
  const doneDownloads = $derived.by(() => {
    const activeKeys = new Set(activeDownloads.map((item) => item.key));
    return downloadUi.completed.filter((entry) => !activeKeys.has(entry.key));
  });
  const showDownloadPanel = $derived(activeDownloads.length > 0 || doneDownloads.length > 0);

  // The "ready" list lingers until the user opens and closes Settings, so clear
  // it on the settings-close transition (true → false).
  let prevSettingsOpen = appStore.settingsOpen;
  $effect(() => {
    const open = appStore.settingsOpen;
    if (prevSettingsOpen && !open) acknowledgeDownloads();
    prevSettingsOpen = open;
  });

  // ── Sliding active highlight ────────────────────────────────────────────
  // A single pill positioned against the active rail item rather than a
  // per-item background, so the highlight travels when the selection moves —
  // including across the morph, where it slides up from the Settings button.
  let sidebarEl = $state<HTMLElement | null>(null);
  let pillTop = $state(0);
  let pillHeight = $state(0);
  // Suppresses the CSS transition for one frame so the pill can be teleported
  // to a new origin (or placed on first paint) without animating from nowhere.
  let pillSnap = $state(true);

  function activeRailButton(): HTMLElement | null {
    if (!sidebarEl) return null;
    const selector = appStore.settingsOpen
      ? '.rail-list .settings-nav-item.active'
      : '.rail-list .nav-item.active';
    return sidebarEl.querySelector<HTMLElement>(selector);
  }

  function movePillTo(el: HTMLElement | null, { snap = false } = {}) {
    if (!el) return;
    if (snap) pillSnap = true;
    pillTop = el.offsetTop;
    pillHeight = el.offsetHeight;
    if (snap) {
      requestAnimationFrame(() => { pillSnap = false; });
    } else {
      pillSnap = false;
    }
  }

  /** Opens settings, seeding the pill at the Settings button so it slides up from it. */
  function openSettings(event: MouseEvent) {
    movePillTo(event.currentTarget as HTMLElement, { snap: true });
    appStore.settingsOpen = true;
  }

  $effect(() => {
    // Re-measure whenever the rail contents or the selection change.
    appStore.settingsOpen;
    appStore.currentPage;
    appStore.settingsSection;
    settingsEntries;
    requestAnimationFrame(() => movePillTo(activeRailButton()));
  });
</script>

<aside class="sidebar" class:rail-settings={appStore.settingsOpen} class:sidebar-windows={isWindows} bind:this={sidebarEl}>
  <Brand />

  <div class="rail-pill" class:rail-pill-snap={pillSnap} style="top:{pillTop}px; height:{pillHeight}px"></div>

  <div class="nav-section">
    {#if appStore.settingsOpen}
      <div class="rail-list">
        {#each settingsEntries as entry, i (entry.key)}
          {#if entry.kind === 'label'}
            <div
              class="settings-section-label"
              in:fly|global={{ x: -motionPx(RAIL_TRAVEL_PX), duration: motionMs(RAIL_IN_MS), delay: railDelay(i, RAIL_IN_DELAY_MS), easing: cubicOut }}
              out:fly|global={{ x: -motionPx(RAIL_TRAVEL_PX), duration: motionMs(RAIL_OUT_MS), delay: railDelay(i, 0, RAIL_OUT_STAGGER_MS), easing: cubicOut }}
            >{entry.label}</div>
          {:else}
            <button
              type="button"
              class="settings-nav-item"
              class:active={appStore.settingsSection === entry.id}
              onclick={() => goToSection(entry.id)}
              in:fly|global={{ x: -motionPx(RAIL_TRAVEL_PX), duration: motionMs(RAIL_IN_MS), delay: railDelay(i, RAIL_IN_DELAY_MS), easing: cubicOut }}
              out:fly|global={{ x: -motionPx(RAIL_TRAVEL_PX), duration: motionMs(RAIL_OUT_MS), delay: railDelay(i, 0, RAIL_OUT_STAGGER_MS), easing: cubicOut }}
            >
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width={appStore.settingsSection === entry.id ? '2.2' : '1.6'} stroke-linecap="round" stroke-linejoin="round">{@html icons[entry.icon]}</svg>
              <span>{entry.label}</span>
              {#if entry.id === 'advanced' && import.meta.env.DEV}
                <span class="legacy-label" aria-hidden="true">Microphone</span>
              {/if}
            </button>
          {/if}
        {/each}
      </div>
    {:else}
      <div class="rail-list">
        {#each navItems as entry, i (entry.id)}
          <button
            type="button"
            class="nav-item"
            class:active={appStore.currentPage === entry.id}
            disabled={entry.locked}
            onclick={() => nav(entry.id)}
            in:fly|global={{ x: -motionPx(RAIL_TRAVEL_PX), duration: motionMs(RAIL_IN_MS), delay: railDelay(i, RAIL_IN_DELAY_MS), easing: cubicOut }}
            out:fly|global={{ x: -motionPx(RAIL_TRAVEL_PX), duration: motionMs(RAIL_OUT_MS), delay: railDelay(i, 0, RAIL_OUT_STAGGER_MS), easing: cubicOut }}
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width={appStore.currentPage === entry.id ? '2.2' : '1.6'} stroke-linecap="round" stroke-linejoin="round">{@html icons[entry.icon]}</svg>
            <span>{entry.label}</span>
            {#if entry.locked}
              <span class="lock-tag">Soon</span>
            {/if}
          </button>
        {/each}
      </div>
    {/if}
  </div>

  <div class="sidebar-spacer"></div>

  {#if showDownloadPanel}
    <div
      class="dl-panel"
      in:fly={{ x: -motionPx(14), duration: motionMs(260), easing: cubicOut }}
      out:slide={{ duration: motionMs(200), easing: cubicOut }}
    >
      {#each activeDownloads as item (item.key)}
        <div class="dl-item" in:slide={{ duration: motionMs(200), easing: cubicOut }}>
          <div class="dl-item-top">
            <span class="dl-item-name" title={item.name}>{item.name}</span>
            <button
              type="button"
              class="dl-cancel"
              aria-label={`Cancel ${item.name} download`}
              title="Cancel download"
              onclick={() => cancelDownload(item)}
            >
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
            </button>
          </div>
          <LocalDownloadProgress
            stage={item.stage}
            label={item.label}
            percent={item.percent}
            indeterminate={item.indeterminate}
          />
        </div>
      {/each}
      {#each doneDownloads as entry (entry.key)}
        <div class="dl-done" in:slide={{ duration: motionMs(200), easing: cubicOut }}>
          <span class="dl-dot" aria-hidden="true"></span>
          <span class="dl-done-name" title={entry.name}>{entry.name} ready</span>
        </div>
      {/each}
    </div>
  {/if}

  <!--
    One persistent button rather than a swapped pair: it is the pill's origin
    when opening settings, and keeping it mounted means there is never a moment
    with two of it in the DOM during a fast close/reopen.
  -->
  <div class="sidebar-foot">
    <button
      type="button"
      class={appStore.settingsOpen ? 'settings-back' : 'nav-item'}
      onclick={appStore.settingsOpen ? backToApp : openSettings}
    >
      {#if appStore.settingsOpen}
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M15 18l-6-6 6-6"/></svg>
        <span>Back to app</span>
      {:else}
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">{@html icons.settings}</svg>
        <span>Settings</span>
      {/if}
    </button>
  </div>

  <!-- Shown in both modes: "running locally" is app-level status, not page
       chrome, and keeping it fixed means the rail's bottom never swaps. -->
  <div class="local-bar">
    <div class="local-bar-row">
      <span class="local-dot"></span>
      <span>Running locally</span>
      <div class="meta-wrapper">
        <span class="meta">
          {#each String(rawMemoryMb).split('') as digit, i (i)}
            <span class="digit-slot">
              {#key digit}
                <span
                  class="digit-char"
                  in:fly={{ y: memoryDir * 10, duration: 400, easing: expoOut }}
                  out:fly={{ y: -memoryDir * 10, duration: 400, easing: expoOut }}
                >{digit}</span>
              {/key}
            </span>
          {/each}<span class="meta-unit"> MB</span>
        </span>
      </div>
    </div>
    <div class="local-meter-thin"><span style="width:{Math.min($memoryMb / 200 * 100, 100)}%; background:{$memoryMb >= 150 ? 'var(--accent)' : 'var(--arm-300, #9caa8e)'}"></span></div>
  </div>
</aside>

<style>
  .sidebar {
    width: var(--sidebar-w);
    background: var(--bg-elev);
    border-right: 1px solid var(--line);
    /* .body keeps its bottom gutter for the content column; pull the sidebar
       through it so it runs flush into the bottom-left window corner. */
    margin-bottom: calc(-1 * var(--app-gutter));
    position: relative;
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    overflow: hidden;
  }

  /*
   * The settings wash (.settings-overlay, z-index 60) covers the whole app so
   * click-outside still works in the left gutter; the rail has to sit above it
   * to stay visible and interactive. .app is position:relative with z-index
   * auto and .body is static, so both compare in the same stacking context.
   */
  .sidebar.rail-settings {
    z-index: 61;
  }

  /*
   * Both rail lists occupy the same grid cell so the outgoing and incoming sets
   * cross-fade in place instead of stacking and shoving each other down. The
   * cell is as tall as the taller list mid-morph; .sidebar-spacer absorbs that,
   * so the footer never moves.
   */
  .nav-section {
    /* The settings rail sits 12px below the brand block; the app nav starts
       a bit lower (24px) so the clickable list reads as stepped down from the
       brand without floating. The rail's grid cell absorbs the taller list
       during the settings morph. */
    padding: 12px 8px 4px;
    display: grid;
  }

  .sidebar:not(.rail-settings) .nav-section { padding-top: 24px; }

  /* No Windows-specific nav offset: the brand block owns the rail header on
     every platform, and its min-height matches the native titlebar height, so
     the first nav target always starts below the caption. */

  .rail-list {
    grid-area: 1 / 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  /*
   * Single travelling highlight. Positioned against .sidebar (the nearest
   * positioned ancestor) from the active item's offsetTop, so it slides between
   * rail entries — and up from the Settings button when settings opens.
   * Rail buttons are position:relative and come later in tree order, so they
   * paint above this without needing a z-index.
   */
  .rail-pill {
    position: absolute;
    left: 8px;
    right: 8px;
    border-radius: 7px;
    background: var(--control-active);
    pointer-events: none;
    /* cubic-bezier(0.33, 1, 0.68, 1) is the CSS form of cubicOut, which the rail
       items and pageSwap both use — one curve across the whole morph. */
    transition:
      top var(--rail-pill-ms) cubic-bezier(0.33, 1, 0.68, 1),
      height var(--rail-pill-ms) cubic-bezier(0.33, 1, 0.68, 1);
  }

  .rail-pill.rail-pill-snap { transition: none; }

  .sidebar { --rail-pill-ms: 300ms; }

  @media (prefers-reduced-motion: reduce) {
    .sidebar { --rail-pill-ms: 170ms; }
  }

  /*
   * App nav, settings sections and the back button share one set of metrics so
   * the rail reads as the same list changing contents rather than two different
   * lists trading places.
   */
  .nav-item,
  .settings-nav-item,
  .settings-back {
    border: 0;
    background: transparent;
    display: flex;
    align-items: center;
    gap: 9px;
    min-height: 30px;
    padding: 6px;
    border-radius: 7px;
    color: var(--ink-soft);
    cursor: pointer;
    font-size: 12.5px;
    font-weight: 450;
    user-select: none;
    position: relative;
    text-align: left;
    width: 100%;
  }

  .nav-item :global(svg),
  .settings-nav-item :global(svg),
  .settings-back :global(svg) { opacity: 0.75; flex-shrink: 0; }

  .sidebar-windows .nav-item :global(svg),
  .sidebar-windows .settings-nav-item :global(svg),
  .sidebar-windows .settings-back :global(svg) {
    width: 15px;
    height: 15px;
  }

  .nav-item:hover,
  .settings-nav-item:hover,
  .settings-back:hover { color: var(--ink-strong); background: var(--control-hover); }

  .nav-item:focus-visible,
  .settings-nav-item:focus-visible,
  .settings-back:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  /* No background here — .rail-pill supplies it so the highlight can travel. */
  .nav-item.active,
  .settings-nav-item.active {
    color: var(--ink);
    font-weight: 500;
  }
  .nav-item.active :global(svg),
  .settings-nav-item.active :global(svg) { opacity: 1; }

  /* Hover must not paint over the pill on the item that already owns it. */
  .nav-item.active:hover,
  .settings-nav-item.active:hover { background: transparent; }

  .settings-section-label {
    font-family: var(--sans);
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--ink-mute);
    padding: 10px 10px 5px;
  }

  .legacy-label {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }

  /* The version line lives on the settings page itself now, not in the rail. */

  .nav-item:disabled {
    color: var(--ink-faint);
    cursor: default;
    opacity: 1;
  }
  .nav-item:disabled:hover { background: transparent; color: var(--ink-faint); }
  .nav-item:disabled :global(svg) { opacity: 0.5; }

  .lock-tag {
    margin-left: auto;
    font-family: var(--sans);
    font-size: 9px;
    color: var(--ink-mute);
    padding: 1px 6px;
    border-radius: 999px;
    font-weight: 500;
    letter-spacing: 0.04em;
    border: 1px solid var(--line);
  }

  .sidebar-spacer { flex: 1; }

  /* Download panel: sits just above the foot button, slides in from the left. */
  .dl-panel {
    margin: 0 8px 6px;
    padding: 10px 11px;
    max-height: min(30vh, 240px);
    overflow-y: auto;
    border-radius: 9px;
    background: var(--control-active);
    border: 1px solid var(--line-soft);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .dl-item-top {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .dl-item-name {
    flex: 1;
    min-width: 0;
    font-size: 11.5px;
    font-weight: 500;
    color: var(--ink-soft);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .dl-cancel {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--ink-mute);
    cursor: pointer;
    transition: background 140ms ease, color 140ms ease;
  }

  .dl-cancel:hover {
    background: var(--danger-bg, color-mix(in srgb, var(--danger) 12%, transparent));
    color: var(--danger);
  }

  .dl-cancel:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .dl-done {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 11.5px;
    color: var(--ink-soft);
  }

  .dl-dot {
    flex-shrink: 0;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }

  .dl-done-name {
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .sidebar-foot {
    padding: 6px 8px 8px;
    border-top: 1px solid var(--line-soft);
    margin: 0 8px;
  }

  .local-bar {
    margin: 4px 8px 10px;
    padding: 9px 10px;
    border-radius: 8px;
    background: var(--control-active);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .local-bar-row {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 11px;
    color: var(--ink-soft);
  }

  .local-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--arm-700);
    flex-shrink: 0;
    display: block;
  }

  .meta-wrapper {
    margin-left: auto;
    display: flex;
    align-items: center;
  }

  .meta {
    display: inline-flex;
    align-items: center;
    font-family: var(--mono);
    font-size: 10px;
    color: var(--ink-mute);
    font-variant-numeric: tabular-nums;
  }

  .digit-slot {
    position: relative;
    display: inline-block;
    overflow: hidden;
    width: 1ch;
    height: 14px;
  }

  .digit-char {
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    text-align: center;
    line-height: 14px;
  }

  .meta-unit {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--ink-mute);
  }

  .local-meter-thin {
    height: 2px;
    background: var(--amber-200);
    border-radius: 999px;
    overflow: hidden;
  }

  .local-meter-thin span {
    display: block;
    height: 100%;
    border-radius: 999px;
    transition: background 0.4s ease;
  }
</style>
