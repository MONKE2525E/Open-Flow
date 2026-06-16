<script lang="ts">
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';
  import { flip } from 'svelte/animate';
  import { expoOut } from 'svelte/easing';
  import { invoke, getVersion, listen } from '../tauri';
  import { icons } from '../icons';
  import { appStore, type UpdateInfo } from '../stores';
  import { saveSetting } from '../settings';
  import { formatKeyLabel, defaultHotkey } from '../platform';

  let hotkey = defaultHotkey;
  $: hk1 = formatKeyLabel(hotkey[0]);
  $: hk2 = formatKeyLabel(hotkey[1]);

  interface Entry { id: number; clean_text: string; words: number; created_at: string; }
  interface Stats { total_words: number; avg_wpm: number; day_streak: number; }

  let copiedId: number | null = null;
  let currentVersion = '';
  let installing = false;
  let failedEntry: { created_at: string } | null = null;
  let retrying = false;
  let failedTimer: ReturnType<typeof setTimeout> | null = null;
  let unlistenFailed: (() => void) | undefined;

  function getGreeting(): string {
    const now = new Date();
    const h = now.getHours();
    // Days since epoch — avoids the obvious 7-day weekday cycle
    const seed = Math.floor(now.getTime() / 86_400_000);

    const pick = (msgs: string[]) => msgs[seed % msgs.length];

    if (h >= 5 && h < 12) {
      return pick([
        'Good morning.',
        'Morning — ready to roll.',
        'Early start. Let\'s get into it.',
        'Morning. Coffee first, then dictation.',
        'Rise and grind.',
        'Big day ahead?',
        'Morning. Let\'s make it count.',
        'Up and at it.',
        'Another day, another wall of text.',
        'Morning. What\'s on the agenda?',
        'Fresh start. Let\'s go.',
        'Good morning. The day\'s yours.',
      ]);
    } else if (h >= 12 && h < 17) {
      return pick([
        'Good afternoon.',
        'Afternoon. Keep the momentum.',
        'Halfway through — still going.',
        'Afternoon grind. Let\'s go.',
        'Still going strong?',
        'Post-lunch slump? Push through.',
        'Afternoon. Knock out the list.',
        'How\'s the day treating you?',
        'Deep work hour. Let\'s make it count.',
        'Head down, get it done.',
        'Afternoon. The finish line\'s in sight.',
        'Good afternoon. Lot left to do?',
      ]);
    } else if (h >= 17 && h < 21) {
      return pick([
        'Good evening.',
        'Wrapping things up?',
        'Almost done for the day.',
        'Evening — one last push.',
        'How\'d the day go?',
        'Winding down? Get those last thoughts out.',
        'Evening mode.',
        'End of day. Finish strong.',
        'Evening. You made it.',
        'Tying up loose ends?',
        'Good evening. Almost there.',
        'Last stretch of the day.',
      ]);
    } else {
      return pick([
        'Working late?',
        'Burning the midnight oil.',
        'Still at it. Respect.',
        'Late night session.',
        'Night owl mode.',
        'The quiet hours hit different.',
        'Up late. You\'ve got this.',
        'Late night. Make it count.',
        'Can\'t sleep, or just in the zone?',
        'Night shift. Let\'s go.',
        'Everyone else is asleep. Your move.',
        'Late night grind. Respect.',
      ]);
    }
  }

  const greeting = getGreeting();

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

  async function copyText(entry: Entry) {
    try {
      await navigator.clipboard.writeText(entry.clean_text);
      copiedId = entry.id;
      setTimeout(() => { copiedId = null; }, 1500);
    } catch { /* clipboard not available in dev */ }
  }

  let recents: Entry[] = [];
  let stats: Stats = { total_words: 0, avg_wpm: 0, day_streak: 0 };
  let loading = true;

  function parseTimestamp(value: string): Date {
    if (value.includes('T')) {
      return new Date(value.endsWith('Z') ? value : `${value}Z`);
    }
    return new Date(value.replace(' ', 'T') + 'Z');
  }

  function fmtTime(iso: string) {
    try {
      return parseTimestamp(iso).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
    } catch { return iso; }
  }

  function fmtDate(iso: string) {
    try {
      const d = parseTimestamp(iso);
      const today = new Date();
      const yesterday = new Date(today); yesterday.setDate(today.getDate() - 1);
      if (d.toDateString() === today.toDateString()) return 'Today';
      if (d.toDateString() === yesterday.toDateString()) return 'Yesterday';
      return d.toLocaleDateString([], { weekday: 'short', month: 'short', day: 'numeric' });
    } catch { return iso.slice(0, 10); }
  }

  type RenderItem =
    | { type: 'header'; key: string; label: string }
    | { type: 'row'; key: string; entry: Entry };

  let flatItems: RenderItem[] = [];
  $: {
    const seenHeaders = new Set<string>();
    flatItems = recents.reduce<RenderItem[]>((acc, entry) => {
      const label = fmtDate(entry.created_at);
      if (!seenHeaders.has(label)) {
        seenHeaders.add(label);
        if (!(failedEntry && label === 'Today')) {
          acc.push({ type: 'header', label, key: `header-${label}` });
        }
      }
      acc.push({ type: 'row', entry, key: `row-${entry.id}` });
      return acc;
    }, []);
  }

  let container: HTMLElement | null = null;
  let cachedHeights: Record<string, number> = {};
  
  let visibleItems: { item: RenderItem; index: number }[] = [];
  let topSpacerHeight = 0;
  let bottomSpacerHeight = 0;
  let scrollTop = 0;
  let clientHeight = 600;

  let tops: number[] = [];
  let totalHeight = 0;

  function updateLayout() {
    tops = [];
    let currentTop = 0;
    for (let i = 0; i < flatItems.length; i++) {
      tops.push(currentTop);
      const item = flatItems[i];
      const h = cachedHeights[item.key] || (item.type === 'header' ? 38 : 58);
      currentTop += h;
    }
    totalHeight = currentTop;
  }

  function updateVirtualList() {
    if (!container || flatItems.length === 0) {
      visibleItems = [];
      topSpacerHeight = 0;
      bottomSpacerHeight = 0;
      return;
    }
    scrollTop = container.scrollTop;
    clientHeight = container.clientHeight;

    const buffer = 400; // scroll buffer (px)
    const startY = Math.max(0, scrollTop - buffer);
    const endY = scrollTop + clientHeight + buffer;

    let start = 0;
    let end = flatItems.length;

    // Binary search for start index (first item ending after startY)
    let low = 0;
    let high = flatItems.length - 1;
    while (low <= high) {
      const mid = (low + high) >> 1;
      const top = tops[mid];
      const h = cachedHeights[flatItems[mid].key] || (flatItems[mid].type === 'header' ? 38 : 58);
      if (top + h >= startY) {
        start = mid;
        high = mid - 1;
      } else {
        low = mid + 1;
      }
    }

    // Binary search for end index (first item starting after endY)
    low = start;
    high = flatItems.length - 1;
    while (low <= high) {
      const mid = (low + high) >> 1;
      if (tops[mid] > endY) {
        end = mid;
        high = mid - 1;
      } else {
        low = mid + 1;
      }
    }

    start = Math.max(0, Math.min(start, flatItems.length));
    end = Math.max(start, Math.min(end, flatItems.length));

    visibleItems = flatItems.slice(start, end).map((item, idx) => ({
      item,
      index: start + idx
    }));

    topSpacerHeight = tops[start] || 0;
    bottomSpacerHeight = totalHeight - (end < flatItems.length ? tops[end] : totalHeight);
  }

  $: {
    flatItems;
    updateLayout();
    updateVirtualList();
  }

  function handleScroll() {
    updateVirtualList();
  }

  let sharedObserver: ResizeObserver | null = null;
  const nodeKeys = new WeakMap<HTMLElement, string>();

  function getSharedObserver() {
    if (!sharedObserver && typeof ResizeObserver !== 'undefined') {
      sharedObserver = new ResizeObserver((entries) => {
        let changed = false;
        for (const entry of entries) {
          const node = entry.target as HTMLElement;
          const key = nodeKeys.get(node);
          if (key) {
            const rect = node.getBoundingClientRect();
            if (rect.height > 0 && cachedHeights[key] !== rect.height) {
              cachedHeights[key] = rect.height;
              changed = true;
            }
          }
        }
        if (changed) {
          updateLayout();
          updateVirtualList();
        }
      });
    }
    return sharedObserver;
  }

  function measureItem(node: HTMLElement, key: string) {
    nodeKeys.set(node, key);
    const observer = getSharedObserver();
    if (observer) {
      observer.observe(node);
    }

    const rect = node.getBoundingClientRect();
    if (rect.height > 0 && cachedHeights[key] !== rect.height) {
      cachedHeights[key] = rect.height;
      updateLayout();
      updateVirtualList();
    }

    return {
      destroy() {
        if (observer) {
          observer.unobserve(node);
        }
        nodeKeys.delete(node);
      }
    };
  }

  async function load() {
    try {
      const [r, s] = await Promise.all([invoke<Entry[]>('get_recent'), invoke<Stats>('get_stats')]);
      recents = r;
      stats = s;
      setTimeout(() => {
        updateVirtualList();
      }, 0);
    } catch (err) {
      console.error('Home load failed:', err);
      recents = [];
      stats = { total_words: 0, avg_wpm: 0, day_streak: 0 };
    }
    loading = false;
  }

  function handleInstall() {
    if (!appStore.updateInfo) return;
    installing = true;
    invoke('install_update', { downloadUrl: appStore.updateInfo.downloadUrl }).catch((e) => {
      console.error('Install failed:', e);
      installing = false;
    });
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
    invoke<string[] | null>('get_setting', { key: 'hotkey' })
      .then(hk => { if (hk?.length === 2) hotkey = hk; })
      .catch(() => { /* use platform default if setting unavailable */ });
    load();

    container = document.querySelector('.content');
    if (container) {
      container.addEventListener('scroll', handleScroll);
    }
    window.addEventListener('resize', handleScroll);

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

    invoke<UpdateInfo | null>('check_for_update').then(async (update) => {
      if (update) {
        try {
          const dismissed = await invoke<string | null>('get_setting', { key: 'update_dismissed_version' });
          if (dismissed === update.version) return;
        } catch { /* dev mode */ }
        appStore.updateInfo = update;
      }
    }).catch(() => {});

    trackListener(listen('verenu:transcribed', () => {
      failedEntry = null;
      if (failedTimer) { clearTimeout(failedTimer); failedTimer = null; }
      load();
    }));

    trackListener(listen<string>('verenu:pipeline-failed', (ev) => {
      failedEntry = { created_at: ev.payload };
      if (failedTimer) clearTimeout(failedTimer);
      failedTimer = setTimeout(() => {
        failedEntry = null;
        failedTimer = null;
      }, 10 * 60 * 1000);
    }));

    return () => {
      mounted = false;
      if (container) {
        container.removeEventListener('scroll', handleScroll);
      }
      window.removeEventListener('resize', handleScroll);
      while (unlisteners.length > 0) {
        unlisteners.pop()?.();
      }
      if (failedTimer) {
        clearTimeout(failedTimer);
        failedTimer = null;
      }
    };
  });
