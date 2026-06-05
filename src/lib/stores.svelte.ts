import { invoke } from './tauri';

export type PageId = 'home' | 'dictionary' | 'snippets' | 'style';
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
}

export const appStore = $state({
  currentPage: 'home' as PageId,
  settingsOpen: false,
  devModeEnabled: false,
  appearanceMode: 'system' as AppearanceMode,
  pillState: 'idle' as PillState,
  setupComplete: null as boolean | null,
  snippets: [] as Snippet[],
  snippetsFetchStatus: 'idle' as FetchStatus,
  snippetsFetchError: '',
  dictionary: [] as DictionaryEntry[],
  dictionaryFetchStatus: 'idle' as FetchStatus,
  dictionaryFetchError: '',
  updateInfo: null as UpdateInfo | null,
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
    appStore.snippetsFetchStatus = 'idle';
  } catch (err) {
    if (token !== snippetsFetchToken) return;
    console.error('IPC fetchSnippets failed:', err);
    appStore.snippetsFetchStatus = 'error';
    appStore.snippetsFetchError = formatIpcError(err);
  }
}

export async function fetchDictionary(): Promise<void> {
  const token = ++dictionaryFetchToken;
  appStore.dictionaryFetchStatus = 'loading';
  appStore.dictionaryFetchError = '';
  try {
    const data = await invoke<DictionaryEntry[]>('get_dictionary');
    if (token !== dictionaryFetchToken) return;
    appStore.dictionary = data ?? [];
    appStore.dictionaryFetchStatus = 'idle';
  } catch (err) {
    if (token !== dictionaryFetchToken) return;
    console.error('IPC fetchDictionary failed:', err);
    appStore.dictionaryFetchStatus = 'error';
    appStore.dictionaryFetchError = formatIpcError(err);
  }
}
