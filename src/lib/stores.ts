import { writable } from 'svelte/store';

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
