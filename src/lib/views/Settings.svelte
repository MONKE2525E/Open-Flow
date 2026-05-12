<script lang="ts">
  import { settingsOpen } from '../stores';
  import { tick, onMount } from 'svelte';

  let section = 'general';
  let prevSection: string | null = null;
  let animDir: 'up' | 'down' | null = null;
  let isAnimating = false;
  let modalReady = false;

  // API key status — true means a key is saved; never expose the value
  let keyStatus = { groq: false, openai: false, google: false };

  // Draft key inputs (only held in memory while settings is open, never read back from store)
  let draftKeys = { groq: '', openai: '', google: '' };

  // Microphone state
  let microphones: string[] = [];
  let selectedMic = '';
  let micDropdownOpen = false;

  // Model selection state
  let transcriptionModel = 'groq/whisper-large-v3-turbo';
  let cleanupModel = 'groq/llama-3.3-70b-versatile';

  // Toggle states
  let toggleState = { cleanup: true, autoLearn: true, crashReports: false };

  const sectionOrder = ['general','keys','models','privacy','advanced','about'];

  $: if ($settingsOpen) {
    tick().then(() => tick()).then(() => { modalReady = true; });
    loadSettings();
  } else {
    modalReady = false;
    draftKeys = { groq: '', openai: '', google: '' };
    micDropdownOpen = false;
  }

  async function loadSettings() {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      keyStatus = await invoke('get_api_key_status');
      microphones = await invoke<string[]>('get_microphones');

      const tModel = await invoke<string | null>('get_setting', { key: 'transcription_model' });
      if (tModel) transcriptionModel = tModel;

      const cModel = await invoke<string | null>('get_setting', { key: 'cleanup_model' });
      if (cModel) cleanupModel = cModel;

      const cleanupEnabled = await invoke<boolean | null>('get_setting', { key: 'cleanup_enabled' });
      if (cleanupEnabled !== null && cleanupEnabled !== undefined) {
        toggleState = { ...toggleState, cleanup: cleanupEnabled };
      }

      const mic = await invoke<string | null>('get_setting', { key: 'microphone_device' });
      selectedMic = mic ?? '';
    } catch {
      // dev mode without Tauri — best-effort
    }
  }

  async function saveKey(provider: 'groq' | 'openai' | 'google') {
    const key = draftKeys[provider].trim();
    if (!key) return;
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('save_api_key', { provider, key });
      keyStatus = { ...keyStatus, [provider]: true };
      draftKeys = { ...draftKeys, [provider]: '' };
    } catch (e) {
      console.error('save_api_key failed', e);
    }
  }

  async function setTranscriptionModel(id: string) {
    transcriptionModel = id;
    const provider = id.split('/')[0];
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('save_setting', { key: 'transcription_model', value: id });
      await invoke('save_setting', { key: 'transcription_provider', value: provider });
    } catch {}
  }

  async function setCleanupModel(id: string) {
    cleanupModel = id;
    const provider = id.split('/')[0];
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('save_setting', { key: 'cleanup_model', value: id });
      await invoke('save_setting', { key: 'cleanup_provider', value: provider });
    } catch {}
  }

  async function toggleCleanup() {
    toggleState = { ...toggleState, cleanup: !toggleState.cleanup };
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('save_setting', { key: 'cleanup_enabled', value: toggleState.cleanup });
    } catch {}
  }

  async function saveMic(name: string) {
    selectedMic = name;
    micDropdownOpen = false;
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('save_setting', { key: 'microphone_device', value: name || null });
    } catch {}
  }

  function closeMicDropdown(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (!target.closest('.mic-dropdown')) micDropdownOpen = false;
  }

  $: if (micDropdownOpen) {
    tick().then(() => window.addEventListener('click', closeMicDropdown, { once: true }));
  }

  async function openRepo() {
    try {
      const { open } = await import('@tauri-apps/plugin-shell');
      await open('https://github.com/MONKE2525E/Open-Flow');
    } catch {
      window.open('https://github.com/MONKE2525E/Open-Flow', '_blank');
    }
  }

  function close() { $settingsOpen = false; }

  function goTo(id: string) {
    if (id === section || isAnimating) return;
    const oldIdx = sectionOrder.indexOf(section);
    const newIdx = sectionOrder.indexOf(id);
    animDir = newIdx > oldIdx ? 'up' : 'down';
    prevSection = section;
    isAnimating = true;
    section = id;
    setTimeout(() => {
      prevSection = null;
      isAnimating = false;
      animDir = null;
    }, 280);
  }

  const navSections = [
    { group: 'Settings', items: [
      { id: 'general',  label: 'General',  paths: `<circle cx="12" cy="12" r="3"/><path d="M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6"/>` },
      { id: 'keys',     label: 'API Keys', paths: `<circle cx="7.5" cy="15.5" r="3.5"/><path d="m21 2-9.6 9.6M15 6l3 3"/>` },
      { id: 'models',   label: 'Models',   paths: `<path d="M18 3a3 3 0 0 0-3 3v12a3 3 0 0 0 3 3 3 3 0 0 0 3-3 3 3 0 0 0-3-3H6a3 3 0 0 0-3 3 3 3 0 0 0 3 3 3 3 0 0 0 3-3V6a3 3 0 0 0-3-3 3 3 0 0 0-3 3 3 3 0 0 0 3 3h12a3 3 0 0 0 3-3 3 3 0 0 0-3-3z"/>` },
      { id: 'privacy',  label: 'Privacy',  paths: `<rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>` },
      { id: 'advanced', label: 'Advanced', paths: `<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.6 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>` },
    ]},
    { group: 'Account', items: [
      { id: 'about', label: 'About', paths: `<circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3M12 17h.01"/>` },
    ]},
  ];

  const keyProviders: { id: 'groq' | 'openai' | 'google'; label: string; ph: string; models: string }[] = [
    { id: 'groq',   label: 'Groq',   ph: 'gsk_…',  models: 'whisper-large-v3-turbo · llama-3.3-70b' },
    { id: 'openai', label: 'OpenAI', ph: 'sk-…',   models: 'gpt-4o-transcribe · gpt-4o-mini' },
    { id: 'google', label: 'Google', ph: 'AIza…',  models: 'Chirp 3 · gemini-2.5-flash' },
  ];

  const transcriptionModels = [
    { id: 'groq/whisper-large-v3-turbo', provider: 'Groq',   name: 'whisper-large-v3-turbo', note: '~0.5s · free tier · recommended', recommended: true },
    { id: 'openai/gpt-4o-transcribe',    provider: 'OpenAI', name: 'gpt-4o-transcribe',       note: '~1s · best accuracy for accents & noise', recommended: false },
    { id: 'google/gemini-2.5-flash',     provider: 'Google', name: 'gemini-2.5-flash',        note: '~3s · slow for transcription, not recommended', recommended: false },
  ];

  const cleanupModels = [
    { id: 'groq/llama-3.3-70b-versatile', provider: 'Groq',   name: 'llama-3.3-70b-versatile', note: '~0.3s · free tier · recommended', recommended: true },
    { id: 'openai/gpt-4o-mini',           provider: 'OpenAI', name: 'gpt-4o-mini',              note: '~0.5s · best cost/quality balance', recommended: false },
    { id: 'google/gemini-2.5-flash',      provider: 'Google', name: 'gemini-2.5-flash',          note: '~1s · fused with transcription when both Google', recommended: false },
  ];

  function micLabel(name: string) {
    // Trim Windows-style long device names for display
    return name.length > 32 ? name.slice(0, 32) + '…' : name;
  }
