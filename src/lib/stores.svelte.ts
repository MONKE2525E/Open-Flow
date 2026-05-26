import { invoke } from '@tauri-apps/api/core';

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
  recentDictations: [] as { time: string; text: string }[],
  stats: { totalWords: 0, wpm: 0, dayStreak: 0 },
  currentIntensity: 'medium',
  currentTone: 'casual',
  styleTab: 'cleanup',
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

function formatIpcError(err: unknown): string {
  if (typeof err === 'object' && err !== null && 'message' in err) {
    const message = (err as { message?: unknown }).message;
    if (typeof message === 'string' && message.trim()) {
      return message.trim();
    }
  }
  if (err instanceof Error && err.message.trim()) {
    return err.message.trim();
  }
  const raw = String(err ?? '').trim();
  return raw || 'The backend is unavailable.';
}

export async function fetchSnippets(): Promise<void> {
  appStore.snippetsFetchStatus = 'loading';
  appStore.snippetsFetchError = '';
  try {
    const data = await invoke<Snippet[]>('get_snippets');
    appStore.snippets = data;
    appStore.snippetsFetchStatus = 'loaded';
  } catch (err) {
    console.error('IPC fetchSnippets failed:', err);
    appStore.snippetsFetchStatus = 'error';
    appStore.snippetsFetchError = formatIpcError(err);
  }
}

export async function fetchDictionary(): Promise<void> {
  appStore.dictionaryFetchStatus = 'loading';
  appStore.dictionaryFetchError = '';
  try {
    const data = await invoke<DictionaryEntry[]>('get_dictionary');
    appStore.dictionary = data;
    appStore.dictionaryFetchStatus = 'loaded';
  } catch (err) {
    console.error('IPC fetchDictionary failed:', err);
    appStore.dictionaryFetchStatus = 'error';
    appStore.dictionaryFetchError = formatIpcError(err);
  }
}
