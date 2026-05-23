<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';

  const keyProviders: { id: 'groq' | 'openai' | 'google'; label: string; ph: string; models: string }[] = [
    { id: 'groq',   label: 'Groq',   ph: 'gsk_…',  models: 'whisper-large-v3-turbo · llama-3.3-70b' },
    { id: 'openai', label: 'OpenAI', ph: 'sk-…',   models: 'gpt-4o-transcribe · gpt-4o-mini' },
    { id: 'google', label: 'Google', ph: 'AIza…',  models: 'Chirp 3 · gemini-3.5-flash' },
  ];

  let keyStatus = $state({ groq: false, openai: false, google: false });
  let draftKeys = $state({ groq: '', openai: '', google: '' });

  async function loadKeyStatus() {
    try {
      keyStatus = await invoke<typeof keyStatus>('get_api_key_status');
    } catch (err) {
      console.error('get_api_key_status failed:', err);
    }
  }

  async function saveKey(provider: 'groq' | 'openai' | 'google') {
    const key = draftKeys[provider].trim();
    if (!key) return;
    try {
      await invoke('save_api_key', { provider, key });
      keyStatus = { ...keyStatus, [provider]: true };
      draftKeys = { ...draftKeys, [provider]: '' };
    } catch (e) {
      console.error('save_api_key failed', e);
    }
  }

  loadKeyStatus();
</script>

<h2 class="settings-h">API Keys</h2>
<p class="panel-note">Keys are stored locally and never readable from the UI after saving.</p>

{#each keyProviders as item}
  <div class="setting-row key-row">
    <div class="key-left">
      <div class="label">
        {item.label}
        {#if keyStatus[item.id]}
          <span class="key-saved">● saved</span>
        {/if}
      </div>
      <div class="desc">{item.models}</div>
    </div>
    <div class="key-right">
      <input
        type="password"
        class="key-input"
        placeholder={keyStatus[item.id] ? '••••••••••••' : item.ph}
        bind:value={draftKeys[item.id]}
        onkeydown={(e) => e.key === 'Enter' && saveKey(item.id)}
        autocomplete="off"
      />
      <button
        class="btn-ghost"
        onclick={() => saveKey(item.id)}
        disabled={!draftKeys[item.id].trim()}
      >Save</button>
    </div>
  </div>
{/each}

<style>
  .key-row { align-items: flex-start; gap: 12px; }
  .key-left { flex: 1; min-width: 0; }
  .key-right { display: flex; gap: 6px; align-items: center; flex-shrink: 0; }
  .key-saved {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--success);
    font-weight: 400;
    margin-left: 6px;
    letter-spacing: 0.02em;
  }
  .key-input {
    font-family: var(--mono);
    font-size: 11.5px;
    background: transparent;
    border: 1px solid var(--line);
    padding: 5px 9px;
    border-radius: 6px;
    color: var(--ink-soft);
    width: 200px;
    letter-spacing: 0.04em;
  }
</style>
