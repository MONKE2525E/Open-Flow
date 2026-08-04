<script lang="ts">
  import type { ProviderId } from '../../settings';
  import { providerGuides } from '../setupData';
  import { isMac } from '../../platform';
  import { fade } from 'svelte/transition';
  import { motionMs } from '../../motion';

  type KeyValidation = { status: 'idle' | 'checking' | 'valid' | 'invalid' | 'unknown'; message: string };

  let {
    provider,
    providerName,
    apiKeyDraft = $bindable(),
    showKey = $bindable(),
    mode = $bindable('fork'),
    keySaved,
    keySaving,
    keyError,
    keyValidation,
  }: {
    provider: ProviderId;
    providerName: string;
    apiKeyDraft: string;
    showKey: boolean;
    /** 'fork' asks whether they have a key; 'tutorial' walks them through making one. */
    mode: 'fork' | 'tutorial' | 'paste';
    keySaved: boolean;
    keySaving: boolean;
    keyError: string;
    keyValidation: KeyValidation;
  } = $props();

  let guide = $derived(providerGuides[provider]);

  // Screenshots are optional. Anything dropped into src/assets/setup/ gets
  // picked up here by filename; a step with no matching file falls back to a
  // placeholder frame, so an empty folder is a valid state.
  // Root-absolute on purpose — a '../../../' glob does not resolve from inside
  // a .svelte module and silently matched nothing.
  const shotModules = import.meta.glob('/src/assets/setup/*.png', {
    eager: true,
    query: '?url',
    import: 'default',
  }) as Record<string, string>;

  const shotsByKey = new Map<string, string>();
  for (const [path, url] of Object.entries(shotModules)) {
    const file = path.split('/').pop() ?? '';
    const match = /^(.+)-(\d+)-/.exec(file);
    if (match) shotsByKey.set(`${match[1]}-${match[2]}`, url);
  }

  let slide = $state(0);
  const slideCount = $derived(guide.steps.length);
  const safeSlide = $derived(Math.min(slide, Math.max(0, slideCount - 1)));
  const currentShot = $derived(shotsByKey.get(`${provider}-${safeSlide + 1}`));

  // Reset the carousel when the provider changes underneath us.
  $effect(() => {
    void provider;
    slide = 0;
  });

  function step(delta: number) {
    slide = (safeSlide + delta + slideCount) % slideCount;
  }

  async function openExternal(url: string) {
    const full = url.startsWith('http') ? url : `https://${url}`;
    try {
      const { open } = await import('@tauri-apps/plugin-shell');
      await open(full);
    } catch {
      window.open(full, '_blank');
    }
  }

</script>

