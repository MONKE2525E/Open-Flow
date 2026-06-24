<script lang="ts">
  import { invoke } from '../../tauri';
  import { fly, fade } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import Toggle from '../Toggle.svelte';
  import { saveSetting, type HistoryRetention } from '../../settings';
  import { modalFocusTrap } from '../../modalFocus';
  import { animateWidth, modalBackdrop, modalCard, MOTION_MS, MOTION_PX, motionMs, motionPx } from '../../motion';

  const historyOptions = ['7 days', '30 days', '90 days', 'Forever'];
  const HISTORY_MENU_ID = 'history-retention-menu';
  let confirmRetention = $state<{ value: string; count: number } | null>(null);
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

  async function requestHistoryRetention(value: string) {
    historyDropdownOpen = false;
    if (value === historyRetention) return;
    try {
      const count = await invoke<number>('count_old_transcriptions', { retention: value });
      if (count > 0) {
        confirmRetention = { value, count };
      } else {
        await saveHistoryRetention(value);
      }
    } catch (err) {
      console.error('count_old_transcriptions failed:', err);
    }
  }

  async function confirmHistoryRetention() {
    if (!confirmRetention) return;
    const { value } = confirmRetention;
    confirmRetention = null;
    await saveHistoryRetention(value);
  }

  function handleRetentionModalKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && confirmRetention) confirmRetention = null;
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

  function closeHistoryDropdown(e: MouseEvent | PointerEvent) {
    const target = e.target;
    if (target instanceof Element && !target.closest('.history-dropdown')) {
      historyDropdownOpen = false;
    }
  }

  function handleHistoryButtonKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && historyDropdownOpen) {
      historyDropdownOpen = false;
      e.stopPropagation();
    }
  }

  $effect(() => {
    if (!historyDropdownOpen) return;

    const timeout = window.setTimeout(() => {
      window.addEventListener('pointerdown', closeHistoryDropdown);
    });

    return () => {
      window.clearTimeout(timeout);
      window.removeEventListener('pointerdown', closeHistoryDropdown);
    };
  });

  loadSettings();

  // ---------- import / export ----------

  type ImportSummary = {
    settings_applied: number;
    settings_skipped: number;
    dictionary_inserted: number;
    dictionary_skipped: number;
    dictionary_already_existed: number;
    snippets_inserted: number;
    snippets_skipped: number;
    snippets_already_existed: number;
  };

  let exporting = $state(false);
  let exportMsg = $state('');
  let exportMsgKind = $state<'ok' | 'err' | ''>('');
  let importing = $state(false);
  let importMsg = $state('');
  let importMsgKind = $state<'ok' | 'err' | ''>('');
  let fileInput: HTMLInputElement | null = $state(null);
  let historyRetentionButton: HTMLButtonElement | null = $state(null);
  let retentionCancelButton: HTMLButtonElement | null = $state(null);

  async function handleExport() {
    exporting = true;
    exportMsg = '';
    exportMsgKind = '';
    try {
      const path = await invoke<string>('export_data');
      exportMsg = `Saved to ${path}`;
      exportMsgKind = 'ok';
    } catch {
      exportMsg = 'Export failed.';
      exportMsgKind = 'err';
    } finally {
      exporting = false;
    }
  }

  async function handleFileSelected(e: Event) {
    const file = (e.target as HTMLInputElement).files?.[0];
    if (!file) return;
    importing = true;
    importMsg = '';
    importMsgKind = '';
    try {
      const json = await file.text();
      const s = await invoke<ImportSummary>('import_data', { json });
      const dictParts = [
        s.dictionary_inserted > 0 ? `${s.dictionary_inserted} added` : '',
        s.dictionary_already_existed > 0 ? `${s.dictionary_already_existed} already on device` : '',
        s.dictionary_skipped > 0 ? `${s.dictionary_skipped} skipped` : '',
      ].filter(Boolean).join(', ') || 'none';
      const snipParts = [
        s.snippets_inserted > 0 ? `${s.snippets_inserted} added` : '',
        s.snippets_already_existed > 0 ? `${s.snippets_already_existed} already on device` : '',
        s.snippets_skipped > 0 ? `${s.snippets_skipped} skipped` : '',
      ].filter(Boolean).join(', ') || 'none';
      importMsg = `Applied ${s.settings_applied} settings. Dictionary: ${dictParts}. Snippets: ${snipParts}.`;
      importMsgKind = 'ok';
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      importMsg =
        msg.startsWith('Invalid backup') || msg.startsWith('Unsupported backup')
          ? msg
          : 'Import failed.';
      importMsgKind = 'err';
    } finally {
      importing = false;
      if (fileInput) fileInput.value = '';
    }
  }
