<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke, getVersion, listen } from '../tauri';
  import { appStore } from '../stores';
  import { saveSetting } from '../settings';
  import { formatKeyLabel, defaultHotkey } from '../platform';
  import { getGreeting, HISTORY_PAGE_SIZE, type Entry, type Stats } from './home/helpers';
  import HomeHero from './home/HomeHero.svelte';
  import UpdateBanner from './home/UpdateBanner.svelte';
  import ProviderStatusBanner from './home/ProviderStatusBanner.svelte';
  import GlobalMessageBanner from './home/GlobalMessageBanner.svelte';
  import HistoryList from './home/HistoryList.svelte';
  import StatsCard from './home/StatsCard.svelte';

  let hotkey = defaultHotkey;
  $: hk1 = formatKeyLabel(hotkey[0]);
  $: hk2 = formatKeyLabel(hotkey[1]);

  let copiedId: number | null = null;
  let currentVersion = '';
  let installing = false;
  let failedEntry: { created_at: string } | null = null;
  let retrying = false;
  let failedTimer: ReturnType<typeof setTimeout> | null = null;
  let cancelledEntry: { created_at: string } | null = null;
  let resumingCancelled = false;
  let cancelledTimer: ReturnType<typeof setTimeout> | null = null;

  const greeting = getGreeting();

  let recents: Entry[] = [];
  let stats: Stats = { total_words: 0, avg_wpm: 0, day_streak: 0 };
  let loading = true;
  let loadingMore = false;
  let hasMoreHistory = false;

  // History filters are session-only. Keep the debounce/load sequence here so
  // pagination and filter changes cannot overwrite each other with stale
  // responses.
  let search = '';
  let debouncedSearch = '';
  let appFilter: string | null = null;
  let apps: string[] = [];
  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  let loadSeq = 0;

  function handleSearchChange(value: string) {
    search = value;
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => {
      searchTimer = null;
      if (debouncedSearch !== value) {
        debouncedSearch = value;
        load(true);
      }
    }, 120);
  }

  function handleAppFilterChange(app: string | null) {
    if (appFilter === app) return;
    if (searchTimer) {
      clearTimeout(searchTimer);
      searchTimer = null;
      debouncedSearch = search;
    }
    appFilter = app;
    load(true);
  }

  function resetHistoryFilters(): boolean {
    const had = search !== '' || debouncedSearch !== '' || appFilter !== null;
    if (!had) return false;
    search = '';
    debouncedSearch = '';
    appFilter = null;
    if (searchTimer) {
      clearTimeout(searchTimer);
      searchTimer = null;
    }
    return true;
  }

  function clearHistoryFilters() {
    if (resetHistoryFilters()) load(true);
  }

  async function retryTranscription() {
    if (retrying) return;
    retrying = true;
    try {
      await invoke('retry_transcription');
      // success: verenu:transcribed listener clears failedEntry and calls load()
    } catch (err) {
      console.error('Retry failed:', err);
      // keep failedEntry so user can try again
    } finally {
      retrying = false;
    }
  }

  function clearCancelledEntry() {
    cancelledEntry = null;
    if (cancelledTimer) {
      clearTimeout(cancelledTimer);
      cancelledTimer = null;
    }
  }

  async function continueCancelled() {
    if (resumingCancelled) return;
    resumingCancelled = true;
    try {
      await invoke('resume_cancelled_capture');
      // Rust also emits verenu:cancelled-capture-cleared, but clear locally
      // too so the banner drops immediately rather than waiting on the event.
      clearCancelledEntry();
    } catch (err) {
      console.error('Resume cancelled capture failed:', err);
      // keep cancelledEntry so the user can try again
    } finally {
      resumingCancelled = false;
    }
  }

  async function dismissCancelled() {
    clearCancelledEntry();
    try {
      await invoke('dismiss_cancelled_capture');
    } catch { /* dev mode */ }
  }

  async function copyText(entry: Entry) {
    try {
      await navigator.clipboard.writeText(entry.clean_text);
      copiedId = entry.id;
      setTimeout(() => { copiedId = null; }, 1500);
    } catch { /* clipboard not available in dev */ }
  }

  async function load(reset = true) {
    const seq = ++loadSeq;
    const nextOffset = reset ? 0 : recents.length;
    try {
      const [r, s] = await Promise.all([
        invoke<Entry[]>('get_recent', {
          limit: HISTORY_PAGE_SIZE,
          offset: nextOffset,
          search: debouncedSearch.trim() || undefined,
          appName: appFilter ?? undefined,
        }),
        reset ? invoke<Stats>('get_stats') : Promise.resolve(stats),
      ]);
      if (seq !== loadSeq) return;
      recents = reset ? (r ?? []) : [...recents, ...(r ?? [])];
      stats = s;
      hasMoreHistory = (r?.length ?? 0) === HISTORY_PAGE_SIZE;
    } catch (err) {
      if (seq !== loadSeq) return;
      console.error('Home load failed:', err);
      if (reset) {
        recents = [];
        stats = { total_words: 0, avg_wpm: 0, day_streak: 0 };
        hasMoreHistory = false;
      }
    }
    if (reset) loading = false;
  }

  async function loadOlder() {
    if (loadingMore || !hasMoreHistory) return;
    loadingMore = true;
    try {
      await load(false);
    } finally {
      loadingMore = false;
    }
  }

  async function handleInstall() {
    if (!appStore.updateInfo) return;
    installing = true;
    try {
      await invoke('install_update', { downloadUrl: appStore.updateInfo.downloadUrl });
    } catch (e) {
      console.error('Install failed:', e);
    } finally {
      installing = false;
    }
  }

  async function dismissUpdate() {
    if (!appStore.updateInfo) return;
    try {
      await saveSetting('update_dismissed_version', appStore.updateInfo.version);
    } catch { /* dev mode */ }
    appStore.updateInfo = null;
  }

  onMount(() => {
    getVersion().then(v => currentVersion = v);
    invoke<string[] | null>('get_history_apps')
      .then(list => { apps = list ?? []; })
      .catch(() => { apps = []; });
    invoke<string[] | null>('get_setting', { key: 'hotkey' })
      .then(hk => { if (hk?.length === 2) hotkey = hk; })
      .catch(() => { /* use platform default if setting unavailable */ });
    load();

    let mounted = true;
    const unlisteners: (() => void)[] = [];

    function trackListener(promise: Promise<() => void>) {
      promise
        .then((cleanup) => {
          if (!mounted) {
            cleanup();
            return;
          }
          unlisteners.push(cleanup);
        })
        .catch(() => {});
    }

    trackListener(listen('verenu:transcribed', () => {
      failedEntry = null;
      if (failedTimer) { clearTimeout(failedTimer); failedTimer = null; }
      load(true);
    }));

    trackListener(listen<Entry>('verenu:history-updated', (ev) => {
      failedEntry = null;
      if (failedTimer) { clearTimeout(failedTimer); failedTimer = null; }
      recents = [ev.payload, ...recents.filter((entry) => entry.id !== ev.payload.id)];
      invoke<Stats>('get_stats').then((nextStats) => { stats = nextStats; }).catch(() => {});
    }));

    trackListener(listen('verenu:history-pruned', () => load(true)));

    trackListener(listen<string>('verenu:pipeline-failed', (ev) => {
      failedEntry = { created_at: ev.payload };
      if (failedTimer) clearTimeout(failedTimer);
      failedTimer = setTimeout(() => {
        failedEntry = null;
        failedTimer = null;
      }, 10 * 60 * 1000);
    }));

    trackListener(listen<string>('verenu:cancelled-capture', (ev) => {
      cancelledEntry = { created_at: ev.payload };
      if (cancelledTimer) clearTimeout(cancelledTimer);
      cancelledTimer = setTimeout(() => {
        cancelledEntry = null;
        cancelledTimer = null;
      }, 10 * 60 * 1000);
    }));

    // Fired when the capture is resumed or dismissed from elsewhere (the
    // pill's own buttons) — drop the banner if it's still showing.
    trackListener(listen('verenu:cancelled-capture-cleared', () => {
      clearCancelledEntry();
    }));

    return () => {
      mounted = false;
      while (unlisteners.length > 0) {
        unlisteners.pop()?.();
      }
      if (failedTimer) {
        clearTimeout(failedTimer);
        failedTimer = null;
      }
      if (cancelledTimer) {
        clearTimeout(cancelledTimer);
        cancelledTimer = null;
      }
      if (searchTimer) {
        clearTimeout(searchTimer);
        searchTimer = null;
      }
    };
  });

  // Settings is an overlay over Home; do not leave filters active behind it.
  $: if (appStore.settingsOpen) {
    if (resetHistoryFilters()) load(true);
  }
