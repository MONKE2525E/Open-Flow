<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { tick } from 'svelte';
  import Toggle from '../Toggle.svelte';
  import { saveSetting, type HistoryRetention } from '../../settings';

  const historyOptions = ['7 days', '30 days', '90 days', 'Forever'];

  let historyRetention = $state('30 days');
  let historyDropdownOpen = $state(false);
  let appContextHint = $state(false);

  async function loadSettings() {
    try {
      const [retention, hint] = await Promise.all([
        invoke<string | null>('get_setting', { key: 'history_retention' }),
        invoke<boolean | null>('get_setting', { key: 'app_context_hint' }),
      ]);
      if (retention) historyRetention = retention;
      appContextHint = hint ?? false;
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
    <button class="btn-ghost mic-btn" onclick={() => (historyDropdownOpen = !historyDropdownOpen)}>
      <span>{historyRetention}</span>
      <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="m6 9 6 6 6-6"/>
      </svg>
    </button>
    {#if historyDropdownOpen}
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <div class="mic-menu" role="presentation" onclick={(e) => e.stopPropagation()}>
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
</style>
