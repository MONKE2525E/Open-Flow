<script lang="ts">
  import { onMount } from 'svelte';
  import { appStore } from './lib/stores';
  import { cleanupPromptEditor } from './lib/stores.svelte';
  import { isWindows } from './lib/platform';
  import Sidebar from './lib/components/layout/Sidebar.svelte';
  import Home from './lib/views/Home.svelte';
  import Insights from './lib/views/Insights.svelte';
  import Contexts from './lib/views/Contexts.svelte';
  import Dictionary from './lib/views/Dictionary.svelte';
  import Snippets from './lib/views/Snippets.svelte';
  import Style from './lib/views/Style.svelte';
  import Settings from './lib/views/Settings.svelte';
  import CleanupPromptModal from './lib/components/settings/CleanupPromptModal.svelte';
  import SyncPairModal from './lib/components/settings/SyncPairModal.svelte';
  import { startSyncListeners, syncStore } from './lib/syncStore.svelte';
  import DictationPill from './lib/components/layout/DictationPill.svelte';
  import Setup from './lib/views/Setup.svelte';
  import { getVersion, invoke, isTauriRuntime, listen } from './lib/tauri';
  import { startAutomaticUpdateChecks } from './lib/updates';
  import { startLocalSttListeners } from './lib/localSttStore.svelte';
  import { startLocalLlmListeners } from './lib/localLlmStore.svelte';
  import { startDownloadManagerListeners } from './lib/downloadManager.svelte';
  import { refreshTranscriptionModel } from './lib/transcriptionModelStore.svelte';
  import { startProviderStatusChecks, startApiHealthChecks } from './lib/serviceStatus';
  import { classifyIpcError, settingsSectionForKind, type ErrorKind } from './lib/errors';
  import type { SettingsSectionId } from './lib/settingsSections';
  import { scrollEdges } from './lib/scrollFade';
  import { fly } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { MOTION_MS, MOTION_PX, NAV_ORDER, SETTINGS_SECTION_ORDER, directionFromOrder, motionMs, motionPx, pageSwap, reducedMotionEnabled } from './lib/motion';

  type EffectiveTheme = 'light' | 'dark';
  type NativeTitleBarMetrics = { height: number; leftInset: number; rightInset: number; scaleFactor: number };

  function applyNativeTitleBarMetrics(metrics: NativeTitleBarMetrics | null) {
    // Browser dev mode and the test harness have no Windows caption; skip
    // silently instead of logging a crash for a missing metrics payload.
    if (!metrics) return;
    const root = document.documentElement;
    root.style.setProperty('--native-titlebar-height', `${metrics.height}px`);
    root.style.setProperty('--native-caption-left-inset', `${metrics.leftInset}px`);
    root.style.setProperty('--native-caption-right-inset', `${metrics.rightInset}px`);
  }

  function systemTheme(): EffectiveTheme {
    return window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }

  function effectiveTheme(mode: 'system' | 'light' | 'dark'): EffectiveTheme {
    return mode === 'system' ? systemTheme() : mode;
  }

  function applyTheme() {
    const theme = effectiveTheme(appStore.appearanceMode);
    document.documentElement.dataset.theme = theme;
  }

  $effect(() => {
    appStore.appearanceMode;
    if (typeof document !== 'undefined') applyTheme();
  });

  // Error toast
  let errorToast = $state('');
  let errorToastKind = $state<ErrorKind | null>(null);
  let toastTimer: ReturnType<typeof setTimeout>;
  let pageDir = $state<1 | -1>(1);
  let prevPage = $state<string>('home');
  let contentEl = $state<HTMLDivElement | null>(null);
  let fadeTop = $state(false);
  let fadeBottom = $state(false);

  $effect(() => {
    const next = appStore.currentPage;
    pageDir = directionFromOrder(prevPage, next, NAV_ORDER);
    prevPage = next;
  });

  $effect(() => {
    appStore.currentPage;
    requestAnimationFrame(() => {
      contentEl?.scrollTo({ top: 0, behavior: reducedMotionEnabled() ? 'auto' : 'smooth' });
    });
  });

  $effect(() => {
    if (
      !appStore.legacyFeaturesEnabled &&
      (appStore.currentPage === 'dictionary' || appStore.currentPage === 'snippets')
    ) {
      appStore.currentPage = 'contexts';
    } else if (appStore.legacyFeaturesEnabled && appStore.currentPage === 'contexts') {
      appStore.currentPage = 'home';
    }
  });

  // The wizard is the first thing in the app and unmounts when it finishes;
  // without this, focus drops to <body> and a keyboard user lands nowhere.
  // Land on the first rail entry (Home) only on a real completion transition
  // (wizard → app), so a normal app start never gets an unexpected focus ring.
  let prevSetupComplete = appStore.setupComplete;
  $effect(() => {
    const complete = appStore.setupComplete;
    if (prevSetupComplete === false && complete === true) {
      requestAnimationFrame(() => {
        document.querySelector<HTMLElement>('.sidebar .nav-item')?.focus();
      });
    }
    prevSetupComplete = complete;
  });

  async function pingConnectivity() {
    try {
      const online = await invoke<boolean>('check_connectivity');
      appStore.isOnline = online;
    } catch {
      appStore.isOnline = false;
    }
  }

  function openNotificationDestination(destination: string) {
    if (destination === 'models') {
      appStore.settingsAnimDir = directionFromOrder(
        appStore.settingsSection,
        'models',
        SETTINGS_SECTION_ORDER,
      );
      appStore.settingsSection = 'models';
      appStore.settingsOpen = true;
      return;
    }

    appStore.settingsOpen = false;
    appStore.currentPage = 'home';
  }

  function openSettingsSection(section: SettingsSectionId) {
    appStore.settingsAnimDir = directionFromOrder(
      appStore.settingsSection,
      section,
      SETTINGS_SECTION_ORDER,
    );
    appStore.settingsSection = section;
    appStore.settingsOpen = true;
    errorToast = '';
    errorToastKind = null;
    clearTimeout(toastTimer);
  }

  onMount(() => {
    let mounted = true;
    let cleanupFn: (() => void) | undefined;
    let stopNotificationClickListener: (() => void) | undefined;
    let stopConnectivityRecheckListener: (() => void) | undefined;
    let stopSettingsImportedListener: (() => void) | undefined;
    let stopAutomaticUpdateChecks: (() => void) | undefined;
    let stopLocalSttListeners: (() => void) | undefined;
    let stopLocalLlmListeners: (() => void) | undefined;
    let stopDownloadManagerListeners: (() => void) | undefined;
    let stopProviderStatusChecks: (() => void) | undefined;
    let stopApiHealthChecks: (() => void) | undefined;
    let stopSyncListeners: (() => void) | undefined;
    let stopTitleBarMetricsListener: (() => void) | undefined;

    if (isWindows && isTauriRuntime()) {
      invoke<NativeTitleBarMetrics>('get_native_titlebar_metrics')
        .then(applyNativeTitleBarMetrics)
        .catch((error) => console.error('Failed to read native title bar metrics:', error));
      listen<NativeTitleBarMetrics>('verenu:native-titlebar-metrics', (event) => applyNativeTitleBarMetrics(event.payload))
        .then((unlisten) => { stopTitleBarMetricsListener = unlisten; })
        .catch((error) => console.error('Failed to listen for native title bar metrics:', error));
    }

    // Re-reads the settings the app mirrors globally. Called once on mount and
    // again whenever a backup import lands, so an import can't leave these
    // stores disagreeing with what import_data actually wrote to disk.
    async function reloadGlobalSettings() {
      try {
        const [done, appearance, forceSetupOnLaunch, cleanupEnabled, betaUpdatesEnabled, legacyFeaturesEnabled, syncEnabled] = await Promise.all([
          invoke<boolean | null>('get_setting', { key: 'setup_complete' }),
          invoke<'system' | 'light' | 'dark' | null>('get_setting', { key: 'appearance_mode' }),
          invoke<boolean | null>('get_setting', { key: 'force_setup_on_launch' }),
          invoke<boolean | null>('get_setting', { key: 'cleanup_enabled' }),
          invoke<boolean | null>('get_setting', { key: 'beta_updates_enabled' }),
          invoke<boolean | null>('get_setting', { key: 'legacy_features_enabled' }),
          invoke<boolean | null>('get_setting', { key: 'sync_enabled' }),
        ]);
        appStore.setupComplete = forceSetupOnLaunch ? false : done === true;
        if (appearance === 'light' || appearance === 'dark' || appearance === 'system') {
          appStore.appearanceMode = appearance;
        }
        appStore.cleanupEnabled = cleanupEnabled ?? true;
        appStore.betaUpdatesEnabled = betaUpdatesEnabled ?? false;
        appStore.legacyFeaturesEnabled = legacyFeaturesEnabled ?? false;
        appStore.syncEnabled = syncEnabled ?? false;
      } catch {
        appStore.setupComplete = false;
      }
    }

    (async () => {
      await reloadGlobalSettings();
      if (!mounted) return;
      if (appStore.syncEnabled) {
        try { stopSyncListeners = startSyncListeners(); }
        catch (error) { console.error('Failed to start sync listeners:', error); }
      }

      const unlisten = await listen<string>('verenu:error', (ev) => {
        const raw = ev.payload ?? '';
        const classified = classifyIpcError(raw);
        errorToast = raw ? classified.message : 'Something went wrong';
        errorToastKind = raw ? classified.kind : null;
        clearTimeout(toastTimer);
        toastTimer = setTimeout(() => { errorToast = ''; errorToastKind = null; }, 5000);
      });
      cleanupFn = unlisten;
    })();

    listen('verenu:settings-imported', () => {
      void reloadGlobalSettings();
    })
      .then((unlisten) => {
        if (!mounted) {
          unlisten();
          return;
        }
        stopSettingsImportedListener = unlisten;
      })
      .catch((error) => { console.warn('Failed to listen for settings-imported events:', error); });

    listen<string>('verenu:notification-clicked', (event) => {
      openNotificationDestination(event.payload);
    })
      .then((unlisten) => {
        if (!mounted) {
          unlisten();
          return;
        }
        stopNotificationClickListener = unlisten;
      })
      .catch((error) => { console.warn('Failed to listen for notification clicks:', error); });

    // The backend fires this after a transcription request fails with a
    // connection error and it has actively confirmed the connection is down —
    // re-ping immediately so the persistent offline toast shows up now instead
    // of on the next 60s interval.
    listen('verenu:recheck-connectivity', () => void pingConnectivity())
      .then((unlisten) => {
        if (!mounted) {
          unlisten();
          return;
        }
        stopConnectivityRecheckListener = unlisten;
      })
      .catch((error) => { console.warn('Failed to listen for connectivity rechecks:', error); });

    // Synchronous: startAutomaticUpdateChecks fires its first check in the
    // background and returns the cleanup immediately, so there's no unmount
    // race to guard and the interval is always registered before we return.
    try {
      stopAutomaticUpdateChecks = startAutomaticUpdateChecks();
    } catch (error) {
      console.error('Failed to start automatic update checks:', error);
    }

    try {
      stopLocalSttListeners = startLocalSttListeners();
      stopLocalLlmListeners = startLocalLlmListeners();
      stopDownloadManagerListeners = startDownloadManagerListeners();
      stopProviderStatusChecks = startProviderStatusChecks();
      stopApiHealthChecks = startApiHealthChecks();
    } catch (error) {
      console.error('Failed to start listeners and status checks:', error);
    }

    refreshTranscriptionModel().catch((error) => {
      console.error('Failed to load transcription model:', error);
    });

    // Shown in the sidebar footer and About; fetched once here since both read it.
    getVersion()
      .then((version) => { appStore.appVersion = version; })
      .catch((error) => { console.error('Failed to read app version:', error); });

    const media = window.matchMedia?.('(prefers-color-scheme: dark)');
    const onSystemThemeChange = () => {
      if (appStore.appearanceMode === 'system') applyTheme();
    };
    media?.addEventListener?.('change', onSystemThemeChange);

    pingConnectivity();
    let connectivityTimer = setInterval(pingConnectivity, 60_000);
    const handleVisibility = () => {
      clearInterval(connectivityTimer);
      if (!document.hidden) {
        pingConnectivity();
        connectivityTimer = setInterval(pingConnectivity, 60_000);
      }
    };
    document.addEventListener('visibilitychange', handleVisibility);

    return () => {
      mounted = false;
      if (cleanupFn) cleanupFn();
      if (stopNotificationClickListener) stopNotificationClickListener();
      if (stopConnectivityRecheckListener) stopConnectivityRecheckListener();
      if (stopSettingsImportedListener) stopSettingsImportedListener();
      if (stopAutomaticUpdateChecks) stopAutomaticUpdateChecks();
      if (stopLocalSttListeners) stopLocalSttListeners();
      if (stopLocalLlmListeners) stopLocalLlmListeners();
      if (stopDownloadManagerListeners) stopDownloadManagerListeners();
      if (stopProviderStatusChecks) stopProviderStatusChecks();
      if (stopApiHealthChecks) stopApiHealthChecks();
      if (stopSyncListeners) stopSyncListeners();
      if (stopTitleBarMetricsListener) stopTitleBarMetricsListener();
      media?.removeEventListener?.('change', onSystemThemeChange);
      clearInterval(connectivityTimer);
      document.removeEventListener('visibilitychange', handleVisibility);
    };
  });