</script>

<svelte:window onkeydown={handleRetentionModalKeydown} />

<h2 class="settings-h">Privacy</h2>

<h3 class="settings-subhead first">Context</h3>
<div class="setting-row">
  <div><div class="label">App context hint</div><div class="desc">Passes the active app to the cleanup model to tailor formatting</div></div>
  <Toggle checked={appContextHint} onchange={handleAppContextHint} label="App context hint" />
</div>

<h3 class="settings-subhead">On-device learning</h3>
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
  <Toggle checked={autoLearn} onchange={handleAutoLearn} label="Auto-learn corrections" />
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

<h3 class="settings-subhead">History & storage</h3>
<div class="setting-row">
  <div><div class="label">Transcription history</div><div class="desc">How long to keep past dictations</div></div>
  <div class="history-dropdown">
    <button
      bind:this={historyRetentionButton}
      class="btn-ghost mic-btn"
      use:animateWidth={{ text: historyRetention }}
      onclick={() => (historyDropdownOpen = !historyDropdownOpen)}
      onkeydown={handleHistoryButtonKeydown}
      aria-haspopup="listbox"
      aria-expanded={historyDropdownOpen}
      aria-controls={HISTORY_MENU_ID}
      aria-label="Transcription history retention"
    >
      <span>{historyRetention}</span>
      <svg class:open={historyDropdownOpen} width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="m6 9 6 6 6-6"/>
      </svg>
    </button>
    {#if historyDropdownOpen}
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <div
        id={HISTORY_MENU_ID}
        class="mic-menu scroll-styled scroll-thumb-elev"
        role="listbox"
        tabindex="-1"
        aria-label="History retention options"
        onclick={(e) => e.stopPropagation()}
        in:fly={{ y: -motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.panel), easing: expoOut }}
        out:fade={{ duration: motionMs(MOTION_MS.fast) }}
      >
        {#each historyOptions as opt}
          <button
            class="mic-item"
            class:active={historyRetention === opt}
            onclick={() => requestHistoryRetention(opt)}
            onkeydown={handleHistoryButtonKeydown}
            role="option"
            aria-selected={historyRetention === opt}
          >
            {opt}
          </button>
        {/each}
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

<h3 class="settings-subhead">Backup</h3>
<div class="setting-row">
  <div>
    <div class="label">Export Backup</div>
    {#if exportMsg}
      <div
        class="desc data-status"
        class:data-ok={exportMsgKind === 'ok'}
        class:data-err={exportMsgKind === 'err'}
      >{exportMsg}</div>
    {/if}
  </div>
  <button class="btn-ghost" onclick={handleExport} disabled={exporting}>
    {exporting ? 'Exporting…' : 'Export'}
  </button>
</div>
<div class="setting-row">
  <div>
    <div class="label">Import Backup</div>
    {#if importMsg}
      <div
        class="desc data-status"
        class:data-ok={importMsgKind === 'ok'}
        class:data-err={importMsgKind === 'err'}
      >{importMsg}</div>
    {/if}
  </div>
  <button class="btn-ghost" onclick={() => fileInput?.click()} disabled={importing}>
    {importing ? 'Importing…' : 'Import'}
  </button>
</div>
<input
  bind:this={fileInput}
  type="file"
  accept=".json"
  style="display:none"
  onchange={handleFileSelected}
/>

{#if confirmRetention}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <button class="modal-backdrop" aria-label="Close dialog" onclick={() => (confirmRetention = null)} in:modalBackdrop={{ duration: 180 }} out:modalBackdrop={{ duration: 160 }}></button>
  <div
    class="modal-card"
    use:modalFocusTrap={{
      active: !!confirmRetention,
      initialFocus: () => retentionCancelButton,
      restoreFocus: () => historyRetentionButton,
    }}
    role="dialog"
    aria-modal="true"
    aria-labelledby="retention-confirm-title"
    tabindex="-1"
    in:modalCard={{ duration: 220, distance: motionPx(MOTION_PX.panel), scaleFrom: 0.97 }}
    out:modalCard={{ duration: 160, distance: motionPx(MOTION_PX.nudge), scaleFrom: 0.985 }}
  >
    <div class="modal-header">
      <h2 id="retention-confirm-title" class="modal-title">Delete older history?</h2>
    </div>
    <div class="modal-body">
      <p class="confirm-copy">
        Looks like you have {confirmRetention.count} transcription{confirmRetention.count === 1 ? '' : 's'} before this point. Changing this will delete that data. Are you sure?
      </p>
    </div>
    <div class="modal-footer">
      <div class="footer-actions">
        <button bind:this={retentionCancelButton} class="btn-ghost" onclick={() => (confirmRetention = null)}>Cancel</button>
        <button class="btn-danger" onclick={confirmHistoryRetention}>Delete history</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .data-ok { color: var(--success); }
  .data-err { color: var(--accent); }
  .data-status {
    animation: data-drop 260ms cubic-bezier(0.22, 1, 0.36, 1) both;
  }
  @keyframes data-drop {
    from { opacity: 0; transform: translateY(-6px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  .history-dropdown { position: relative; flex-shrink: 0; }
  .mic-btn { display: flex; align-items: center; gap: 6px; max-width: 180px; }
  .mic-btn svg { transition: transform 150ms; }
  .mic-btn svg.open { transform: rotate(180deg); }
  .mic-btn span { overflow: hidden; white-space: nowrap; }
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

  /* ── retention confirm modal ── */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    border: 0;
    padding: 0;
    appearance: none;
    background: var(--overlay);
    z-index: 50;
    outline: none;
  }
  .modal-card {
    position: fixed;
    top: 50%;
    left: 50%;
    translate: -50% -50%;
    z-index: 51;
    isolation: isolate;
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    width: min(420px, calc(100vw - 40px));
    box-shadow: var(--shadow-elev);
    overflow: hidden;
  }
  .modal-header {
    padding: 20px 20px 0;
  }
  .modal-title {
    font-family: var(--serif);
    font-size: 18px;
    font-weight: 500;
    letter-spacing: -0.015em;
    color: var(--ink);
    margin: 0;
  }
  .modal-body { padding: 10px 20px 18px; }
  .confirm-copy {
    margin: 0;
    font-size: 13px;
    line-height: 1.5;
    color: var(--ink-soft);
  }
  .modal-footer {
    padding: 0 20px 20px;
  }
  .footer-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .btn-danger {
    background: var(--danger-bg);
    color: var(--danger);
    border: 1px solid var(--danger-line);
    border-radius: 6px;
    padding: 5px 12px;
    font-size: 12px;
    font-family: var(--sans);
    font-weight: 500;
    cursor: pointer;
    transition: background 0.12s, color 0.12s, border-color 0.12s;
  }
  .btn-danger:hover {
    background: var(--danger);
    color: var(--on-accent);
    border-color: var(--danger);
  }
</style>
