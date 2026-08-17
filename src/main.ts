import { mount } from 'svelte';
import App from './App.svelte';
import { invoke } from './lib/tauri';
import { disableBrowserContextMenu } from './lib/disable-context-menu';
import './theme.css';
import './app.css';

disableBrowserContextMenu(); // The app webview lives until the process exits.

const app = mount(App, {
  target: document.getElementById('app') as HTMLElement,
});

void invoke('frontend_ready').catch((error) => {
  console.error('Failed to complete startup handshake:', error);
});

export default app;
