<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { fly } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { invoke } from '../../tauri';
  import { motionMs } from '../../motion';
  import type { ProviderId } from '../../settings';

  type MacPermissionStatus = 'authorized' | 'needs_permission' | 'not_determined' | 'denied' | 'restricted' | 'unknown';
  type KeychainStatus = 'authorized' | 'not_configured' | 'denied' | 'unknown';

  let { provider, allCoreGranted = $bindable(false) }: { provider: ProviderId; allCoreGranted?: boolean } = $props();

  let accessibilityPermission = $state<MacPermissionStatus>('unknown');
  let microphonePermission = $state<MacPermissionStatus>('unknown');
  let keychainStatus = $state<KeychainStatus>('unknown');
  let permissionsLoading = $state(false);
  let permissionsError = $state('');
  let accessibilityPrompting = $state(false);
  let keychainLoading = $state(false);
  let pollInterval: ReturnType<typeof setInterval> | null = null;

  $effect(() => {
    allCoreGranted = accessibilityPermission === 'authorized' && microphonePermission === 'authorized';
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

  function startPolling() {
    stopPolling();
    pollInterval = setInterval(async () => {
      if (permissionsLoading) return;
      await refreshMacPermissions();
      if (accessibilityPermission === 'authorized' && microphonePermission === 'authorized') {
        stopPolling();
      }
    }, 5000);
  }

  function stopPolling() {
    if (pollInterval !== null) {
      clearInterval(pollInterval);
      pollInterval = null;
    }
  }

  async function refreshMacPermissions() {
    if (permissionsLoading) return;
    permissionsLoading = true;
    permissionsError = '';
    try {
      const [accessibility, microphone] = await Promise.all([
        invoke<string>('get_accessibility_permission_status'),
        invoke<string>('get_microphone_permission_status'),
      ]);
      accessibilityPermission = (accessibility as MacPermissionStatus) || 'unknown';
      microphonePermission = (microphone as MacPermissionStatus) || 'unknown';
    } catch {
      permissionsError = 'Could not refresh permission status right now.';
    } finally {
      permissionsLoading = false;
    }
  }

  async function triggerKeychainAccess() {
    if (keychainLoading) return;
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

  async function requestAccessibilityPrompt() {
    accessibilityPrompting = true;
    permissionsError = '';
    try {
      await invoke('check_accessibility_permission', { prompt: true });
    } catch {}
    await refreshMacPermissions();
    accessibilityPrompting = false;
    startPolling();
  }

  async function openPermissionSettings(kind: 'accessibility' | 'microphone') {
    try {
      await invoke(kind === 'accessibility' ? 'open_accessibility_settings' : 'open_microphone_settings');
      startPolling();
    } catch {
      permissionsError = 'Could not open System Settings.';
    }
  }

  function refreshAll() {
    void refreshMacPermissions();
    void triggerKeychainAccess();
  }

  onMount(() => {
    refreshAll();
    startPolling();
  });

  onDestroy(() => {
    stopPolling();
  });
</script>

<div class="step">
  {#if allCoreGranted}
    <div class="permission-success" in:fly={{ y: -8, duration: motionMs(220), easing: expoOut }}>
      <svg width="13" height="13" viewBox="0 0 16 16" fill="none" aria-hidden="true" style="display:inline;vertical-align:-1px;margin-right:5px"><path d="M3 8l3.5 3.5L13 5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
      Core permissions granted — you're ready to continue.
    </div>
  {/if}

  <div class="perm-rows">
    <!-- Accessibility -->
    <div class="perm-row" class:perm-granted={accessibilityPermission === 'authorized'}>
      <div class="perm-row-icon" aria-hidden="true">
        <svg viewBox="0 0 20 20" fill="none"><circle cx="10" cy="10" r="8.5" stroke="currentColor" stroke-width="1.5"/><circle cx="10" cy="7" r="1.5" fill="currentColor"/><path d="M7 10.5h6M10 10.5V14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
      </div>
      <div class="perm-row-body">
        <p class="perm-row-title">Accessibility</p>
        <p class="perm-row-desc">Lets Verenu listen for the global hotkey and inject text into any app.</p>
      </div>
      <div class="perm-row-right">
        {#key accessibilityPermission}
          <span class="permission-badge" class:warn={accessibilityPermission !== 'authorized'} in:fly={{ y: -4, duration: motionMs(160), easing: expoOut }}>
            {permissionLabel(accessibilityPermission)}
          </span>
        {/key}
        {#if accessibilityPermission !== 'authorized'}
          <div class="permission-actions">
            {#if accessibilityPermission === 'needs_permission' || accessibilityPermission === 'unknown'}
              <button class="btn-ghost permission-btn" onclick={requestAccessibilityPrompt} disabled={accessibilityPrompting}>
                {accessibilityPrompting ? 'Prompting…' : 'Show system prompt'}
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
    </div>

    <!-- Microphone -->
    <div class="perm-row" class:perm-granted={microphonePermission === 'authorized'}>
      <div class="perm-row-icon" aria-hidden="true">
        <svg viewBox="0 0 20 20" fill="none"><rect x="7.5" y="2.5" width="5" height="9" rx="2.5" stroke="currentColor" stroke-width="1.5"/><path d="M4.5 10a5.5 5.5 0 0 0 11 0" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><line x1="10" y1="15.5" x2="10" y2="17.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
      </div>
      <div class="perm-row-body">
        <p class="perm-row-title">Microphone</p>
        <p class="perm-row-desc">Needed to capture your voice. macOS will prompt on first use if not yet granted.</p>
      </div>
      <div class="perm-row-right">
        {#key microphonePermission}
          <span class="permission-badge" class:warn={microphonePermission !== 'authorized'} in:fly={{ y: -4, duration: motionMs(160), easing: expoOut }}>
            {permissionLabel(microphonePermission)}
          </span>
        {/key}
        {#if microphonePermission !== 'authorized'}
          <div class="permission-actions">
            {#if microphonePermission === 'restricted'}
              <p class="permission-restricted-note">Managed by your organization.</p>
            {:else}
              <button class="btn-ghost permission-btn" onclick={() => openPermissionSettings('microphone')}>
                Open Settings
              </button>
            {/if}
          </div>
        {/if}
      </div>
    </div>

    <!-- Keychain Access -->
    <div class="perm-row" class:perm-granted={keychainStatus === 'authorized'}>
      <div class="perm-row-icon" aria-hidden="true">
        <svg viewBox="0 0 20 20" fill="none"><rect x="4" y="9" width="12" height="9" rx="2" stroke="currentColor" stroke-width="1.5"/><path d="M7 9V7a3 3 0 0 1 6 0v2" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
      </div>
      <div class="perm-row-body">
        <p class="perm-row-title">Keychain Access</p>
        <p class="perm-row-desc">
          {#if keychainStatus === 'not_configured'}
            No API key saved yet. Go back to step 2 to add one.
          {:else if keychainStatus === 'denied'}
            Access denied. Click <strong>Unlock access</strong> below or allow Verenu in Keychain Access.app.
          {:else if keychainStatus === 'authorized'}
            Secures your API key and keeps it in your Keychain.
          {:else}
            Secures your API key. Verenu will prompt for access when it needs it.
          {/if}
        </p>
      </div>
      <div class="perm-row-right">
        {#key keychainStatus}
          <span class="permission-badge" class:warn={keychainStatus !== 'authorized'} in:fly={{ y: -4, duration: motionMs(160), easing: expoOut }}>
            {keychainLabel(keychainStatus)}
          </span>
        {/key}
        {#if keychainStatus !== 'authorized' && keychainStatus !== 'not_configured'}
          <div class="permission-actions">
            <button class="btn-ghost permission-btn" onclick={triggerKeychainAccess} disabled={keychainLoading}>
              {keychainLoading ? 'Checking…' : 'Unlock access'}
            </button>
          </div>
        {/if}
      </div>
    </div>
  </div>

  {#if permissionsError}
    <p class="permission-error">{permissionsError}</p>
  {/if}

  <div class="permission-note-row">
    <div class="permission-note">
      <strong>Tip:</strong> Grant permissions in System Settings and this page refreshes automatically within 5 seconds.
    </div>
    <button
      class="permission-refresh-btn"
      onclick={refreshAll}
      disabled={permissionsLoading || keychainLoading}
      title="Refresh permission status"
    >
      <span class:refresh-spin={permissionsLoading || keychainLoading} aria-hidden="true">↻</span>
      {permissionsLoading || keychainLoading ? 'Refreshing…' : 'Refresh'}
    </button>
  </div>
</div>

<style>
  .permission-success {
    display: flex;
    align-items: center;
    background: color-mix(in srgb, var(--accent-soft) 65%, var(--paper));
    border: 1px solid color-mix(in srgb, var(--accent) 28%, var(--line));
    border-radius: var(--r-sm);
    padding: 9px 13px;
    font-size: 13px;
    color: var(--accent-ink);
    font-weight: 500;
  }

  .perm-rows { display: flex; flex-direction: column; gap: 10px; }

  .perm-row {
    display: flex;
    align-items: flex-start;
    gap: 13px;
    background: var(--bg-elev);
    border: 1.5px solid var(--line);
    border-radius: var(--r-md);
    padding: 14px 16px;
    transition: border-color 0.25s, background 0.25s;
  }

  .perm-row.perm-granted { border-color: var(--accent); background: var(--accent-soft); }

  .perm-row-icon { width: 22px; height: 22px; flex-shrink: 0; color: var(--ink-mute); margin-top: 1px; }
  .perm-row.perm-granted .perm-row-icon { color: var(--accent-ink); }

  .perm-row-body { flex: 1; min-width: 0; }
  .perm-row-title { font-size: 13.5px; font-weight: 500; color: var(--ink-strong); margin: 0 0 3px; }
  .perm-row-desc { font-size: 12px; color: var(--ink-mute); margin: 0; line-height: 1.45; }
  .perm-row-desc strong { color: var(--accent-ink); font-weight: 600; }

  .perm-row-right { display: flex; flex-direction: column; align-items: flex-end; gap: 7px; flex-shrink: 0; }

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

  .perm-row.perm-granted .permission-badge { background: color-mix(in srgb, var(--accent) 15%, var(--accent-soft)); }
  .permission-badge.warn { background: var(--warning-bg); color: var(--warning); }

  .permission-actions { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; justify-content: flex-end; }

  .permission-restricted-note { font-size: 11.5px; color: var(--ink-mute); margin: 0; font-style: italic; text-align: right; }

  .permission-btn { padding: 5px 11px; border-radius: 999px; font-size: 11.5px; }

  .permission-error { margin: 0; color: var(--danger); font-size: 12px; }

  .permission-note-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; }

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
    flex-shrink: 0;
    transition: color 0.15s;
    font-family: inherit;
  }

  .permission-refresh-btn:hover { color: var(--ink-soft); }
  .permission-refresh-btn:disabled { cursor: default; }

  @keyframes spin { to { transform: rotate(360deg); } }
  .refresh-spin { animation: spin 0.75s linear infinite; display: inline-block; }

  .permission-note {
    background: color-mix(in srgb, var(--accent-soft) 55%, var(--paper));
    border: 1px solid color-mix(in srgb, var(--accent) 18%, var(--line));
    border-radius: var(--r-sm);
    padding: 9px 12px;
    font-size: 12px;
    color: var(--ink-soft);
    line-height: 1.45;
    flex: 1;
  }

  .permission-note strong { color: var(--ink-strong); }

  @media (max-width: 960px) {
    .perm-row { flex-wrap: wrap; }
    .perm-row-right { width: 100%; flex-direction: row; align-items: center; justify-content: flex-start; }
    .permission-note-row { flex-direction: column; align-items: stretch; }
  }
</style>