</script>

{#if $settingsOpen}
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
  <div
    class="settings-overlay"
    style:opacity={modalReady ? 1 : 0}
    onclick={close}
  >
    <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
    <div
      class="settings-modal"
      style:transform={modalReady ? 'scale(1) translateY(0)' : 'scale(0.94) translateY(12px)'}
      style:opacity={modalReady ? 1 : 0}
      onclick={(e) => e.stopPropagation()}
    >
      <!-- Left nav -->
      <div class="settings-nav">
        {#each navSections as g}
          <div class="settings-section-label">{g.group}</div>
          {#each g.items as it}
            <div
              class="settings-nav-item"
              class:active={section === it.id}
              role="button"
              tabindex="0"
              onclick={() => goTo(it.id)}
              onkeydown={(e) => e.key === 'Enter' && goTo(it.id)}
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">{@html it.paths}</svg>
              <span>{it.label}</span>
            </div>
          {/each}
        {/each}
        <div style="flex:1"></div>
        <div class="settings-foot">Open Flow v0.4.2 · MIT</div>
      </div>

      <!-- Right panel -->
      <div class="settings-body">
        <div
          class="panel"
          style:animation={isAnimating ? `panelEnter${animDir === 'up' ? 'Up' : 'Down'} 0.28s cubic-bezier(0.22,1,0.36,1) both` : 'none'}
        >
          {#if section === 'general'}
            <h2 class="settings-h">General</h2>
            <div class="setting-row">
              <div><div class="label">Hotkey</div><div class="desc">Hold to record, release to transcribe</div></div>
              <kbd class="badge key-badge">Alt Space</kbd>
            </div>
            <div class="setting-row">
              <div><div class="label">Microphone</div><div class="desc">Input device for capture</div></div>
              <div class="mic-dropdown">
                <button class="btn-ghost mic-btn" onclick={() => (micDropdownOpen = !micDropdownOpen)}>
                  <span>{selectedMic ? micLabel(selectedMic) : 'Default Device'}</span>
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                    <path d="m6 9 6 6 6-6"/>
                  </svg>
                </button>
                {#if micDropdownOpen}
                  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
                  <div class="mic-menu" onclick={(e) => e.stopPropagation()}>
                    <button class="mic-item" class:active={!selectedMic} onclick={() => saveMic('')}>
                      Default Device
                    </button>
                    {#each microphones as m}
                      <button class="mic-item" class:active={selectedMic === m} onclick={() => saveMic(m)}>
                        {micLabel(m)}
                      </button>
                    {/each}
                    {#if microphones.length === 0}
                      <div class="mic-empty">No devices found</div>
                    {/if}
                  </div>
                {/if}
              </div>
            </div>
            <div class="setting-row">
              <div><div class="label">Auto-cleanup</div><div class="desc">Run LLM cleanup on every transcription</div></div>
              <div class="toggle" class:on={toggleState.cleanup} role="switch" aria-checked={toggleState.cleanup} tabindex="0"
                onclick={toggleCleanup}
                onkeydown={(e) => e.key === 'Enter' && toggleCleanup()}
              ></div>
            </div>

          {:else if section === 'keys'}
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

          {:else if section === 'models'}
            <h2 class="settings-h">Models</h2>

            <div class="model-section-label">Transcription</div>
            <div class="model-desc">Converts audio to raw text</div>
            <div class="model-list">
              {#each transcriptionModels as m}
                <button
                  class="model-row"
                  class:active={transcriptionModel === m.id}
                  onclick={() => setTranscriptionModel(m.id)}
                >
                  <div class="model-radio"></div>
                  <div class="model-info">
                    <div class="model-name">
                      <span class="model-provider">{m.provider}</span> {m.name}
                      {#if m.recommended}<span class="model-badge">recommended</span>{/if}
                    </div>
                    <div class="model-note">{m.note}</div>
                  </div>
                </button>
              {/each}
            </div>

            <div class="model-section-label" style="margin-top:20px">Cleanup LLM</div>
            <div class="model-desc">Rewrites and formats each transcription</div>
            <div class="model-list">
              {#each cleanupModels as m}
                <button
                  class="model-row"
                  class:active={cleanupModel === m.id}
                  onclick={() => setCleanupModel(m.id)}
                >
                  <div class="model-radio"></div>
                  <div class="model-info">
                    <div class="model-name">
                      <span class="model-provider">{m.provider}</span> {m.name}
                      {#if m.recommended}<span class="model-badge">recommended</span>{/if}
                    </div>
                    <div class="model-note">{m.note}</div>
                  </div>
                </button>
              {/each}
            </div>

          {:else if section === 'privacy'}
            <h2 class="settings-h">Privacy</h2>
            <div class="setting-row">
              <div><div class="label">Auto-learn corrections</div><div class="desc">Add confirmed corrections to dictionary</div></div>
              <div class="toggle" class:on={toggleState.autoLearn} role="switch" aria-checked={toggleState.autoLearn} tabindex="0"
                onclick={() => (toggleState = { ...toggleState, autoLearn: !toggleState.autoLearn })}
                onkeydown={(e) => e.key === 'Enter' && (toggleState = { ...toggleState, autoLearn: !toggleState.autoLearn })}
              ></div>
            </div>
            <div class="setting-row">
              <div><div class="label">Crash reports</div><div class="desc">Send anonymised errors to improve Open Flow</div></div>
              <div class="toggle" class:on={toggleState.crashReports} role="switch" aria-checked={toggleState.crashReports} tabindex="0"
                onclick={() => (toggleState = { ...toggleState, crashReports: !toggleState.crashReports })}
                onkeydown={(e) => e.key === 'Enter' && (toggleState = { ...toggleState, crashReports: !toggleState.crashReports })}
              ></div>
            </div>

          {:else if section === 'advanced'}
            <h2 class="settings-h">Advanced</h2>
            <div class="setting-row">
              <div><div class="label">Transcription history</div><div class="desc">How long to keep past dictations</div></div>
              <div class="badge">30 days</div>
            </div>
            <div class="setting-row">
              <div><div class="label">Injection method</div><div class="desc">How text is inserted into apps</div></div>
              <div class="badge">Clipboard (Ctrl+V)</div>
            </div>

          {:else if section === 'about'}
            <h2 class="settings-h">About</h2>
            <div class="setting-row">
              <div><div class="label">Version</div></div>
              <span class="desc">0.4.2</span>
            </div>
            <div class="setting-row">
              <div><div class="label">License</div></div>
              <span class="desc">MIT</span>
            </div>
            <div class="setting-row">
              <div><div class="label">Source</div></div>
              <button class="btn-ghost" onclick={openRepo}>github.com/MONKE2525E/Open-Flow</button>
            </div>
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .settings-overlay {
    position: absolute;
    inset: 0;
    background: rgba(13,10,8,0.30);
    display: grid;
    place-items: center;
    z-index: 5;
    transition: opacity 0.2s;
  }

  .settings-modal {
    width: 720px;
    height: 540px;
    background: var(--bg-elev);
    border-radius: var(--r-lg);
    border: 1px solid var(--line);
    box-shadow: 0 24px 60px rgba(13,10,8,0.18);
    display: flex;
    overflow: hidden;
    transition: transform 0.25s cubic-bezier(0.22,1,0.36,1), opacity 0.2s;
    transform-origin: bottom right;
  }

  /* Nav */
  .settings-nav {
    width: 200px;
    background: var(--paper);
    border-right: 1px solid var(--line);
    padding: 14px 10px;
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
  }

  .settings-section-label {
    font-family: var(--mono);
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--ink-mute);
    padding: 8px 10px 6px;
  }

  .settings-nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    border-radius: 6px;
    font-size: 12.5px;
    color: var(--ink-soft);
    cursor: pointer;
  }

  .settings-nav-item :global(svg) { opacity: 0.7; }

  .settings-nav-item:hover { color: var(--ink-strong); }

  .settings-nav-item.active {
    color: var(--ink);
    font-weight: 500;
    background: var(--bg-elev);
  }

  .settings-nav-item.active :global(svg) { opacity: 1; }

  .settings-foot {
    padding: 8px 10px;
    font-family: var(--mono);
    font-size: 9.5px;
    color: var(--ink-mute);
  }

  /* Panel area */
  .settings-body {
    flex: 1;
    position: relative;
    overflow: hidden;
  }

  .panel {
    position: absolute;
    inset: 0;
    padding: 26px 30px;
    overflow-y: auto;
  }

  @keyframes panelEnterUp {
    from { transform: translateY(28px); opacity: 0; }
    to   { transform: translateY(0);    opacity: 1; }
  }

  @keyframes panelEnterDown {
    from { transform: translateY(-28px); opacity: 0; }
    to   { transform: translateY(0);     opacity: 1; }
  }

  .settings-h {
    font-family: var(--serif);
    font-size: 19px;
    font-weight: 500;
    margin: 0 0 14px;
    letter-spacing: -0.015em;
    color: var(--ink);
  }

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 13px 0;
    border-top: 1px solid var(--line);
  }

  .setting-row:last-of-type { border-bottom: 1px solid var(--line); }

  .label { font-size: 13px; font-weight: 500; color: var(--ink-strong); }
  .desc  { font-size: 12px; color: var(--ink-mute); margin-top: 3px; }

  .btn-ghost {
    background: transparent;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    padding: 5px 12px;
    font-size: 12px;
    color: var(--ink-strong);
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
  }

  .btn-ghost:hover { background: var(--paper); }

  .badge {
    background: transparent;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    padding: 5px 12px;
    font-size: 12px;
    color: var(--ink-mute);
    font-weight: 500;
    user-select: none;
    cursor: default;
    white-space: nowrap;
  }

  .key-badge {
    font-family: var(--mono);
    font-size: 11px;
    letter-spacing: 0.04em;
    pointer-events: none;
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

  input.key-input { font-family: var(--mono); }

  .panel-note {
    font-size: 12px;
    color: var(--ink-mute);
    margin: 0 0 16px;
    line-height: 1.5;
  }

  .key-row { align-items: flex-start; gap: 12px; }

  .key-left { flex: 1; min-width: 0; }

  .key-right {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-shrink: 0;
  }

  .key-saved {
    font-family: var(--mono);
    font-size: 10px;
    color: #5a8a52;
    font-weight: 400;
    margin-left: 6px;
    letter-spacing: 0.02em;
  }

  .btn-ghost:disabled {
    opacity: 0.4;
    cursor: default;
  }

  /* Toggle */
  .toggle {
    width: 30px;
    height: 16px;
    background: var(--jap-300);
    border-radius: 999px;
    position: relative;
    cursor: pointer;
    transition: background 0.15s;
    flex-shrink: 0;
  }

  .toggle::after {
    content: '';
    position: absolute;
    width: 12px;
    height: 12px;
    background: white;
    border-radius: 50%;
    top: 2px;
    left: 2px;
    transition: left 0.15s;
    box-shadow: 0 1px 2px rgba(13,10,8,0.15);
  }

  .toggle.on { background: var(--jap-400); }
  .toggle.on::after { left: 16px; }

  /* Microphone dropdown */
  .mic-dropdown {
    position: relative;
    flex-shrink: 0;
  }

  .mic-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    max-width: 180px;
  }

  .mic-btn span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 140px;
  }

  .mic-menu {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    box-shadow: 0 8px 24px rgba(13,10,8,0.14);
    min-width: 200px;
    max-width: 280px;
    max-height: 200px;
    overflow-y: auto;
    z-index: 10;
  }

  .mic-item {
    display: block;
    width: 100%;
    text-align: left;
    padding: 8px 12px;
    font-size: 12px;
    font-family: var(--sans);
    color: var(--ink-strong);
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--line);
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .mic-item:last-child { border-bottom: none; }
  .mic-item:hover { background: var(--paper); }
  .mic-item.active {
    background: var(--accent-soft);
    color: var(--ink);
    font-weight: 500;
  }

  .mic-empty {
    padding: 10px 12px;
    font-size: 12px;
    color: var(--ink-mute);
    font-style: italic;
  }

  /* Models */
  .model-section-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--ink-strong);
    margin-bottom: 2px;
  }

  .model-desc {
    font-size: 12px;
    color: var(--ink-mute);
    margin-bottom: 10px;
  }

  .model-list {
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    overflow: hidden;
  }

  .model-row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    border: none;
    border-bottom: 1px solid var(--line);
    background: var(--bg-elev);
    cursor: pointer;
    text-align: left;
    transition: background 0.12s;
    font-family: var(--sans);
  }

  .model-row:last-child { border-bottom: none; }
  .model-row:hover { background: var(--paper); }
  .model-row.active { background: var(--accent-soft); }

  .model-radio {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 1.5px solid var(--line-strong);
    flex-shrink: 0;
    position: relative;
    transition: border-color 0.12s;
  }

  .model-row.active .model-radio {
    border-color: var(--accent);
  }

  .model-row.active .model-radio::after {
    content: '';
    position: absolute;
    inset: 2px;
    background: var(--accent);
    border-radius: 50%;
  }

  .model-name {
    font-size: 12.5px;
    font-weight: 500;
    color: var(--ink-strong);
    line-height: 1.3;
  }

  .model-provider {
    color: var(--ink-mute);
    font-weight: 400;
  }

  .model-info { flex: 1; min-width: 0; }

  .model-note {
    font-size: 11px;
    color: var(--ink-mute);
    font-family: var(--mono);
    margin-top: 1px;
  }

  .model-badge {
    display: inline-block;
    font-size: 9.5px;
    font-weight: 500;
    font-family: var(--mono);
    letter-spacing: 0.04em;
    color: #5a8a52;
    background: rgba(90,138,82,0.1);
    border: 1px solid rgba(90,138,82,0.25);
    border-radius: 4px;
    padding: 1px 5px;
    margin-left: 6px;
    vertical-align: middle;
    text-transform: uppercase;
  }
</style>
