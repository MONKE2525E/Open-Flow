<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '../../tauri';
  import { getProviderLogo } from '../../setup/ProviderLogos';

  type ProviderId = 'groq' | 'openai' | 'google';
  type KeyStatus = Record<ProviderId, boolean>;
  type KeyDrafts = Record<ProviderId, string>;
  type KeyValidation = { status: 'idle' | 'checking' | 'valid' | 'invalid' | 'unknown'; message: string };

  const keyProviders: { id: ProviderId; label: string; ph: string; models: string }[] = [
    { id: 'groq',   label: 'Groq',   ph: 'gsk_…',  models: 'whisper-large-v3-turbo · llama-3.3-70b' },
    { id: 'openai', label: 'OpenAI', ph: 'sk-…',   models: 'gpt-4o-transcribe · gpt-4o-mini' },
    { id: 'google', label: 'Google', ph: 'AIza…',  models: 'Chirp 3 · gemini-3.5-flash' },
  ];

  let keyStatus = $state<KeyStatus>({ groq: false, openai: false, google: false });
  let draftKeys = $state<KeyDrafts>({ groq: '', openai: '', google: '' });
  let keySaving = $state<Record<ProviderId, boolean>>({ groq: false, openai: false, google: false });
  let keyErrors = $state<KeyDrafts>({ groq: '', openai: '', google: '' });
  let keyValidation = $state<Record<ProviderId, KeyValidation>>({
    groq: { status: 'idle', message: '' },
    openai: { status: 'idle', message: '' },
    google: { status: 'idle', message: '' },
  });

  async function loadKeyStatus() {
    try {
      const status = await invoke<KeyStatus>('get_api_key_status');
      keyStatus = status;
      return status;
    } catch (err) {
      console.error('get_api_key_status failed:', err);
      return keyStatus;
    }
  }

  async function testKey(provider: ProviderId, key: string) {
    if (!key.trim()) return;
    keyValidation[provider] = { status: 'checking', message: '' };
    try {
      const result = await invoke<{ ok: boolean; message: string }>('validate_api_key', { provider, key: key.trim() });
      keyValidation[provider] = { status: result.ok ? 'valid' : 'invalid', message: result.message };
    } catch {
      keyValidation[provider] = { status: 'unknown', message: "Couldn't verify the key right now." };
    }
  }

  async function saveKey(provider: ProviderId) {
    const key = draftKeys[provider].trim();
    if (!key) return;
    keyErrors[provider] = '';
    keySaving[provider] = true;
    try {
      await invoke('save_api_key', { provider, key });
      const status = await loadKeyStatus();
      if (status[provider]) {
        draftKeys[provider] = '';
        void testKey(provider, key);
      } else {
        keyErrors[provider] = 'The key did not persist after saving. Please try again.';
      }
    } catch (e) {
      console.error('save_api_key failed', e);
      keyErrors[provider] = 'Could not save this key locally. Please try again.';
    } finally {
      keySaving[provider] = false;
    }
  }

  async function clearKey(provider: ProviderId) {
    keyErrors[provider] = '';
    keySaving[provider] = true;
    try {
      await invoke('delete_api_key', { provider });
      await loadKeyStatus();
      draftKeys[provider] = '';
      keyValidation[provider] = { status: 'idle', message: '' };
    } catch (e) {
      console.error('delete_api_key failed', e);
      keyErrors[provider] = 'Could not remove this key locally. Please try again.';
    } finally {
      keySaving[provider] = false;
    }
  }

  onMount(() => {
    void loadKeyStatus();
  });
</script>

<h2 class="settings-h">API Keys</h2>
<p class="panel-note">Keys are stored locally and never readable from the UI after saving.</p>

{#each keyProviders as item}
  <div class="setting-row key-row">
    <div class="key-left">
      <div class="label">
        <span class="key-logo">{@html getProviderLogo(item.id)}</span>
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
        aria-invalid={keyErrors[item.id] ? 'true' : 'false'}
      />
      <button
        class="btn-ghost"
        onclick={() => testKey(item.id, draftKeys[item.id])}
        disabled={!draftKeys[item.id].trim() || keySaving[item.id] || keyValidation[item.id].status === 'checking'}
      >{keyValidation[item.id].status === 'checking' ? 'Testing…' : 'Test'}</button>
      <button
        class="btn-ghost"
        onclick={() => saveKey(item.id)}
        disabled={!draftKeys[item.id].trim() || keySaving[item.id]}
      >{keySaving[item.id] ? 'Saving…' : 'Save'}</button>
      {#if keyStatus[item.id]}
        <button
          class="btn-ghost btn-clear"
          onclick={() => clearKey(item.id)}
          disabled={keySaving[item.id]}
        >Clear</button>
      {/if}
    </div>
    {#if keyErrors[item.id]}
      <p class="key-error">{keyErrors[item.id]}</p>
    {/if}
    {#if keyValidation[item.id].status !== 'idle' && keyValidation[item.id].status !== 'checking'}
      <p class="key-validation" class:valid={keyValidation[item.id].status === 'valid'}>
        {keyValidation[item.id].status === 'valid' ? 'Key verified.' : keyValidation[item.id].message}
      </p>
    {/if}
  </div>
{/each}

<style>
  .key-row { align-items: flex-start; gap: 12px; flex-wrap: wrap; }
  .key-left { flex: 1; min-width: 0; }
  .key-logo {
    display: inline-flex;
    width: 16px;
    height: 16px;
    color: var(--ink-mute);
    vertical-align: -3px;
    margin-right: 6px;
  }
  .key-logo :global(svg) { width: 100%; height: 100%; }
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
  .key-input::-ms-reveal {
    display: none;
  }

  .key-input::-ms-clear {
    display: none;
  }

  .key-input::-webkit-credentials-auto-fill-button {
    display: none;
  }

  .key-input::-webkit-contacts-auto-fill-button {
    display: none;
  }
  .key-input:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 15%, transparent);
  }
  .btn-clear {
    color: var(--danger);
    border-color: var(--danger-line);
  }
  .btn-clear:hover {
    background: var(--danger-bg);
    border-color: var(--danger-line);
  }
  .key-error {
    width: 100%;
    margin: 4px 0 0;
    font-size: 11px;
    color: var(--danger);
  }
  .key-validation {
    width: 100%;
    margin: 4px 0 0;
    font-size: 11px;
    color: var(--warning);
  }
  .key-validation.valid { color: var(--success); }
</style>
