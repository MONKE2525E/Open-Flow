<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import Toggle from '../Toggle.svelte';

  let logs = $state<string[]>([]);
  let autoScroll = $state(true);
  let exportMessage = $state('');
  let exporting = $state(false);
  let logViewport: HTMLDivElement | null = null;
  let verboseEnabled = $state(true);
  let forceSetupOnLaunch = $state(false);

  async function loadRecentLogs() {
    try {
      logs = await invoke<string[]>('get_recent_logs', { limit: 300 });
      queueMicrotask(scrollToBottom);
    } catch (err) {
      console.error('Failed to load logs:', err);
    }
  }

  function scrollToBottom() {
    if (!autoScroll || !logViewport) return;
    logViewport.scrollTop = logViewport.scrollHeight;
  }

  async function downloadLogs() {
    if (exporting) return;
    exporting = true;
    exportMessage = '';
    try {
      const path = await invoke<string>('download_logs');
      exportMessage = `Saved: ${path}`;
    } catch (err) {
      exportMessage = 'Failed to save logs.';
      console.error('downloadLogs failed:', err);
    } finally {
      exporting = false;
    }
  }

  async function toggleVerbose() {
    verboseEnabled = !verboseEnabled;
    try {
      await invoke('set_dev_logging_enabled', { enabled: verboseEnabled });
    } catch (err) {
      verboseEnabled = !verboseEnabled;
      console.error('set_dev_logging_enabled failed:', err);
    }
  }

  async function loadDevFlags() {
    try {
      const force = await invoke<boolean | null>('get_setting', { key: 'force_setup_on_launch' });
      forceSetupOnLaunch = force ?? false;
    } catch (err) {
      console.error('Failed to load dev flags:', err);
    }
  }

  async function handleForceSetupOnLaunch(value: boolean) {
    forceSetupOnLaunch = value;
    try {
      await invoke('save_setting', {
        key: 'force_setup_on_launch',
        value,
      });
    } catch (err) {
      forceSetupOnLaunch = !value;
      console.error('Failed to save force_setup_on_launch:', err);
    }
  }

  onMount(() => {
    let active = true;
    let unlisten: (() => void) | null = null;

    loadRecentLogs();
    loadDevFlags();
    (async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event');
        unlisten = await listen<string>('open-flow:log', (ev) => {
          if (!active) return;
          logs = [...logs.slice(-499), ev.payload];
          queueMicrotask(scrollToBottom);
        });
      } catch (err) {
        console.error('Failed to listen for log events:', err);
      }
    })();

    return () => {
      active = false;
      if (unlisten) unlisten();
    };
  });
</script>

<h2 class="settings-h">Developer</h2>
<p class="panel-note">Session log stream from backend runtime. Dev mode resets after app restart.</p>
<div class="setting-row">
  <div>
    <div class="label">Force Setup On Launch</div>
    <div class="desc">Shows onboarding on startup without erasing saved settings.</div>
  </div>
  <Toggle checked={forceSetupOnLaunch} onchange={handleForceSetupOnLaunch} label="Force setup on launch" />
</div>
<div class="setting-row">
  <div>
    <div class="label">Real-time Logs</div>
    <div class="desc">{logs.length} lines loaded</div>
  </div>
  <div class="actions">
    <button class="btn-ghost" onclick={toggleVerbose}>
      {verboseEnabled ? 'Verbose: On' : 'Verbose: Off'}
    </button>
    <button class="btn-ghost" onclick={() => (autoScroll = !autoScroll)}>
      {autoScroll ? 'Pause Auto-scroll' : 'Resume Auto-scroll'}
    </button>
  </div>
</div>
<div class="logs-panel scroll-styled" bind:this={logViewport}>
  {#if logs.length === 0}
    <div class="logs-empty">No logs yet.</div>
  {:else}
    {#each logs as line}
      <div class="log-line">{line}</div>
    {/each}
  {/if}
</div>
<div class="setting-row">
  <div>
    <div class="label">Download Logs</div>
    <div class="desc">Writes current session logs to your Downloads folder.</div>
    {#if exportMessage}
      <div class="desc export-status">{exportMessage}</div>
    {/if}
  </div>
  <button class="btn-ghost" onclick={downloadLogs} disabled={exporting}>
    {exporting ? 'Saving...' : 'Download Logs'}
  </button>
</div>

<style>
  .logs-panel {
    height: 280px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--paper);
    padding: 8px 10px;
    overflow: auto;
    margin-top: 12px;
    margin-bottom: 12px;
  }
  .log-line {
    font-family: var(--mono);
    font-size: 10.5px;
    color: var(--ink-soft);
    line-height: 1.45;
    padding: 2px 0;
    border-bottom: 1px solid var(--line-soft);
    word-break: break-word;
  }
  .log-line:last-child {
    border-bottom: none;
  }
  .logs-empty {
    font-size: 12px;
    color: var(--ink-mute);
    padding: 8px 2px;
  }
  .export-status {
    margin-top: 6px;
    color: var(--ink-faint);
  }
  .actions {
    display: flex;
    gap: 8px;
  }
</style>
