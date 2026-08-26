<script lang="ts">
  import { invoke, listen } from '../../tauri';
  import { onMount } from 'svelte';
  import { fly, fade } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import Toggle from '../Toggle.svelte';
  import { icons } from '../../icons';
  import { appStore } from '../../stores';
  import { checkStatus } from '../../serviceStatus';
  import { ensureNotificationPermission } from '../../notifications';
  import Dropdown from '../Dropdown.svelte';
  import { saveSetting, type ProviderId } from '../../settings';
  import { MOTION_MS, MOTION_PX, animateWidth, motionMs, motionPx } from '../../motion';

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
  let notificationsTesting = $state(false);
  type NotificationTestType = 'update' | 'model' | 'service';
  let notificationTestType = $state<NotificationTestType>('update');
  let notificationDropdownOpen = $state(false);
  let notificationTestMessage = $state('');
  let installerTesting = $state(false);
  let installerTestMessage = $state('');
  let simulationMessage = $state('');
  let simulatedProvider = $state<ProviderId>('groq');
  let providerDropdownOpen = $state(false);
  let syncEnabled = $state(false);
  let syncApprovalOpen = $state(false);
  let syncMessage = $state('');

  const simulatedProviders: { id: ProviderId; label: string }[] = [
    { id: 'groq', label: 'Groq' },
    { id: 'openai', label: 'OpenAI' },
    { id: 'google', label: 'Gemini' },
    { id: 'assemblyai', label: 'AssemblyAI' },
  ];

  const notificationTestTypes: { value: NotificationTestType; label: string }[] = [
    { value: 'update', label: 'Update available' },
    { value: 'model', label: 'Model ready' },
    { value: 'service', label: 'Service notice' },
  ];

  function notificationTypeLabel() {
    return notificationTestTypes.find((option) => option.value === notificationTestType)?.label ?? 'Update available';
  }

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

  function handleSyncToggle(enabled: boolean) {
    if (enabled) {
      syncApprovalOpen = true;
    } else {
      void persistSyncEnabled(false);
    }
  }

  async function persistSyncEnabled(enabled: boolean) {
    syncMessage = '';
    try {
      await saveSetting('sync_enabled', enabled);
      syncEnabled = enabled;
      appStore.syncEnabled = enabled;
      syncMessage = 'Saved. Relaunching Verenu to apply the change…';
      setTimeout(() => {
        void invoke('restart_app').catch(() => {
          syncMessage = 'Saved, but Verenu could not relaunch. Quit and reopen it to apply the change.';
        });
      }, 250);
    } catch (err) {
      syncEnabled = !enabled;
      syncMessage = 'Could not save the Sync setting.';
      console.error('save sync_enabled failed:', err);
    }
  }

  function approveSync() {
    syncApprovalOpen = false;
    void persistSyncEnabled(true);
  }

  async function loadDevFlags() {
    try {
      const [force, verbose, betaUpdates, sync] = await Promise.all([
        invoke<boolean | null>('get_setting', { key: 'force_setup_on_launch' }),
        invoke<boolean>('get_dev_logging_enabled'),
        invoke<boolean | null>('get_setting', { key: 'beta_updates_enabled' }),
        invoke<boolean | null>('get_setting', { key: 'sync_enabled' }),
      ]);
      forceSetupOnLaunch = force ?? false;
      verboseEnabled = verbose ?? false;
      appStore.betaUpdatesEnabled = betaUpdates ?? false;
      syncEnabled = sync ?? false;
      appStore.syncEnabled = sync ?? false;
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

  async function testNotifications() {
    if (notificationsTesting) return;
    notificationsTesting = true;
    notificationTestMessage = '';
    simulationMessage = '';
    try {
      if (!(await ensureNotificationPermission())) {
        notificationTestMessage = 'Notification permission was not granted.';
        return;
      }
      await invoke('test_notifications', { notificationType: notificationTestType });
      notificationTestMessage = 'Notification sent.';
    } catch (err) {
      notificationTestMessage = 'Notification test failed.';
      console.error('test_notifications failed:', err);
    } finally {
      notificationsTesting = false;
    }
  }

  async function testLatestInstaller() {
    if (installerTesting) return;
    installerTesting = true;
    installerTestMessage = '';
    try {
      const version = await invoke<string>('reinstall_latest_update');
      installerTestMessage = `Starting reinstall of v${version}. Verenu will reopen when it finishes.`;
    } catch (err) {
      installerTestMessage = `Installer test failed: ${err}`;
      console.error('reinstall_latest_update failed:', err);
    } finally {
      installerTesting = false;
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

  function simulateProviderDown() {
    const provider = simulatedProviders.find(({ id }) => id === simulatedProvider) ?? simulatedProviders[0];
    appStore.providerStatusSimulation = true;
    appStore.providerStatusAlerts = [{
      providerId: provider.id,
      providerName: provider.label,
      status: 'degraded',
      severity: 'high',
      message: 'Some requests may be delayed or unavailable.',
      detailsUrl: '',
    }];
    simulationMessage = `${provider.label} status previewed.`;
  }

  function simulateWifiOffline() {
    appStore.isOnline = false;
    simulationMessage = 'Offline state previewed.';
  }

  async function simulateGlobalMessage() {
    appStore.globalMessageSimulation = true;
    await refreshStatusPreview('Global message previewed.');
    if (!appStore.globalMessage) {
      appStore.globalMessage = {
        message: 'Verenu has an important update.',
        showToUsers: true,
      };
    }
  }

  async function clearSimulations() {
    appStore.providerStatusAlerts = [];
    appStore.providerStatusSimulation = false;
    appStore.globalMessage = null;
    appStore.globalMessageSimulation = false;
    appStore.isOnline = true;
    await refreshStatusPreview('Simulations cleared.');
  }

  async function refreshStatusPreview(successMessage: string) {
    try {
      await checkStatus();
      simulationMessage = successMessage;
    } catch (err) {
      simulationMessage = 'Status preview failed.';
      console.error('Status preview refresh failed:', err);
    }
  }

  function handleWindowClick(event: MouseEvent) {
    const target = event.target;
    if (providerDropdownOpen && (!(target instanceof Element) || !target.closest('.simulation-provider-dropdown'))) {
      providerDropdownOpen = false;
    }
  }

  onMount(() => {
    let active = true;
    let unlisten: (() => void) | null = null;

    loadRecentLogs();
    loadDevFlags();
    window.addEventListener('click', handleWindowClick);
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
      window.removeEventListener('click', handleWindowClick);
    };
  });
</script>

<h2 class="settings-h">Developer</h2>
<p class="panel-note">Session log stream from backend runtime. Dev mode resets after app restart.</p>
<div class="setting-row beta-setting-row">
  <div>
    <div class="label">LAN Device Sync</div>
    <div class="desc">Experimental encrypted device-to-device sync. Off by default; the Sync settings page is hidden until enabled.</div>
    {#if syncMessage}<div class="desc export-status">{syncMessage}</div>{/if}
  </div>
  <Toggle checked={syncEnabled} onchange={handleSyncToggle} label="Enable LAN device sync" />
</div>
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
{#if syncApprovalOpen}
  <div class="sync-approval-backdrop" role="presentation" onclick={() => (syncApprovalOpen = false)}>
    <!-- svelte-ignore a11y_interactive_supports_focus a11y_click_events_have_key_events -->
    <div class="sync-approval" role="dialog" aria-modal="true" aria-labelledby="sync-approval-title" tabindex="-1" onclick={(event) => event.stopPropagation()} onkeydown={(event) => { if (event.key !== 'Escape') event.stopPropagation(); }}>
      <h3 id="sync-approval-title">Enable LAN Device Sync?</h3>
      <p>This is a beta feature. It is not fully secure or built out yet. Devices on your local network may discover this installation and paired devices can exchange selected Verenu data.</p>
      <div class="actions">
        <button class="btn-ghost" onclick={() => (syncApprovalOpen = false)}>Cancel</button>
        <button class="btn-primary" onclick={approveSync}>I understand — enable Sync</button>
      </div>
    </div>
  </div>
{/if}
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
<div class="setting-row dev-simulations">
  <div>
    <div class="label">UI Simulations</div>
    <div class="desc">Preview outage, offline, and global-message notices without changing the live APIs.</div>
    {#if simulationMessage}
      <div class="desc export-status" role="status">{simulationMessage}</div>
    {/if}
  </div>
  <div class="simulation-actions">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="ui-dropdown simulation-provider-dropdown" onclick={(event) => event.stopPropagation()} onkeydown={(event) => { if (event.key === 'Escape') providerDropdownOpen = false; }}>
      <button
        class="btn-ghost ui-dropdown-trigger simulation-provider-button"
        onclick={() => (providerDropdownOpen = !providerDropdownOpen)}
        aria-haspopup="true"
        aria-expanded={providerDropdownOpen}
        aria-controls="provider-status-preview-menu"
        aria-label="Provider for status preview"
      >
        <span>{simulatedProviders.find(({ id }) => id === simulatedProvider)?.label}</span>
        <svg class:open={providerDropdownOpen} width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="m6 9 6 6 6-6"/>
        </svg>
      </button>
      {#if providerDropdownOpen}
        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
        <div
          id="provider-status-preview-menu"
          class="ui-dropdown-menu ui-dropdown-menu--padded simulation-provider-menu"
          aria-label="Provider status preview options"
          onclick={(event) => event.stopPropagation()}
          in:fly={{ y: -motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.panel), easing: expoOut }}
          out:fade={{ duration: motionMs(MOTION_MS.fast) }}
        >
          {#each simulatedProviders as provider}
            <button
              class="ui-dropdown-option simulation-provider-item"
              class:active={simulatedProvider === provider.id}
              onclick={() => { simulatedProvider = provider.id; providerDropdownOpen = false; }}
            >{provider.label}</button>
          {/each}
        </div>
      {/if}
    </div>
    <button class="btn-ghost" onclick={simulateProviderDown}>Provider Down</button>
    <button class="btn-ghost" onclick={simulateWifiOffline}>Wi-Fi Offline</button>
    <button class="btn-ghost" onclick={simulateGlobalMessage}>Global Message</button>
    <button class="btn-ghost" onclick={clearSimulations}>Clear</button>
  </div>
</div>
<div class="setting-row">
  <div>
    <div class="label">System Notification Test</div>
    <div class="desc">Choose a notification type, then send the native toast and test its click destination.</div>
    {#if notificationTestMessage}
      <div class="desc export-status" role="status">{notificationTestMessage}</div>
    {/if}
  </div>
  <div class="notification-test-controls">
    <Dropdown bind:open={notificationDropdownOpen} closeSelector=".notification-test-dropdown">
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="ui-dropdown notification-test-dropdown"
        onclick={(event) => event.stopPropagation()}
        onkeydown={(event) => {
          if (event.key === 'Escape' && notificationDropdownOpen) {
            notificationDropdownOpen = false;
            event.stopPropagation();
          }
        }}
      >
        <button
          class="btn-ghost ui-dropdown-trigger notification-test-dropdown-button"
          type="button"
          use:animateWidth={{ text: notificationTypeLabel(), max: 180 }}
          onclick={() => (notificationDropdownOpen = !notificationDropdownOpen)}
          aria-haspopup="listbox"
          aria-expanded={notificationDropdownOpen}
          aria-controls="notification-test-menu"
          aria-label="Notification type"
        >
          <span>{notificationTypeLabel()}</span>
          <svg class:open={notificationDropdownOpen} width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="m6 9 6 6 6-6"/>
          </svg>
        </button>
        {#if notificationDropdownOpen}
          <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
          <div
            id="notification-test-menu"
            class="ui-dropdown-menu ui-dropdown-menu--padded notification-test-menu"
            role="listbox"
            tabindex="0"
            aria-label="Notification type options"
            onclick={(event) => event.stopPropagation()}
            in:fly={{ y: -motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.panel), easing: expoOut }}
            out:fade={{ duration: motionMs(MOTION_MS.fast) }}
          >
            {#each notificationTestTypes as option}
              <button
                class="ui-dropdown-option notification-test-item"
                class:active={notificationTestType === option.value}
                type="button"
                role="option"
                aria-selected={notificationTestType === option.value}
                onclick={() => {
                  notificationTestType = option.value;
                  notificationDropdownOpen = false;
                }}
              >{option.label}</button>
            {/each}
          </div>
        {/if}
      </div>
    </Dropdown>
    <button class="btn-ghost notification-test-send-button" onclick={testNotifications} disabled={notificationsTesting}>
      {notificationsTesting ? 'Sending...' : 'Send Notification'}
    </button>
  </div>
</div>
<div class="setting-row">
  <div>
    <div class="label">Installer Test</div>
    <div class="desc">
      Reinstalls the latest {appStore.betaUpdatesEnabled ? 'beta' : 'stable'} release and restarts Verenu.
    </div>
    {#if installerTestMessage}
      <div class="desc export-status" role="status">{installerTestMessage}</div>
    {/if}
  </div>
  <button class="btn-ghost" onclick={testLatestInstaller} disabled={installerTesting}>
    {installerTesting
      ? 'Starting...'
      : appStore.betaUpdatesEnabled
        ? 'Reinstall Latest Beta'
        : 'Reinstall Latest Stable'}
  </button>
</div>

<style>
  .sync-approval-backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: grid;
    place-items: center;
    padding: 24px;
    background: color-mix(in srgb, var(--ink) 28%, transparent);
  }
  .sync-approval {
    width: min(440px, 100%);
    padding: 22px;
    border: 1px solid var(--line-strong);
    border-radius: 14px;
    background: var(--paper);
    box-shadow: 0 18px 60px rgba(0, 0, 0, 0.24);
  }
  .sync-approval h3 {
    margin: 0 0 9px;
    color: var(--ink);
    font-size: 17px;
  }
  .sync-approval p {
    margin: 0 0 18px;
    color: var(--ink-soft);
    font-size: 13px;
    line-height: 1.55;
  }
  .sync-approval .actions {
    justify-content: flex-end;
    flex-wrap: wrap;
  }
  .logs-panel-wrap {
    position: relative;
    margin-top: 12px;
    margin-bottom: 12px;
  }
  .logs-panel {
    /* Grows with the window now that settings is full-height, instead of being
       a fixed 280px viewport inside a much taller page. */
    height: clamp(280px, 42vh, 520px);
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
    max-height: clamp(320px, 46vh, 560px);
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
  .notification-test-controls {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: 8px;
  }
  .notification-test-dropdown-button {
    justify-content: space-between;
  }
  .notification-test-send-button {
    display: inline-flex;
    align-items: center;
    height: 32px;
  }
  .notification-test-menu {
    min-width: 170px;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  .simulation-actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
  }
  .simulation-provider-button {
    gap: 6px;
  }
  .simulation-provider-menu {
    min-width: 160px;
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
