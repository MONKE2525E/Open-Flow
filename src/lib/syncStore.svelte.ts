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

/** Backend PAIRING_TIMEOUT / PAIRING_PROMPT_LIFETIME (sync/manager.rs). */
export const PAIRING_WINDOW_MS = 180_000;

export const syncStore = $state({
  loaded: false,
  status: null as SyncStatus | null,
  // Incoming pairing prompt state (rendered globally by App.svelte).
  incoming: null as { uuid: string; name: string; startedAt: number } | null,
  // Outgoing pairing state (rendered inside SyncSection).
  outgoing: null as { uuid: string; name: string; code: string; startedAt: number } | null,
  // One shared banner slot for every sync outcome, so a pairing that fails in
  // the background is not swallowed silently.
  flash: null as { message: string; kind: 'ok' | 'err'; id: number } | null,
});

let flashId = 0;
let flashTimer: ReturnType<typeof setTimeout> | undefined;

/** Shows a transient sync message. Later calls replace earlier ones. */
export function flashSync(message: string, kind: 'ok' | 'err'): void {
  const id = ++flashId;
  syncStore.flash = { message, kind, id };
  clearTimeout(flashTimer);
  flashTimer = setTimeout(() => {
    if (syncStore.flash?.id === id) syncStore.flash = null;
  }, kind === 'err' ? 8000 : 4500);
}

export function clearSyncFlash(): void {
  clearTimeout(flashTimer);
  syncStore.flash = null;
}

export function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export async function refreshSyncStatus(): Promise<void> {
  try {
    const status = await invoke<SyncStatus>('sync_get_status');
    syncStore.status = status;
    syncStore.loaded = true;
  } catch (error) {
    // Still mark it loaded: an unreachable manager should land on the empty
    // state, not spin "Searching your network…" forever.
    syncStore.loaded = true;
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
      syncStore.incoming = {
        uuid: event.payload.uuid,
        name: event.payload.name,
        startedAt: Date.now(),
      };
      void refreshSyncStatus();
    }),
    listen<{ uuid: string; ok: boolean; message: string }>('verenu:sync-pair-result', (event) => {
      // The outgoing handshake runs in a backend task: this event is the only
      // place its success or failure ever surfaces.
      const { ok, message } = event.payload ?? { ok: false, message: '' };
      if (message) flashSync(message, ok ? 'ok' : 'err');
      syncStore.incoming = null;
      syncStore.outgoing = null;
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

