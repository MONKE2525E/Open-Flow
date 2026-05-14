import { writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';

export const currentPage = writable<'home' | 'dictionary' | 'snippets' | 'style'>('home');
export const settingsOpen = writable(false);
export const accentColor = writable<'terracotta' | 'moss' | 'slate' | 'ink'>('terracotta');
export const pillState = writable<'idle' | 'recording' | 'processing' | 'handsfree'>('idle');

// Home page data — populated by Home.svelte via invoke('get_recent') / invoke('get_stats')
export const recentDictations = writable<{ time: string; text: string }[]>([]);
export const stats = writable({ totalWords: 0, wpm: 0, dayStreak: 0 });

// Style page
export const currentIntensity = writable('medium');
export const currentTone = writable('casual');
export const styleTab = writable('cleanup');

// Setup — null means not yet checked, false = show wizard, true = done
export const setupComplete = writable<boolean | null>(null);

// Snippets
export interface Snippet {
  id: number;
  trigger: string;
  expansion: string;
  instructions: string;
  use_count: number;
  created_at: string;
}

export const snippets = writable<Snippet[]>([]);

export async function fetchSnippets(): Promise<void> {
  try {
    const data = await invoke<Snippet[]>('get_snippets');
    snippets.set(data);
  } catch { /* dev mode — no backend */ }
}

// Dictionary
export interface DictionaryEntry {
  id: number;
  term: string;           // the actual word/term the user wants
  mistake: string | null; // optional: what the transcription typically writes instead
  auto_learned: boolean;
  correction_count: number;
  created_at: string;
}

export const dictionary = writable<DictionaryEntry[]>([]);

export async function fetchDictionary(): Promise<void> {
  try {
    const data = await invoke<DictionaryEntry[]>('get_dictionary');
    dictionary.set(data);
  } catch { /* dev mode — no backend */ }
}

// Updates
export interface UpdateInfo {
  version: string;
  downloadUrl: string;
}

export const updateInfo = writable<UpdateInfo | null>(null);
