// Global state for LAN device sync: status snapshots, live peer states, and
// the incoming-pairing prompt. Listeners are started once from App.svelte.

import { invoke, listen } from './tauri';
import { fetchSnippets, fetchDictionary } from './stores.svelte';
import { loadContexts } from './contextsStore.svelte';
import { startPolling } from './polling';

export interface SyncDeviceInfo {
  uuid: string;
  name: string;
}

export interface DiscoveredDevice {
  uuid: string;
  name: string;
  addresses: string[];
  port: number;
  paired: boolean;
  last_seen_ms: number;
}

export interface PairedDevice {
  uuid: string;
  name: string;
  added_at: string | null;
  last_sync_at: string | null;
  state: string;
  error: string | null;
  online: boolean;
}

export interface PairingState {
  kind: 'incoming' | 'outgoing';
  phase: 'connecting' | 'waiting_for_code' | 'awaiting_code' | 'verifying' | 'failed';
  peer_uuid: string;
  peer_name: string;
  code: string | null;
  error: string | null;
}

export interface SyncStatus {
  this_device: SyncDeviceInfo;
  listener_active: boolean;
  pairing: PairingState | null;
  discovered: DiscoveredDevice[];
  peers: PairedDevice[];
  last_error_hint: string | null;
}

export const syncStore = $state({
  loaded: false,
  status: null as SyncStatus | null,
});

export async function refreshSyncStatus(): Promise<void> {
  try {
    const status = await invoke<SyncStatus>('sync_get_status');
    syncStore.status = status;
    syncStore.loaded = true;
  } catch (error) {
    console.error('Failed to load sync status:', error);
    syncStore.loaded = true;
  }
}

export function thisDeviceName(): string {
  return syncStore.status?.this_device.name ?? '';
}

/** Starts the backend event listeners. Returns a cleanup function. */
export function startSyncListeners(): () => void {
  // Events reduce latency, but correctness does not depend on catching one.
  // Poll backend-owned state so a reload or suspended WebView can always
  // reconstruct an active incoming or outgoing pairing session.
  const poll = startPolling(refreshSyncStatus,
    () => syncStore.status?.pairing ? 1000 : 30_000,
    { hiddenIntervalMs: 30_000, immediate: false });

  const unlisteners: Array<Promise<() => void>> = [
    listen('verenu:sync-devices-changed', () => {
      poll.request();
    }),
    listen('verenu:sync-status', () => {
      poll.request();
    }),
    listen<{ uuid: string; name: string }>('verenu:sync-pair-request', () => {
      poll.request();
    }),
    listen<{ uuid: string; ok: boolean; message: string }>('verenu:sync-pair-result', () => {
      poll.request();
    }),
    listen<{ tables: string[] }>('verenu:sync-data-changed', (event) => {
      const tables = event.payload.tables ?? [];
      if (tables.includes('dictionary')) void fetchDictionary();
      if (tables.includes('snippets')) void fetchSnippets();
      if (tables.includes('contexts')) void loadContexts(true);
      poll.request();
    }),
  ];
  // Reconstruct after registration, closing the startup event gap. This also
  // runs when started in the tray, where incoming pairing must stay available.
  void Promise.allSettled(unlisteners).then(() => poll.request());

  return () => {
    poll.stop();
    for (const promise of unlisteners) {
      void promise.then((unlisten) => unlisten()).catch(() => {});
    }
  };
}
