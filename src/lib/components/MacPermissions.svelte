<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { fly, slide } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { invoke, listen } from '../tauri';
  import { isMac } from '../platform';
  import { motionMs } from '../motion';
  import { extractIpcErrorMessage } from '../errors';
  import type { ProviderId } from '../settings';

  type MacPermissionStatus =
    | 'authorized'
    | 'not_granted'
    | 'not_determined'
    | 'denied'
    | 'restricted'
    | 'unknown';
  type KeychainStatus = 'available' | 'configuration_error' | 'authentication_required' | 'interaction_unavailable' | 'not_checked' | 'unknown' | 'error';
  type KeychainDiagnostic = { state: KeychainStatus; operation: string; osStatus: number; osStatusMeaning: string };
  type NotificationPermission = { authorization: MacPermissionStatus | 'provisional' | 'error'; alerts: string; sounds: string; badges: string; notificationCenter: string; lockScreen: string; rawAuthorization: number | null };
  type MacPermissionSnapshot = {
    accessibility: MacPermissionStatus;
    microphone: MacPermissionStatus;
    notifications: NotificationPermission;
    keychain: KeychainStatus;
    allCoreGranted: boolean;
    lastCheckedAt: string;
    diagnostics: {
      bundleIdentifier: string | null;
      bundleDisplayName: string | null;
      bundleName: string | null;
      bundlePath: string | null;
      executablePath: string | null;
      bundleUrl: string | null;
      executableUrl: string | null;
      bundleUrlExtension: string | null;
      isRunningInsideApp: boolean;
      processId: number;
      processName: string;
      macosVersion: string;
      signingIdentity: string | null;
      teamIdentifier: string | null;
      buildProfile: string;
      snapshotGeneration: number;
      accessibilityTrusted: boolean;
      microphoneAvAudioStatus: MacPermissionStatus | null;
      microphoneAvAudioRaw: number | null;
      microphoneAvAudioFourcc: string | null;
      microphoneAvCaptureStatus: MacPermissionStatus;
      microphoneAvCaptureRaw: number;
    };
  };
  type TccResetResult = {
    bundleIdentifier: string | null;
    steps: Array<{ service: string; ok: boolean; message: string }>;
  };

  type Props = {
    /** True once Accessibility + Microphone are both granted. */
    allGranted?: boolean;
    /** 'setup' shows a success banner once all granted; 'settings' is steady-state. */
    variant?: 'setup' | 'settings';
    /** When provided, also surfaces the Keychain Access row for that provider. */
    provider?: ProviderId | null;
  };

  let { allGranted = $bindable(false), variant = 'settings', provider = null }: Props = $props();

  let snapshot = $state<MacPermissionSnapshot>({
    accessibility: isMac ? 'unknown' : 'authorized',
    microphone: isMac ? 'unknown' : 'authorized',
    notifications: { authorization: isMac ? 'unknown' : 'authorized', alerts: 'unknown', sounds: 'unknown', badges: 'unknown', notificationCenter: 'unknown', lockScreen: 'unknown', rawAuthorization: null },
    keychain: 'not_checked',
    allCoreGranted: !isMac,
    lastCheckedAt: '',
    diagnostics: {
      bundleIdentifier: null,
      bundleDisplayName: null,
      bundleName: null,
      bundlePath: null,
      executablePath: null,
      bundleUrl: null,
      executableUrl: null,
      bundleUrlExtension: null,
      isRunningInsideApp: false,
      processId: 0,
      processName: '',
      macosVersion: '',
      signingIdentity: null,
      teamIdentifier: null,
      buildProfile: '',
      snapshotGeneration: 0,
      accessibilityTrusted: !isMac,
      microphoneAvAudioStatus: null,
      microphoneAvAudioRaw: null,
      microphoneAvAudioFourcc: null,
      microphoneAvCaptureStatus: isMac ? 'unknown' : 'authorized',
      microphoneAvCaptureRaw: isMac ? -1 : 3,
    },
  });
  let permissionsLoading = $state(false);
  let refreshAnimating = $state(false);
  let permissionsError = $state('');
  let accessibilityPrompting = $state(false);
  let microphoneRequesting = $state(false);
  let keychainLoading = $state(false);
  let keychainDiagnostic = $state<KeychainDiagnostic | null>(null);
  let repairing = $state(false);
  let restarting = $state(false);

  let accessibilityActionTaken = $state(false);
  let watchInterval: ReturnType<typeof setInterval> | null = null;
  let restartTimeout: ReturnType<typeof setTimeout> | null = null;
  let refreshAnimationTimeout: ReturnType<typeof setTimeout> | null = null;
  let unlistenError: (() => void) | null = null;
  let active = true;
  let refreshGeneration = 0;
  let autoCheckedKeychainProvider: ProviderId | null = null;

  const accessibilityPermission = $derived(snapshot.accessibility);
  const microphonePermission = $derived(snapshot.microphone);
  const notificationPermission = $derived(snapshot.notifications);
  const keychainStatus = $derived(snapshot.keychain);
  const keychainProvider = $derived(provider);
  const showKeychainRow = $derived(variant === 'settings' && !!keychainProvider);

  let showDiagnostics = $state(false);

  type StatusKind = 'granted' | 'checking' | 'attention';
  function statusKind(status: MacPermissionStatus | KeychainStatus): StatusKind {
    if (status === 'authorized' || status === 'available') return 'granted';
    if (status === 'unknown') return 'attention';
    return 'attention';
  }
  const showRepairHint = $derived(accessibilityPermission !== 'authorized');
  const canRepairStaleGrant = $derived(
    showRepairHint && accessibilityActionTaken && !!snapshot.diagnostics.bundleIdentifier,
  );
  const invalidDevLaunch = $derived(
    isMac && snapshot.diagnostics.processId > 0 && !snapshot.diagnostics.bundleIdentifier,
  );

  // Keep the bindable flag in sync with the core OS permissions.
  $effect(() => {
    allGranted = snapshot.allCoreGranted;
  });

  // Provider settings can arrive after this component mounts. Run the full
  // Verenu-owned Keychain coverage check once when that provider becomes
  // available, rather than depending on mount timing.
  $effect(() => {
    const currentProvider = keychainProvider;
    if (
      isMac &&
      variant === 'settings' &&
      currentProvider &&
      currentProvider !== autoCheckedKeychainProvider
    ) {
      autoCheckedKeychainProvider = currentProvider;
      void triggerKeychainAccess();
    }
  });

  function permissionLabel(status: MacPermissionStatus) {
    switch (status) {
      case 'authorized': return 'Granted';
      case 'not_determined': return 'Not yet asked';
      case 'denied': return 'Denied';
      case 'restricted': return 'Restricted by org';
      case 'not_granted': return 'Not granted';
      default: return 'Unavailable';
    }
  }

  function keychainLabel(status: KeychainStatus) {
    switch (status) {
      case 'available': return 'Available';
      case 'configuration_error': return 'Configuration error';
      case 'authentication_required': return 'Authentication required';
      case 'interaction_unavailable': return 'Interaction unavailable';
      case 'not_checked': return 'Not checked';
      case 'error': return 'Error';
      default: return 'Not checked';
    }
  }

  function applySnapshot(next: MacPermissionSnapshot, providerOverride: ProviderId | null) {
    snapshot = {
      ...next,
      accessibility: next.accessibility || 'unknown',
      microphone: next.microphone || 'unknown',
      keychain: providerOverride
        ? next.keychain || 'unknown'
        : keychainProvider
          ? snapshot.keychain
          : 'unknown',
    };
    console.info('[permissions][frontend] received/applied', {
      generation: next.diagnostics?.snapshotGeneration,
      microphone: next.microphone,
      microphoneCapture: next.diagnostics?.microphoneAvCaptureStatus,
      microphoneAudio: next.diagnostics?.microphoneAvAudioStatus,
      notifications: next.notifications?.authorization,
      keychain: snapshot.keychain,
    });
    return snapshot;
  }

  async function readSnapshot(providerOverride = keychainProvider) {
    const next = await invoke<MacPermissionSnapshot>('get_macos_permission_snapshot', {
      provider: providerOverride,
    });
    return applySnapshot(next, providerOverride);
  }

  async function refreshMacPermissions(silent = false) {
    if (!isMac) return;
    const generation = ++refreshGeneration;
    permissionsLoading = true;
    if (!silent) permissionsError = '';
    try {
      const next = await invoke<MacPermissionSnapshot>('get_macos_permission_snapshot', { provider: null });
      // A request can finish after a newer refresh/request. Never let stale
      // native data overwrite the newest coherent snapshot.
      if (generation === refreshGeneration && active) applySnapshot(next, null);
    } catch {
      if (!silent) {
        permissionsError = 'Could not refresh permission status right now.';
      }
    } finally {
      if (generation === refreshGeneration) permissionsLoading = false;
    }
  }

  async function triggerKeychainAccess() {
    if (!isMac || !keychainProvider || keychainLoading) return;
    keychainLoading = true;
    permissionsError = '';
    try {
      const result = await invoke<KeychainDiagnostic>('check_keychain_access', { provider: keychainProvider });
      keychainDiagnostic = result;
      snapshot = { ...snapshot, keychain: result.state };
    } catch (error) {
      snapshot = { ...snapshot, keychain: 'error' };
      permissionsError = `Keychain check failed before completion: ${extractIpcErrorMessage(error)}`;
    } finally {
      keychainLoading = false;
    }
  }

  // This component only exists while the Permissions surface is visible. Keep
  // reconciling with macOS for that lifetime so grants and revocations appear
  // without relying on a focus event (System Settings can remain over Verenu),
  // and so slower users are not abandoned after a short timeout.
  const WATCH_INTERVAL_MS = 1500;

  function startWatch() {
    if (!isMac) return;
    if (watchInterval !== null) return;
    watchInterval = setInterval(async () => {
      if (permissionsLoading) return;
      await refreshMacPermissions(true);
    }, WATCH_INTERVAL_MS);
  }

  function stopWatch() {
    if (watchInterval !== null) {
      clearInterval(watchInterval);
      watchInterval = null;
    }
  }

  function looksLikePermissionError(message: string): boolean {
    const m = message.toLowerCase();
    return (
      m.includes('permission') ||
      m.includes('accessibility') ||
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
      ++refreshGeneration;
      const next = await invoke<MacPermissionSnapshot>('request_accessibility_permission', { provider: null });
      applySnapshot(next, null);
    } catch {
      permissionsError = 'Could not request Accessibility permission.';
    }
    accessibilityPrompting = false;
    startWatch();
  }

  async function requestMicrophonePrompt() {
    if (!isMac) return;
    microphoneRequesting = true;
    permissionsError = '';
    try {
      ++refreshGeneration;
      const next = await invoke<MacPermissionSnapshot>('request_microphone_permission_snapshot', { provider: null });
      applySnapshot(next, null);
    } catch (error) {
      permissionsError = `Could not request Microphone permission: ${extractIpcErrorMessage(error)}`;
    }
    microphoneRequesting = false;
    startWatch();
  }

  async function relaunchApp() {
    restarting = true;
    permissionsError = '';
    restartTimeout = setTimeout(() => {
      restarting = false;
      permissionsError = 'Could not relaunch automatically — please quit and reopen Verenu.';
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
        permissionsError = 'Old macOS grant was reset. Add Verenu again under Accessibility, then relaunch.';
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

  async function requestNotificationPrompt() {
    if (!isMac) return;
    permissionsError = '';
    try {
      // The watcher may refresh while the native prompt is open. The result
      // of this explicit user action is still a fresh authoritative query and
      // must be applied when this component is active.
      ++refreshGeneration;
      const notifications = await invoke<NotificationPermission>('request_notification_permission');
      if (active) snapshot = { ...snapshot, notifications };
    } catch (error) {
      permissionsError = `Could not request Notifications permission: ${extractIpcErrorMessage(error)}`;
    }
  }

  async function refreshKeychainAccess() {
    if (!showKeychainRow || keychainLoading) return;
    await triggerKeychainAccess();
  }

  async function openPermissionSettings(kind: 'accessibility' | 'microphone' | 'notifications') {
    try {
      const cmd = kind === 'accessibility'
        ? 'open_accessibility_settings'
        : kind === 'microphone' ? 'open_microphone_settings' : 'open_notifications_settings';
      await invoke(cmd);
      startWatch();
    } catch {
      permissionsError = 'Could not open System Settings.';
    }
  }

  async function manualRefresh() {
    if (permissionsLoading) return;
    refreshAnimating = false;
    requestAnimationFrame(() => {
      refreshAnimating = true;
      if (refreshAnimationTimeout) clearTimeout(refreshAnimationTimeout);
      refreshAnimationTimeout = setTimeout(() => (refreshAnimating = false), motionMs(520));
    });
    await Promise.all([refreshMacPermissions(), refreshKeychainAccess()]);
  }

  // Returning from System Settings refocuses the window — re-check immediately.
  function onWindowFocus() {
    if (!permissionsLoading) void refreshMacPermissions();
  }

  onMount(() => {
    if (!isMac) return;
    // Check immediately, then keep the visible surface synchronized with TCC.
    void refreshMacPermissions();
    startWatch();
    window.addEventListener('focus', onWindowFocus);
    // Permission failures also trigger an immediate refresh instead of waiting
    // for the next reconciliation tick.
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
    if (refreshAnimationTimeout) clearTimeout(refreshAnimationTimeout);
    if (isMac) window.removeEventListener('focus', onWindowFocus);
    unlistenError?.();
  });
</script>

{#snippet statusIndicator(status: MacPermissionStatus | KeychainStatus, label: string)}
  {@const kind = statusKind(status)}
  <span class="perm-status perm-status-{kind}">
    {#key label}
      <span class="perm-status-change" in:fly={{ y: -3, duration: motionMs(180), easing: expoOut }}>
        <span class="perm-status-dot" aria-hidden="true"></span>
        <span class="perm-status-label">{label}</span>
      </span>
    {/key}
  </span>
{/snippet}

{#if isMac}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- Escape collapses the open diagnostics panel no matter which control
       inside it has focus (setup wizard, settings), one layer at a time.
       preventDefault marks the key as handled for Settings' window guard. -->
  <div class="mac-permissions" onkeydown={(event) => { if (event.key === 'Escape' && showDiagnostics) { event.preventDefault(); showDiagnostics = false; } }}>
  {#if invalidDevLaunch}
    <div class="permission-warning" role="alert">
      <strong>Verenu is still running from the old raw development executable.</strong>
      Stop the current terminal dev command and run <code>npm run tauri dev</code> again.
      The in-app Relaunch button cannot replace this process with the signed app bundle.
    </div>
  {/if}
  {#if variant === 'setup' && allGranted}
    <div class="permission-success" in:fly={{ y: -8, duration: motionMs(300), easing: expoOut }}>
      <svg class="success-check" width="13" height="13" viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M3 8l3.5 3.5L13 5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
      Core permissions granted — you're ready to continue.
    </div>
  {/if}

  <div class="perm-list">
    <!-- Accessibility -->
    <div class="perm-row perm-row-animated">
      <div class="perm-row-main">
        <div class="perm-row-title">Accessibility</div>
        <div class="perm-row-desc">Lets Verenu type your dictation into other apps. The global hotkey (default <strong>⌥ Space</strong>) needs no extra permission.</div>
      </div>
      <div class="perm-row-side">
        {@render statusIndicator(accessibilityPermission, permissionLabel(accessibilityPermission))}
        {#if accessibilityPermission === 'restricted'}
          <span class="perm-restricted">Managed by org</span>
        {:else if accessibilityPermission !== 'authorized'}
          {#if accessibilityPermission === 'not_granted' || accessibilityPermission === 'not_determined' || accessibilityPermission === 'unknown'}
            <button class="perm-action" onclick={requestAccessibilityPrompt} disabled={accessibilityPrompting}>
              {accessibilityPrompting ? 'Prompting…' : 'Request'}
            </button>
          {/if}
          <button class="perm-action" onclick={() => openPermissionSettings('accessibility')}>Open Settings</button>
        {/if}
      </div>
    </div>

    <!-- Microphone -->
    <div class="perm-row perm-row-animated">
      <div class="perm-row-main">
        <div class="perm-row-title">Microphone</div>
        <div class="perm-row-desc">Needed to capture your voice. macOS prompts on first recording if not yet granted.</div>
      </div>
      <div class="perm-row-side">
        {@render statusIndicator(microphonePermission, permissionLabel(microphonePermission))}
        {#if microphonePermission === 'restricted'}
          <span class="perm-restricted">Managed by org</span>
        {:else if microphonePermission !== 'authorized' && !invalidDevLaunch}
          {#if microphonePermission === 'not_determined'}
            <button class="perm-action" onclick={requestMicrophonePrompt} disabled={microphoneRequesting}>
              {microphoneRequesting ? 'Requesting…' : 'Request access'}
            </button>
          {:else if microphonePermission === 'denied' || microphonePermission === 'not_granted'}
            <button class="perm-action" onclick={() => openPermissionSettings('microphone')}>Allow in Settings</button>
          {/if}
        {/if}
      </div>
    </div>

    <!-- Notifications (optional capability) -->
    <div class="perm-row perm-row-animated">
      <div class="perm-row-main">
        <div class="perm-row-title">Notifications</div>
        <div class="perm-row-desc">Optional status and update alerts. Authorization is independent from alert, sound, and badge settings.</div>
      </div>
      <div class="perm-row-side">
        {@render statusIndicator(notificationPermission.authorization === 'authorized' || notificationPermission.authorization === 'provisional' ? 'authorized' : notificationPermission.authorization === 'denied' ? 'denied' : 'unknown', notificationPermission.authorization === 'authorized' || notificationPermission.authorization === 'provisional' ? 'Granted' : notificationPermission.authorization === 'not_determined' ? 'Not yet asked' : notificationPermission.authorization === 'denied' ? 'Denied' : 'Unavailable')}
        {#if notificationPermission.authorization === 'not_determined'}
          <button class="perm-action" onclick={requestNotificationPrompt}>Request access</button>
        {:else if notificationPermission.authorization === 'denied'}
          <button class="perm-action" onclick={() => openPermissionSettings('notifications')}>Allow in Settings</button>
        {/if}
      </div>
    </div>

    <!-- Keychain Access (optional) -->
    {#if showKeychainRow}
      <div class="perm-row">
        <div class="perm-row-main">
          <div class="perm-row-title">Keychain Access</div>
          <div class="perm-row-desc">
            {#if keychainStatus === 'available'}
              Verenu successfully created, read, and removed a private test item using its normal credential storage path.
            {:else if keychainStatus === 'unknown' || keychainStatus === 'not_checked'}
              Verenu checks its own API-key and sync-identity storage paths automatically.
            {:else}
              The explicit Keychain storage test failed. Details shows the native operation and OSStatus.
            {/if}
          </div>
        </div>
        <div class="perm-row-side">
          {@render statusIndicator(keychainStatus, keychainLabel(keychainStatus))}
          {#if keychainStatus !== 'available'}
            <button class="perm-action" onclick={triggerKeychainAccess} disabled={keychainLoading}>
              {keychainStatus === 'not_checked' ? 'Check access' : 'Check again'}
            </button>
          {/if}
        </div>
      </div>
    {/if}
  </div>

  {#if permissionsError}
    <p class="permission-error">{permissionsError}</p>
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
        disabled={restarting || invalidDevLaunch}
        title={invalidDevLaunch ? 'Restart the terminal dev command to launch the signed app bundle' : 'Relaunch Verenu to apply permission changes'}
      >
        <span aria-hidden="true">⏻</span>
        {restarting ? 'Relaunching…' : 'Relaunch'}
      </button>
      <button
        class="permission-refresh-btn"
        onclick={manualRefresh}
        aria-busy={permissionsLoading}
        title="Refresh permission status"
      >
        <span class:refresh-spin={refreshAnimating} aria-hidden="true">↻</span>
        Refresh
      </button>
    </div>
  </div>

  {#if showDiagnostics}
    <div class="permission-diagnostics" transition:slide={{ duration: motionMs(200) }}>
      {#if canRepairStaleGrant}
        <div class="permission-repair">
          <p class="repair-copy">
            <strong>Accessibility shows as enabled but typing still doesn't work?</strong>
            macOS can hold a stale grant from an older build. Reset it so you can re-add
            this version fresh.
          </p>
          <button class="btn-repair" onclick={repairStaleGrants} disabled={repairing}>
            {repairing ? 'Resetting…' : 'Reset stale grants'}
          </button>
        </div>
      {:else if showRepairHint && !snapshot.diagnostics.bundleIdentifier}
        <div class="permission-repair">
          <p class="repair-copy">
            <strong>This development process is not running from a macOS app bundle.</strong>
            Relaunch it through <code>npm run tauri dev</code> so macOS can attach permissions to Verenu reliably.
          </p>
        </div>
      {/if}
      <p class="diag-line">
        {snapshot.diagnostics.processName || 'Unknown process'} · {snapshot.diagnostics.bundleIdentifier ?? 'Unknown bundle'} ·
        {snapshot.diagnostics.bundleDisplayName ?? snapshot.diagnostics.bundleName ?? 'Unknown app name'} ·
        {snapshot.diagnostics.executablePath ?? 'Unknown executable'} ·
        PID <strong>{snapshot.diagnostics.processId}</strong> ·
        {snapshot.diagnostics.macosVersion} · <strong>{snapshot.diagnostics.buildProfile}</strong>
      </p>
      <p class="diag-line">
        Signing: <strong>{snapshot.diagnostics.signingIdentity ?? 'unknown'}</strong> ·
        Team: <strong>{snapshot.diagnostics.teamIdentifier ?? 'unknown'}</strong>
      </p>
      <p class="diag-line">
        Bundle URL: <strong>{snapshot.diagnostics.bundleUrl ?? 'unknown'}</strong> ·
        executable URL: <strong>{snapshot.diagnostics.executableUrl ?? 'unknown'}</strong> ·
        extension: <strong>{snapshot.diagnostics.bundleUrlExtension ?? 'none'}</strong> ·
        inside .app: <strong>{snapshot.diagnostics.isRunningInsideApp ? 'YES' : 'NO'}</strong>
      </p>
      <p class="diag-line">
        Accessibility — AXIsProcessTrusted: <strong>{snapshot.diagnostics.accessibilityTrusted ? 'true' : 'false'}</strong> ·
        state: <strong>{snapshot.accessibility}</strong>
      </p>
      <p class="diag-line">
        Microphone — AVCaptureDevice raw: <strong>{snapshot.diagnostics.microphoneAvCaptureRaw}</strong> state: <strong>{snapshot.diagnostics.microphoneAvCaptureStatus}</strong> ·
        AVAudioApplication raw: <strong>{snapshot.diagnostics.microphoneAvAudioRaw ?? 'unavailable'}</strong> ({snapshot.diagnostics.microphoneAvAudioFourcc ?? 'n/a'}) state: <strong>{snapshot.diagnostics.microphoneAvAudioStatus ?? 'unavailable'}</strong> ·
        final: <strong>{snapshot.microphone}</strong>
      </p>
      <p class="diag-line">
        Notifications — authorization: <strong>{snapshot.notifications.authorization}</strong> ·
        alerts: <strong>{snapshot.notifications.alerts}</strong> · sounds: <strong>{snapshot.notifications.sounds}</strong> · badges: <strong>{snapshot.notifications.badges}</strong>
      </p>
      <p class="diag-line">
        Keychain — automatic coverage check: <strong>{keychainDiagnostic ? 'YES' : 'PENDING'}</strong> · last operation: <strong>{keychainDiagnostic?.operation ?? 'none'}</strong> ·
        OSStatus: <strong>{keychainDiagnostic?.osStatus ?? 'n/a'}</strong> ({keychainDiagnostic?.osStatusMeaning ?? 'not checked'}) · final: <strong>{snapshot.keychain}</strong>
      </p>
      <p class="diag-line">Snapshot generation: <strong>{snapshot.diagnostics.snapshotGeneration}</strong></p>
    </div>
  {/if}
  </div>
{/if}

<style>
  /* Single block root so the component lays out in normal flow regardless of the
     parent (Settings is a block container; the Setup shell centers a flex row). */
  .mac-permissions { display: block; width: 100%; }

  .permission-warning {
    padding: 11px 13px;
    margin-bottom: 14px;
    border: 1px solid color-mix(in srgb, var(--warning) 32%, var(--line));
    border-radius: var(--r-sm);
    background: color-mix(in srgb, var(--warning-bg) 58%, var(--paper));
    color: var(--ink-soft);
    font-size: 12px;
    line-height: 1.5;
  }

  .permission-warning strong { display: block; color: var(--ink-strong); margin-bottom: 2px; }

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
    position: relative;
    overflow: hidden;
    animation: success-settle 0.55s 0.1s cubic-bezier(0.22, 1, 0.36, 1) both;
  }

  .permission-success::after {
    content: '';
    position: absolute;
    inset: 0;
    pointer-events: none;
    background: linear-gradient(100deg, transparent 25%, color-mix(in srgb, var(--accent) 11%, transparent) 50%, transparent 75%);
    transform: translateX(-110%);
    animation: success-sheen 0.8s 0.18s ease-out both;
  }

  .success-check path {
    stroke-dasharray: 18;
    stroke-dashoffset: 18;
    animation: check-draw 0.38s 0.18s cubic-bezier(0.22, 1, 0.36, 1) forwards;
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

  .perm-row-animated {
    opacity: 0;
    animation: permission-row-in 0.4s cubic-bezier(0.22, 1, 0.36, 1) forwards;
  }

  .perm-row-animated:nth-child(1) { animation-delay: 0.06s; }
  .perm-row-animated:nth-child(2) { animation-delay: 0.12s; }
  .perm-row-animated:nth-child(3) { animation-delay: 0.18s; }

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

  .perm-status-change { display: inline-flex; align-items: center; gap: 6px; }

  .perm-status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
    background: var(--ink-faint);
    transition: background 0.2s;
  }

  .perm-status-granted { color: var(--ink-soft); }
  .perm-status-granted .perm-status-dot {
    background: var(--success);
    animation: granted-pop 0.42s cubic-bezier(0.22, 1, 0.36, 1);
  }

  .perm-status-attention { color: var(--ink-soft); }
  .perm-status-attention .perm-status-dot { background: var(--warning); }

  .perm-status-checking { color: var(--ink-mute); }
  .perm-status-checking .perm-status-dot { animation: status-pulse 1.1s ease-in-out infinite; }

  @keyframes status-pulse { 0%, 100% { opacity: 0.35; } 50% { opacity: 1; } }

  @keyframes permission-row-in {
    from { opacity: 0; transform: translateY(8px); }
    to { opacity: 1; transform: translateY(0); }
  }

  @keyframes granted-pop {
    0% { transform: scale(0.45); box-shadow: 0 0 0 0 color-mix(in srgb, var(--success) 35%, transparent); }
    55% { transform: scale(1.25); box-shadow: 0 0 0 5px color-mix(in srgb, var(--success) 0%, transparent); }
    100% { transform: scale(1); box-shadow: none; }
  }

  @keyframes check-draw { to { stroke-dashoffset: 0; } }

  @keyframes success-settle {
    0% { transform: scale(0.985); }
    100% { transform: scale(1); }
  }

  @keyframes success-sheen { to { transform: translateX(110%); } }

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

  @media (prefers-reduced-motion: reduce) {
    .permission-success,
    .permission-success::after,
    .success-check path,
    .perm-row-animated,
    .perm-status-granted .perm-status-dot,
    .perm-status-checking .perm-status-dot { animation: none; }
    .perm-row-animated { opacity: 1; }
  }

  /* Repair action, shown inside the expanded Details section when Accessibility
     isn't granted — resets a stale TCC grant and re-requests. */
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
  .btn-repair:hover { filter: brightness(1.06); }
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

  .diag-line { margin: 0; font-size: 11.5px; color: var(--ink-mute); line-height: 1.5; word-break: break-word; }
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
  .refresh-spin { animation: spin 0.52s cubic-bezier(0.2, 0.8, 0.2, 1) 1; display: inline-block; }

  /*
   * Two queries for one rule. This component renders both in settings (inside
   * the .panel-inner measure) and in the Setup wizard (inside a 560px step with
   * no container), so neither query alone covers both: the container query is
   * what makes the rule mean "the column is narrow" in settings, and the
   * viewport query preserves the original behaviour in Setup. They can both
   * match in a narrow settings window, which is harmless — same declarations.
   */
  @container settings-panel (max-width: 560px) {
    .perm-row { flex-direction: column; align-items: stretch; gap: 8px; }
    .perm-row-side { justify-content: flex-start; }
  }

  @media (max-width: 560px) {
    .perm-row { flex-direction: column; align-items: stretch; gap: 8px; }
    .perm-row-side { justify-content: flex-start; }
  }

  @media (prefers-reduced-motion: reduce) {
    .perm-status-checking .perm-status-dot { animation: none; }
    .permission-details-toggle .chev { transition: none; }
  }
</style>
