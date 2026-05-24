<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { tick } from 'svelte';
  import Toggle from '../Toggle.svelte';
  import { saveSetting, type HistoryRetention } from '../../settings';
  import { animateWidth } from '../../motion';

  const historyOptions = ['7 days', '30 days', '90 days', 'Forever'];
  type CleanupCacheStatus = {
    entry_count: number;
    is_space_constrained: boolean;
    free_bytes: number;
  } | null;

  let historyRetention = $state('30 days');
  let historyDropdownOpen = $state(false);
  let appContextHint = $state(false);
  let autoLearn = $state(false);
  let cleanupCacheEntries = $state(0);
  let cleanupCacheSpaceConstrained = $state(false);
  let cleanupCacheFreeBytes = $state<number | null>(null);
  let clearingCleanupCache = $state(false);
  let autoLearnSummary = $state({
    monitors_started: 0,
    anchor_misses: 0,
    low_confidence_rejections: 0,
    promotions: 0,
    duplicate_monitor_skips: 0,
    timeout_finishes: 0,
  });
  let recentAutoLearn = $state<Array<{ id: number; event_type: string; reason_code: string; created_at: string }>>([]);

  async function loadSettings() {
    try {
      const [retention, hint, learn, cacheStatus, summary, recent] = await Promise.all([
        invoke<string | null>('get_setting', { key: 'history_retention' }),
        invoke<boolean | null>('get_setting', { key: 'app_context_hint' }),
        invoke<boolean | null>('get_setting', { key: 'auto_learn_enabled' }),
        invoke<CleanupCacheStatus>('get_cleanup_cache_status'),
        invoke<typeof autoLearnSummary>('get_auto_learn_status_summary'),
        invoke<typeof recentAutoLearn>('get_recent_auto_learn_activity', { limit: 5 }),
      ]);
      if (retention) historyRetention = retention;
      appContextHint = hint ?? false;
      autoLearn = learn ?? false;
      cleanupCacheEntries = cacheStatus?.entry_count ?? 0;
      cleanupCacheSpaceConstrained = cacheStatus?.is_space_constrained ?? false;
      cleanupCacheFreeBytes = cacheStatus?.free_bytes ?? null;
      autoLearnSummary = summary ?? autoLearnSummary;
      recentAutoLearn = recent ?? [];
    } catch (err) {
      console.error('PrivacySection load failed:', err);
    }
  }

  async function saveHistoryRetention(value: string) {
    historyRetention = value;
    historyDropdownOpen = false;
    try {
      await saveSetting('history_retention', value as HistoryRetention);
    } catch (err) {
      console.error('saveHistoryRetention failed:', err);
    }
  }

  async function handleAppContextHint(value: boolean) {
    appContextHint = value;
    try {
      await saveSetting('app_context_hint', value);
    } catch (err) {
      appContextHint = !value;
      console.error('save app_context_hint failed:', err);
    }
  }

  async function handleAutoLearn(value: boolean) {
    autoLearn = value;
    try {
      await saveSetting('auto_learn_enabled', value);
    } catch (err) {
      autoLearn = !value;
      console.error('save auto_learn_enabled failed:', err);
    }
  }

  async function clearCleanupCache() {
    if (clearingCleanupCache) return;
    clearingCleanupCache = true;
    try {
      await invoke<number>('clear_cleanup_cache');
      const status = await invoke<CleanupCacheStatus>('get_cleanup_cache_status');
      cleanupCacheEntries = status?.entry_count ?? 0;
      cleanupCacheSpaceConstrained = status?.is_space_constrained ?? false;
      cleanupCacheFreeBytes = status?.free_bytes ?? null;
    } catch (err) {
      console.error('clearCleanupCache failed:', err);
    } finally {
      clearingCleanupCache = false;
    }
  }

  function closeHistoryDropdown(e: MouseEvent) {
    if (!(e.target as HTMLElement).closest('.history-dropdown')) historyDropdownOpen = false;
  }

  $effect(() => {
    if (historyDropdownOpen) {
      tick().then(() => window.addEventListener('click', closeHistoryDropdown, { once: true }));
    }
  });

  loadSettings();
</script>

