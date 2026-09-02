import { mount } from 'svelte';
import PillApp from './PillApp.svelte';
import { invoke, listen } from './lib/tauri';
import { ACCENT_CHANGE_EVENT, applyAccentTheme, normalizeAccentColor } from './lib/accentTheme';
import { disableBrowserContextMenu } from './lib/disable-context-menu';
import './theme.css';

disableBrowserContextMenu(); // The pill webview lives until the process exits.

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
    const [mode, accentColor] = await Promise.all([
      invoke<AppearanceMode | null>('get_setting', { key: 'appearance_mode' }),
      invoke<string | null>('get_setting', { key: 'accent_color' }),
    ]);
    if (mode === 'system' || mode === 'light' || mode === 'dark') {
      applyTheme(mode);
    }
    applyAccentTheme(document.documentElement, normalizeAccentColor(accentColor), { animate: false });
  } catch {}
})();

void listen<string | null>(ACCENT_CHANGE_EVENT, (event) => {
  applyAccentTheme(document.documentElement, normalizeAccentColor(event.payload));
});

window.matchMedia?.('(prefers-color-scheme: dark)').addEventListener?.('change', () => {
  if (currentMode === 'system') applyTheme('system');
});

mount(PillApp, { target: document.getElementById('pill-root')! });

void invoke('frontend_ready').catch((error) => {
  console.error('Failed to complete pill startup handshake:', error);
});
