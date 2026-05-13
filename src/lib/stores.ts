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