<div class="step apikey-step">
  {#if provider === 'local'}
    <div class="key-guide">
      <p class="group-label">Local setup</p>
      <ol class="guide-steps">
        {#each guide.steps as s}
          <li>{s}</li>
        {/each}
      </ol>
      <div class="local-note">
        You can finish setup without any cloud credential. Local transcription still needs the model download before dictation works.
      </div>
    </div>

  {:else if mode === 'fork'}
    <!-- The old step dropped straight into a password field, which is a dead end
         for anyone who has never made an account. Ask first. -->
    <div class="fork" in:fade={{ duration: motionMs(180) }}>
      <p class="fork-question">Do you already have a {providerName} API key?</p>
      <div class="fork-options">
        <button class="fork-card" onclick={() => { mode = 'paste'; }}>
          <span class="fork-icon" aria-hidden="true">
            <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4"/></svg>
          </span>
          <span class="fork-title">Yes, I have one</span>
          <span class="fork-sub">{keySaved ? 'A key is already saved — paste a new one to replace it' : "Paste it and you're done"}</span>
        </button>
        <button class="fork-card" onclick={() => { mode = 'tutorial'; slide = 0; }}>
          <span class="fork-icon" aria-hidden="true">
            <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>
          </span>
          <span class="fork-title">No, walk me through it</span>
          <span class="fork-sub">Takes about a minute, and it's free</span>
        </button>
      </div>
      <p class="fork-note">Keys are stored in your OS credential manager — never in a file, a log, or our servers.</p>
    </div>

  {:else if mode === 'tutorial'}
    <div class="tutorial" in:fade={{ duration: motionMs(180) }}>
      <div class="shot-frame">
        {#key slide}
          <div class="shot-inner" in:fade={{ duration: motionMs(150) }}>
            {#if currentShot}
              <img class="shot-img" src={currentShot} alt="Step {safeSlide + 1}: {guide.steps[safeSlide]}" />
            {:else}
              <!-- No screenshot for this step yet (see src/assets/setup/README.md).
                   The step number and caption are both already in the row below,
                   so this says only what it needs to. -->
              <div class="shot-placeholder">
                <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="8.5" cy="9.5" r="1.5"/><path d="m21 15-5-5L5 20"/></svg>
                <span class="shot-placeholder-text">Screenshot coming soon</span>
              </div>
            {/if}
          </div>
        {/key}
      </div>

      <div class="shot-caption">
        <button
          class="shot-nav"
          onclick={() => step(-1)}
          disabled={slideCount < 2}
          aria-label="Previous step"
        ><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="15 18 9 12 15 6"/></svg></button>
        <div class="shot-caption-text">
          <span class="shot-step">Step {safeSlide + 1} of {slideCount}</span>
          <span class="shot-text">{guide.steps[safeSlide]}</span>
        </div>
        <button
          class="shot-nav"
          onclick={() => step(1)}
          disabled={slideCount < 2}
          aria-label="Next step"
        ><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="9 18 15 12 9 6"/></svg></button>
      </div>

      <div class="tutorial-actions">
        <button class="btn-open" onclick={() => openExternal(guide.url)}>
          Open {guide.url}
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 17L17 7M9 7h8v8"/></svg>
        </button>
        <button class="btn-ghost btn-got-key" onclick={() => { mode = 'paste'; }}>I've got my key →</button>
      </div>
    </div>

  {:else}
    <div class="key-input-wrap" in:fade={{ duration: motionMs(180) }}>
      <div class="key-input-row">
        <input
          class="key-input"
          type={showKey ? 'text' : 'password'}
          bind:value={apiKeyDraft}
          placeholder="Paste your {providerName} API key here…"
          aria-label="API key"
          spellcheck="false"
          autocomplete="off"
        />
        <button class="show-btn" onclick={() => { showKey = !showKey; }} title={showKey ? 'Hide' : 'Show'} aria-label={showKey ? 'Hide API key' : 'Show API key'}>
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
        <div class="key-status" class:is-bad={keyValidation.status === 'invalid'} class:is-warn={keyValidation.status === 'unknown'}>
          {#if keyValidation.status === 'checking'}
            <span class="status-spinner" aria-hidden="true"></span>
            <span>Verifying key…</span>
          {:else if keyValidation.status === 'invalid'}
            <svg class="status-glyph" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><line x1="12" y1="8" x2="12" y2="13"/><line x1="12" y1="16.5" x2="12.01" y2="16.5"/></svg>
            <span>{keyValidation.message || 'This key was rejected. You can re-enter it or continue anyway.'}</span>
          {:else if keyValidation.status === 'unknown'}
            <svg class="status-glyph" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M9.6 9.4a2.5 2.5 0 0 1 4.86.85c0 1.65-2.46 2.5-2.46 2.5"/><line x1="12" y1="16.5" x2="12.01" y2="16.5"/></svg>
            <span>{keyValidation.message || "Couldn't verify the key right now — saved anyway."}</span>
          {:else if keyValidation.status === 'valid'}
            <svg class="status-glyph" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="20 6 9 17 4 12"/></svg>
            <span>Key verified — {providerName} accepted it.</span>
          {:else}
            <svg class="status-glyph" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="20 6 9 17 4 12"/></svg>
            <span>Key saved for {providerName}.</span>
          {/if}
        </div>
      {/if}

      {#if !keySaved}
        <div class="key-warning">
          Dictation won't work until a key is added — you can also do this later in Settings → API Keys.
        </div>
      {/if}

      <div class="paste-help">
        <button class="help-btn" onclick={() => { mode = 'tutorial'; }}>
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>
          Show me where to find it
        </button>
        <button class="help-btn" onclick={() => openExternal(guide.url)}>
          Open {guide.url}
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 17L17 7M9 7h8v8"/></svg>
        </button>
      </div>

      {#if isMac}
        <div class="keychain-note">
          <strong>macOS note:</strong>
          If Keychain asks for your login password, choose <span>Always Allow</span>.
          That keeps your API key stored securely without repeating the prompt.
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .apikey-step { gap: 16px; }

  /* ── Fork ─────────────────────────────────────────────────────────── */
  .fork { display: flex; flex-direction: column; gap: 14px; }

  .fork-question {
    margin: 0;
    font-size: 14px;
    color: var(--ink-soft);
    text-align: center;
  }

  .fork-options { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }

  .fork-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: 7px;
    min-height: 148px;
    background: var(--bg-elev);
    border: 1.5px solid var(--line);
    border-radius: var(--r-md);
    padding: 26px 22px;
    cursor: pointer;
    font-family: var(--sans);
    transition: border-color 0.16s ease, background 0.16s ease, transform 0.12s ease;
  }

  .fork-card:hover { border-color: var(--accent); background: var(--paper-2); }
  .fork-card:active { transform: scale(0.985); }
  .fork-card:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }

  .fork-icon { color: var(--ink-faint); margin-bottom: 4px; transition: color 0.16s ease; }
  .fork-card:hover .fork-icon { color: var(--accent-ink); }

  .fork-title { font-size: 15px; font-weight: 500; color: var(--ink-strong); }
  .fork-sub { font-size: 12px; color: var(--ink-mute); line-height: 1.4; }

  .fork-note { margin: 0; font-size: 11.5px; color: var(--ink-faint); text-align: center; line-height: 1.5; }

  /* ── Tutorial carousel ────────────────────────────────────────────── */
  .tutorial { display: flex; flex-direction: column; gap: 10px; }

  .shot-frame {
    position: relative;
    /* Height drives width. `width:100% + max-height` let aspect-ratio compute a
       316px box that max-height then clipped, cutting the bottom off every shot. */
    height: 260px;
    aspect-ratio: 16 / 9;
    max-width: 100%;
    margin: 0 auto;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--paper-2);
    overflow: hidden;
  }

  /* Absolute, not grid-stacked: an auto-sized grid row took its height from the
     image's own aspect ratio and overflowed the max-height'd frame by ~56px. */
  .shot-inner { position: absolute; inset: 0; display: grid; place-items: center; }

  .shot-img { width: 100%; height: 100%; object-fit: contain; display: block; }

  .shot-placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 20px 28px;
    text-align: center;
    color: var(--ink-faint);
  }

  .shot-placeholder-text { font-size: 12.5px; color: var(--ink-faint); line-height: 1.5; }

  .shot-caption {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .shot-nav {
    width: 28px;
    height: 28px;
    flex-shrink: 0;
    border-radius: 50%;
    border: 1px solid var(--line-strong);
    background: transparent;
    color: var(--ink-mute);
    /* SVG chevrons, not "‹"/"›" — the text glyphs have asymmetric side bearings
       that no amount of centring fixes, so each arrow sat off-centre its own way. */
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    cursor: pointer;
    transition: color 0.15s, border-color 0.15s;
  }

  .shot-nav:hover:not(:disabled) { color: var(--ink-strong); border-color: var(--accent); }
  .shot-nav:disabled { opacity: 0.35; cursor: not-allowed; }

  .shot-caption-text { flex: 1; display: flex; flex-direction: column; gap: 2px; text-align: center; min-width: 0; }

  .shot-step {
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--ink-faint);
  }

  .shot-text { font-size: 13px; color: var(--ink-soft); line-height: 1.4; }

  .tutorial-actions { display: flex; align-items: center; gap: 10px; }

  .btn-open {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    background: var(--accent-soft);
    border: 1px solid color-mix(in srgb, var(--accent) 40%, transparent);
    border-radius: var(--r-sm);
    color: var(--accent-ink);
    font-family: var(--sans);
    font-size: 12.5px;
    font-weight: 500;
    padding: 9px 14px;
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
  }

  .btn-open:hover { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 18%, var(--paper-2)); }

  .btn-got-key {
    border-radius: var(--r-sm);
    padding: 9px 14px;
    font-family: var(--sans);
    font-size: 12.5px;
    flex-shrink: 0;
  }

  /* ── Paste ────────────────────────────────────────────────────────── */
  .key-guide {
    background: var(--paper-2);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .guide-steps { margin: 0; padding-left: 20px; display: flex; flex-direction: column; gap: 5px; }
  .guide-steps li { font-size: 13px; color: var(--ink-soft); line-height: 1.45; }

  .local-note {
    padding: 10px 12px;
    border-radius: var(--r-sm);
    border: 1px solid color-mix(in srgb, var(--accent) 25%, var(--line));
    background: color-mix(in srgb, var(--accent-soft) 42%, var(--paper-2));
    color: var(--ink-soft);
    font-size: 12.5px;
    line-height: 1.45;
  }

  .key-input-wrap { display: flex; flex-direction: column; gap: 9px; }

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
    padding: 11px 13px;
    outline: none;
  }

  .key-input::-ms-reveal { display: none; }
  .key-input::-ms-clear { display: none; }
  .key-input::-webkit-credentials-auto-fill-button { display: none; }
  .key-input::-webkit-contacts-auto-fill-button { display: none; }

  .key-input::placeholder { color: var(--ink-faint); font-family: var(--sans); font-size: 13px; }

  .show-btn {
    background: transparent;
    border: none;
    border-left: 1px solid var(--line);
    padding: 0 12px;
    align-self: stretch;
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

  /* An outlined glyph in the flow's accent, matching the pick-radio treatment —
     the old solid disc with a text "✓" was the only filled badge in the wizard. */
  .status-glyph { color: var(--accent); flex-shrink: 0; }
  .key-status.is-bad .status-glyph { color: var(--danger); }
  .key-status.is-warn .status-glyph { color: var(--warning); }

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

  /* These were 11.5px underlined links and read as fine print — the "where do I
     get one?" escape hatch is the most-needed control on this screen. */
  .paste-help { display: flex; align-items: stretch; gap: 8px; }

  .help-btn {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    padding: 10px 14px;
    background: var(--bg-elev);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-sm);
    font-family: var(--sans);
    font-size: 13px;
    font-weight: 500;
    color: var(--ink-soft);
    cursor: pointer;
    transition: border-color 0.15s ease, color 0.15s ease, background 0.15s ease;
  }

  .help-btn:hover { border-color: var(--accent); color: var(--accent-ink); background: var(--paper-2); }
  .help-btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }

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

  @media (prefers-reduced-motion: reduce) {
    .fork-card { transition: none; }
    .status-spinner { animation-duration: 1.4s; }
  }
</style>
