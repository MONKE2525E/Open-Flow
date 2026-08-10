import { invoke } from './tauri';
import type { ProviderId } from './settings';
import type { SettingsSectionId } from './settingsSections';

export type PageId = 'home' | 'insights' | 'dictionary' | 'snippets' | 'style';
export type AppearanceMode = 'system' | 'light' | 'dark';
export type PillState = 'idle' | 'recording' | 'processing' | 'handsfree';
export type FetchStatus = 'idle' | 'loading' | 'loaded' | 'error';

export interface Snippet {
  id: number;
  trigger: string;
  expansion: string;
  instructions: string;
  use_count: number;
  created_at: string;
}

export interface DictionaryEntry {
  id: number;
  term: string;
  mistake: string | null;
  auto_learned: boolean;
  correction_count: number;
  confidence_tier?: 'manual' | 'low' | 'medium' | 'high';
  last_seen_at?: string | null;
  created_at: string;
}

export interface UpdateInfo {
  version: string;
  downloadUrl: string;
  assetName: string;
  installMode: 'install' | 'download';
}

export interface ProviderStatusAlert {
  providerId: ProviderId;
  providerName: string;
  status: string;
  severity: string;
  message: string;
  detailsUrl: string;
}

export interface GlobalMessage {
  message: string;
  showToUsers: boolean;
  visibleUntil?: number | null;
}

export const appStore = $state({
  currentPage: 'home' as PageId,
  settingsOpen: false,
  // The settings rail lives in the Sidebar but the panel lives in Settings, so
  // the active section and its swap direction have to be shared state.
  settingsSection: 'general' as SettingsSectionId,
  settingsAnimDir: 1 as 1 | -1,
  appVersion: '',
  devModeEnabled: false,
  appearanceMode: 'system' as AppearanceMode,
  // Mirrors the `cleanup_enabled` setting. Shared here (rather than owned
  // privately by GeneralSection) so Style.svelte and the App Mappings
  // settings page can react live to the toggle without their own
  // fetch/listener plumbing — both are inert once cleanup is off, since
  // profile/tone/per-app cleanup-intensity overrides only ever feed the
  // cleanup LLM call that this setting skips entirely.
  cleanupEnabled: true,
  pillState: 'idle' as PillState,
  setupComplete: null as boolean | null,
  snippets: [] as Snippet[],
  snippetsFetchStatus: 'idle' as FetchStatus,
  snippetsFetchError: '',
  dictionary: [] as DictionaryEntry[],
  dictionaryFetchStatus: 'idle' as FetchStatus,
  dictionaryFetchError: '',
  updateInfo: null as UpdateInfo | null,
  betaUpdatesEnabled: false,
  providerStatusAlerts: [] as ProviderStatusAlert[],
  providerStatusSimulation: false,
  globalMessage: null as GlobalMessage | null,
  globalMessageSimulation: false,
  apiHealthy: null as boolean | null,
  isOnline: true,
});

let snippetsFetchToken = 0;
let dictionaryFetchToken = 0;

export function cancelSnippetsFetch() {
  snippetsFetchToken++;
  if (appStore.snippetsFetchStatus === 'loading') appStore.snippetsFetchStatus = 'loaded';
}
export function cancelDictionaryFetch() {
  dictionaryFetchToken++;
  if (appStore.dictionaryFetchStatus === 'loading') appStore.dictionaryFetchStatus = 'loaded';
}

export function formatIpcError(err: unknown): string {
  if (typeof err === 'object' && err !== null) {
    if ('message' in err) {
      const message = (err as { message?: unknown }).message;
      if (typeof message === 'string' && message.trim()) {
        return message.trim();
      }
    }
    if ('error' in err) {
      const error = (err as { error?: unknown }).error;
      if (typeof error === 'string' && error.trim()) {
        return error.trim();
      }
    }
  }
  if (err instanceof Error && err.message?.trim()) {
    return err.message.trim();
  }
  const raw = String(err ?? '').trim();
  if (!raw || raw === '[object Object]') {
    return 'The backend is unavailable.';
  }
  return raw;
}

export async function fetchSnippets(): Promise<void> {
  const token = ++snippetsFetchToken;
  appStore.snippetsFetchStatus = 'loading';
  appStore.snippetsFetchError = '';
  try {
    const data = await invoke<Snippet[]>('get_snippets');
    if (token !== snippetsFetchToken) return;
    appStore.snippets = data ?? [];
    appStore.snippetsFetchStatus = 'loaded';
  } catch (err) {
    if (token !== snippetsFetchToken) return;
    console.error('IPC fetchSnippets failed:', err);
    appStore.snippetsFetchStatus = 'error';
    appStore.snippetsFetchError = formatIpcError(err);
  }
}

export const cleanupPromptOverridesStore = $state<{ overrides: Record<string, string> }>({
  overrides: {},
});

export const cleanupPromptEditor = $state<{
  open: boolean;
  provider: ProviderId | null;
  model: string | null;
  origin: { x: number; y: number } | null;
}>({
  open: false,
  provider: null,
  model: null,
  origin: null,
});

export function openCleanupPromptEditor(
  provider: ProviderId,
  model: string,
  triggerRect: DOMRect
) {
  cleanupPromptEditor.provider = provider;
  cleanupPromptEditor.model = model;
  cleanupPromptEditor.origin = {
    x: triggerRect.left + triggerRect.width / 2,
    y: triggerRect.top + triggerRect.height / 2,
  };
  cleanupPromptEditor.open = true;
}

export function closeCleanupPromptEditor() {
  cleanupPromptEditor.open = false;
}

export async function fetchDictionary(): Promise<void> {
  const token = ++dictionaryFetchToken;
  appStore.dictionaryFetchStatus = 'loading';
  appStore.dictionaryFetchError = '';
  try {
    const data = await invoke<DictionaryEntry[]>('get_dictionary');
    if (token !== dictionaryFetchToken) return;
    appStore.dictionary = data ?? [];
    appStore.dictionaryFetchStatus = 'loaded';
  } catch (err) {
    if (token !== dictionaryFetchToken) return;
    console.error('IPC fetchDictionary failed:', err);
    appStore.dictionaryFetchStatus = 'error';
    appStore.dictionaryFetchError = formatIpcError(err);
  }
}
