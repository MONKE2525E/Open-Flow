<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { fly, slide } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { invoke, listen } from '../tauri';
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
  type MacPermissionSnapshot = {
    accessibility: MacPermissionStatus;
    inputMonitoring: MacPermissionStatus;
    microphone: MacPermissionStatus;
    keychain: KeychainStatus;
    allCoreGranted: boolean;
    needsRelaunch: boolean;
    lastCheckedAt: string;
    sourceHints: {
      hotkeyTapActive: boolean;
      globalInputSeen: boolean;
      microphoneVerified: boolean;
      accessibilityVerified: boolean;
    };
    diagnostics: {
      bundleIdentifier: string | null;
      bundlePath: string | null;
      executablePath: string | null;
      processId: number;
      accessibilityTrusted: boolean;
      inputMonitoringRaw: MacPermissionStatus;
    };
  };
  type TccResetResult = {
    bundleIdentifier: string | null;
    steps: Array<{ service: string; ok: boolean; message: string }>;
  };

  type Props = {
    /** True once Accessibility + Input Monitoring + Microphone are all granted. */
    allGranted?: boolean;
    /** 'setup' shows a success banner once all granted; 'settings' is steady-state. */
    variant?: 'setup' | 'settings';
    /** When provided, also surfaces the Keychain Access row for that provider. */
    provider?: ProviderId | null;
  };

  let { allGranted = $bindable(false), variant = 'settings', provider = null }: Props = $props();

  let snapshot = $state<MacPermissionSnapshot>({
    accessibility: isMac ? 'unknown' : 'authorized',
    inputMonitoring: isMac ? 'unknown' : 'authorized',
    microphone: isMac ? 'unknown' : 'authorized',
    keychain: 'unknown',
    allCoreGranted: !isMac,
    needsRelaunch: false,
    lastCheckedAt: '',
    sourceHints: {
      hotkeyTapActive: !isMac,
      globalInputSeen: !isMac,
      microphoneVerified: !isMac,
      accessibilityVerified: !isMac,
    },
    diagnostics: {
      bundleIdentifier: null,
      bundlePath: null,
      executablePath: null,
      processId: 0,
      accessibilityTrusted: !isMac,
      inputMonitoringRaw: !isMac ? 'authorized' : 'unknown',
    },
  });
  let keychainProvider = $state<ProviderId | null>(null);
  let permissionsLoading = $state(false);
  let permissionsError = $state('');
  let accessibilityPrompting = $state(false);
  let inputMonitoringRequesting = $state(false);
  let microphoneRequesting = $state(false);
  let keychainLoading = $state(false);
  let repairing = $state(false);
  let restarting = $state(false);

  let accessibilityActionTaken = $state(false);
  let inputMonitoringActionTaken = $state(false);
  let watchInterval: ReturnType<typeof setInterval> | null = null;
  let watchTicksLeft = 0;
  let restartTimeout: ReturnType<typeof setTimeout> | null = null;
  let unlistenError: (() => void) | null = null;
  let active = true;

  const accessibilityPermission = $derived(snapshot.accessibility);
  const inputMonitoringPermission = $derived(snapshot.inputMonitoring);
  const microphonePermission = $derived(snapshot.microphone);
  const keychainStatus = $derived(snapshot.keychain);
  const showKeychainRow = $derived(!!keychainProvider && keychainStatus !== 'not_configured');

  let showDiagnostics = $state(false);

  type StatusKind = 'granted' | 'checking' | 'attention';
  function statusKind(status: MacPermissionStatus | KeychainStatus): StatusKind {
    if (status === 'authorized') return 'granted';
    if (status === 'unknown') return 'checking';
    return 'attention';
  }
  const showRestartHint = $derived(
    snapshot.needsRelaunch &&
    (accessibilityActionTaken || inputMonitoringActionTaken) &&
    (accessibilityPermission === 'authorized' || inputMonitoringPermission === 'authorized')
  );
  const showRepairHint = $derived(
    accessibilityPermission !== 'authorized' || inputMonitoringPermission !== 'authorized'
  );

  // Keep the bindable flag in sync with the three core OS permissions.
  $effect(() => {
    allGranted = snapshot.allCoreGranted;
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

  async function resolveKeychainProvider() {
    if (!isMac || !provider) {
      keychainProvider = null;
      return null;
    }
    try {
      const keyStatus = await invoke<Record<ProviderId, boolean>>('get_api_key_status');
      keychainProvider = keyStatus?.[provider] ? provider : null;
    } catch {
      keychainProvider = null;
    }
    return keychainProvider;
  }

  function applySnapshot(next: MacPermissionSnapshot, providerOverride: ProviderId | null) {
    snapshot = {
      ...next,
      accessibility: next.accessibility || 'unknown',
      inputMonitoring: next.inputMonitoring || 'unknown',
      microphone: next.microphone || 'unknown',
      keychain: providerOverride
        ? next.keychain || 'unknown'
        : keychainProvider
          ? snapshot.keychain
          : 'unknown',
    };
    return snapshot;
  }

  async function readSnapshot(providerOverride = keychainProvider) {
    const next = await invoke<MacPermissionSnapshot>('get_macos_permission_snapshot', {
      provider: providerOverride,
    });
    return applySnapshot(next, providerOverride);
  }

  async function refreshMacPermissions(silent = false) {
    if (!isMac || permissionsLoading) return;
    permissionsLoading = true;
    if (!silent) permissionsError = '';
    try {
      await resolveKeychainProvider();
      await readSnapshot(null);
    } catch {
      if (!silent) {
        permissionsError = 'Could not refresh permission status right now.';
      }
    } finally {
      permissionsLoading = false;
    }
  }

  async function triggerKeychainAccess() {
    if (!isMac || !keychainProvider || keychainLoading) return;
    keychainLoading = true;
    permissionsError = '';
    try {
      await readSnapshot(keychainProvider);
    } catch {
      snapshot = { ...snapshot, keychain: 'denied' };
    } finally {
      keychainLoading = false;
    }
  }

  // We deliberately do NOT poll in steady state. Permissions are re-checked on
  // mount (startup), when the window regains focus (returning from System
  // Settings), and when a dictation fails for a permission reason (see the
  // verenu:error listener). `startWatch()` only runs a short, self-terminating
  // burst right after the user takes a grant action, so a change made in System
  // Settings is reflected promptly — then it stops. It never runs indefinitely.
  const WATCH_TICKS = 6; // ~12s at the cadence below
  const WATCH_INTERVAL_MS = 2000;

  function startWatch() {
    if (!isMac) return;
    watchTicksLeft = WATCH_TICKS;
    if (watchInterval !== null) return; // already watching — ticks were just refilled
    watchInterval = setInterval(async () => {
      if (permissionsLoading) return;
      await refreshMacPermissions(true);
      watchTicksLeft -= 1;
      if ((snapshot.allCoreGranted && !snapshot.needsRelaunch) || watchTicksLeft <= 0) {
        stopWatch();
      }
    }, WATCH_INTERVAL_MS);
  }

  function stopWatch() {
    if (watchInterval !== null) {
      clearInterval(watchInterval);
      watchInterval = null;
    }
    watchTicksLeft = 0;
  }

  function looksLikePermissionError(message: string): boolean {
    const m = message.toLowerCase();
    return (
      m.includes('permission') ||
      m.includes('accessibility') ||
      m.includes('input monitoring') ||
      m.includes('microphone') ||
      m.includes('system settings')
    );
  }

  async function requestAccessibilityPrompt() {
    if (!isMac) return;
    accessibilityPrompting = true;
    accessibilityActionTaken = true;
    permissionsError = '';
    try {
      const next = await invoke<MacPermissionSnapshot>('request_accessibility_permission', { provider: null });
      applySnapshot(next, null);
    } catch {
      permissionsError = 'Could not request Accessibility permission.';
    }
    accessibilityPrompting = false;
    startWatch();
  }

  async function requestInputMonitoringPrompt() {
    if (!isMac) return;
    inputMonitoringRequesting = true;
    inputMonitoringActionTaken = true;
    permissionsError = '';
    try {
      const next = await invoke<MacPermissionSnapshot>('request_input_monitoring_permission_snapshot', { provider: null });
      applySnapshot(next, null);
    } catch {
      permissionsError = 'Could not request Input Monitoring permission.';
    }
    inputMonitoringRequesting = false;
    startWatch();
  }

  async function requestMicrophonePrompt() {
    if (!isMac) return;
    microphoneRequesting = true;
    permissionsError = '';
    try {
      const next = await invoke<MacPermissionSnapshot>('request_microphone_permission_snapshot', { provider: null });
      applySnapshot(next, null);
    } catch {
      permissionsError = 'Could not request Microphone permission.';
    }
    microphoneRequesting = false;
    startWatch();
  }

  async function relaunchApp() {
    restarting = true;
    permissionsError = '';
    restartTimeout = setTimeout(() => {
      restarting = false;
      permissionsError = 'Could not relaunch automatically — please quit and reopen Open Flow.';
    }, 5000);
    void invoke('restart_app').catch(() => {
      // Ignore immediate IPC disconnection errors during restart.
    });
  }

  async function repairStaleGrants() {
    if (!isMac || repairing) return;
    repairing = true;
    permissionsError = '';
    try {
      const result = await invoke<TccResetResult>('reset_macos_core_permissions');
      const failed = result.steps.filter((step) => !step.ok);
      if (failed.length > 0) {
        permissionsError = `Could not reset ${failed.map((step) => step.service).join(', ')}.`;
      } else {
        permissionsError = 'Old macOS grants were reset. Add Open Flow again in Accessibility and Input Monitoring, then relaunch.';
      }
      await invoke('open_accessibility_settings');
      startWatch();
      await refreshMacPermissions(true);
    } catch {
      permissionsError = 'Could not reset stale macOS permission grants.';
    } finally {
      repairing = false;
    }
  }

  async function openPermissionSettings(kind: 'accessibility' | 'microphone' | 'input_monitoring') {
    try {
      if (kind === 'input_monitoring') inputMonitoringActionTaken = true;
      if (kind === 'accessibility') accessibilityActionTaken = true;
      const cmd =
        kind === 'accessibility' ? 'open_accessibility_settings'
        : kind === 'input_monitoring' ? 'open_input_monitoring_settings'
        : 'open_microphone_settings';
      await invoke(cmd);
      startWatch();
    } catch {
      permissionsError = 'Could not open System Settings.';
    }
  }

  function manualRefresh() {
    void refreshMacPermissions();
  }

  // Returning from System Settings refocuses the window — re-check immediately.
  function onWindowFocus() {
    if (!permissionsLoading) void refreshMacPermissions();
  }

  onMount(() => {
    if (!isMac) return;
    // Check once at startup. No steady-state polling — see startWatch().
    void refreshMacPermissions();
    window.addEventListener('focus', onWindowFocus);
    // Re-check when a dictation fails for a permission-related reason, so a
    // revoked/missing grant surfaces here without continuous polling.
    listen<string>('verenu:error', (ev) => {
      if (active && typeof ev.payload === 'string' && looksLikePermissionError(ev.payload)) {
        void refreshMacPermissions(true);
      }
    }).then((un) => {
      if (active) {
        unlistenError = un;
      } else {
        un();
      }
    });
  });

  onDestroy(() => {
    active = false;
    stopWatch();
    if (restartTimeout) clearTimeout(restartTimeout);
    if (isMac) window.removeEventListener('focus', onWindowFocus);
    unlistenError?.();
  });
</script>

{#snippet statusIndicator(status: MacPermissionStatus | KeychainStatus, label: string)}
  {@const kind = statusKind(status)}
  <span class="perm-status perm-status-{kind}">
    <span class="perm-status-dot" aria-hidden="true"></span>
    {#key label}
      <span class="perm-status-label" in:fly={{ y: -3, duration: motionMs(140), easing: expoOut }}>{label}</span>
    {/key}
  </span>
{/snippet}

{#if isMac}
  <div class="mac-permissions">
  {#if variant === 'setup' && allGranted}
    <div class="permission-success" in:fly={{ y: -8, duration: motionMs(220), easing: expoOut }}>
      <svg width="13" height="13" viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M3 8l3.5 3.5L13 5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
      Core permissions granted — you're ready to continue.
    </div>
  {/if}

  <div class="perm-list">
    <!-- Accessibility -->
    <div class="perm-row">
      <div class="perm-row-main">
        <div class="perm-row-title">Accessibility</div>
        <div class="perm-row-desc">Lets Open Flow listen for the global hotkey and inject text into any app.</div>
      </div>
      <div class="perm-row-side">
        {@render statusIndicator(accessibilityPermission, permissionLabel(accessibilityPermission))}
        {#if accessibilityPermission === 'restricted'}
          <span class="perm-restricted">Managed by org</span>
        {:else if accessibilityPermission !== 'authorized'}
          {#if accessibilityPermission === 'needs_permission' || accessibilityPermission === 'not_determined' || accessibilityPermission === 'unknown'}
            <button class="perm-action" onclick={requestAccessibilityPrompt} disabled={accessibilityPrompting}>
              {accessibilityPrompting ? 'Prompting…' : 'Request'}
            </button>
          {/if}
          <button class="perm-action" onclick={() => openPermissionSettings('accessibility')}>Open Settings</button>
        {/if}
      </div>
    </div>

    <!-- Input Monitoring -->
    <div class="perm-row">
      <div class="perm-row-main">
        <div class="perm-row-title">Input Monitoring</div>
        <div class="perm-row-desc">Keeps the hotkey working while other apps are focused — without it, it only fires when Open Flow is frontmost.</div>
      </div>
      <div class="perm-row-side">
        {@render statusIndicator(inputMonitoringPermission, permissionLabel(inputMonitoringPermission))}
        {#if inputMonitoringPermission === 'restricted'}
          <span class="perm-restricted">Managed by org</span>
        {:else if inputMonitoringPermission !== 'authorized'}
          {#if inputMonitoringPermission === 'not_determined' || inputMonitoringPermission === 'unknown'}
            <button class="perm-action" onclick={requestInputMonitoringPrompt} disabled={inputMonitoringRequesting}>
              {inputMonitoringRequesting ? 'Prompting…' : 'Request'}
            </button>
          {/if}
          <button class="perm-action" onclick={() => openPermissionSettings('input_monitoring')}>Open Settings</button>
        {/if}
      </div>
    </div>

    <!-- Microphone -->
    <div class="perm-row">
      <div class="perm-row-main">
        <div class="perm-row-title">Microphone</div>
        <div class="perm-row-desc">Needed to capture your voice. macOS prompts on first recording if not yet granted.</div>
      </div>
      <div class="perm-row-side">
        {@render statusIndicator(microphonePermission, permissionLabel(microphonePermission))}
        {#if microphonePermission === 'restricted'}
          <span class="perm-restricted">Managed by org</span>
        {:else if microphonePermission !== 'authorized'}
          {#if microphonePermission === 'not_determined' || microphonePermission === 'unknown' || microphonePermission === 'needs_permission'}
            <button class="perm-action" onclick={requestMicrophonePrompt} disabled={microphoneRequesting}>
              {microphoneRequesting ? 'Prompting…' : 'Request'}
            </button>
          {/if}
          <button class="perm-action" onclick={() => openPermissionSettings('microphone')}>Open Settings</button>
        {/if}
      </div>
    </div>

    <!-- Keychain Access (optional) -->
    {#if showKeychainRow}
      <div class="perm-row">
        <div class="perm-row-main">
          <div class="perm-row-title">Keychain Access</div>
          <div class="perm-row-desc">
            {#if keychainStatus === 'denied'}
              Access denied. Click <strong>Unlock access</strong> or allow Open Flow in Keychain Access.app.
            {:else if keychainStatus === 'authorized'}
              Secures your API key and keeps it in your Keychain.
            {:else}
              Secures your API key. Open Flow prompts for access when it needs it.
            {/if}
          </div>
        </div>
        <div class="perm-row-side">
          {@render statusIndicator(keychainStatus, keychainLabel(keychainStatus))}
          {#if keychainStatus !== 'authorized' && keychainStatus !== 'not_configured'}
            <button class="perm-action" onclick={triggerKeychainAccess} disabled={keychainLoading}>
              {keychainLoading ? 'Checking…' : 'Unlock access'}
            </button>
          {/if}
        </div>
      </div>
    {/if}
  </div>

  {#if permissionsError}
    <p class="permission-error">{permissionsError}</p>
  {/if}

  {#if showRestartHint}
    <div class="permission-restart-hint" transition:slide={{ duration: motionMs(200) }}>
      <span class="restart-hint-text">
        Just changed a permission in System Settings? Relaunch Open Flow so macOS
        re-reads the latest Accessibility and Input Monitoring grants.
      </span>
      <button class="btn-relaunch" onclick={relaunchApp} disabled={restarting}>
        {restarting ? 'Relaunching…' : 'Relaunch now'}
      </button>
    </div>
  {/if}

  <div class="permission-foot">
    <button
      class="permission-details-toggle"
      class:open={showDiagnostics}
      onclick={() => (showDiagnostics = !showDiagnostics)}
      aria-expanded={showDiagnostics}
    >
      <svg class="chev" width="11" height="11" viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M5 3l5 5-5 5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
      Details
    </button>
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
        disabled={permissionsLoading}
        title="Refresh permission status"
      >
        <span class:refresh-spin={permissionsLoading} aria-hidden="true">↻</span>
        {permissionsLoading ? 'Refreshing…' : 'Refresh'}
      </button>
    </div>
  </div>

  {#if showDiagnostics}
    <div class="permission-diagnostics" transition:slide={{ duration: motionMs(200) }}>
      {#if showRepairHint}
        <div class="permission-repair">
          <p class="repair-copy">
            <strong>Permission shows as enabled in System Settings but still isn't working?</strong>
            macOS can hold a stale grant tied to an older build of Open Flow. Clear the
            old grants so you can re-add this version fresh.
          </p>
          <button class="btn-repair" onclick={repairStaleGrants} disabled={repairing}>
            {repairing ? 'Clearing…' : 'Clear old grants & re-request'}
          </button>
        </div>
      {/if}
      <p class="permission-note">
        <strong>Bundle checked:</strong>
        {snapshot.diagnostics.bundleIdentifier ?? 'Unknown bundle'}
        {#if snapshot.diagnostics.bundlePath}
          <br />{snapshot.diagnostics.bundlePath}
        {/if}
      </p>
      <p class="diag-line">
        Raw OS state — Accessibility: <strong>{snapshot.diagnostics.accessibilityTrusted ? 'trusted' : 'not trusted'}</strong>,
        Input Monitoring: <strong>{snapshot.diagnostics.inputMonitoringRaw}</strong>.
        If these disagree with the status above, macOS is likely holding a stale grant from a previous build.
      </p>
    </div>
  {/if}
  </div>
{/if}

<style>
  /* Single block root so the component lays out in normal flow regardless of the
     parent (Settings is a block container; the Setup shell centers a flex row). */
  .mac-permissions { display: block; width: 100%; }

  /* Flat row list — mirrors the app's .setting-row pattern so Permissions feels
     native in both Settings and the Setup wizard. */
  .permission-success {
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--accent-soft, #e6f4ff);
    background: color-mix(in srgb, var(--accent-soft) 65%, var(--paper));
    border: 1px solid var(--line, #d0d0d0);
    border: 1px solid color-mix(in srgb, var(--accent) 28%, var(--line));
    border-radius: var(--r-sm);
    padding: 9px 13px;
    margin-bottom: 14px;
    font-size: 13px;
    color: var(--accent-ink);
    font-weight: 500;
  }

  .perm-list { display: flex; flex-direction: column; }

  .perm-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 13px 0;
    border-top: 1px solid var(--line);
  }

  .perm-row:last-child { border-bottom: 1px solid var(--line); }

  .perm-row-main { min-width: 0; }

  .perm-row-title { font-size: 13px; font-weight: 500; color: var(--ink-strong); }

  .perm-row-desc {
    font-size: 12px;
    color: var(--ink-mute);
    margin-top: 3px;
    line-height: 1.45;
    max-width: 48ch;
  }

  .perm-row-desc strong { color: var(--ink-soft); font-weight: 600; }

  .perm-row-side {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
    flex-shrink: 0;
    flex-wrap: wrap;
  }

  /* Status: a small dot + label, matching the app's restrained accent usage. */
  .perm-status {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 500;
    white-space: nowrap;
  }

  .perm-status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
    background: var(--ink-faint);
    transition: background 0.2s;
  }

  .perm-status-granted { color: var(--ink-soft); }
  .perm-status-granted .perm-status-dot { background: var(--success); }

  .perm-status-attention { color: var(--ink-soft); }
  .perm-status-attention .perm-status-dot { background: var(--warning); }

  .perm-status-checking { color: var(--ink-mute); }
  .perm-status-checking .perm-status-dot { animation: status-pulse 1.1s ease-in-out infinite; }

  @keyframes status-pulse { 0%, 100% { opacity: 0.35; } 50% { opacity: 1; } }

  /* Action buttons — mirror the global .btn-ghost so they read as native. */
  .perm-action {
    background: transparent;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    padding: 5px 12px;
    font-size: 12px;
    font-family: inherit;
    color: var(--ink-strong);
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.12s, border-color 0.12s;
  }

  .perm-action:hover { background: var(--control-hover); }
  .perm-action:disabled { opacity: 0.4; cursor: default; }

  .perm-restricted { font-size: 12px; color: var(--ink-mute); font-style: italic; }

  .permission-error { margin: 12px 0 0; color: var(--danger); font-size: 12px; }

  /* Auxiliary hint — relaunch banner. */
  .permission-restart-hint {
    display: flex;
    align-items: center;
    gap: 12px;
    border-radius: var(--r-sm);
    padding: 10px 12px;
    margin-top: 12px;
    font-size: 12px;
    color: var(--ink-soft);
    line-height: 1.45;
    background: var(--paper, #fafafa);
    background: color-mix(in srgb, oklch(75% 0.12 60) 12%, var(--paper));
    border: 1px solid var(--line, #d0d0d0);
    border: 1px solid color-mix(in srgb, oklch(75% 0.12 60) 30%, var(--line));
  }

  .restart-hint-text { flex: 1; }

  /* Repair action, shown inside the expanded Details section when a core
     permission isn't granted — clears stale TCC grants and re-requests. */
  .permission-repair {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    border-radius: var(--r-sm);
    padding: 10px 12px;
    background: var(--warning-bg, #fff3cd);
    background: color-mix(in srgb, var(--warning-bg) 56%, var(--paper));
    border: 1px solid var(--line, #d0d0d0);
    border: 1px solid color-mix(in srgb, var(--warning) 26%, var(--line));
  }

  .repair-copy {
    flex: 1;
    min-width: 200px;
    margin: 0;
    font-size: 11.5px;
    color: var(--ink-soft);
    line-height: 1.5;
  }

  .repair-copy strong { color: var(--ink-strong); display: block; margin-bottom: 2px; }

  .btn-relaunch,
  .btn-repair {
    flex-shrink: 0;
    border: 0;
    border-radius: 6px;
    padding: 6px 13px;
    font-size: 12px;
    font-weight: 600;
    font-family: inherit;
    color: var(--paper);
    background: var(--accent);
    cursor: pointer;
    white-space: nowrap;
    transition: filter 0.15s;
  }
  .btn-relaunch:hover,
  .btn-repair:hover { filter: brightness(1.06); }
  .btn-relaunch:disabled,
  .btn-repair:disabled { opacity: 0.6; cursor: default; }

  .permission-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    margin-top: 14px;
  }

  .permission-foot-actions { display: flex; align-items: center; gap: 16px; flex-shrink: 0; }

  .permission-details-toggle {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    background: transparent;
    border: none;
    padding: 0;
    font-family: inherit;
    font-size: 12px;
    color: var(--ink-faint);
    cursor: pointer;
    transition: color 0.15s;
  }

  .permission-details-toggle:hover { color: var(--ink-soft); }
  .permission-details-toggle .chev { transition: transform 0.2s ease; }
  .permission-details-toggle.open .chev { transform: rotate(90deg); }

  .permission-diagnostics { margin-top: 10px; display: flex; flex-direction: column; gap: 8px; }

  .permission-note {
    background: var(--paper-2);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    padding: 9px 12px;
    margin: 0;
    font-size: 11.5px;
    color: var(--ink-soft);
    line-height: 1.5;
    word-break: break-word;
  }

  .permission-note strong { color: var(--ink-strong); }

  .diag-line { margin: 0; font-size: 11.5px; color: var(--ink-mute); line-height: 1.5; }
  .diag-line strong { color: var(--ink-soft); font-weight: 600; }

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

  @media (max-width: 560px) {
    .perm-row { flex-direction: column; align-items: stretch; gap: 8px; }
    .perm-row-side { justify-content: flex-start; }
  }

  @media (prefers-reduced-motion: reduce) {
    .perm-status-checking .perm-status-dot { animation: none; }
    .permission-details-toggle .chev { transition: none; }
  }
</style>
