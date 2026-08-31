// Global state for LAN device sync: status snapshots, live peer states, and
// the incoming-pairing prompt. Listeners are started once from App.svelte.

import { invoke, listen } from './tauri';
import { fetchSnippets, fetchDictionary } from './stores.svelte';
import { loadContexts } from './contextsStore.svelte';

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
  }
}

export function thisDeviceName(): string {
  return syncStore.status?.this_device.name ?? '';
}

/** Starts the backend event listeners. Returns a cleanup function. */
export function startSyncListeners(): () => void {
  void refreshSyncStatus();

  // Events reduce latency, but correctness does not depend on catching one.
  // Poll backend-owned state so a reload or suspended WebView can always
  // reconstruct an active incoming or outgoing pairing session.
  const poll = window.setInterval(() => {
    void refreshSyncStatus();
  }, 1000);

  const unlisteners: Array<Promise<() => void>> = [
    listen('verenu:sync-devices-changed', () => {
      void refreshSyncStatus();
    }),
    listen('verenu:sync-status', () => {
      void refreshSyncStatus();
    }),
    listen<{ uuid: string; name: string }>('verenu:sync-pair-request', () => {
      void refreshSyncStatus();
    }),
    listen<{ uuid: string; ok: boolean; message: string }>('verenu:sync-pair-result', () => {
      void refreshSyncStatus();
    }),
    listen<{ tables: string[] }>('verenu:sync-data-changed', (event) => {
      const tables = event.payload.tables ?? [];
      if (tables.includes('dictionary')) void fetchDictionary();
      if (tables.includes('snippets')) void fetchSnippets();
      if (tables.includes('contexts')) void loadContexts(true);
      void refreshSyncStatus();
    }),
  ];

  return () => {
    window.clearInterval(poll);
    for (const promise of unlisteners) {
      void promise.then((unlisten) => unlisten()).catch(() => {});
    }
  };
}
