<script lang="ts">
  import type { ProviderId } from '../../settings';
  import { providerGuides } from '../setupData';
  import { isMac } from '../../platform';

  type KeyValidation = { status: 'idle' | 'checking' | 'valid' | 'invalid' | 'unknown'; message: string };

  let {
    provider,
    providerName,
    apiKeyDraft = $bindable(),
    showKey = $bindable(),
    keySaved,
    keySaving,
    keyError,
    keyValidation,
  }: {
    provider: ProviderId;
    providerName: string;
    apiKeyDraft: string;
    showKey: boolean;
    keySaved: boolean;
    keySaving: boolean;
    keyError: string;
    keyValidation: KeyValidation;
  } = $props();

  let guide = $derived(providerGuides[provider]);

  function copyUrl(url: string) {
    navigator.clipboard.writeText('https://' + url).catch(() => {});
  }
</script>

<div class="step">
  <div class="key-guide">
    <p class="guide-label">How to get your key</p>
    <ol class="guide-steps">
      {#each guide.steps as s}
        <li>{s}</li>
      {/each}
    </ol>
    <div class="url-row">
      <span class="url-display">{guide.url}</span>
      <button class="copy-btn" onclick={() => copyUrl(guide.url)}>Copy link</button>
    </div>
  </div>

  <div class="key-input-wrap">
    <div class="key-input-row">
      <input
        class="key-input"
        type={showKey ? 'text' : 'password'}
        bind:value={apiKeyDraft}
        placeholder="Paste your API key here…"
        spellcheck="false"
        autocomplete="off"
      />
      <button class="show-btn" onclick={() => { showKey = !showKey; }} title={showKey ? 'Hide' : 'Show'}>
        {#if showKey}
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"/><line x1="1" y1="1" x2="23" y2="23"/></svg>
        {:else}
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>
        {/if}
      </button>
    </div>

    {#if keyError}
      <p class="key-error">{keyError}</p>
    {/if}

    {#if keySaved && !apiKeyDraft}
      <div class="key-status">
        {#if keyValidation.status === 'checking'}
          <span class="status-spinner" aria-hidden="true"></span>
          <span>Verifying key…</span>
        {:else if keyValidation.status === 'valid'}
          <span class="status-icon status-ok">✓</span>
          <span>Key verified — {providerName} accepted it.</span>
        {:else if keyValidation.status === 'invalid'}
          <span class="status-icon status-bad">!</span>
          <span>{keyValidation.message || "This key was rejected. You can re-enter it or continue anyway."}</span>
        {:else if keyValidation.status === 'unknown'}
          <span class="status-icon status-warn">?</span>
          <span>{keyValidation.message || "Couldn't verify the key right now — saved anyway."}</span>
        {:else}
          <span class="status-icon status-ok">✓</span>
          <span>Key saved for {providerName}.</span>
        {/if}
      </div>
    {/if}

    {#if !keySaved}
      <div class="key-warning">
        Dictation won't work until a key is added — you can also do this later in Settings → API Keys.
      </div>
    {/if}

    {#if isMac}
      <div class="keychain-note">
        <strong>macOS note:</strong>
        If Keychain asks for your login password, choose <span>Always Allow</span>.
        That keeps your API key stored securely without repeating the prompt.
      </div>
    {/if}
  </div>
</div>

<style>
  .key-guide {
    background: var(--paper-2);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .guide-label {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--ink-faint);
    margin: 0;
  }

  .guide-steps { margin: 0; padding-left: 20px; display: flex; flex-direction: column; gap: 5px; }
  .guide-steps li { font-size: 13px; color: var(--ink-soft); line-height: 1.45; }

  .url-row {
    display: flex;
    align-items: center;
    gap: 10px;
    background: var(--paper);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    padding: 8px 12px;
  }

  .url-display { flex: 1; font-family: var(--mono); font-size: 12px; color: var(--accent-ink); word-break: break-all; }

  .copy-btn {
    background: transparent;
    border: 1px solid var(--line-strong);
    border-radius: 5px;
    padding: 3px 10px;
    font-family: var(--sans);
    font-size: 11.5px;
    color: var(--ink-mute);
    cursor: pointer;
    flex-shrink: 0;
    transition: color 0.15s, border-color 0.15s;
  }

  .copy-btn:hover { color: var(--ink-soft); border-color: var(--accent); }

  .key-input-wrap { display: flex; flex-direction: column; gap: 8px; }

  .key-input-row {
    display: flex;
    align-items: center;
    gap: 0;
    border: 1.5px solid var(--line-strong);
    border-radius: var(--r-sm);
    background: var(--bg-elev);
    overflow: hidden;
    transition: border-color 0.15s;
  }

  .key-input-row:focus-within { border-color: var(--accent); }

  .key-input {
    flex: 1;
    border: none;
    background: transparent;
    font-family: var(--mono);
    font-size: 12.5px;
    color: var(--ink);
    padding: 10px 12px;
    outline: none;
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

  .key-input::placeholder { color: var(--ink-faint); font-family: var(--sans); font-size: 13px; }

  .show-btn {
    background: transparent;
    border: none;
    border-left: 1px solid var(--line);
    padding: 0 12px;
    height: 100%;
    color: var(--ink-faint);
    cursor: pointer;
    display: flex;
    align-items: center;
    transition: color 0.15s;
  }

  .show-btn:hover { color: var(--ink-mute); }

  .key-error { font-size: 12px; color: var(--danger); margin: 0; }

  .key-status {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12.5px;
    color: var(--ink-soft);
  }

  .status-icon {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    font-weight: 700;
    flex-shrink: 0;
  }

  .status-ok { background: var(--accent-soft); color: var(--accent-ink); }
  .status-bad { background: var(--danger-bg); color: var(--danger); }
  .status-warn { background: var(--warning-bg); color: var(--warning); }

  .status-spinner {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid var(--line-strong);
    border-top-color: var(--accent);
    animation: spin 0.7s linear infinite;
    flex-shrink: 0;
  }

  @keyframes spin { to { transform: rotate(360deg); } }

  .key-warning {
    padding: 9px 12px;
    border-radius: var(--r-sm);
    border: 1px solid var(--warning-line);
    background: var(--warning-bg);
    color: var(--ink-soft);
    font-size: 12px;
    line-height: 1.45;
  }

  .keychain-note {
    padding: 11px 12px;
    border-radius: var(--r-sm);
    border: 1px solid color-mix(in srgb, var(--accent) 25%, var(--line));
    background: color-mix(in srgb, var(--accent-soft) 42%, var(--paper-2));
    color: var(--ink-soft);
    font-size: 12.5px;
    line-height: 1.45;
  }

  .keychain-note strong { color: var(--ink-strong); font-weight: 600; }
  .keychain-note span { color: var(--accent-ink); font-weight: 600; }
</style>
