<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { fly, slide } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { invoke } from '../tauri';
  import { isMac } from '../platform';
  import { motionMs } from '../motion';
  import type { ProviderId } from '../settings';

  type MacPermissionStatus =
    | 'authorized'
    | 'needs_permission'
    | 'not_determined'
    | 'denied'
    | 'restricted'
    | 'unknown';
  type KeychainStatus = 'authorized' | 'not_configured' | 'denied' | 'unknown';

  type Props = {
    /** True once Accessibility + Input Monitoring + Microphone are all granted. */
    allGranted?: boolean;
    /** 'setup' shows a success banner once all granted; 'settings' is steady-state. */
    variant?: 'setup' | 'settings';
    /** When provided, also surfaces the Keychain Access row for that provider. */
    provider?: ProviderId | null;
  };

  let { allGranted = $bindable(false), variant = 'settings', provider = null }: Props = $props();

  let accessibilityPermission = $state<MacPermissionStatus>(isMac ? 'unknown' : 'authorized');
  let inputMonitoringPermission = $state<MacPermissionStatus>(isMac ? 'unknown' : 'authorized');
  let microphonePermission = $state<MacPermissionStatus>(isMac ? 'unknown' : 'authorized');
  let keychainStatus = $state<KeychainStatus>('unknown');

  let permissionsLoading = $state(false);
  let permissionsError = $state('');
  let accessibilityPrompting = $state(false);
  let inputMonitoringRequesting = $state(false);
  let microphoneRequesting = $state(false);
  let keychainLoading = $state(false);
  let restarting = $state(false);

  let accessibilityPromptAttempted = $state(false);
  let inputMonitoringActionTaken = $state(false);
  let showRestartHint = $state(false);
  let permissionsPollingStartMs = $state<number | null>(null);
  let pollInterval: ReturnType<typeof setInterval> | null = null;

  // Keep the bindable flag in sync with the three core OS permissions.
  $effect(() => {
    allGranted =
      accessibilityPermission === 'authorized' &&
      inputMonitoringPermission === 'authorized' &&
      microphonePermission === 'authorized';
  });

  function permissionLabel(status: MacPermissionStatus) {
    switch (status) {
      case 'authorized': return 'Granted';
      case 'not_determined': return 'Not yet asked';
      case 'denied': return 'Blocked';
      case 'restricted': return 'Restricted by org';
      case 'needs_permission': return 'Needs access';
      default: return 'Checking…';
    }
  }

  function keychainLabel(status: KeychainStatus) {
    switch (status) {
      case 'authorized': return 'Granted';
      case 'not_configured': return 'No key saved';
      case 'denied': return 'Access denied';
      default: return 'Checking…';
    }
  }

  async function refreshMacPermissions(silent = false) {
    if (!isMac || permissionsLoading) return;
    permissionsLoading = true;
    if (!silent) permissionsError = '';
    try {
      const [accessibility, microphone, inputMonitoring] = await Promise.all([
        invoke<string>('get_accessibility_permission_status'),
        invoke<string>('get_microphone_permission_status'),
        invoke<string>('get_input_monitoring_permission_status'),
      ]);
      // Report the REAL Accessibility status. We previously inferred it from the
      // event tap being alive, but the tap also runs on Input Monitoring alone —
      // so a granted tap with NO Accessibility showed a false "Granted", and the
      // user never enabled Accessibility, so synthetic Cmd+V (which needs it) was
      // silently dropped and nothing pasted. Input Monitoring keeps its own
      // empirical override server-side (has_seen_global_input).
      accessibilityPermission = (accessibility as MacPermissionStatus) || 'unknown';
      microphonePermission = (microphone as MacPermissionStatus) || 'unknown';
      inputMonitoringPermission = (inputMonitoring as MacPermissionStatus) || 'unknown';
    } catch {
      if (!silent) {
        permissionsError = 'Could not refresh permission status right now.';
      }
    } finally {
      permissionsLoading = false;
    }
  }

  async function triggerKeychainAccess() {
    if (!isMac || !provider || keychainLoading) return;
    keychainLoading = true;
    try {
      const result = await invoke<string>('check_keychain_access', { provider });
      keychainStatus = (result as KeychainStatus) || 'unknown';
    } catch {
      keychainStatus = 'denied';
    } finally {
      keychainLoading = false;
    }
  }

  function startPolling() {
    stopPolling();
    permissionsPollingStartMs = Date.now();
    pollInterval = setInterval(async () => {
      if (permissionsLoading) return;
      await refreshMacPermissions(true);
      // Compute synchronously from the freshly-updated state variables rather
      // than from the `allGranted` bindable, which is set via a $effect and may
      // not have re-run yet after the await above.
      const currentlyAllGranted =
        accessibilityPermission === 'authorized' &&
        inputMonitoringPermission === 'authorized' &&
        microphonePermission === 'authorized';
      // Input Monitoring granted mid-session needs a relaunch before the event
      // tap sees global keystrokes — surface the hint once the user acted on it.
      // Must run before the stopPolling() return so it is never skipped.
      if (inputMonitoringActionTaken && inputMonitoringPermission === 'authorized') {
        showRestartHint = true;
      } else if (
        accessibilityPromptAttempted &&
        accessibilityPermission !== 'authorized' &&
        permissionsPollingStartMs !== null &&
        Date.now() - permissionsPollingStartMs > 20_000
      ) {
        showRestartHint = true;
      }
      if (currentlyAllGranted) {
        stopPolling();
      }
    }, 2000);
  }

  function stopPolling() {
    if (pollInterval !== null) {
      clearInterval(pollInterval);
      pollInterval = null;
    }
    permissionsPollingStartMs = null;
  }

  async function requestAccessibilityPrompt() {
    if (!isMac) return;
    accessibilityPrompting = true;
    accessibilityPromptAttempted = true;
    permissionsError = '';
    try {
      await invoke('check_accessibility_permission', { prompt: true });
    } catch {}
    await refreshMacPermissions();
    accessibilityPrompting = false;
    startPolling();
  }

  async function requestInputMonitoringPrompt() {
    if (!isMac) return;
    inputMonitoringRequesting = true;
    inputMonitoringActionTaken = true;
    permissionsError = '';
    try {
      const result = await invoke<string>('request_input_monitoring_permission');
      inputMonitoringPermission = (result as MacPermissionStatus) || 'unknown';
    } catch {}
    await refreshMacPermissions();
    inputMonitoringRequesting = false;
    startPolling();
  }

  async function requestMicrophonePrompt() {
    if (!isMac) return;
    microphoneRequesting = true;
    permissionsError = '';
    try {
      const result = await invoke<string>('request_microphone_permission');
      microphonePermission = (result as MacPermissionStatus) || 'unknown';
    } catch {}
    await refreshMacPermissions();
    microphoneRequesting = false;
    startPolling();
  }

  async function relaunchApp() {
    restarting = true;
    try {
      await invoke('restart_app');
    } catch {
      // restart_app does not return on success; reaching here means it failed.
      restarting = false;
      permissionsError = 'Could not relaunch automatically — please quit and reopen Open Flow.';
    }
  }

  async function openPermissionSettings(kind: 'accessibility' | 'microphone' | 'input_monitoring') {
    try {
      if (kind === 'input_monitoring') inputMonitoringActionTaken = true;
      if (kind === 'accessibility') accessibilityPromptAttempted = true;
      const cmd =
        kind === 'accessibility' ? 'open_accessibility_settings'
        : kind === 'input_monitoring' ? 'open_input_monitoring_settings'
        : 'open_microphone_settings';
      await invoke(cmd);
      startPolling();
    } catch {
      permissionsError = 'Could not open System Settings.';
    }
  }

  function manualRefresh() {
    void refreshMacPermissions();
    void triggerKeychainAccess();
  }

  // Returning from System Settings refocuses the window — re-check immediately
  // rather than waiting for the next poll tick.
  function onWindowFocus() {
    if (!permissionsLoading) void refreshMacPermissions();
  }

  onMount(() => {
    if (!isMac) return;
    void refreshMacPermissions();
    void triggerKeychainAccess();
    startPolling();
    window.addEventListener('focus', onWindowFocus);
  });

  onDestroy(() => {
    stopPolling();
    if (isMac) window.removeEventListener('focus', onWindowFocus);
  });
