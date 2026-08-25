<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '../../tauri';
  import { appStore } from '../../stores';
  import { icons } from '../../icons';
  import { isMac, isWindows } from '../../platform';
  import { MOTION_MS, SETTINGS_SECTION_ORDER, directionFromOrder, motionMs, motionPx } from '../../motion';
  import { visibleSettingsSections, type SettingsSectionId } from '../../settingsSections';
  import Brand from './Brand.svelte';
  import AppIcon from '../AppIcon.svelte';
  import SiteIcon from '../SiteIcon.svelte';
  import { classifyIpcError } from '../../errors';
  import {
    contextsStore,
    loadContexts,
    orderedContexts,
    selectContext,
    isPinned,
    compactAge,
    EVERYWHERE_ID,
  } from '../../contextsStore.svelte';
  import type { Context } from '../../stores';
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
  import { flip } from 'svelte/animate';

  let rawMemoryMb = $state(0);
  let memoryDir = $state(1);
  let memoryMb = tweened(0, { duration: motionMs(800), easing: expoOut });

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

  const HOME_NAV_ITEMS = [
    { id: 'home',       label: 'Home',       icon: 'home',  locked: false },
    { id: 'insights',   label: 'Insights',   icon: 'chart', locked: false },
  ] as const;

  const STYLE_NAV_ITEMS = [
    { id: 'style',      label: 'Style',      icon: 'pencil', locked: false },
  ] as const;

  // Dictionary and Snippets are no longer primary nav — Contexts (Everywhere)
  // is the primary vocabulary/snippet surface now. The old pages still exist
  // as routes, surfaced here only once Settings > General > Legacy is on —
  // which also hides Contexts, since Legacy mode means "manage app tones,
  // vocabulary, and snippets the old way" and running both surfaces at once
  // would just mean two conflicting places to edit the same data.
  const LEGACY_NAV_ITEMS = [
    { id: 'dictionary', label: 'Dictionary', icon: 'book',     locked: false },
    { id: 'snippets',   label: 'Snippets',   icon: 'scissors', locked: false },
  ] as const;

  // Contexts are no longer a nav button — they live in their own section of
  // the rail below, built from the user's actual context groups.
  const navItems = $derived(
    appStore.legacyFeaturesEnabled
      ? [...HOME_NAV_ITEMS, ...LEGACY_NAV_ITEMS, ...STYLE_NAV_ITEMS]
      : [...HOME_NAV_ITEMS, ...STYLE_NAV_ITEMS]
  );

  // The rail is shared: it shows app navigation normally and swaps to the
  // settings sections while settings is open, so the sidebar never unmounts.
  const settingsGroups = $derived(
    visibleSettingsSections({ isMac, devMode: appStore.devModeEnabled, legacyMode: appStore.legacyFeaturesEnabled })
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

  // ── Contexts section ────────────────────────────────────────────────────
  // Legacy mode swaps Contexts back out for the standalone Dictionary and
  // Snippets pages, so the section is hidden there for the same reason the
  // old nav item was.
  const showContexts = $derived(!appStore.settingsOpen && !appStore.legacyFeaturesEnabled);
  const contextRows = $derived(orderedContexts(contextsStore.contexts));

  onMount(() => { void loadContexts(); });

  // Only pinned rows carry an icon stack — that is what makes pinning worth
  // doing, and it keeps every other row a clean single line. Four icons at
  // rest, up to eight revealed on hover, and anything past that stays behind a
  // residual count so a long app list can't push the stack out of the rail.
  const STACK_COMPACT = 4;
  const STACK_EXPANDED = 8;
  /** Per-icon stagger for the hover reveal, in ms. */
  const STACK_REVEAL_STEP = 35;

  type StackEntry = { key: string; kind: 'app' | 'site'; value: string };

  /** Apps and sites share one stack — splitting them just adds visual noise. */
  function stackEntries(contextId: number): StackEntry[] {
    return [
      ...contextsStore.targets
        .filter((target) => target.context_id === contextId)
        .map((target) => ({ key: `app:${target.id}`, kind: 'app' as const, value: target.executable })),
      ...contextsStore.websites
        .filter((site) => site.context_id === contextId)
        .map((site) => ({ key: `site:${site.id}`, kind: 'site' as const, value: site.domain })),
    ];
  }

  const pinnedRows = $derived(contextRows.filter((context) => isPinned(context)));
  const unpinnedRows = $derived(contextRows.filter((context) => !isPinned(context)));

  function openContext(id: number) {
    selectContext(id);
    appStore.currentPage = 'contexts';
    closeContextMenu();
  }

  function createContext() {
    contextsStore.modalRequest = { mode: 'create' };
    appStore.currentPage = 'contexts';
    closeContextMenu();
  }

  // ── Context row menu (kebab and right-click open the same one) ───────────
  // Positioned by top/left and then clamped into the viewport once mounted —
  // right-clicking near the rail's left edge used to push the menu off-screen.
  let contextMenu = $state<{ id: number; top: number; left: number; alignRight: boolean } | null>(null);
  let contextMenuEl = $state<HTMLElement | null>(null);
  let contextDeleteArmed = $state(false);
  let contextError = $state('');
  const MENU_VIEWPORT_MARGIN = 8;

  const menuContext = $derived(
    contextMenu ? contextsStore.contexts.find((c) => c.id === contextMenu!.id) ?? null : null
  );

  function closeContextMenu() {
    contextMenu = null;
    contextDeleteArmed = false;
  }

  function openContextMenu(id: number, top: number, left: number, alignRight: boolean) {
    if (contextMenu?.id === id) {
      closeContextMenu();
      return;
    }
    contextMenu = { id, top, left, alignRight };
    contextDeleteArmed = false;
  }

  function toggleContextMenuFromKebab(id: number, trigger: HTMLElement) {
    const rect = trigger.getBoundingClientRect();
    // Right-aligned to the button; the clamp effect resolves the real left
    // once the menu's width is known.
    openContextMenu(id, rect.bottom + 4, rect.right, true);
  }

  function handleContextRowContextMenu(id: number, event: MouseEvent) {
    event.preventDefault();
    // Fresh position on every right-click, even if the menu was already open
    // on this row from a previous click elsewhere.
    contextMenu = null;
    openContextMenu(id, event.clientY + 4, event.clientX, false);
  }

  /*
   * Measures the mounted menu and pulls it fully on-screen. offsetWidth/Height
   * rather than getBoundingClientRect: the entry transition scales and shifts
   * the box, and a mid-flight rect would clamp against the wrong size.
   */
  $effect(() => {
    const el = contextMenuEl;
    const menu = contextMenu;
    if (!el || !menu) return;
    const width = el.offsetWidth;
    const height = el.offsetHeight;
    const desiredLeft = menu.alignRight ? menu.left - width : menu.left;
    const left = Math.max(
      MENU_VIEWPORT_MARGIN,
      Math.min(desiredLeft, window.innerWidth - width - MENU_VIEWPORT_MARGIN),
    );
    const top = Math.max(
      MENU_VIEWPORT_MARGIN,
      Math.min(menu.top, window.innerHeight - height - MENU_VIEWPORT_MARGIN),
    );
    if (left === menu.left && top === menu.top && !menu.alignRight) return;
    contextMenu = { ...menu, left, top, alignRight: false };
  });

  $effect(() => {
    if (!contextMenu) return;
    const handleClose = (event: Event) => {
      if (event.target instanceof Element && event.target.closest('.ctx-kebab, .ctx-menu')) return;
      closeContextMenu();
    };
    const timeout = window.setTimeout(() => {
      window.addEventListener('pointerdown', handleClose);
      window.addEventListener('scroll', handleClose, { capture: true, passive: true });
    });
    return () => {
      window.clearTimeout(timeout);
      window.removeEventListener('pointerdown', handleClose);
      window.removeEventListener('scroll', handleClose, { capture: true });
    };
  });

  function editContext(context: Context) {
    contextsStore.selectedId = context.id;
    contextsStore.modalRequest = { mode: 'edit', id: context.id };
    appStore.currentPage = 'contexts';
    closeContextMenu();
  }

  function editContextById(id: number) {
    const context = contextsStore.contexts.find((candidate) => candidate.id === id);
    if (context) editContext(context);
  }

  async function togglePin(context: Context) {
    const pinned = !context.pinned_at;
    closeContextMenu();
    try {
      await invoke('set_context_pinned', { contextId: context.id, pinned });
      // Re-pinning restamps, so the row jumps to the top of the pinned group;
      // mirror the backend's `datetime('now')` locally rather than refetching.
      // The stamp must match SQLite's `YYYY-MM-DD HH:MM:SS` UTC format or the
      // pinned sort (a plain string compare) would interleave the two formats.
      const pinnedAt = pinned ? new Date().toISOString().slice(0, 19).replace('T', ' ') : null;
      contextsStore.contexts = contextsStore.contexts.map((c) =>
        c.id === context.id ? { ...c, pinned_at: pinnedAt } : c
      );
    } catch (error) {
      contextError = classifyIpcError(error).message;
    }
  }

  /** Two-click arm/confirm — the same delete affordance the Contexts rows had. */
  async function deleteContext(context: Context) {
    if (context.is_everywhere) return;
    if (!contextDeleteArmed) {
      contextDeleteArmed = true;
      return;
    }
    try {
      await invoke('delete_context', { contextId: context.id });
      contextsStore.contexts = contextsStore.contexts.filter((c) => c.id !== context.id);
      contextsStore.targets = contextsStore.targets.filter((t) => t.context_id !== context.id);
      contextsStore.websites = contextsStore.websites.filter((w) => w.context_id !== context.id);
      if (contextsStore.selectedId === context.id) contextsStore.selectedId = EVERYWHERE_ID;
    } catch (error) {
      contextError = classifyIpcError(error).message;
    } finally {
      closeContextMenu();
    }
  }
</script>

<aside class="sidebar" class:rail-settings={appStore.settingsOpen} class:sidebar-windows={isWindows} bind:this={sidebarEl}>
  <Brand />

  <div
    class="rail-pill"
    class:rail-pill-snap={pillSnap}
    class:rail-pill-hidden={!appStore.settingsOpen && appStore.currentPage === 'contexts'}
    style="top:{pillTop}px; height:{pillHeight}px"
  ></div>

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

  {#if showContexts}
    <!--
      The section owns the rail's free vertical space: header and footer stay
      fixed, only .ctx-list scrolls, and it only scrolls when the rows actually
      overflow (min-height:0 + overflow-y:auto).
    -->
    <div class="ctx-section">
      <div class="ctx-head">
        <span class="ctx-head-label">Contexts</span>
        <span class="ctx-head-rule" aria-hidden="true"></span>
        <button type="button" class="ctx-add" aria-label="New context group" title="New context group" onclick={createContext}>
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 5v14M5 12h14"/></svg>
        </button>
      </div>

      {#if contextError}
        <p class="ctx-error" role="alert">{contextError}</p>
      {/if}

      <!--
        Two keyed lists rather than one: an `animate:` directive has to be the
        only child of its each block, so the group labels can't live inside it.
        Splitting also means pin/unpin reads as a row leaving one group and
        entering the other, which is what actually happened.
      -->
      {#snippet contextRow(context: Context)}
        {@const pinned = isPinned(context)}
        {@const stack = pinned ? stackEntries(context.id) : []}
        {@const shown = stack.slice(0, STACK_EXPANDED)}
        {@const restCount = stack.length - STACK_EXPANDED}
            <button
              type="button"
              class="ctx-row"
              class:has-stack={stack.length > 0}
              class:active={appStore.currentPage === 'contexts' && contextsStore.selectedId === context.id}
              onclick={() => openContext(context.id)}
            >
              <span class="ctx-icon" style={context.color ? `color: ${context.color}` : ''} aria-hidden="true">
                {#if context.is_everywhere}
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"><circle cx="12" cy="12" r="8"/><path d="M4 12h16M12 4c2 2.3 3 5 3 8s-1 5.7-3 8c-2-2.3-3-5-3-8s1-5.7 3-8Z"/></svg>
                {:else if context.icon && icons[context.icon as keyof typeof icons]}
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">{@html icons[context.icon as keyof typeof icons]}</svg>
                {:else}
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M7 8h10M7 12h6M7 16h3"/></svg>
                {/if}
              </span>
              <span class="ctx-name">{context.name}</span>
              {#if stack.length > 0}
                <!-- Spans from grid column 1, so the first icon lines up under
                     the context icon rather than under the name. -->
                <span class="ctx-stack" aria-hidden="true">
                  {#each shown as entry, iconIndex (entry.key)}
                    <span
                      class="ctx-stack-item"
                      class:is-extra={iconIndex >= STACK_COMPACT}
                      style={iconIndex >= STACK_COMPACT ? `--reveal-delay: ${(iconIndex - STACK_COMPACT) * STACK_REVEAL_STEP}ms` : ''}
                    >
                      {#if entry.kind === 'app'}
                        <AppIcon exe={entry.value} size={15} />
                      {:else}
                        <SiteIcon domain={entry.value} size={15} />
                      {/if}
                    </span>
                  {/each}
                  {#if stack.length > STACK_COMPACT}
                    <span class="ctx-stack-more">+{stack.length - STACK_COMPACT}</span>
                  {/if}
                  {#if restCount > 0}
                    <span class="ctx-stack-more ctx-stack-more-rest">+{restCount}</span>
                  {/if}
                </span>
              {/if}
            </button>

            <!-- Fixed right-hand column: the age reserves a constant width so
                 the pin lands at the same x whether the row reads "1w" or
                 "1mo", and both give way to one overflow control on hover. -->
            <span class="ctx-tail">
              {#if pinned}
                <span class="ctx-pin" title="Pinned">
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M18 3v2h-1v6l2 3v2h-6v5h-2v-5H5v-2l2-3V5H6V3z"/></svg>
                </span>
              {/if}
              <span class="ctx-age">{compactAge(context.created_at)}</span>
              <button
                  type="button"
                  class="ctx-kebab"
                  aria-label={`More actions for ${context.name}`}
                  aria-haspopup="menu"
                  aria-expanded={contextMenu?.id === context.id}
                  onclick={(event) => toggleContextMenuFromKebab(context.id, event.currentTarget)}
                >
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor"><circle cx="5" cy="12" r="1.8"/><circle cx="12" cy="12" r="1.8"/><circle cx="19" cy="12" r="1.8"/></svg>
                </button>
            </span>
      {/snippet}

      <div class="ctx-list">
        {#each pinnedRows as context (context.id)}
          <div
            class="ctx-row-wrap"
            class:menu-open={contextMenu?.id === context.id}
            oncontextmenu={(event) => handleContextRowContextMenu(context.id, event)}
            role="presentation"
            animate:flip={{ duration: motionMs(300), easing: cubicOut }}
            in:slide={{ duration: motionMs(200), easing: cubicOut }}
            out:slide={{ duration: motionMs(160), easing: cubicOut }}
          >
            {@render contextRow(context)}
          </div>
        {/each}

        <!--
          "More contexts", not "Everything else": these are the same kind of
          thing as the rows above, just not pinned. A catch-all noun made the
          group read as a separate category sitting outside contexts.
        -->
        {#if unpinnedRows.length > 0 && pinnedRows.length > 0}
          <div class="ctx-subhead">
            <span class="ctx-subhead-label">More contexts</span>
            <span class="ctx-subhead-rule" aria-hidden="true"></span>
          </div>
        {/if}
        {#each unpinnedRows as context (context.id)}
          <div
            class="ctx-row-wrap"
            class:menu-open={contextMenu?.id === context.id}
            oncontextmenu={(event) => handleContextRowContextMenu(context.id, event)}
            role="presentation"
            animate:flip={{ duration: motionMs(300), easing: cubicOut }}
            in:slide={{ duration: motionMs(200), easing: cubicOut }}
            out:slide={{ duration: motionMs(160), easing: cubicOut }}
          >
            {@render contextRow(context)}
          </div>
        {/each}
      </div>
    </div>
  {:else}
    <div class="sidebar-spacer"></div>
  {/if}

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
                  in:fly={{ y: memoryDir * 10, duration: motionMs(400), easing: expoOut }}
                  out:fly={{ y: -memoryDir * 10, duration: motionMs(400), easing: expoOut }}
                >{digit}</span>
              {/key}
            </span>
          {/each}<span class="meta-unit"> MB</span>
        </span>
      </div>
    </div>
    <div class="local-meter-thin"><span style="width:{Math.min($memoryMb / 200 * 100, 100)}%; background:{$memoryMb >= 150 ? 'var(--accent)' : 'var(--line-strong)'}"></span></div>
  </div>
</aside>

{#if contextMenu && menuContext}
  <div
    class="ui-dropdown-menu ctx-menu"
    role="menu"
    tabindex="-1"
    bind:this={contextMenuEl}
    style="top: {contextMenu.top}px; left: {contextMenu.left}px;"
    onpointerdown={(event) => event.stopPropagation()}
    in:fly={{ y: motionPx(6), duration: motionMs(MOTION_MS.fast) }}
    out:fly={{ y: motionPx(4), duration: motionMs(120) }}
  >
    <button class="ui-dropdown-option" type="button" role="menuitem" onclick={() => editContextById(menuContext.id)}>Edit</button>
    <button class="ui-dropdown-option" type="button" role="menuitem" onclick={() => void togglePin(menuContext)}>
      {menuContext.pinned_at ? 'Unpin' : 'Pin'}
    </button>
    <!-- Everywhere is the fallback the pipeline resolves to when nothing else
         matches, so it is the one context that cannot be removed. -->
    {#if !menuContext.is_everywhere}
      <button
        class="ui-dropdown-option ctx-menu-delete"
        class:is-armed={contextDeleteArmed}
        type="button"
        role="menuitem"
        onclick={() => void deleteContext(menuContext)}
      >
        {contextDeleteArmed ? 'Confirm delete' : 'Delete'}
      </button>
    {/if}
  </div>
{/if}

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

  .sidebar:not(.rail-settings) .nav-section { padding-top: 18px; }

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
    /* Softer than the selected context row: page nav is the weaker selection,
       the open context is the thing that actually matters. */
    background: var(--control-hover);
    pointer-events: none;
    /* cubic-bezier(0.33, 1, 0.68, 1) is the CSS form of cubicOut, which the rail
       items and pageSwap both use — one curve across the whole morph. */
    transition:
      top var(--rail-pill-ms) cubic-bezier(0.33, 1, 0.68, 1),
      height var(--rail-pill-ms) cubic-bezier(0.33, 1, 0.68, 1);
  }

  .rail-pill.rail-pill-snap { transition: none; }

  /* Contexts have their own per-row selected background, so the shared pill
     steps aside instead of sitting on a nav item that isn't the active page. */
  .rail-pill.rail-pill-hidden { opacity: 0; }
  .rail-pill { opacity: 1; transition: opacity 160ms ease, top var(--rail-pill-ms) cubic-bezier(0.33, 1, 0.68, 1), height var(--rail-pill-ms) cubic-bezier(0.33, 1, 0.68, 1); }

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
    min-height: 28px;
    padding: 5px 6px;
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

  /* ── Contexts section ──────────────────────────────────────────────────
     Takes the rail's leftover height. Header and the rest of the sidebar stay
     put; only .ctx-list scrolls, and only once the rows outgrow the space. */
  .ctx-section {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    /* Small breathing gap between the primary nav and this section. */
    padding: 10px 8px 0;
  }

  .ctx-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 6px 4px;
  }

  .ctx-head-label {
    flex: none;
    font-size: 10.5px;
    font-weight: 550;
    color: var(--ink-mute);
  }

  /* Anchors the section rather than letting the label float; stops short of
     the + so the control keeps its own breathing room. */
  .ctx-head-rule {
    flex: 1;
    min-width: 8px;
    height: 1px;
    background: var(--line-soft);
  }

  .ctx-add {
    flex-shrink: 0;
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--ink-mute);
    cursor: pointer;
    transition: background-color 140ms ease, color 140ms ease;
  }
  .ctx-add:hover { background: var(--control-hover); color: var(--ink-strong); }
  .ctx-add:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }

  .ctx-error {
    margin: 0 6px 6px;
    font-size: 10.5px;
    color: var(--danger);
  }

  .ctx-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    display: flex;
    flex-direction: column;
    gap: 1px;
    /* Keep the subtle scrollbar off the row content when it does appear. */
    scrollbar-width: thin;
    scrollbar-color: var(--line-strong) transparent;
  }
  .ctx-list::-webkit-scrollbar { width: 6px; }
  .ctx-list { padding-bottom: 4px; }
  .ctx-list::-webkit-scrollbar-track { background: transparent; }
  .ctx-list::-webkit-scrollbar-thumb { background: var(--line); border-radius: 999px; }
  .ctx-list:hover::-webkit-scrollbar-thumb { background: var(--line-strong); }

  .ctx-row-wrap { position: relative; }

  /* One level down from the CONTEXTS header: same anchored-rule shape, but
     smaller and dimmer so it divides without announcing itself. */
  .ctx-subhead {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 14px 6px 4px;
  }

  .ctx-subhead-label {
    flex: none;
    font-size: 10px;
    font-weight: 500;
    color: var(--ink-faint);
    opacity: 0.9;
  }

  .ctx-subhead-rule {
    flex: 1;
    min-width: 8px;
    height: 1px;
    background: color-mix(in srgb, var(--line-soft) 70%, transparent);
  }

  /*
   * One grid for every row, so nothing is positioned by eye:
   *   column 1  fixed icon gutter
   *   column 2  name, and - on pinned rows - the icon stack directly beneath it
   * The metadata lane is absolute (a button cannot nest inside a button) but is
   * locked to the same first-row box, so it centres against the name on both
   * row heights.
   */
  .ctx-row {
    width: 100%;
    border: 0;
    background: transparent;
    border-radius: 7px;
    color: var(--ink-soft);
    cursor: pointer;
    user-select: none;
    text-align: left;
    display: grid;
    grid-template-columns: var(--ctx-gutter) minmax(0, 1fr);
    column-gap: var(--ctx-gutter-gap);
    row-gap: 1px;
    align-items: center;
    padding: var(--ctx-row-pad-y) 40px var(--ctx-row-pad-y) var(--ctx-row-pad-x);
    --stack-ring: var(--bg-elev);
    transition:
      background-color 170ms cubic-bezier(0.33, 1, 0.68, 1),
      color 170ms cubic-bezier(0.33, 1, 0.68, 1);
  }

  .ctx-section {
    --ctx-gutter: 20px;
    --ctx-gutter-gap: 7px;
    --ctx-row-pad-x: 6px;
    --ctx-row-pad-y: 5px;
    /* Both grid rows are fixed, so a pinned row is the same height whether it
       holds one favicon or eight. */
    --ctx-line-h: 18px;
  }

  /* Just enough extra beneath the stack to acknowledge the second line
     without making pinned rows feel bulky. */
  .ctx-row.has-stack { padding-bottom: calc(var(--ctx-row-pad-y) + 3px); }
  .ctx-row:hover { background: var(--control-hover); color: var(--ink-strong); --stack-ring: var(--control-hover); }
  /* The strongest selected state in the rail: a filled surface plus a hairline
     edge, so the open context reads as a card rather than a tint. */
  .ctx-row.active {
    background: var(--control-active);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--ink);
    --stack-ring: var(--control-active);
  }
  .ctx-row.active .ctx-name { font-weight: 500; color: var(--ink-strong); }
  .ctx-row:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }

  /*
   * Fixed optical box with every glyph rendered at the same 14px and stroke
   * weight, so the leading icons share one column even though they come from
   * different sources (shared icon set, per-context choice, inline fallbacks).
   */
  .ctx-icon {
    display: grid;
    place-items: center;
    width: var(--ctx-gutter);
    height: var(--ctx-line-h);
    color: var(--ink-faint);
    /* Inactive leading icons sit well back; hover and selection bring them up. */
    opacity: 0.7;
    transition: opacity 170ms ease, color 170ms ease;
  }
  .ctx-icon :global(svg) { width: 14px; height: 14px; }
  .ctx-row:hover .ctx-icon,
  .ctx-row.active .ctx-icon { opacity: 1; color: var(--ink-mute); }

  .ctx-name {
    min-width: 0;
    color: var(--ink);
    font-size: 12px;
    font-weight: 450;
    line-height: var(--ctx-line-h);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /*
   * Second grid row, spanning from column 1 - so the first icon sits under the
   * context icon rather than under the name. justify-self keeps the box to its
   * contents, which is also the hover target for the reveal: stretching it
   * across the row made the reveal fire whenever the cursor merely crossed it.
   */
  .ctx-stack {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    justify-self: start;
    max-width: 100%;
    height: var(--ctx-line-h);
    --stack-dwell: 500ms;
  }

  /* The ring separates overlapping icons; it tracks whatever surface the row
     is currently painting, so it doesn't halo against a hovered or selected
     background. */
  .ctx-stack-item {
    display: grid;
    place-items: center;
    border-radius: 5px;
    box-shadow: 0 0 0 1.5px var(--stack-ring);
    background: var(--stack-ring);
  }
  .ctx-stack-item + .ctx-stack-item { margin-left: -4px; }

  /* Icons past the resting four collapse to zero width and unfurl on hover,
     staggered by --reveal-delay so they cascade rather than pop. Collapsing
     back carries no delay, so leaving feels immediate. */
  .ctx-stack-item.is-extra {
    max-width: 0;
    opacity: 0;
    margin-left: 0 !important;
    overflow: hidden;
    transition:
      max-width 220ms cubic-bezier(0.33, 1, 0.68, 1),
      opacity 150ms ease,
      margin-left 220ms cubic-bezier(0.33, 1, 0.68, 1);
  }
  .ctx-stack:hover .ctx-stack-item.is-extra {
    max-width: 18px;
    opacity: 1;
    margin-left: -4px !important;
    transition-delay: calc(var(--stack-dwell) + var(--reveal-delay, 0ms));
  }

  .ctx-stack-more {
    margin-left: 5px;
    font-family: var(--sans);
    font-size: 8px;
    font-variant-numeric: tabular-nums;
    color: var(--ink-faint);
    opacity: 0.7;
    white-space: nowrap;
    overflow: hidden;
    transition: opacity 150ms ease, max-width 200ms ease, margin-left 200ms ease;
  }
  /* The resting "+N" gives way to the revealed icons, and a residual count for
     anything past the reveal cap slides into its place. Both collapse their own
     width rather than just fading, so the hidden one leaves no gap behind. */
  .ctx-stack:hover .ctx-stack-more {
    opacity: 0;
    max-width: 0;
    margin-left: 0;
    transition-delay: var(--stack-dwell);
  }
  .ctx-stack-more-rest {
    opacity: 0;
    max-width: 0;
    margin-left: 0;
  }
  .ctx-stack:hover .ctx-stack-more-rest {
    opacity: 0.7;
    max-width: 30px;
    margin-left: 5px;
    transition-delay: calc(var(--stack-dwell) + 140ms);
  }

  /*
   * Metadata lane. Locked to the first grid row's box, so it centres on the
   * name identically on one- and two-line rows. Inert by default so clicks land
   * on the row underneath; only the kebab takes pointer events.
   */
  .ctx-tail {
    position: absolute;
    top: var(--ctx-row-pad-y);
    right: var(--ctx-row-pad-x);
    height: var(--ctx-line-h);
    display: flex;
    align-items: center;
    gap: 3px;
    pointer-events: none;
  }

  .ctx-pin {
    display: grid;
    place-items: center;
    width: 10px;
    height: 10px;
    /* Timestamps and pins are the quietest thing in the row. */
    color: color-mix(in srgb, var(--ink-faint) 70%, transparent);
    transition: opacity 150ms ease;
  }

  /* Fixed width, right-aligned, and no wider than the longest age it has to
     hold ("11mo") — the pin keeps a constant x without drifting away from the
     value it sits next to. */
  .ctx-age {
    width: 23px;
    text-align: right;
    font-family: var(--sans);
    font-size: 9.5px;
    font-variant-numeric: tabular-nums;
    color: color-mix(in srgb, var(--ink-faint) 70%, transparent);
    letter-spacing: 0.02em;
    transition: opacity 150ms ease;
  }

  .ctx-kebab {
    position: absolute;
    right: -3px;
    top: 50%;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--ink-mute);
    cursor: pointer;
    pointer-events: auto;
    opacity: 0;
    z-index: 1;
    transform: translateY(-50%) scale(0.82);
    transition: opacity 150ms ease, transform 170ms cubic-bezier(0.33, 1, 0.68, 1), background-color 140ms ease, color 140ms ease;
  }
  .ctx-kebab:hover { background: var(--control-hover); color: var(--ink-strong); }
  .ctx-kebab:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
    opacity: 1;
    pointer-events: auto;
    transform: translateY(-50%) scale(1);
  }

  /* Age and pin both give way to a single overflow control in the same lane,
     so hover always resolves to one thing regardless of row type. */
  .ctx-row-wrap:hover .ctx-age,
  .ctx-row-wrap:focus-within .ctx-age,
  .ctx-row-wrap.menu-open .ctx-age,
  .ctx-row-wrap:hover .ctx-pin,
  .ctx-row-wrap:focus-within .ctx-pin,
  .ctx-row-wrap.menu-open .ctx-pin { opacity: 0; }
  .ctx-row-wrap:hover .ctx-kebab,
  .ctx-row-wrap:focus-within .ctx-kebab,
  .ctx-row-wrap.menu-open .ctx-kebab {
    opacity: 1;
    pointer-events: auto;
    transform: translateY(-50%) scale(1);
  }

  @media (prefers-reduced-motion: reduce) {
    .ctx-row,
    .ctx-kebab,
    .ctx-age,
    .ctx-pin,
    .ctx-icon,
    .ctx-stack-item.is-extra,
    .ctx-stack-more { transition-duration: 1ms; transition-delay: 0ms; }
    .ctx-stack { --stack-dwell: 0ms; }
  }

  .ctx-menu {
    position: fixed;
    /* .ui-dropdown-menu pins right:0 for its normal absolute usage; this popup
       is placed with an inline left, so right has to be released or the box
       stretches to the viewport edge. */
    right: auto;
    z-index: 70;
    min-width: 132px;
    max-width: 200px;
  }
  .ctx-menu-delete { transition: color .15s ease, background-color .15s ease; }
  .ctx-menu-delete:hover { color: var(--danger); }
  .ctx-menu-delete.is-armed { color: var(--danger); font-weight: 500; background: var(--danger-bg); }


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
    border-top: 1px solid color-mix(in srgb, var(--line-soft) 55%, transparent);
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

  @media (max-width: 720px) {
    .sidebar { width: 58px; }
    .sidebar :global(.brand) { padding-inline: 0; justify-content: center; }
    .sidebar :global(.brand-name) { display: none; }
    .nav-section { padding-inline: 7px; }
    .sidebar:not(.rail-settings) .nav-section { padding-top: 18px; }
    .nav-item,
    .settings-nav-item,
    .settings-back { justify-content: center; gap: 0; padding-inline: 0; }
    .nav-item > span,
    .settings-nav-item > span,
    .settings-back > span,
    .settings-section-label { display: none; }
    .sidebar-foot { margin-inline: 7px; padding-inline: 0; }
    .local-bar { margin-inline: 7px; padding-inline: 0; align-items: center; }
    .local-bar-row { justify-content: center; }
    .local-bar-row > span:not(.local-dot),
    .meta-wrapper,
    .local-meter-thin { display: none; }
    .dl-panel { margin-inline: 7px; padding-inline: 6px; }
    .dl-item-name,
    .dl-done-name { display: none; }
    .ctx-section { padding-inline: 7px; }
    .ctx-head { justify-content: center; padding-inline: 0; }
    .ctx-head-label,
    .ctx-head-rule { display: none; }
    .ctx-subhead { justify-content: center; padding-inline: 0; }
    .ctx-subhead-label { display: none; }
    .ctx-row {
      grid-template-columns: 1fr;
      column-gap: 0;
      justify-items: center;
      padding: 6px 0;
    }
    .ctx-name,
    .ctx-stack,
    .ctx-tail { display: none; }
  }
</style>
