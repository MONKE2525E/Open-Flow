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
  peer_uuid: string;
  peer_name: string;
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
  // Incoming pairing prompt state (rendered globally by App.svelte).
  incoming: null as { uuid: string; name: string } | null,
  // Outgoing pairing state (rendered inside SyncSection).
  outgoing: null as { uuid: string; name: string; code: string } | null,
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

  const unlisteners: Array<Promise<() => void>> = [
    listen('verenu:sync-devices-changed', () => {
      void refreshSyncStatus();
    }),
    listen('verenu:sync-status', () => {
      void refreshSyncStatus();
    }),
    listen<{ uuid: string; name: string }>('verenu:sync-pair-request', (event) => {
      syncStore.incoming = { uuid: event.payload.uuid, name: event.payload.name };
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
    for (const promise of unlisteners) {
      void promise.then((unlisten) => unlisten()).catch(() => {});
    }
  };
}
