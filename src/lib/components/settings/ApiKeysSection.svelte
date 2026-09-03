<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '../../tauri';
  import { getProviderLogo } from '../../setup/ProviderLogos';

  type ProviderId = 'groq' | 'openai' | 'google' | 'assemblyai';
  type KeyStatus = Record<ProviderId, boolean>;
  type KeyDrafts = Record<ProviderId, string>;
  type KeyValidation = { status: 'idle' | 'checking' | 'valid' | 'invalid' | 'unknown'; message: string };

  const keyProviders: { id: ProviderId; label: string; ph: string; models: string }[] = [
    { id: 'groq',       label: 'Groq',       ph: 'gsk_…',        models: 'whisper-large-v3-turbo · llama-3.3-70b' },
    { id: 'openai',     label: 'OpenAI',     ph: 'sk-…',         models: 'gpt-4o-transcribe · gpt-4o-mini' },
    { id: 'google',     label: 'Gemini',     ph: 'AIza…',        models: 'gemini-3.5-transcribe · gemini-3.5-flash-lite' },
    { id: 'assemblyai', label: 'AssemblyAI', ph: '32-char key',  models: 'universal-3-5-pro · universal-2' },
  ];

  let keyStatus = $state<KeyStatus>({ groq: false, openai: false, google: false, assemblyai: false });
  let draftKeys = $state<KeyDrafts>({ groq: '', openai: '', google: '', assemblyai: '' });
  let keySaving = $state<Record<ProviderId, boolean>>({ groq: false, openai: false, google: false, assemblyai: false });
  let keyErrors = $state<KeyDrafts>({ groq: '', openai: '', google: '', assemblyai: '' });
  let keyValidation = $state<Record<ProviderId, KeyValidation>>({
    groq: { status: 'idle', message: '' },
    openai: { status: 'idle', message: '' },
    google: { status: 'idle', message: '' },
    assemblyai: { status: 'idle', message: '' },
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

  // Save now tests first: a definitively-rejected key (401/403) is never persisted.
  async function saveKey(provider: ProviderId) {
    const key = draftKeys[provider].trim();
    if (!key) return;
    keyErrors[provider] = '';
    keyValidation[provider] = { status: 'checking', message: '' };
    keySaving[provider] = true;
    try {
      let validation: { status: 'valid' | 'invalid' | 'unknown'; message: string };
      try {
        const result = await invoke<{ ok: boolean; status: 'valid' | 'invalid' | 'unknown'; message: string }>('validate_api_key', { provider, key });
        validation = { status: result.status, message: result.message };
      } catch {
        validation = { status: 'unknown', message: "Couldn't verify the key right now." };
      }

      // Definitive rejection — don't store it, surface the failure in red.
      if (validation.status === 'invalid') {
        keyValidation[provider] = validation;
        return;
      }

      await invoke('save_api_key', { provider, key });
      const status = await loadKeyStatus();
      if (!status[provider]) {
        keyValidation[provider] = { status: 'idle', message: '' };
        keyErrors[provider] = 'The key did not persist after saving. Please try again.';
        return;
      }

      draftKeys[provider] = '';
      // The model picker lives in a sibling section and can only list a
      // provider's models once the key is actually saved, so tell it now
      // rather than making the user reopen Settings.
      window.dispatchEvent(new CustomEvent('verenu:api-key-saved', { detail: { provider } }));
      // 'unknown' = couldn't reach the provider; we saved it anyway (might be fine)
      // but say so plainly instead of claiming it's verified.
      keyValidation[provider] =
        validation.status === 'valid'
          ? { status: 'valid', message: 'Key verified.' }
          : { status: 'unknown', message: "Saved, but couldn't verify it right now." };
    } catch (e) {
      console.error('save_api_key failed', e);
      keyValidation[provider] = { status: 'idle', message: '' };
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
  <div class="setting-row key-row" data-setting-target={`api-key-${item.id}`}>
    <div class="key-left">
      <div class="label">
        <span class="key-logo">{@html getProviderLogo(item.id)}</span>
        {item.label}
        {#if keyValidation[item.id].status === 'invalid'}
          <span class="key-status failed" title={keyValidation[item.id].message}>
            <span class="key-status-dot"></span>failed
          </span>
        {:else if keyStatus[item.id]}
          <span class="key-status saved" title={keyValidation[item.id].message || 'Key saved'}>
            <span class="key-status-dot"></span>saved
          </span>
        {/if}
      </div>
      <div class="desc">{item.models}</div>
    </div>
    <div class="key-right">
      <input
        type="password"
        class="key-input"
        aria-label={`${item.label} API key`}
        class:failed={keyValidation[item.id].status === 'invalid'}
        placeholder={keyStatus[item.id] ? '••••••••••••' : item.ph}
        bind:value={draftKeys[item.id]}
        oninput={() => {
          if (keyValidation[item.id].status === 'invalid') keyValidation[item.id] = { status: 'idle', message: '' };
          if (keyErrors[item.id]) keyErrors[item.id] = '';
        }}
        onkeydown={(e) => e.key === 'Enter' && saveKey(item.id)}
        autocomplete="off"
        aria-invalid={keyErrors[item.id] || keyValidation[item.id].status === 'invalid' ? 'true' : 'false'}
      />
      <div class="flip-btn" class:flipped={keyStatus[item.id] && !draftKeys[item.id].trim()}>
        <button
          class="btn-ghost flip-face front"
          onclick={() => saveKey(item.id)}
          disabled={!draftKeys[item.id].trim() || keySaving[item.id]}
          tabindex={keyStatus[item.id] && !draftKeys[item.id].trim() ? -1 : 0}
          aria-hidden={keyStatus[item.id] && !draftKeys[item.id].trim() ? 'true' : 'false'}
        >{keySaving[item.id] ? 'Saving…' : 'Save'}</button>
        <button
          class="btn-ghost btn-clear flip-face back"
          onclick={() => clearKey(item.id)}
          disabled={!keyStatus[item.id] || draftKeys[item.id].trim().length > 0 || keySaving[item.id]}
          tabindex={keyStatus[item.id] && !draftKeys[item.id].trim() ? 0 : -1}
          aria-hidden={keyStatus[item.id] && !draftKeys[item.id].trim() ? 'false' : 'true'}
        >Clear</button>
      </div>
    </div>
    {#if keyErrors[item.id]}
      <p class="key-error">{keyErrors[item.id]}</p>
    {/if}
  </div>
{/each}

<p class="trademark-note">
  The logos above belong to their respective companies. Verenu is not affiliated with, endorsed by, or sponsored by Groq, OpenAI, Google, or AssemblyAI — they are shown solely to indicate provider compatibility.
</p>

<style>
  .trademark-note {
    font-size: 11px;
    color: var(--ink-faint);
    line-height: 1.5;
    margin: 14px 0 0;
  }

  /* Centered rather than top-aligned: the row no longer wraps at the widths the
     settings column actually reaches, so flex-start just read as top-heavy. */
  .key-row { align-items: center; gap: 12px; flex-wrap: wrap; }
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
  /* Inline status chip next to the provider name — saved (green) / failed (red).
     Slides + pops in; the failed dot pulses a ring twice to draw the eye. */
  .key-status {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin-left: 8px;
    font-family: var(--mono);
    font-size: 10px;
    font-weight: 400;
    letter-spacing: 0.02em;
    animation: status-in 0.26s cubic-bezier(0.22, 1, 0.36, 1) both;
  }
  .key-status.saved { color: var(--success); }
  .key-status.failed { color: var(--danger); }
  .key-status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
    flex-shrink: 0;
    animation: dot-pop 0.3s cubic-bezier(0.22, 1, 0.36, 1) both;
  }
  .key-status.failed .key-status-dot {
    animation:
      dot-pop 0.3s cubic-bezier(0.22, 1, 0.36, 1) both,
      dot-pulse 1.1s ease-out 0.18s 2;
  }
  @keyframes status-in {
    from { opacity: 0; transform: translateX(-5px); }
    to { opacity: 1; transform: none; }
  }
  @keyframes dot-pop {
    from { transform: scale(0); }
    60% { transform: scale(1.3); }
    to { transform: scale(1); }
  }
  @keyframes dot-pulse {
    0% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--danger) 50%, transparent); }
    70% { box-shadow: 0 0 0 5px color-mix(in srgb, var(--danger) 0%, transparent); }
    100% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--danger) 0%, transparent); }
  }
  .key-input {
    font-family: var(--mono);
    font-size: 11.5px;
    background: transparent;
    border: 1px solid var(--line);
    padding: 5px 9px;
    border-radius: 6px;
    color: var(--ink-soft);
    width: clamp(200px, 40cqi, 320px);
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

  /* Save ⇄ Clear split-flap flip. Both faces share one grid cell so the
     container auto-sizes to the wider face; the whole thing rotates on X. */
  .flip-btn {
    display: inline-grid;
    min-width: 72px;
    flex-shrink: 0;
    position: relative;
  }
  .flip-btn.flipped { min-width: 72px; }
  .flip-face {
    grid-area: 1 / 1;
    width: 100%;
    text-align: center;
    transition: opacity 160ms var(--ui-ease-out);
  }
  .flip-face.front { pointer-events: auto; }
  .flip-face.back { opacity: 0; pointer-events: none; }
  .flip-btn.flipped .flip-face.front { opacity: 0; pointer-events: none; }
  .flip-btn.flipped .flip-face.back { opacity: 1; pointer-events: auto; }

  /* Failure feedback: red border + one-shot shake when a key is rejected. */
  .key-input[aria-invalid='true'] { border-color: var(--danger); }
  .key-input.failed {
    border-color: var(--danger);
    animation: key-shake 0.4s ease;
  }
  .key-input.failed:focus {
    border-color: var(--danger);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--danger) 18%, transparent);
  }
  @keyframes key-shake {
    0%, 100% { transform: translateX(0); }
    20% { transform: translateX(-4px); }
    40% { transform: translateX(4px); }
    60% { transform: translateX(-3px); }
    80% { transform: translateX(2px); }
  }

  .key-error {
    width: 100%;
    margin: 4px 0 0;
    font-size: 11px;
    color: var(--danger);
  }

  @media (prefers-reduced-motion: reduce) {
    .flip-face { transition: none; }
    .key-input.failed,
    .key-status,
    .key-status-dot { animation: none; }
  }
</style>
