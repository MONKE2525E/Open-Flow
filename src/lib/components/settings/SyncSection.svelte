<script lang="ts">
  import { invoke } from '../../tauri';
  import {
    syncStore,
    refreshSyncStatus,
    thisDeviceName,
    flashSync,
    clearSyncFlash,
    errorText,
    PAIRING_WINDOW_MS,
    type DiscoveredDevice,
    type PairedDevice,
  } from '../../syncStore.svelte';
  import { modalFocusTrap } from '../../modalFocus';
  import { modalBackdrop, modalCard, motionMs, motionPx, MOTION_MS } from '../../motion';
  import { fade, fly } from 'svelte/transition';
  import { flip } from 'svelte/animate';
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
  let codeCopied = $state(false);

  let confirmRemove = $state<PairedDevice | null>(null);
  let cancelRemoveButton = $state<HTMLButtonElement | null>(null);

  const discovered = $derived(syncStore.status?.discovered ?? []);
  const peers = $derived(syncStore.status?.peers ?? []);
  const unpairedNearby = $derived(discovered.filter((d) => !d.paired));
  const outgoing = $derived(syncStore.outgoing);
  const listenerActive = $derived(syncStore.status?.listener_active ?? false);
  const flash = $derived(syncStore.flash);

  // A single clock drives both the "last synced" labels (which used to freeze
  // at whatever they said when the snapshot arrived) and the pairing-code
  // countdown. It only ticks per second while a code is on screen.
  let now = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => (now = Date.now()), syncStore.outgoing ? 1000 : 30_000);
    return () => clearInterval(id);
  });

  const codeExpiresIn = $derived(
    outgoing ? Math.max(0, PAIRING_WINDOW_MS - (now - outgoing.startedAt)) : 0,
  );
  const codeExpired = $derived(!!outgoing && codeExpiresIn === 0);

  onMount(() => {
    void refreshSyncStatus().then(() => {
      deviceName = thisDeviceName();
      nameSaved = deviceName;
    });
    return () => clearSyncFlash();
  });

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
      flashSync(errorText(err), 'err');
    } finally {
      nameBusy = false;
    }
  }

  async function startPairing(device: DiscoveredDevice): Promise<void> {
    if (pairingUuid) return;
    pairingUuid = device.uuid;
    codeCopied = false;
    try {
      let code: string;
      try {
        code = await invoke<string>('sync_start_pairing', { deviceUuid: device.uuid });
      } catch (err) {
        // A pairing the backend still holds (app restarted while a code was on
        // screen) leaves the user with no way to reach it and no way to clear
        // it. Drop the stale one and take the fresh code.
        if (!/already in progress/i.test(errorText(err))) throw err;
        await invoke('sync_cancel_pairing');
        code = await invoke<string>('sync_start_pairing', { deviceUuid: device.uuid });
      }
      syncStore.outgoing = { uuid: device.uuid, name: device.name, code, startedAt: Date.now() };
      now = Date.now();
      await refreshSyncStatus();
    } catch (err) {
      flashSync(errorText(err), 'err');
    } finally {
      pairingUuid = '';
    }
  }

  function cancelOutgoing(): void {
    syncStore.outgoing = null;
    void invoke('sync_cancel_pairing').catch(() => {});
    void refreshSyncStatus();
  }

  async function copyCode(): Promise<void> {
    if (!outgoing) return;
    try {
      await navigator.clipboard.writeText(outgoing.code);
      codeCopied = true;
      setTimeout(() => (codeCopied = false), 1600);
    } catch {
      /* clipboard unavailable — the code is on screen anyway */
    }
  }

  async function syncNow(device: PairedDevice): Promise<void> {
    if (syncingUuid) return;
    syncingUuid = device.uuid;
    try {
      await invoke('sync_now', { deviceUuid: device.uuid });
    } catch (err) {
      flashSync(errorText(err), 'err');
    } finally {
      setTimeout(() => {
        syncingUuid = '';
        void refreshSyncStatus();
      }, 800);
    }
  }

  async function removeDevice(): Promise<void> {
    if (!confirmRemove) return;
    removingUuid = confirmRemove.uuid;
    const name = confirmRemove.name;
    try {
      await invoke('sync_remove_device', { deviceUuid: confirmRemove.uuid });
      confirmRemove = null;
      flashSync(`${name} removed. It can no longer sync with this device.`, 'ok');
      await refreshSyncStatus();
    } catch (err) {
      flashSync(errorText(err), 'err');
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

  function relativeTime(iso: string | null, from: number): string {
    if (!iso) return 'never';
    const withT = iso.includes('T') ? iso : iso.replace(' ', 'T');
    const normalized = /(?:Z|[+-]\d{2}:?\d{2})$/.test(withT) ? withT : `${withT}Z`;
    const then = new Date(normalized).getTime();
    if (Number.isNaN(then)) return iso;
    const seconds = Math.max(0, Math.round((from - then) / 1000));
    if (seconds < 45) return 'just now';
    if (seconds < 90) return 'a minute ago';
    if (seconds < 3600) return `${Math.round(seconds / 60)} min ago`;
    if (seconds < 7200) return 'an hour ago';
    if (seconds < 86400) return `${Math.round(seconds / 3600)} hours ago`;
    const days = Math.round(seconds / 86400);
    return days === 1 ? 'yesterday' : `${days} days ago`;
  }

  function countdown(ms: number): string {
    const total = Math.ceil(ms / 1000);
    return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`;
  }

  // Escape closes both dialogs — the code modal used to trap the user into
  // hunting for Cancel.
  function handleKeydown(event: KeyboardEvent): void {
    if (event.key !== 'Escape') return;
    if (confirmRemove) {
      event.preventDefault();
      confirmRemove = null;
    } else if (syncStore.outgoing) {
      event.preventDefault();
      cancelOutgoing();
    }
  }

  const rowIn = (index: number) => ({
    y: motionPx(6),
    duration: motionMs(MOTION_MS.base),
    delay: motionMs(index * 40),
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<h2 class="settings-h">Sync</h2>

<!-- This device -->
<div class="id-card">
  <div class="tile tile-accent" class:is-live={listenerActive} aria-hidden="true">
    <span class="tile-ring"></span>
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
        placeholder="This device"
        maxlength={60}
        spellcheck="false"
        aria-label="This device's sync name"
        disabled={nameBusy}
        onkeydown={(e) => {
          if (e.key === 'Enter') (e.currentTarget as HTMLInputElement).blur();
        }}
        onblur={() => void saveName()}
      />
      <svg class="id-pencil" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        {@html icons.pencil}
      </svg>
    </div>
    <div class="id-sub">
      <span class="live-dot" class:is-live={listenerActive} aria-hidden="true"></span>
      {#if !syncStore.loaded}
        Checking your network…
      {:else if listenerActive}
        Visible to nearby Verenu devices
      {:else}
        Sync unavailable right now
      {/if}
    </div>
  </div>
</div>

{#if flash}
  {#key flash.id}
    <div
      class="flash"
      class:is-err={flash.kind === 'err'}
      role="status"
      in:fly={{ y: motionPx(-4), duration: motionMs(MOTION_MS.base) }}
      out:fade={{ duration: motionMs(MOTION_MS.fast) }}
    >
      <svg class="flash-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        {@html flash.kind === 'err' ? icons.close : icons.check}
      </svg>
      {flash.message}
    </div>
  {/key}
{/if}

{#if syncStore.status && !listenerActive}
  <div class="flash is-err" role="alert" in:fade={{ duration: motionMs(MOTION_MS.fast) }}>
    <svg class="flash-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      {@html icons.shield}
    </svg>
    {syncStore.status.last_error_hint ?? 'Sync is unavailable on this device right now.'}
  </div>
{/if}

<!-- Paired devices -->
<div class="subhead-row">
  <h3 class="settings-subhead">Paired devices</h3>
  {#if peers.length}
    <span class="count-chip">{peers.length}</span>
  {/if}
</div>

{#if peers.length === 0}
  <div class="desc empty-note">
    Nothing paired yet. Devices you pair below stay connected until either side removes them.
  </div>
{:else}
  <div class="card-list">
    {#each peers as device, i (device.uuid)}
      <div
        class="device-card"
        class:is-error={device.state === 'error'}
        class:is-busy={device.state === 'syncing' || device.state === 'connecting'}
        animate:flip={{ duration: motionMs(MOTION_MS.base) }}
        in:fly={rowIn(i)}
        out:fade={{ duration: motionMs(MOTION_MS.fast) }}
      >
        <div class="tile" class:tile-online={device.online} aria-hidden="true">
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
            Last synced {relativeTime(device.last_sync_at, now)}{device.online
              ? ' · on this network'
              : ''}
          </div>
          {#if device.error && device.state === 'error'}
            <div class="desc device-error" transition:fade={{ duration: motionMs(MOTION_MS.fast) }}>
              {device.error}
            </div>
          {/if}
        </div>
        <div class="device-actions">
          <button
            class="btn-ghost btn-compact sync-btn"
            onclick={() => void syncNow(device)}
            disabled={syncingUuid !== '' || device.state === 'syncing'}
          >
            <svg
              class="sync-icon"
              class:is-spinning={syncingUuid === device.uuid || device.state === 'syncing'}
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              {@html icons.refresh}
            </svg>
            {syncingUuid === device.uuid || device.state === 'syncing' ? 'Syncing…' : 'Sync now'}
          </button>
          <button
            class="btn-ghost btn-compact danger-ghost"
            onclick={() => (confirmRemove = device)}
            disabled={removingUuid !== ''}
          >
            Remove
          </button>
        </div>
        <span class="busy-bar" aria-hidden="true"></span>
      </div>
    {/each}
  </div>
{/if}

<!-- Nearby devices -->
<div class="subhead-row">
  <h3 class="settings-subhead">Nearby devices</h3>
  {#if syncStore.loaded && listenerActive}
    <span class="scan-note">
      <span class="search-dots" aria-hidden="true"><i></i><i></i><i></i></span>
      scanning
    </span>
  {/if}
</div>

{#if !syncStore.loaded}
  <div class="discover-card" role="status" in:fade={{ duration: motionMs(MOTION_MS.fast) }}>
    <span class="search-dots" aria-hidden="true"><i></i><i></i><i></i></span>
    Searching your network for other devices…
  </div>
{:else if unpairedNearby.length === 0}
  <div class="discover-card discover-empty" in:fade={{ duration: motionMs(MOTION_MS.base) }}>
    <span class="radar" aria-hidden="true">
      <span class="radar-ring"></span>
      <span class="radar-ring"></span>
      <svg class="discover-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
        {@html icons.devices}
      </svg>
    </span>
    <div class="discover-title">No devices found yet</div>
    <div class="discover-hint">
      Open Verenu on your other device and make sure both are on the same Wi-Fi or wired
      network. New devices appear here automatically.
    </div>
  </div>
{:else}
  <div class="card-list">
    {#each unpairedNearby as device, i (device.uuid)}
      <div
        class="device-card is-nearby"
        animate:flip={{ duration: motionMs(MOTION_MS.base) }}
        in:fly={rowIn(i)}
        out:fade={{ duration: motionMs(MOTION_MS.fast) }}
      >
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
    aria-label="Pairing with {outgoing.name}"
    use:modalFocusTrap={{ active: !!outgoing, initialFocus: () => null }}
    in:modalCard={{ duration: motionMs(MOTION_MS.panel) }}
    out:modalCard={{ duration: motionMs(MOTION_MS.fast) }}
  >
    <div class="pair-head">
      <div class="tile tile-accent is-live" aria-hidden="true">
        <span class="tile-ring"></span>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          {@html icons.devices}
        </svg>
      </div>
      <div>
        <div class="pair-title">Pair with {outgoing.name}</div>
        <div class="pair-sub">Enter this code on {outgoing.name} to confirm the connection.</div>
      </div>
    </div>

    <button
      class="outgoing-code ui-focus-ring"
      class:is-expired={codeExpired}
      onclick={() => void copyCode()}
      title="Copy code"
    >
      {#each outgoing.code.split('') as digit, i}
        <span
          class="code-digit"
          class:gap-after={i === 2}
          style:--digit-delay="{motionMs(60 + i * 45)}ms">{digit}</span
        >
      {/each}
      <span class="code-copy" class:is-copied={codeCopied}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          {@html codeCopied ? icons.check : icons.copy}
        </svg>
        {codeCopied ? 'Copied' : 'Copy'}
      </span>
    </button>

    <div class="code-meter" aria-hidden="true">
      <span
        class="code-meter-fill"
        class:is-expired={codeExpired}
        style:width="{(codeExpiresIn / PAIRING_WINDOW_MS) * 100}%"
      ></span>
    </div>

    <div class="pair-wait" role="status">
      {#if codeExpired}
        <svg class="flash-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          {@html icons.close}
        </svg>
        This code expired. Start over to get a fresh one.
      {:else}
        <span class="search-dots" aria-hidden="true"><i></i><i></i><i></i></span>
        Waiting for {outgoing.name} · expires in {countdown(codeExpiresIn)}
      {/if}
    </div>

    <div class="pair-actions">
      <button class="btn-ghost btn-compact" onclick={cancelOutgoing}>
        {codeExpired ? 'Start over' : 'Cancel'}
      </button>
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
  /* Shared device glyph tile */
  .tile {
    width: 38px;
    height: 38px;
    border-radius: var(--r-md);
    background: var(--control-hover);
    color: var(--ink-mute);
    display: grid;
    place-items: center;
    flex-shrink: 0;
    position: relative;
    transition:
      background-color var(--ui-duration-base) var(--ui-ease-out),
      color var(--ui-duration-base) var(--ui-ease-out);
  }
  .tile svg {
    width: 20px;
    height: 20px;
    position: relative;
    z-index: 1;
  }
  .tile-dim {
    background: var(--control-hover);
    color: var(--ink-faint);
  }
  /* Neutral surface + inked glyph. A tinted fill behind a same-hue icon reads
     as a smudge, not as emphasis — the ring and the live dot carry "live". */
  .tile-accent {
    background: var(--control-active);
    color: var(--ink-soft);
    border: 1px solid var(--line);
  }
  .tile-online {
    color: var(--ink-soft);
  }

  /* Broadcast ring — only while this device is actually discoverable. */
  .tile-ring {
    position: absolute;
    inset: -1px;
    border-radius: inherit;
    border: 1px solid var(--success);
    opacity: 0;
  }
  .tile.is-live .tile-ring {
    animation: ring-out 2.6s ease-out infinite;
  }
  @keyframes ring-out {
    0% {
      opacity: 0.5;
      transform: scale(1);
    }
    70%,
    100% {
      opacity: 0;
      transform: scale(1.35);
    }
  }

  /* This device identity card */
  .id-card {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 18px 20px;
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
    transition:
      border-color var(--ui-duration-fast) var(--ui-ease-out),
      background-color var(--ui-duration-fast) var(--ui-ease-out);
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
    transform: translateY(1px);
    pointer-events: none;
    transition:
      opacity var(--ui-duration-fast) var(--ui-ease-out),
      transform var(--ui-duration-fast) var(--ui-ease-out);
  }
  .id-name-wrap:hover .id-pencil,
  .id-name:focus-visible ~ .id-pencil {
    opacity: 1;
    transform: translateY(0);
  }
  .id-name::placeholder {
    color: var(--ink-faint);
  }
  .id-sub {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--ink-mute);
    margin-top: 7px;
  }
  .live-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--ink-faint);
    flex-shrink: 0;
  }
  .live-dot.is-live {
    background: var(--success);
    box-shadow: 0 0 0 0 color-mix(in srgb, var(--success) 55%, transparent);
    animation: live-pulse 2.6s ease-out infinite;
  }
  @keyframes live-pulse {
    0% {
      box-shadow: 0 0 0 0 color-mix(in srgb, var(--success) 55%, transparent);
    }
    70%,
    100% {
      box-shadow: 0 0 0 5px transparent;
    }
  }

  /* Status flash banner */
  .flash {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin-top: 14px;
    padding: 11px 14px;
    border-radius: var(--r-sm);
    border: 1px solid var(--success-line);
    background: var(--success-bg);
    color: var(--success);
    font-size: 12.5px;
    line-height: 1.45;
  }
  .flash.is-err {
    border-color: var(--danger-line);
    background: var(--danger-bg);
    color: var(--danger);
  }
  .flash-icon {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    margin-top: 1px;
  }

  /* Section subheads */
  .subhead-row {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 8px;
  }
  .count-chip {
    font-size: 11px;
    font-weight: 500;
    color: var(--ink-mute);
    background: var(--control-hover);
    border-radius: 999px;
    padding: 1px 7px;
  }
  .scan-note {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
    font-size: 11px;
    color: var(--ink-faint);
  }

  /* Device cards */
  .card-list {
    display: grid;
    gap: 10px;
  }
  .device-card {
    position: relative;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 15px 18px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--bg-elev);
    overflow: hidden;
    transition:
      border-color var(--ui-duration-base) var(--ui-ease-out),
      box-shadow var(--ui-duration-base) var(--ui-ease-out),
      transform var(--ui-duration-base) var(--ui-ease-out);
  }
  .device-card:hover {
    border-color: var(--line-strong);
    box-shadow: var(--shadow-popover);
    transform: translateY(-1px);
  }
  .device-card.is-error {
    border-color: var(--danger-line);
  }
  .device-card.is-nearby:hover .tile-dim {
    background: var(--accent-soft);
    color: var(--accent-ink);
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
    margin-bottom: 5px;
  }
  .device-name {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--ink-strong);
  }
  .device-error {
    color: var(--danger);
    margin-top: 6px;
  }
  .device-actions {
    display: flex;
    gap: 8px;
    flex-shrink: 0;
    margin-left: 8px;
  }
  .danger-ghost:hover {
    color: var(--danger);
    border-color: var(--danger);
  }
  .sync-btn {
    gap: 6px;
  }
  .sync-icon {
    width: 12px;
    height: 12px;
    opacity: 0.75;
  }
  .sync-icon.is-spinning {
    opacity: 1;
    animation: spin 900ms linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  /* Indeterminate bar along the card's bottom edge while a sync is in flight */
  .busy-bar {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 2px;
    width: 35%;
    background: linear-gradient(90deg, transparent, var(--accent), transparent);
    opacity: 0;
    transform: translateX(-100%);
  }
  .device-card.is-busy .busy-bar {
    opacity: 1;
    animation: busy-sweep 1.25s ease-in-out infinite;
  }
  @keyframes busy-sweep {
    0% {
      transform: translateX(-100%);
    }
    100% {
      transform: translateX(340%);
    }
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
    transition:
      background-color var(--ui-duration-base) var(--ui-ease-out),
      border-color var(--ui-duration-base) var(--ui-ease-out),
      color var(--ui-duration-base) var(--ui-ease-out);
  }
  .pill-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--ink-faint);
    flex-shrink: 0;
    transition: background-color var(--ui-duration-base) var(--ui-ease-out);
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
    padding: 20px 18px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    color: var(--ink-mute);
    font-size: 12.5px;
  }
  .discover-empty {
    flex-direction: column;
    text-align: center;
    gap: 9px;
    padding: 36px 28px;
    border-style: dashed;
    border-color: var(--line-strong);
  }
  .radar {
    position: relative;
    display: grid;
    place-items: center;
    width: 46px;
    height: 46px;
    margin-bottom: 2px;
  }
  .radar-ring {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    border: 1px solid var(--line-strong);
    opacity: 0;
    animation: radar-out 3.2s ease-out infinite;
  }
  .radar-ring:nth-child(2) {
    animation-delay: 1.6s;
  }
  @keyframes radar-out {
    0% {
      opacity: 0.7;
      transform: scale(0.45);
    }
    100% {
      opacity: 0;
      transform: scale(1);
    }
  }
  .discover-icon {
    width: 22px;
    height: 22px;
    color: var(--ink-faint);
  }
  .discover-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--ink-soft);
  }
  .discover-hint {
    max-width: 380px;
    line-height: 1.6;
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
    padding: 0 0 4px;
    line-height: 1.55;
  }

  /* Pairing modal */
  .outgoing-card,
  .remove-card {
    width: min(400px, calc(100vw - 48px));
    padding: 24px;
    display: grid;
    gap: 16px;
  }
  .pair-head {
    display: flex;
    gap: 14px;
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
    margin-top: 4px;
    line-height: 1.55;
  }

  /* Code plate — click to copy */
  .outgoing-code {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 2px;
    padding: 18px 8px 16px;
    border-radius: var(--r-md);
    border: 1px solid var(--line);
    background: var(--control-hover);
    color: var(--ink-strong);
    cursor: pointer;
    transition:
      border-color var(--ui-duration-fast) var(--ui-ease-out),
      background-color var(--ui-duration-fast) var(--ui-ease-out);
  }
  .outgoing-code:hover {
    border-color: var(--line-strong);
    background: var(--control-active);
  }
  .outgoing-code.is-expired {
    opacity: 0.45;
  }
  .code-digit {
    font-family: var(--mono);
    font-size: 34px;
    font-weight: 500;
    line-height: 1;
    font-variant-numeric: tabular-nums;
    padding: 0 5px;
    animation: digit-in 320ms var(--ui-ease-out) both;
    animation-delay: var(--digit-delay, 0ms);
  }
  .code-digit.gap-after {
    margin-right: 16px;
  }
  @keyframes digit-in {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
  .code-copy {
    position: absolute;
    top: 7px;
    right: 8px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10.5px;
    color: var(--ink-faint);
    opacity: 0;
    transition:
      opacity var(--ui-duration-fast) var(--ui-ease-out),
      color var(--ui-duration-fast) var(--ui-ease-out);
  }
  .code-copy svg {
    width: 11px;
    height: 11px;
  }
  .outgoing-code:hover .code-copy,
  .outgoing-code:focus-visible .code-copy,
  .code-copy.is-copied {
    opacity: 1;
  }
  .code-copy.is-copied {
    color: var(--success);
  }

  /* Expiry meter */
  .code-meter {
    height: 3px;
    border-radius: 999px;
    background: var(--control-active);
    overflow: hidden;
    margin-top: -6px;
  }
  .code-meter-fill {
    display: block;
    height: 100%;
    background: var(--accent);
    border-radius: inherit;
    transition: width 1s linear;
  }
  .code-meter-fill.is-expired {
    background: var(--danger);
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

  /* Spacing goes on padding, not margin: the global `.panel-note` rule in
     Settings.svelte sets the margin shorthand at the same specificity and wins
     on source order, so a margin here is silently zeroed. */
  .sync-note {
    display: flex;
    gap: 9px;
    align-items: flex-start;
    padding-top: 30px;
    padding-bottom: 16px;
    line-height: 1.6;
  }
  .note-icon {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    margin-top: 1px;
    opacity: 0.7;
  }

  @media (prefers-reduced-motion: reduce) {
    .tile.is-live .tile-ring,
    .live-dot.is-live,
    .radar-ring,
    .device-card.is-busy .busy-bar,
    .sync-icon.is-spinning,
    .code-digit {
      animation: none;
    }
    .code-digit {
      opacity: 1;
    }
    .device-card:hover {
      transform: none;
    }
  }
</style>

