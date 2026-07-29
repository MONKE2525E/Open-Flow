<script lang="ts">
  import { invoke, listen } from '../../tauri';
  import { onMount } from 'svelte';
  import Toggle from '../Toggle.svelte';
  import { icons } from '../../icons';

  let logs = $state<string[]>([]);
  let autoScroll = $state(true);
  let exportMessage = $state('');
  let exporting = $state(false);
  let logViewport: HTMLDivElement | null = null;
  let verboseEnabled = $state(false);
  let forceSetupOnLaunch = $state(false);
  let copied = $state(false);
  let copiedTimer: ReturnType<typeof setTimeout> | null = null;
  let providerStatusRaw = $state('');
  let providerStatusChecking = $state(false);

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

  async function copyAllLogs() {
    try {
      await navigator.clipboard.writeText(logs.join('\n'));
      copied = true;
      if (copiedTimer) clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => { copied = false; }, 1500);
    } catch (err) {
      console.error('copyAllLogs failed:', err);
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
      const [force, verbose] = await Promise.all([
        invoke<boolean | null>('get_setting', { key: 'force_setup_on_launch' }),
        invoke<boolean>('get_dev_logging_enabled'),
      ]);
      forceSetupOnLaunch = force ?? false;
      verboseEnabled = verbose ?? false;
    } catch (err) {
      console.error('Failed to load dev flags:', err);
    }
  }

  async function runProviderStatusCheck() {
    if (providerStatusChecking) return;
    providerStatusChecking = true;
    providerStatusRaw = '';
    try {
      const raw = await invoke('check_provider_status_raw');
      providerStatusRaw = JSON.stringify(raw, null, 2);
    } catch (err) {
      providerStatusRaw = `Check failed: ${err}`;
      console.error('check_provider_status_raw failed:', err);
    } finally {
      providerStatusChecking = false;
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
        unlisten = await listen<string>('verenu:log', (ev) => {
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
      if (copiedTimer) clearTimeout(copiedTimer);
    };
  });
</script>

<h2 class="settings-h">Developer</h2>
<p class="panel-note">Session log stream from backend runtime. Dev mode resets after app restart.</p>
<div class="privacy-warn" role="note">
  <strong>Privacy warning:</strong> Verbose logs can capture your full dictated text,
  the prompts sent to AI providers, and the active-app context. Anything you download
  or share contains this content in plain text — only enable verbose logging or export
  logs if you understand what they hold.
</div>
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
<div class="logs-panel-wrap">
  <div class="logs-panel scroll-styled" bind:this={logViewport}>
    {#if logs.length === 0}
      <div class="logs-empty">No logs yet.</div>
    {:else}
      {#each logs as line}
        <div class="log-line">{line}</div>
      {/each}
    {/if}
  </div>
  <button
    class="copy-logs-btn"
    class:copied
    onclick={copyAllLogs}
    disabled={logs.length === 0}
    title="Copy all logs"
    aria-label="Copy all logs"
  >
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      {#if copied}
        {@html icons.check}
      {:else}
        {@html icons.copy}
      {/if}
    </svg>
    {copied ? 'Copied' : 'Copy all'}
  </button>
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
<div class="setting-row">
  <div>
    <div class="label">Provider Status Check</div>
    <div class="desc">Fetches api.verenu.com/v1/provider-status directly and shows the raw response.</div>
  </div>
  <button class="btn-ghost" onclick={runProviderStatusCheck} disabled={providerStatusChecking}>
    {providerStatusChecking ? 'Checking...' : 'Run Check'}
  </button>
</div>
{#if providerStatusRaw}
  <pre class="raw-panel scroll-styled">{providerStatusRaw}</pre>
{/if}

<style>
  .logs-panel-wrap {
    position: relative;
    margin-top: 12px;
    margin-bottom: 12px;
  }
  .logs-panel {
    height: 280px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--paper);
    padding: 8px 10px;
    overflow: auto;
  }
  .copy-logs-btn {
    position: absolute;
    right: 10px;
    bottom: 10px;
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 9px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--paper-2);
    color: var(--ink-mute);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.18);
    transition: color 0.12s, border-color 0.12s, opacity 0.12s;
  }
  .copy-logs-btn:hover:not(:disabled) {
    color: var(--ink);
    border-color: var(--ink-mute);
  }
  .copy-logs-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .copy-logs-btn.copied {
    color: var(--jap-500, #d97757);
    border-color: var(--jap-500, #d97757);
  }
  .copy-logs-btn svg {
    width: 12px;
    height: 12px;
    flex-shrink: 0;
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
  .raw-panel {
    max-height: 320px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--paper);
    padding: 10px 12px;
    overflow: auto;
    margin-top: 12px;
    margin-bottom: 12px;
    font-family: var(--mono);
    font-size: 10.5px;
    color: var(--ink-soft);
    line-height: 1.45;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .export-status {
    margin-top: 6px;
    color: var(--ink-faint);
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  .privacy-warn {
    margin: 10px 0 4px;
    padding: 10px 12px;
    border: 1px solid var(--warn-line, var(--line));
    border-left: 3px solid var(--warn, #c4742a);
    border-radius: 8px;
    background: var(--warn-bg, rgba(196, 116, 42, 0.08));
    font-size: 12px;
    line-height: 1.5;
    color: var(--ink-soft);
  }
  .privacy-warn strong {
    color: var(--ink);
  }
</style>
