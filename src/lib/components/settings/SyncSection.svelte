<script lang="ts">
  import { invoke } from '../../tauri';
  import {
    syncStore,
    refreshSyncStatus,
    thisDeviceName,
    type DiscoveredDevice,
    type PairedDevice,
  } from '../../syncStore.svelte';
  import { modalFocusTrap } from '../../modalFocus';
  import { modalBackdrop, modalCard, motionMs, MOTION_MS } from '../../motion';
  import { fade } from 'svelte/transition';
  import { onMount } from 'svelte';
  import { icons } from '../../icons';

  // Local device name editing.
  let deviceName = $state('');
  let nameSaved = $state('');
  let nameBusy = $state(false);

  // Per-row action state.
  let pairingUuid = $state('');
  let syncingUuid = $state('');
  let removingUuid = $state('');
  let statusMsg = $state('');
  let statusKind = $state<'' | 'ok' | 'err'>('');

  let confirmRemove = $state<PairedDevice | null>(null);
  let cancelRemoveButton = $state<HTMLButtonElement | null>(null);

  const discovered = $derived(syncStore.status?.discovered ?? []);
  const peers = $derived(syncStore.status?.peers ?? []);
  const unpairedNearby = $derived(discovered.filter((d) => !d.paired));
  const outgoing = $derived(
    syncStore.status?.pairing?.kind === 'outgoing' && syncStore.status.pairing.phase !== 'failed'
      ? syncStore.status.pairing
      : null,
  );
  const pairingError = $derived(
    syncStore.status?.pairing?.phase === 'failed'
      ? syncStore.status.pairing.error ?? 'Pairing could not be completed.'
      : '',
  );
  const listenerActive = $derived(syncStore.status?.listener_active ?? true);

  onMount(() => {
    void refreshSyncStatus().then(() => {
      deviceName = thisDeviceName();
      nameSaved = deviceName;
    });
  });

  function flash(message: string, kind: 'ok' | 'err'): void {
    statusMsg = message;
    statusKind = kind;
    setTimeout(() => {
      if (statusMsg === message) {
        statusMsg = '';
        statusKind = '';
      }
    }, 4000);
  }

  async function saveName(): Promise<void> {
    const name = deviceName.trim();
    if (!name) {
      deviceName = nameSaved;
      return;
    }
    if (name === nameSaved || nameBusy) return;
    nameBusy = true;
    try {
      await invoke('sync_set_device_name', { name });
      nameSaved = name;
      await refreshSyncStatus();
    } catch (err) {
      deviceName = nameSaved;
      flash(err instanceof Error ? err.message : String(err), 'err');
    } finally {
      nameBusy = false;
    }
  }

  async function startPairing(device: DiscoveredDevice): Promise<void> {
    if (pairingUuid) return;
    pairingUuid = device.uuid;
    try {
      await invoke<string>('sync_start_pairing', { deviceUuid: device.uuid });
      await refreshSyncStatus();
    } catch (err) {
      flash(err instanceof Error ? err.message : String(err), 'err');
    } finally {
      pairingUuid = '';
    }
  }

  function cancelOutgoing(): void {
    void invoke('sync_cancel_pairing').catch(() => {});
    void refreshSyncStatus();
  }

  async function syncNow(device: PairedDevice): Promise<void> {
    if (syncingUuid) return;
    syncingUuid = device.uuid;
    try {
      await invoke('sync_now', { deviceUuid: device.uuid });
      flash(`Syncing with ${device.name}…`, 'ok');
    } catch (err) {
      flash(err instanceof Error ? err.message : String(err), 'err');
    } finally {
      setTimeout(() => {
        syncingUuid = '';
        void refreshSyncStatus();
      }, 800);
    }
  }

  function askRemove(device: PairedDevice): void {
    confirmRemove = device;
  }

  async function removeDevice(): Promise<void> {
    if (!confirmRemove) return;
    removingUuid = confirmRemove.uuid;
    const name = confirmRemove.name;
    try {
      await invoke('sync_remove_device', { deviceUuid: confirmRemove.uuid });
      confirmRemove = null;
      flash(`${name} removed. It can no longer sync with this device.`, 'ok');
      await refreshSyncStatus();
    } catch (err) {
      flash(err instanceof Error ? err.message : String(err), 'err');
    } finally {
      removingUuid = '';
    }
  }

  function stateLabel(state: string): string {
    switch (state) {
      case 'synced':
        return 'Up to date';
      case 'syncing':
        return 'Syncing';
      case 'connecting':
        return 'Connecting';
      case 'error':
        return 'Sync failed';
      default:
        return 'Offline';
    }
  }

  function relativeTime(iso: string | null): string {
    if (!iso) return 'never';
    const withT = iso.includes('T') ? iso : iso.replace(' ', 'T');
    const normalized = /(?:Z|[+-]\d{2}:?\d{2})$/.test(withT) ? withT : `${withT}Z`;
    const then = new Date(normalized).getTime();
    if (Number.isNaN(then)) return iso;
    const seconds = Math.max(0, Math.round((Date.now() - then) / 1000));
    if (seconds < 45) return 'just now';
    if (seconds < 90) return 'a minute ago';
    if (seconds < 3600) return `${Math.round(seconds / 60)} min ago`;
    if (seconds < 7200) return 'an hour ago';
    if (seconds < 86400) return `${Math.round(seconds / 3600)} hours ago`;
    const days = Math.round(seconds / 86400);
    return days === 1 ? 'yesterday' : `${days} days ago`;
  }

  function groupedCode(code: string): string {
    return code.length === 6 ? `${code.slice(0, 3)} ${code.slice(3)}` : code;
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<h2 class="settings-h">Sync</h2>

<!-- This device -->
<div class="id-card">
  <div class="tile" aria-hidden="true">
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      {@html icons.devices}
    </svg>
  </div>
  <div class="id-main">
    <div class="id-name-wrap">
      <input
        class="id-name"
        bind:value={deviceName}
        maxlength={60}
        spellcheck="false"
        aria-label="This device's sync name"
        disabled={nameBusy}
        onkeydown={(e) => {
          if (e.key === 'Enter') void saveName();
        }}
        onblur={() => void saveName()}
      />
      <svg class="id-pencil" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        {@html icons.pencil}
      </svg>
    </div>
    <div class="id-sub">
      This device
      {#if listenerActive}
        · visible to nearby Verenu devices
      {:else}
        · sync unavailable right now
      {/if}
    </div>
  </div>
</div>

{#if statusMsg}
  <div
    class="desc data-status"
    class:data-ok={statusKind === 'ok'}
    class:data-err={statusKind === 'err'}
    role="status"
  >
    {statusMsg}
  </div>
{/if}

{#if pairingError}
  <div class="desc data-status data-err" role="alert">
    {pairingError}
    <button class="btn-ghost btn-compact" onclick={cancelOutgoing}>Dismiss</button>
  </div>
{/if}

{#if syncStore.status && !listenerActive}
  <div class="desc data-status data-err" role="alert">
    {syncStore.status.last_error_hint ?? 'Sync is unavailable on this device right now.'}
  </div>
{/if}

<!-- Paired devices -->
<h3 class="settings-subhead">Paired devices</h3>
{#if peers.length === 0}
  <div class="desc empty-note">
    Nothing paired yet. Devices you pair below stay connected until either side removes them.
  </div>
{:else}
  <div class="card-list">
    {#each peers as device (device.uuid)}
      <div class="device-card" class:is-error={device.state === 'error'}>
        <div class="tile tile-dim" aria-hidden="true">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            {@html icons.devices}
          </svg>
        </div>
        <div class="device-main">
          <div class="device-top">
            <span class="device-name">{device.name}</span>
            <span class="pill {device.state}">
              <span class="pill-dot" aria-hidden="true"></span>{stateLabel(device.state)}
            </span>
          </div>
          <div class="desc">
            Last synced {relativeTime(device.last_sync_at)}
            {#if device.online}
              · on this network
            {/if}
          </div>
          {#if device.error && device.state === 'error'}
            <div class="desc device-error">{device.error}</div>
          {/if}
        </div>
        <div class="device-actions">
          <button
            class="btn-ghost btn-compact"
            onclick={() => void syncNow(device)}
            disabled={syncingUuid !== '' || device.state === 'syncing'}
          >
            {syncingUuid === device.uuid || device.state === 'syncing' ? 'Syncing…' : 'Sync now'}
          </button>
          <button
            class="btn-ghost btn-compact danger-ghost"
            onclick={() => askRemove(device)}
            disabled={removingUuid !== ''}
          >
            Remove
          </button>
        </div>
      </div>
    {/each}
  </div>
{/if}

<!-- Nearby devices -->
<h3 class="settings-subhead">Nearby devices</h3>
{#if !syncStore.loaded}
  <div class="discover-card" role="status">
    <span class="search-dots" aria-hidden="true"><i></i><i></i><i></i></span>
    Searching your network for other devices…
  </div>
{:else if unpairedNearby.length === 0}
  <div class="discover-card discover-empty">
    <svg class="discover-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      {@html icons.devices}
    </svg>
    <div class="discover-title">No devices found yet</div>
    <div class="discover-hint">
      Open Verenu on your other device and make sure both are on the same Wi-Fi or wired
      network. New devices appear here automatically.
    </div>
  </div>
{:else}
  <div class="card-list">
    {#each unpairedNearby as device (device.uuid)}
      <div class="device-card">
        <div class="tile tile-dim" aria-hidden="true">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            {@html icons.devices}
          </svg>
        </div>
        <div class="device-main">
          <div class="device-top">
            <span class="device-name">{device.name}</span>
          </div>
          <div class="desc">Ready to pair — both sides confirm with a short code.</div>
        </div>
        <div class="device-actions">
          <button
            class="btn-primary btn-compact"
            onclick={() => void startPairing(device)}
            disabled={pairingUuid !== '' || !!outgoing}
          >
            {pairingUuid === device.uuid ? 'Waiting…' : 'Pair'}
          </button>
        </div>
      </div>
    {/each}
  </div>
{/if}

{#if outgoing}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <button
    class="modal-backdrop"
    aria-label="Cancel pairing"
    onclick={cancelOutgoing}
    in:modalBackdrop={{ duration: 180 }}
    out:modalBackdrop={{ duration: 160 }}
  ></button>
  <div
    class="modal-card outgoing-card"
    role="dialog"
    aria-modal="true"
    aria-label="Pairing with {outgoing.peer_name}"
    use:modalFocusTrap={{ active: !!outgoing, initialFocus: () => null }}
    in:modalCard={{ duration: motionMs(MOTION_MS.panel) }}
    out:modalCard={{ duration: motionMs(MOTION_MS.fast) }}
  >
    <div class="pair-head">
      <div class="tile" aria-hidden="true">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          {@html icons.devices}
        </svg>
      </div>
      <div>
        <div class="pair-title">Pair with {outgoing.peer_name}</div>
        <div class="pair-sub">Enter this code on {outgoing.peer_name} to confirm the connection.</div>
      </div>
    </div>
    <div class="outgoing-code">{groupedCode(outgoing.code ?? '')}</div>
    <div class="pair-wait">
      <span class="search-dots" aria-hidden="true"><i></i><i></i><i></i></span>
      {outgoing.phase === 'connecting'
        ? `Connecting to ${outgoing.peer_name}…`
        : `Waiting for ${outgoing.peer_name}… the code expires in a few minutes.`}
    </div>
    <div class="pair-actions">
      <button class="btn-ghost btn-compact" onclick={cancelOutgoing}>Cancel</button>
    </div>
  </div>
{/if}

{#if confirmRemove}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <button
    class="modal-backdrop"
    aria-label="Close dialog"
    onclick={() => (confirmRemove = null)}
    in:modalBackdrop={{ duration: 180 }}
    out:modalBackdrop={{ duration: 160 }}
  ></button>
  <div
    class="modal-card remove-card"
    role="dialog"
    aria-modal="true"
    aria-label="Remove {confirmRemove.name}"
    use:modalFocusTrap={{
      active: !!confirmRemove,
      initialFocus: () => cancelRemoveButton,
    }}
    in:modalCard={{ duration: motionMs(MOTION_MS.panel) }}
    out:modalCard={{ duration: motionMs(MOTION_MS.fast) }}
  >
    <div class="pair-title">Remove {confirmRemove.name}?</div>
    <div class="pair-sub">
      It will immediately lose access to sync with this device. To sync again you'd have to pair
      both devices again.
    </div>
    <div class="pair-actions">
      <button
        bind:this={cancelRemoveButton}
        class="btn-ghost btn-compact"
        onclick={() => (confirmRemove = null)}
      >
        Cancel
      </button>
      <button
        class="btn-danger btn-compact"
        onclick={() => void removeDevice()}
        disabled={removingUuid !== ''}
      >
        {removingUuid ? 'Removing…' : 'Remove device'}
      </button>
    </div>
  </div>
{/if}

<div class="panel-note sync-note" in:fade={{ duration: motionMs(MOTION_MS.fast) }}>
  <svg
    class="note-icon"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    {@html icons.lock}
  </svg>
  Sync runs only between paired devices on your local network, encrypted end to end. Nothing
  leaves your network — no account, no cloud. API keys and microphone settings never sync.
</div>

<style>
  .data-status {
    margin-top: 8px;
    animation: data-drop 260ms cubic-bezier(0.22, 1, 0.36, 1) both;
  }
  .data-ok {
    color: var(--success);
  }
  .data-err {
    color: var(--danger);
  }
  @keyframes data-drop {
    from {
      opacity: 0;
      transform: translateY(-6px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  /* Shared device glyph tile */
  .tile {
    width: 38px;
    height: 38px;
    border-radius: var(--r-md);
    background: var(--accent-soft);
    color: var(--accent-ink);
    display: grid;
    place-items: center;
    flex-shrink: 0;
  }
  .tile svg {
    width: 20px;
    height: 20px;
  }
  .tile-dim {
    background: var(--control-hover);
    color: var(--ink-mute);
  }

  /* This device identity card */
  .id-card {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px 16px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--bg-elev);
  }
  .id-main {
    min-width: 0;
    flex: 1;
  }
  .id-name-wrap {
    position: relative;
    display: inline-flex;
    align-items: center;
    max-width: 100%;
  }
  .id-name {
    font-family: var(--sans);
    font-size: 16px;
    font-weight: 600;
    color: var(--ink-strong);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--r-sm);
    padding: 2px 26px 2px 8px;
    margin-left: -8px;
    min-width: 120px;
    max-width: 100%;
    transition: border-color 120ms ease, background-color 120ms ease;
  }
  .id-name:hover {
    border-color: var(--line-strong);
  }
  .id-name:focus-visible {
    outline: none;
    border-color: var(--accent);
    background: var(--control-active);
  }
  .id-pencil {
    position: absolute;
    right: 8px;
    width: 13px;
    height: 13px;
    color: var(--ink-faint);
    opacity: 0;
    pointer-events: none;
    transition: opacity 120ms ease;
  }
  .id-name-wrap:hover .id-pencil,
  .id-name:focus-visible ~ .id-pencil {
    opacity: 1;
  }
  .id-sub {
    font-size: 12px;
    color: var(--ink-mute);
    margin-top: 3px;
  }

  /* Device cards */
  .card-list {
    display: grid;
    gap: 10px;
    margin-top: 8px;
  }
  .device-card {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 14px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--bg-elev);
  }
  .device-card.is-error {
    border-color: var(--danger-line);
  }
  .device-main {
    min-width: 0;
    flex: 1;
  }
  .device-top {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .device-name {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--ink-strong);
  }
  .device-error {
    color: var(--danger);
    margin-top: 4px;
  }
  .device-actions {
    display: flex;
    gap: 8px;
    flex-shrink: 0;
  }
  .danger-ghost:hover {
    color: var(--danger);
    border-color: var(--danger);
  }

  /* Status pill */
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    font-weight: 500;
    letter-spacing: 0.01em;
    padding: 2px 9px;
    border-radius: 999px;
    border: 1px solid var(--line);
    color: var(--ink-mute);
    white-space: nowrap;
  }
  .pill-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--ink-faint);
    flex-shrink: 0;
  }
  .pill.synced {
    background: var(--success-bg);
    border-color: var(--success-line);
    color: var(--success);
  }
  .pill.synced .pill-dot {
    background: var(--success);
  }
  .pill.syncing,
  .pill.connecting {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent-ink);
  }
  .pill.syncing .pill-dot,
  .pill.connecting .pill-dot {
    background: var(--accent);
    animation: sync-pulse 1.2s ease-in-out infinite;
  }
  .pill.error {
    background: var(--danger-bg);
    border-color: var(--danger-line);
    color: var(--danger);
  }
  .pill.error .pill-dot {
    background: var(--danger);
  }
  @keyframes sync-pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.35;
    }
  }

  /* Searching / empty nearby states */
  .discover-card {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 8px;
    padding: 18px 16px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    color: var(--ink-mute);
    font-size: 12.5px;
  }
  .discover-empty {
    flex-direction: column;
    text-align: center;
    gap: 6px;
    padding: 28px 24px;
    border-style: dashed;
    border-color: var(--line-strong);
  }
  .discover-icon {
    width: 22px;
    height: 22px;
    color: var(--ink-faint);
    margin-bottom: 4px;
  }
  .discover-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--ink-soft);
  }
  .discover-hint {
    max-width: 380px;
    line-height: 1.5;
    font-size: 12px;
  }
  .search-dots {
    display: inline-flex;
    gap: 3px;
    align-items: center;
    flex-shrink: 0;
  }
  .search-dots i {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--ink-faint);
    animation: dot-pulse 1.2s ease-in-out infinite;
  }
  .search-dots i:nth-child(2) {
    animation-delay: 150ms;
  }
  .search-dots i:nth-child(3) {
    animation-delay: 300ms;
  }
  @keyframes dot-pulse {
    0%,
    100% {
      opacity: 0.25;
    }
    50% {
      opacity: 1;
    }
  }

  .empty-note {
    padding: 2px 0 6px;
  }

  /* Pairing modal */
  .outgoing-card,
  .remove-card {
    width: min(400px, calc(100vw - 48px));
    padding: 20px;
    display: grid;
    gap: 14px;
  }
  .pair-head {
    display: flex;
    gap: 12px;
    align-items: center;
  }
  .pair-title {
    font-size: 14.5px;
    font-weight: 600;
    color: var(--ink-strong);
  }
  .pair-sub {
    font-size: 12.5px;
    color: var(--ink-mute);
    margin-top: 2px;
    line-height: 1.45;
  }
  .outgoing-code {
    font-family: var(--mono);
    font-size: 34px;
    font-weight: 500;
    letter-spacing: 0.22em;
    text-align: center;
    padding: 16px 8px 14px;
    border-radius: var(--r-md);
    border: 1px solid var(--line);
    background: var(--control-hover);
    color: var(--ink-strong);
    user-select: all;
    font-variant-numeric: tabular-nums;
  }
  .pair-wait {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--ink-mute);
  }
  .pair-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .sync-note {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin-top: 14px;
  }
  .note-icon {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    margin-top: 1px;
    opacity: 0.7;
  }
</style>
