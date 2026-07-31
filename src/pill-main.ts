import { mount } from 'svelte';
import PillApp from './PillApp.svelte';
import './theme.css';

type AppearanceMode = 'system' | 'light' | 'dark';
type EffectiveTheme = 'light' | 'dark';

function systemTheme(): EffectiveTheme {
  return window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

let currentMode: AppearanceMode = 'system';

function applyTheme(mode: AppearanceMode) {
  currentMode = mode;
  document.documentElement.dataset.theme = mode === 'system' ? systemTheme() : mode;
}

applyTheme('system');

(async () => {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const mode = await invoke<AppearanceMode | null>('get_setting', { key: 'appearance_mode' });
    if (mode === 'system' || mode === 'light' || mode === 'dark') {
      applyTheme(mode);
    }
  } catch {}
})();

window.matchMedia?.('(prefers-color-scheme: dark)').addEventListener?.('change', () => {
  if (currentMode === 'system') applyTheme('system');
});

mount(PillApp, { target: document.getElementById('pill-root')! });

void import('@tauri-apps/api/core')
  .then(({ invoke }) => invoke('frontend_ready'))
  .catch((error) => {
    console.error('Failed to complete pill startup handshake:', error);
  });
