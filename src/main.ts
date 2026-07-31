import { mount } from 'svelte';
import { invoke } from '@tauri-apps/api/core';
import App from './App.svelte';
import './theme.css';
import './app.css';

const app = mount(App, {
  target: document.getElementById('app') as HTMLElement,
});

void invoke('frontend_ready').catch((error) => {
  console.error('Failed to complete startup handshake:', error);
});

export default app;