</script>

<div class="content-inner">
  <div class="home-grid">
    <!-- Left column -->
    <div>
      <h1 class="page-h">Welcome back</h1>
      <p class="page-sub">{greeting}</p>

      <HomeHero {hk1} {hk2} />

      {#if appStore.globalMessage}
        <GlobalMessageBanner message={appStore.globalMessage.message} />
      {/if}

      {#if appStore.providerStatusAlerts.length > 0}
        <ProviderStatusBanner alerts={appStore.providerStatusAlerts} />
      {:else if appStore.updateInfo}
        <UpdateBanner
          {currentVersion}
          updateInfo={appStore.updateInfo}
          {installing}
          onInstall={handleInstall}
          onDismiss={dismissUpdate}
        />
      {/if}

      <HistoryList
        {recents}
        {failedEntry}
        {cancelledEntry}
        {loading}
        {hasMoreHistory}
        {loadingMore}
        {retrying}
        {resumingCancelled}
        {copiedId}
        {hk1}
        {hk2}
        {search}
        {apps}
        {appFilter}
        onSearchChange={handleSearchChange}
        onAppFilterChange={handleAppFilterChange}
        onClearFilters={clearHistoryFilters}
        onRetry={retryTranscription}
        onContinueCancelled={continueCancelled}
        onDismissCancelled={dismissCancelled}
        onLoadOlder={loadOlder}
        onCopy={copyText}
      />
    </div>

    <!-- Right column — flat stats -->
    <div class="stat-stack">
      <StatsCard {stats} />
    </div>
  </div>
</div>

<style>
  .content-inner {
    width: min(100%, var(--page-max));
    margin-inline: auto;
    padding: var(--page-pad-y) var(--page-pad-x) 36px;
    min-width: 0;
  }

  .home-grid {
    display: grid;
    grid-template-columns: minmax(540px, 1fr) minmax(220px, 280px);
    gap: clamp(18px, 3vw, 32px);
    align-items: start;
  }

  .home-grid > div {
    min-width: 0;
  }

  .page-h {
    font-family: var(--serif);
    font-size: 26px;
    font-weight: 500;
    letter-spacing: -0.02em;
    margin: 0 0 4px;
    line-height: 1.1;
    color: var(--ink);
  }

  .page-sub { color: var(--ink-mute); font-size: 12.5px; margin: 0 0 22px; }

  /* Flat stats */
  .stat-stack { display: flex; flex-direction: column; gap: 22px; }

  @media (max-width: 1060px) {
    .home-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