</script>

<div class="app" class:app-windows={isWindows}>
  {#if appStore.setupComplete === false}
    <Setup />
  {/if}
  <div class="body" inert={appStore.setupComplete === false}>
    <Sidebar />
    <div class="content-fade content-fade-top" class:visible={fadeTop && !appStore.settingsOpen} aria-hidden="true"></div>
    <div class="content-fade content-fade-bottom" class:visible={fadeBottom && !appStore.settingsOpen} aria-hidden="true"></div>
    <div
      class="content scroll-styled"
      class:content-behind={appStore.settingsOpen}
      inert={appStore.settingsOpen}
      bind:this={contentEl}
      use:scrollEdges={(top, bottom) => { fadeTop = top; fadeBottom = bottom; }}
    >
      {#key appStore.currentPage}
        <div
          class="page-wrapper"
          in:pageSwap={{ axis: 'y', distance: pageDir * motionPx(MOTION_PX.page), duration: motionMs(MOTION_MS.panel) }}
          out:pageSwap={{ axis: 'y', distance: -pageDir * motionPx(MOTION_PX.page), duration: motionMs(MOTION_MS.base + 40) }}
        >
          {#if appStore.currentPage === 'home'}
            <Home />
          {:else if appStore.currentPage === 'insights'}
            <Insights />
          {:else if appStore.currentPage === 'contexts'}
            <Contexts />
          {:else if appStore.currentPage === 'dictionary'}
            <Dictionary />
          {:else if appStore.currentPage === 'snippets'}
            <Snippets />
          {:else if appStore.currentPage === 'style'}
            <Style />
          {/if}
        </div>
      {/key}
    </div>
  </div>
  <Settings />
  {#if cleanupPromptEditor.open}
    <CleanupPromptModal />
  {/if}
  {#if syncStore.status?.pairing?.kind === 'incoming' && syncStore.status.pairing.phase !== 'failed'}
    <SyncPairModal />
  {/if}
  <DictationPill />

  {#if errorToast}
    <div class="error-toast" role="alert" style:bottom={!appStore.isOnline ? '66px' : '18px'}>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="flex-shrink:0">
        <circle cx="12" cy="12" r="10"/><path d="M12 8v4M12 16h.01"/>
      </svg>
      <span>{errorToast}</span>
      {#if errorToastKind && settingsSectionForKind(errorToastKind)}
        <button class="toast-action ui-focus-ring" onclick={() => openSettingsSection(settingsSectionForKind(errorToastKind!)!)}>
          Fix in Settings
        </button>
      {/if}
      <button class="toast-close ui-focus-ring" onclick={() => { errorToast = ''; errorToastKind = null; clearTimeout(toastTimer); }}>✕</button>
    </div>
  {/if}
  {#if !appStore.isOnline}
    <div class="offline-toast" role="status" transition:fly={{ y: 6, duration: motionMs(180), easing: expoOut }}>
      <span class="offline-dot"></span>
      No internet connection
    </div>
  {/if}
</div>

<style>
  :global(*) {
    box-sizing: border-box;
  }

  :global(html, body) {
    margin: 0;
    padding: 0;
    overflow: hidden;
  }

  :global(body) {
    font-family: var(--sans);
    color: var(--ink-soft);
    background: var(--paper);
    font-size: 13.5px;
    line-height: 1.5;
    -webkit-font-smoothing: antialiased;
    font-feature-settings: 'ss01', 'cv11';
  }

  :global(button) {
    font-family: inherit;
    cursor: pointer;
  }

  .app {
    width: 100%;
    height: 100vh;
    background: var(--paper);
    display: flex;
    flex-direction: column;
    font-family: var(--sans);
    position: relative;
  }

  /* The native Windows caption is non-client chrome, so it does not consume
     space inside this app shell. Keep Windows pages close to that boundary
     without changing the established macOS page rhythm. */
  .app.app-windows {
    --page-pad-y: calc(var(--native-titlebar-height, 32px) + 10px);
  }

  .body {
    flex: 1;
    display: flex;
    min-height: 0;
    padding: 0 0 var(--app-gutter) 0;
    gap: var(--app-gutter);
    position: relative;
  }

  /*
   * Soft top/bottom scroll fades over the main content column (same treatment as
   * the settings panel). Geometry mirrors the content region: it starts to the
   * right of the sidebar and stops short of the scrollbar gutter and the bottom
   * gutter. They fade to the page background and only appear when there's more
   * to scroll in that direction — and never while settings covers the content.
   */
  .content-fade {
    position: absolute;
    left: calc(var(--sidebar-w) + var(--app-gutter));
    right: var(--scrollbar-w, 0);
    height: 30px;
    pointer-events: none;
    z-index: 5;
    opacity: 0;
    transition: opacity 180ms ease;
  }

  .content-fade.visible { opacity: 1; }

  .content-fade-top {
    top: 0;
    background: linear-gradient(to bottom, var(--paper), transparent);
  }

  .content-fade-bottom {
    bottom: var(--app-gutter);
    background: linear-gradient(to top, var(--paper), transparent);
  }

  @media (prefers-reduced-motion: reduce) {
    .content-fade { transition: none; }
  }

  .content {
    flex: 1;
    background: transparent;
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-gutter: stable;
    position: relative;
    display: grid;
    justify-items: center;
    min-width: 0;
  }

  .content::-webkit-scrollbar-thumb { border: 3px solid var(--paper); }

  /*
   * Opening settings is a page change, not a panel appearing over a frozen
   * page: the current view rises and fades out with the same vocabulary the
   * Home/Dictionary/Style swaps use, while the settings page enters beneath the
   * fading wash. Without this the underlying page just sat there and the
   * transition read as nothing happening.
   */
  .content {
    /* Both properties on one curve — cubic-bezier(0.33, 1, 0.68, 1) is the CSS
       form of cubicOut, matching pageSwap. Opacity was on `ease` before, which
       is what made the exit read as slightly out of step with the movement.
       Keep --content-swap-y in sync with SETTINGS_SWAP_PX in Settings.svelte. */
    transition:
      opacity var(--content-swap-ms) cubic-bezier(0.33, 1, 0.68, 1),
      transform var(--content-swap-ms) cubic-bezier(0.33, 1, 0.68, 1);
    --content-swap-ms: 320ms;
    --content-swap-y: 26px;
  }

  .content.content-behind {
    opacity: 0;
    transform: translate3d(0, calc(var(--content-swap-y) * -1), 0);
    pointer-events: none;
  }

  @media (prefers-reduced-motion: reduce) {
    .content {
      --content-swap-ms: 190ms;
      --content-swap-y: 10px;
    }
  }

  .page-wrapper {
    grid-area: 1 / 1;
    width: 100%;
    max-width: 100%;
    min-width: 0;
    min-height: calc(100% + 1px);
    padding-right: 14px;
  }

  .error-toast {
    position: absolute;
    bottom: 18px;
    left: 50%;
    transform: translateX(-50%);
    background: var(--danger-bg);
    border: 1px solid var(--danger-line);
    border-radius: var(--r-sm);
    padding: 9px 14px;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--danger);
    box-shadow: var(--shadow-popover);
    z-index: 20;
    max-width: 480px;
    animation: toastIn 0.18s cubic-bezier(0.22, 1, 0.36, 1);
    transition: bottom 0.15s ease;
  }

  @keyframes toastIn {
    from { opacity: 0; transform: translateX(-50%) translateY(6px); }
    to   { opacity: 1; transform: translateX(-50%) translateY(0); }
  }

  .toast-close {
    background: transparent;
    border: none;
    color: var(--danger);
    opacity: 0.6;
    font-size: 11px;
    cursor: pointer;
    margin-left: 4px;
    padding: 0;
    line-height: 1;
  }
  .toast-close:hover { opacity: 1; }

  .toast-action {
    background: transparent;
    border: 1px solid var(--danger-line);
    border-radius: 6px;
    color: var(--danger);
    font-size: 11.5px;
    font-weight: 600;
    padding: 2px 9px;
    cursor: pointer;
    margin-left: 6px;
    flex-shrink: 0;
    transition: background 0.12s;
  }
  .toast-action:hover { background: var(--danger-bg); }

  .offline-toast {
    position: absolute;
    bottom: 18px;
    left: 0;
    right: 0;
    margin-inline: auto;
    width: fit-content;
    background: var(--danger-bg);
    border: 1px solid var(--danger-line);
    border-radius: var(--r-sm);
    padding: 9px 14px;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--danger);
    box-shadow: var(--shadow-popover);
    z-index: 20;
    max-width: 480px;
  }

  .offline-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: currentColor;
    flex-shrink: 0;
    animation: dot-pulse 2s ease-in-out infinite;
  }

  @keyframes dot-pulse {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.35; }
  }

  @media (max-width: 720px) {
    .app { --sidebar-w: 58px; }
  }
</style>