</script>

{#if isMac}
  {#if variant === 'setup' && allGranted}
    <div class="permission-success" in:fly={{ y: -8, duration: motionMs(220), easing: expoOut }}>
      <svg width="13" height="13" viewBox="0 0 16 16" fill="none" aria-hidden="true" style="display:inline;vertical-align:-1px;margin-right:5px"><path d="M3 8l3.5 3.5L13 5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
      Core permissions granted — you're ready to continue.
    </div>
  {/if}

  <div class="perm-rows">
    <!-- Accessibility -->
    <div class="perm-row" class:perm-granted={accessibilityPermission === 'authorized'}>
      <div class="perm-row-top">
        <span class="perm-row-title">Accessibility</span>
        {#key accessibilityPermission}
          <span class="permission-badge" class:warn={accessibilityPermission !== 'authorized'} in:fly={{ y: -4, duration: motionMs(160), easing: expoOut }}>
            {permissionLabel(accessibilityPermission)}
          </span>
        {/key}
      </div>
      <p class="perm-row-desc">Lets Open Flow listen for the global hotkey and inject text into any app.</p>
      {#if accessibilityPermission !== 'authorized'}
        <div class="permission-actions">
          {#if accessibilityPermission === 'needs_permission' || accessibilityPermission === 'not_determined' || accessibilityPermission === 'unknown'}
            <button class="btn-ghost permission-btn" onclick={requestAccessibilityPrompt} disabled={accessibilityPrompting}>
              {accessibilityPrompting ? 'Prompting…' : 'Request'}
            </button>
          {/if}
          {#if accessibilityPermission !== 'restricted'}
            <button class="btn-ghost permission-btn" onclick={() => openPermissionSettings('accessibility')}>
              Open Settings
            </button>
          {:else}
            <p class="permission-restricted-note">Managed by your organization.</p>
          {/if}
        </div>
      {/if}
    </div>

    <!-- Input Monitoring -->
    <div class="perm-row" class:perm-granted={inputMonitoringPermission === 'authorized'}>
      <div class="perm-row-top">
        <span class="perm-row-title">Input Monitoring</span>
        {#key inputMonitoringPermission}
          <span class="permission-badge" class:warn={inputMonitoringPermission !== 'authorized'} in:fly={{ y: -4, duration: motionMs(160), easing: expoOut }}>
            {permissionLabel(inputMonitoringPermission)}
          </span>
        {/key}
      </div>
      <p class="perm-row-desc">Lets the global hotkey work while other apps are focused — without it, it only fires when Open Flow is frontmost.</p>
      {#if inputMonitoringPermission !== 'authorized'}
        <div class="permission-actions">
          {#if inputMonitoringPermission === 'not_determined' || inputMonitoringPermission === 'unknown'}
            <button class="btn-ghost permission-btn" onclick={requestInputMonitoringPrompt} disabled={inputMonitoringRequesting}>
              {inputMonitoringRequesting ? 'Prompting…' : 'Request'}
            </button>
          {/if}
          {#if inputMonitoringPermission !== 'restricted'}
            <button class="btn-ghost permission-btn" onclick={() => openPermissionSettings('input_monitoring')}>
              Open Settings
            </button>
          {:else}
            <p class="permission-restricted-note">Managed by your organization.</p>
          {/if}
        </div>
      {/if}
    </div>

    <!-- Microphone -->
    <div class="perm-row" class:perm-granted={microphonePermission === 'authorized'}>
      <div class="perm-row-top">
        <span class="perm-row-title">Microphone</span>
        {#key microphonePermission}
          <span class="permission-badge" class:warn={microphonePermission !== 'authorized'} in:fly={{ y: -4, duration: motionMs(160), easing: expoOut }}>
            {permissionLabel(microphonePermission)}
          </span>
        {/key}
      </div>
      <p class="perm-row-desc">Needed to capture your voice. macOS prompts on first recording if not yet granted.</p>
      {#if microphonePermission !== 'authorized'}
        <div class="permission-actions">
          {#if microphonePermission === 'restricted'}
            <p class="permission-restricted-note">Managed by your organization.</p>
          {:else}
            {#if microphonePermission === 'not_determined' || microphonePermission === 'unknown' || microphonePermission === 'needs_permission'}
              <button class="btn-ghost permission-btn" onclick={requestMicrophonePrompt} disabled={microphoneRequesting}>
                {microphoneRequesting ? 'Prompting…' : 'Request'}
              </button>
            {/if}
            <button class="btn-ghost permission-btn" onclick={() => openPermissionSettings('microphone')}>
              Open Settings
            </button>
          {/if}
        </div>
      {/if}
    </div>

    <!-- Keychain Access (optional) -->
    {#if provider}
      <div class="perm-row" class:perm-granted={keychainStatus === 'authorized'}>
        <div class="perm-row-top">
          <span class="perm-row-title">Keychain Access</span>
          {#key keychainStatus}
            <span class="permission-badge" class:warn={keychainStatus !== 'authorized'} in:fly={{ y: -4, duration: motionMs(160), easing: expoOut }}>
              {keychainLabel(keychainStatus)}
            </span>
          {/key}
        </div>
        <p class="perm-row-desc">
          {#if keychainStatus === 'not_configured'}
            No API key saved yet — add one in the API Keys tab.
          {:else if keychainStatus === 'denied'}
            Access denied. Click <strong>Unlock access</strong> or allow Open Flow in Keychain Access.app.
          {:else if keychainStatus === 'authorized'}
            Secures your API key and keeps it in your Keychain.
          {:else}
            Secures your API key. Open Flow prompts for access when it needs it.
          {/if}
        </p>
        {#if keychainStatus !== 'authorized' && keychainStatus !== 'not_configured'}
          <div class="permission-actions">
            <button class="btn-ghost permission-btn" onclick={triggerKeychainAccess} disabled={keychainLoading}>
              {keychainLoading ? 'Checking…' : 'Unlock access'}
            </button>
          </div>
        {/if}
      </div>
    {/if}
  </div>

  {#if permissionsError}
    <p class="permission-error">{permissionsError}</p>
  {/if}

  {#if showRestartHint}
    <div class="permission-restart-hint" transition:slide={{ duration: motionMs(200) }}>
      <span class="restart-hint-text">
        Just changed a permission in System Settings? macOS only applies it — especially
        <strong>Input Monitoring</strong> — after Open Flow relaunches.
      </span>
      <button class="btn-relaunch" onclick={relaunchApp} disabled={restarting}>
        {restarting ? 'Relaunching…' : 'Relaunch now'}
      </button>
    </div>
  {/if}

  <div class="permission-foot">
    <p class="permission-note">
      <strong>Tip:</strong> Flip a permission on in System Settings and this list updates on its own.
      If something stays stuck, use <strong>Relaunch</strong> — it forces macOS to re-read your grants.
    </p>
    <div class="permission-foot-actions">
      <button
        class="permission-refresh-btn"
        onclick={relaunchApp}
        disabled={restarting}
        title="Relaunch Open Flow to apply permission changes"
      >
        <span aria-hidden="true">⏻</span>
        {restarting ? 'Relaunching…' : 'Relaunch'}
      </button>
      <button
        class="permission-refresh-btn"
        onclick={manualRefresh}
        disabled={permissionsLoading || keychainLoading}
        title="Refresh permission status"
      >
        <span class:refresh-spin={permissionsLoading || keychainLoading} aria-hidden="true">↻</span>
        {permissionsLoading || keychainLoading ? 'Refreshing…' : 'Refresh'}
      </button>
    </div>
  </div>
{/if}

<style>
  .permission-success {
    display: flex;
    align-items: center;
    background: var(--accent-soft);
    background: color-mix(in srgb, var(--accent-soft) 65%, var(--paper));
    border: 1px solid var(--line);
    border: 1px solid color-mix(in srgb, var(--accent) 28%, var(--line));
    border-radius: var(--r-sm);
    padding: 9px 13px;
    margin-bottom: 12px;
    font-size: 13px;
    color: var(--accent-ink);
    font-weight: 500;
  }

  /* 2×2 card grid — same visual language as the "things worth knowing" step. */
  .perm-rows {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
    align-items: stretch;
  }

  .perm-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: var(--bg-elev);
    border: 1.5px solid var(--line);
    border-radius: var(--r-md);
    padding: 14px 14px;
    transition: border-color 0.25s, background 0.25s;
  }

  .perm-row.perm-granted {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .perm-row-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .perm-row-title {
    font-size: 13.5px;
    font-weight: 500;
    color: var(--ink-strong);
    margin: 0;
  }

  .perm-row-desc {
    font-size: 12px;
    color: var(--ink-mute);
    margin: 0;
    line-height: 1.4;
    flex: 1;
  }

  .perm-row-desc strong { color: var(--accent-ink); font-weight: 600; }

  @keyframes badge-pop {
    from { transform: scale(0.82); opacity: 0.5; }
    to   { transform: scale(1);    opacity: 1;   }
  }

  .permission-badge {
    border-radius: 999px;
    padding: 3px 9px;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.02em;
    background: var(--accent-soft);
    color: var(--accent-ink);
    white-space: nowrap;
    animation: badge-pop 0.2s ease-out;
    transition: background 0.2s, color 0.2s;
  }

  .perm-row.perm-granted .permission-badge {
    background: var(--accent-soft);
    background: color-mix(in srgb, var(--accent) 15%, var(--accent-soft));
  }

  .permission-badge.warn { background: var(--warning-bg); color: var(--warning); }

  .permission-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    justify-content: flex-start;
    margin-top: auto;
    padding-top: 2px;
  }

  .permission-restricted-note {
    font-size: 11.5px;
    color: var(--ink-mute);
    margin: 0;
    font-style: italic;
    text-align: left;
  }

  .permission-btn {
    padding: 5px 11px;
    border-radius: 999px;
    font-size: 11.5px;
  }

  .permission-error {
    margin: 12px 0 0;
    color: var(--danger);
    font-size: 12px;
  }

  .permission-restart-hint {
    display: flex;
    align-items: center;
    gap: 12px;
    background: var(--bg-elev);
    background: color-mix(in srgb, oklch(75% 0.12 60) 12%, var(--paper));
    border: 1px solid var(--line);
    border: 1px solid color-mix(in srgb, oklch(75% 0.12 60) 30%, var(--line));
    border-radius: var(--r-sm);
    padding: 10px 12px;
    margin-top: 12px;
    font-size: 12px;
    color: var(--ink-soft);
    line-height: 1.45;
  }

  .restart-hint-text { flex: 1; }
  .restart-hint-text strong { color: var(--ink-strong); }

  .btn-relaunch {
    flex-shrink: 0;
    border: 0;
    border-radius: 999px;
    padding: 6px 14px;
    font-size: 12px;
    font-weight: 600;
    font-family: inherit;
    color: var(--paper);
    background: var(--accent);
    cursor: pointer;
    white-space: nowrap;
    transition: filter 0.15s;
  }
  .btn-relaunch:hover { filter: brightness(1.06); }
  .btn-relaunch:disabled { opacity: 0.6; cursor: default; }

  .permission-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    margin-top: 14px;
  }

  .permission-foot-actions {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-shrink: 0;
  }

  .permission-note {
    flex: 1;
    background: var(--accent-soft);
    background: color-mix(in srgb, var(--accent-soft) 55%, var(--paper));
    border: 1px solid var(--line);
    border: 1px solid color-mix(in srgb, var(--accent) 18%, var(--line));
    border-radius: var(--r-sm);
    padding: 9px 12px;
    margin: 0;
    font-size: 12px;
    color: var(--ink-soft);
    line-height: 1.45;
  }

  .permission-note strong { color: var(--ink-strong); }

  .permission-refresh-btn {
    background: transparent;
    border: none;
    padding: 0;
    font-size: 12px;
    color: var(--ink-faint);
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 4px;
    transition: color 0.15s;
    font-family: inherit;
    flex-shrink: 0;
    white-space: nowrap;
  }

  .permission-refresh-btn:hover { color: var(--ink-soft); }
  .permission-refresh-btn:disabled { cursor: default; }

  @keyframes spin { to { transform: rotate(360deg); } }

  .refresh-spin { animation: spin 0.75s linear infinite; display: inline-block; }

  /* Collapse the 2×2 grid to a single column on narrow widths. */
  @media (max-width: 520px) {
    .perm-rows { grid-template-columns: 1fr; }
  }
</style>