</script>

<div class="content-inner">
  <div class="home-grid">
    <!-- Left column -->
    <div>
      <h1 class="page-h">Welcome back</h1>
      <p class="page-sub">{greeting}</p>

      <!-- Dark hero card -->
      <div class="hero-photo">
        <div class="hero-photo-content">
          <h2 class="hero-photo-title">
            Hold <kbd>{hk1}</kbd> <kbd>{hk2}</kbd> to dictate
          </h2>
          <p class="hero-photo-sub">
            Verenu works in any app. Try it in
            <em class="hero-em">email, messages, docs</em> — or anywhere else.
          </p>
        </div>
      </div>

      {#if appStore.updateInfo}
        <div class="notice-wrap">
          <div class="update-banner">
            <span class="update-text">
              Update available — v{currentVersion} → v{appStore.updateInfo.version}
            </span>
            <div class="update-actions">
              <button class="update-dismiss" onclick={dismissUpdate}>Dismiss</button>
              <button class="update-btn" onclick={handleInstall} disabled={installing}>
                {installing ? 'Installing…' : 'Install & Restart'}
              </button>
            </div>
          </div>
        </div>
      {/if}

      {#if loading}
        <div class="empty-state">Loading history…</div>
      {:else}
        {#if failedEntry}
          <div class="day-head">Today</div>
          <div class="day-table">
            <div
              class="day-row"
              transition:fly={{ y: -10, duration: 400, easing: expoOut }}
            >
              <div class="day-time">{fmtTime(failedEntry.created_at)}</div>
              <div class="day-text error-msg">Looks like your last transcription failed.</div>
              <button
                class="retry-btn"
                onclick={retryTranscription}
                disabled={retrying}
              >
                {retrying ? '…' : 'Retry'}
              </button>
            </div>
          </div>
        {/if}

        {#if recents.length === 0 && !failedEntry}
          <div class="empty-state">
            No dictations yet. Hold <kbd>{hk1}</kbd> <kbd>{hk2}</kbd> to get started.
          </div>
        {:else}
          <div style="height: {topSpacerHeight}px;"></div>
          {#each visibleItems as { item, index } (item.key)}
            {#if item.type === 'header'}
              <div use:measureItem={item.key} class="day-head" class:muted={index > 0 || !!failedEntry}>
                {item.label}
              </div>
            {:else if item.type === 'row'}
              <div use:measureItem={item.key} class="day-row" class:first-in-table={flatItems[index - 1]?.type === 'header'}>
                <div class="day-time">{fmtTime(item.entry.created_at)}</div>
                <div class="day-text">{item.entry.clean_text}</div>
                <button
                  class="copy-btn"
                  class:copied={copiedId === item.entry.id}
                  onclick={() => copyText(item.entry)}
                  title="Copy to clipboard"
                  aria-label="Copy"
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    {#if copiedId === item.entry.id}
                      {@html icons.check}
                    {:else}
                      {@html icons.copy}
                    {/if}
                  </svg>
                </button>
              </div>
            {/if}
          {/each}
          <div style="height: {bottomSpacerHeight}px;"></div>
        {/if}
      {/if}
    </div>

    <!-- Right column — flat stats -->
    <div class="stat-stack">
      <div class="stat-card">
        <div class="stat-line">
          <span class="stat-num">
            {#if stats.total_words >= 1000}
              {(stats.total_words / 1000).toFixed(1)}<small>k</small>
            {:else}
              {stats.total_words}
            {/if}
          </span>
          <span class="stat-label">total words</span>
        </div>
        <div class="stat-line">
          <span class="stat-num">{Math.round(stats.avg_wpm) || '—'}</span>
          <span class="stat-label">wpm</span>
        </div>
        <div class="stat-line">
          <span class="stat-num">{stats.day_streak}</span>
          <span class="stat-label">day streak</span>
        </div>
      </div>
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

  .hero-photo {
    position: relative;
    border-radius: var(--r-lg);
    overflow: hidden;
    margin-bottom: 22px;
    height: clamp(112px, 14vw, 160px);
    background: var(--arm-950);
  }

  .hero-photo::before {
    content: '';
    position: absolute;
    right: -80px; top: -80px;
    width: 360px; height: 360px;
    background: radial-gradient(circle, rgba(217,119,87,0.22) 0%, transparent 60%);
    pointer-events: none;
  }

  .hero-photo-content {
    position: relative;
    padding: 22px 28px;
    max-width: min(520px, 100%);
  }

  .hero-photo-title {
    font-family: var(--serif);
    font-size: 22px;
    font-weight: 500;
    letter-spacing: -0.02em;
    margin: 0 0 8px;
    line-height: 1.15;
    color: var(--pill-fg);
  }

  .hero-photo-title :global(kbd) {
    background: color-mix(in srgb, var(--pill-fg) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--pill-fg) 18%, transparent);
    border-radius: 5px;
    font-family: var(--mono);
    font-size: 13px;
    padding: 1px 6px;
    color: var(--jap-300);
    font-weight: 500;
  }

  .hero-photo-sub {
    font-size: 12.5px;
    color: color-mix(in srgb, var(--pill-fg) 58%, transparent);
    margin: 0;
    line-height: 1.5;
  }

  .hero-em {
    font-family: var(--serif);
    font-style: italic;
    color: color-mix(in srgb, var(--pill-fg) 82%, transparent);
  }

  .notice-wrap {
    position: relative;
    margin-bottom: 22px;
  }

  .update-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    padding: 12px 18px;
    background: rgba(217, 119, 87, 0.08);
    border: 1px solid rgba(217, 119, 87, 0.20);
    border-radius: var(--r-lg);
    font-size: 13px;
    color: var(--ink-strong);
  }

  .update-text {
    flex: 1;
    font-family: var(--serif);
    font-weight: 500;
  }

  .update-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .update-dismiss {
    padding: 6px 12px;
    background: transparent;
    color: var(--ink-mute);
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: color 0.15s, border-color 0.15s;
  }
  .update-dismiss:hover {
    color: var(--ink-strong);
    border-color: var(--ink-mute);
  }

  .update-btn {
    flex-shrink: 0;
    padding: 6px 14px;
    background: var(--accent);
    color: var(--on-accent);
    border: none;
    border-radius: 6px;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s, opacity 0.15s;
  }
  .update-btn:hover:not(:disabled) {
    background: var(--accent-ink);
  }
  .update-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .day-head {
    font-family: var(--serif);
    font-style: italic;
    font-size: 14px;
    color: var(--ink-soft);
    margin: 4px 4px 10px;
  }
  .day-head.muted { margin-top: 22px; color: var(--ink-mute); }

  .day-table { border-top: 1px solid var(--line); }

  .day-row {
    display: grid;
    grid-template-columns: 84px 1fr auto;
    align-items: start;
    padding: 11px 4px;
    border-bottom: 1px solid var(--line);
    gap: 14px;
    cursor: default;
  }
  .day-row:hover { background: var(--control-active); }
  .day-row:not(:hover) .copy-btn:not(:focus-visible) { opacity: 0.25; }
  .day-row:hover .copy-btn { opacity: 0.9; }

  .copy-btn {
    all: unset;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: 4px;
    color: var(--ink-mute);
    opacity: 0.25;
    transition: color 0.12s, opacity 0.12s;
    flex-shrink: 0;
    margin-top: 2px;
  }
  .copy-btn:hover { opacity: 0.9; }
  .copy-btn:focus-visible {
    opacity: 0.9;
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .copy-btn.copied { color: var(--jap-500, #d97757); opacity: 1; }
  .copy-btn svg { width: 10px; height: 10px; }

  .day-time {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--ink-mute);
    font-variant-numeric: tabular-nums;
    padding-top: 2px;
    font-weight: 500;
  }

  .day-text {
    font-size: 13px;
    line-height: 1.55;
    color: var(--ink-strong);
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .error-msg {
    color: var(--ink-mute);
    font-style: italic;
  }

  .retry-btn {
    all: unset;
    cursor: pointer;
    font-size: 11px;
    font-weight: 500;
    color: var(--accent);
    padding: 2px 8px;
    border: 1px solid currentColor;
    border-radius: 4px;
    transition: background 0.12s, color 0.12s;
    flex-shrink: 0;
    white-space: nowrap;
    line-height: 1.6;
  }
  .retry-btn:hover:not(:disabled) {
    background: var(--accent);
    color: var(--on-accent, #fff);
  }
  .retry-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .empty-state {
    padding: 32px 4px;
    font-size: 13px;
    color: var(--ink-mute);
    font-style: italic;
  }

  .empty-state :global(kbd) {
    font-style: normal;
    background: var(--paper-2);
    border: 1px solid var(--line-strong);
    border-radius: 4px;
    font-family: var(--mono);
    font-size: 11px;
    padding: 1px 5px;
    color: var(--ink);
  }

  /* Flat stats */
  .stat-stack { display: flex; flex-direction: column; gap: 22px; }

  .stat-card { display: flex; flex-direction: column; gap: 10px; }

  .stat-line {
    display: flex;
    align-items: baseline;
    gap: 8px;
    border-bottom: 1px solid var(--line);
    padding-bottom: 9px;
  }
  .stat-line:last-child { border-bottom: 0; padding-bottom: 0; }

  .stat-num {
    font-family: var(--serif);
    font-size: 24px;
    font-weight: 500;
    letter-spacing: -0.02em;
    line-height: 1;
    color: var(--ink);
  }
  .stat-num :global(small) {
    font-family: var(--serif);
    font-size: 14px;
    color: var(--ink-mute);
    margin-left: 1px;
    font-weight: 400;
  }

  .stat-label { font-size: 11.5px; color: var(--ink-mute); margin-left: auto; }

  @media (max-width: 1060px) {
    .home-grid {
      grid-template-columns: 1fr;
    }

    .stat-card {
      display: grid;
      grid-template-columns: repeat(3, minmax(0, 1fr));
      gap: 12px;
    }

    .stat-line {
      border-bottom: 0;
      border-top: 1px solid var(--line);
      padding: 10px 0 0;
      min-width: 0;
    }
  }

  @media (max-width: 720px) {
    .hero-photo-content {
      padding: 18px 20px;
    }

    .hero-photo-title {
      font-size: 20px;
    }

    .day-row {
      grid-template-columns: 68px 1fr auto;
      gap: 10px;
    }

    .stat-card {
      grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
    }
  }

  .day-row.first-in-table {
    border-top: 1px solid var(--line);
  }
</style>