<h2 class="settings-h">Privacy</h2>
<div class="setting-row">
  <div><div class="label">Transcription history</div><div class="desc">How long to keep past dictations</div></div>
  <div class="history-dropdown">
    <button
      class="btn-ghost mic-btn"
      use:animateWidth={{ text: historyRetention }}
      onclick={() => (historyDropdownOpen = !historyDropdownOpen)}
    >
      <span>{historyRetention}</span>
      <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="m6 9 6 6 6-6"/>
      </svg>
    </button>
    {#if historyDropdownOpen}
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <div class="mic-menu scroll-styled scroll-thumb-elev" role="presentation" onclick={(e) => e.stopPropagation()}>
        {#each historyOptions as opt}
          <button class="mic-item" class:active={historyRetention === opt} onclick={() => saveHistoryRetention(opt)}>
            {opt}
          </button>
        {/each}
      </div>
    {/if}
  </div>
</div>
<div class="setting-row">
  <div><div class="label">App context hint</div><div class="desc">Passes the active app to the cleanup model to tailor formatting</div></div>
  <Toggle checked={appContextHint} onchange={handleAppContextHint} />
</div>
<div class="setting-row">
  <div>
    <div class="label" style="display:flex;align-items:center;gap:7px;">
      Auto-learn corrections
      <span class="privacy-eye-wrap">
        <svg class="privacy-eye" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
          <circle cx="12" cy="12" r="3"/>
        </svg>
        <span class="privacy-tooltip">Entirely on-device — no text is sent to any API.</span>
      </span>
    </div>
    <div class="desc">Add confirmed corrections to dictionary automatically</div>
  </div>
  <Toggle checked={autoLearn} onchange={handleAutoLearn} />
</div>
<div class="setting-row">
  <div>
    <div class="label">Auto-learn activity</div>
    <div class="desc">
      Promoted: {autoLearnSummary.promotions} | Low-confidence blocked: {autoLearnSummary.low_confidence_rejections} | Anchor misses: {autoLearnSummary.anchor_misses}
    </div>
    {#if recentAutoLearn.length > 0}
      <div class="desc" style="margin-top:4px;">
        Latest: {recentAutoLearn[0].event_type} ({recentAutoLearn[0].reason_code})
      </div>
    {/if}
  </div>
</div>
<div class="setting-row">
  <div>
    <div class="label">Cleanup cache</div>
    <div class="desc">
      {cleanupCacheEntries} cached phrase{cleanupCacheEntries === 1 ? '' : 's'}.
      {#if cleanupCacheSpaceConstrained}
        Low disk space (&lt;1 GB free). Clearing cache may help free space.
      {:else if cleanupCacheFreeBytes === null}
        Status unavailable.
      {:else}
        {(cleanupCacheFreeBytes / 1024 / 1024 / 1024).toFixed(1)} GB free.
      {/if}
    </div>
  </div>
  <button
    class="btn-ghost"
    onclick={clearCleanupCache}
    disabled={clearingCleanupCache}
    title={cleanupCacheSpaceConstrained ? 'Low disk space detected (<1 GB free).' : ''}
  >
    {clearingCleanupCache ? 'Clearing…' : 'Clear Cache'}
  </button>
</div>

<style>
  .history-dropdown { position: relative; flex-shrink: 0; }
  .mic-btn { display: flex; align-items: center; gap: 6px; max-width: 180px; }
  .mic-btn span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 140px; }
  .mic-menu {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    box-shadow: var(--shadow-popover);
    min-width: 200px;
    max-width: 280px;
    max-height: 200px;
    overflow-y: auto;
    z-index: 10;
  }
  .mic-item {
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
  .mic-item:last-child { border-bottom: none; }
  .mic-item:hover { background: var(--paper); }
  .mic-item.active { background: var(--accent-soft); color: var(--ink); font-weight: 500; }
  .privacy-eye-wrap { position: relative; display: inline-flex; align-items: center; }
  .privacy-eye { color: var(--ink-mute); cursor: default; flex-shrink: 0; transition: color 0.15s ease, transform 0.15s ease; }
  .privacy-eye-wrap:hover .privacy-eye { color: var(--ink-soft); transform: scale(1.18); }
  .privacy-tooltip {
    position: absolute;
    left: 50%;
    bottom: calc(100% + 7px);
    transform: translateX(-50%) translateY(4px);
    background: var(--ink);
    color: var(--paper);
    font-size: 11px;
    font-family: var(--sans);
    font-weight: 400;
    white-space: nowrap;
    padding: 4px 9px;
    border-radius: 6px;
    pointer-events: none;
    z-index: 20;
    box-shadow: var(--shadow-popover);
    opacity: 0;
    transition: opacity 0.16s ease, transform 0.16s ease;
  }
  .privacy-eye-wrap:hover .privacy-tooltip { opacity: 1; transform: translateX(-50%) translateY(0); }
</style>
