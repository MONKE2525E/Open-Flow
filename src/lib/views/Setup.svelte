<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { setupComplete } from '../stores';
  import { saveSetting, type CleanupIntensity, type ToneId } from '../settings';

  let win: { minimize: () => Promise<void> } | null = null;
  onMount(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      win = getCurrentWindow();
    } catch {}
    setTimeout(() => { introReady = true; }, 60);
  });

  function minimize() { win?.minimize(); }
  async function closeWindow() {
    try { await invoke('hide_main'); } catch {}
  }

  // ── Step state ──────────────────────────────────────────────────────────────
  let step = 0;
  const TOTAL_STEPS = 7; // steps 1–7 show progress dots (0 = intro)

  let direction: 'forward' | 'back' = 'forward';
  let animating = false;
  let visible = true;

  // ── Quick Settings (step 6) ───────────────────────────────────────────────
  let quickPrefs = { cleanup: true, noise: true, caps: true, autoLearn: false, autostart: false, muteAudio: false, apiFallback: false };
  let quickSettingsReady = false;

  // ── Provider ─────────────────────────────────────────────────────────────────
  let selectedProvider: 'groq' | 'openai' | 'google' = 'groq';

  const providers = [
    {
      id: 'groq' as const,
      name: 'Groq',
      tagline: 'Free tier · Fastest',
      badge: 'Recommended',
      desc: 'Free API with very generous limits. LPU inference — the fastest option.',
      icon: `<svg width="28" height="28" viewBox="0 0 100 100" fill="none"><circle cx="50" cy="50" r="46" stroke="currentColor" stroke-width="8"/><path d="M32 50a18 18 0 1 1 36 0v8H50" stroke="currentColor" stroke-width="8" stroke-linecap="round"/></svg>`,
    },
    {
      id: 'openai' as const,
      name: 'OpenAI',
      tagline: 'GPT-4o · High quality',
      badge: '',
      desc: 'Uses gpt-4o-transcribe and gpt-4o-mini. Best cleanup quality.',
      icon: `<svg width="28" height="28" viewBox="0 0 100 100" fill="none"><path d="M50 10 L90 32.5 L90 67.5 L50 90 L10 67.5 L10 32.5Z" stroke="currentColor" stroke-width="7" stroke-linejoin="round"/></svg>`,
    },
    {
      id: 'google' as const,
      name: 'Google',
      tagline: 'Gemini 2.5 Flash',
      badge: '',
      desc: 'Uses Gemini 2.5 Flash for both transcription and cleanup.',
      icon: `<svg width="28" height="28" viewBox="0 0 100 100" fill="none"><circle cx="50" cy="50" r="20" stroke="currentColor" stroke-width="7"/><path d="M70 50h20M50 30V10M30 50H10M50 70v20" stroke="currentColor" stroke-width="7" stroke-linecap="round"/></svg>`,
    },
  ];

  const providerGuides: Record<string, { url: string; steps: string[] }> = {
    groq: {
      url: 'console.groq.com/keys',
      steps: ['Go to console.groq.com/keys', 'Sign in or create a free account', 'Click "Create API Key"', 'Copy and paste it below'],
    },
    openai: {
      url: 'platform.openai.com/api-keys',
      steps: ['Go to platform.openai.com/api-keys', 'Sign in to your OpenAI account', 'Click "Create new secret key"', 'Copy and paste it below'],
    },
    google: {
      url: 'aistudio.google.com/app/apikey',
      steps: ['Go to aistudio.google.com', 'Sign in with your Google account', 'Click "Get API key" → "Create API key"', 'Copy and paste it below'],
    },
  };

  // ── API key ───────────────────────────────────────────────────────────────────
  let apiKeyDraft = '';
  let keySaved = false;
  let keySaving = false;
  let keyError = '';
  let showKey = false;

  // ── Cleanup intensity ─────────────────────────────────────────────────────────
  let selectedIntensity = 'medium';
  const cleanupCards = [
    { id: 'none',   name: 'Verbatim', desc: 'Raw transcription. No AI cleanup at all.' },
    { id: 'light',  name: 'Light',    desc: 'Removes filler words and repeated phrases. Keeps everything else.' },
    { id: 'medium', name: 'Medium',   desc: 'Removes fillers, cuts repetition, tightens phrasing. Keeps your detail.' },
    { id: 'high',   name: 'Direct',   desc: 'Aggressive rewrite. Punchy and concise — about half the words.' },
  ];

  // ── Personal tone ─────────────────────────────────────────────────────────────
  let selectedTone = 'casual';
  const toneCards = [
    { id: 'casual',      name: 'Casual',      desc: 'Conversational. Light caps and punctuation — reads like a Slack message.' },
    { id: 'formal',      name: 'Formal',      desc: 'Professional prose. Full punctuation, expanded contractions, formal vocabulary. No em dashes.' },
    { id: 'very_casual', name: 'Very Casual', desc: 'All lowercase, almost no punctuation. Like a quick text typed without thinking.' },
  ];

  // ── App mappings ──────────────────────────────────────────────────────────────
  interface InstalledApp { name: string; exe: string; }
  interface AppMapping { exe: string; profile: string; name: string; }

  let installedApps: InstalledApp[] = [];
  let mappings: AppMapping[] = [];
  let appSearch = '';
  let appsLoaded = false;
  let openDropdownExe = '';   // which app's profile dropdown is open

  const profileOptions = [
    { id: 'casual',      label: 'Casual'      },
    { id: 'formal',      label: 'Formal'      },
    { id: 'very_casual', label: 'Very Casual' },
  ];

  function toggleProfileDropdown(exe: string, e: MouseEvent) {
    e.stopPropagation();
    openDropdownExe = openDropdownExe === exe ? '' : exe;
  }

  function pickProfile(exe: string, profile: string) {
    updateMappingProfile(exe, profile);
    openDropdownExe = '';
  }

  function closeDropdowns() { openDropdownExe = ''; }

  $: filteredApps = appSearch
    ? installedApps.filter(a =>
        a.name.toLowerCase().includes(appSearch.toLowerCase()) ||
        a.exe.toLowerCase().includes(appSearch.toLowerCase())
      ).slice(0, 50)
    : installedApps.slice(0, 50);

  $: mappingExes = new Set(mappings.map(m => m.exe));

  function toggleMapping(app: InstalledApp) {
    if (mappingExes.has(app.exe)) {
      mappings = mappings.filter(m => m.exe !== app.exe);
    } else {
      mappings = [...mappings, { exe: app.exe, profile: 'casual', name: app.name }];
    }
  }

  function updateMappingProfile(exe: string, profile: string) {
    mappings = mappings.map(m => m.exe === exe ? { ...m, profile } : m);
  }

  async function loadInstalledApps() {
    if (appsLoaded) return;
    try {
      installedApps = await invoke<InstalledApp[]>('get_installed_apps');
      appsLoaded = true;
    } catch {}
  }

  $: if (step === 5) loadInstalledApps();

  // ── Navigation ────────────────────────────────────────────────────────────────
  async function goNext() {
    if (animating) return;
    if (step === 7) { await finish(); return; }
    direction = 'forward';
    animating = true;
    visible = false;
    await delay(220);
    step++;
    if (step === 6) setTimeout(() => { quickSettingsReady = true; }, 60);
    visible = true;
    await delay(220);
    animating = false;
  }

  async function goBack() {
    if (animating || step === 0) return;
    if (step === 6 || step === 7) quickSettingsReady = false;
    direction = 'back';
    animating = true;
    visible = false;
    await delay(220);
    step--;
    if (step === 6) setTimeout(() => { quickSettingsReady = true; }, 60);
    visible = true;
    await delay(220);
    animating = false;
  }

  async function skip() {
    direction = 'forward';
    animating = true;
    visible = false;
    await delay(220);
    step++;
    visible = true;
    await delay(220);
    animating = false;
  }

  async function saveKeyAndNext() {
    const trimmed = apiKeyDraft.trim();
    if (!trimmed) { goNext(); return; }
    keySaving = true;
    keyError = '';
    try {
      await invoke('save_api_key', { provider: selectedProvider, key: trimmed });
      keySaved = true;
      apiKeyDraft = '';
    } catch (e) {
      keyError = 'Could not save the key. Check your connection and try again.';
      keySaving = false;
      return;
    }
    keySaving = false;
    goNext();
  }

  async function finish() {
    try {
      await saveSetting('cleanup_intensity', selectedIntensity as CleanupIntensity);
      await saveSetting('default_tone', selectedTone as ToneId);
      await saveSetting('transcription_provider', selectedProvider);
      await saveSetting('cleanup_provider', selectedProvider);
      if (mappings.length > 0) {
        await invoke('save_app_mappings', { mappings });
      }
      await saveSetting('cleanup_enabled', quickPrefs.cleanup);
      await saveSetting('noise_reduction', quickPrefs.noise);
      await saveSetting('contextual_caps_enabled', quickPrefs.caps);
      await saveSetting('auto_learn_enabled', quickPrefs.autoLearn);
      await saveSetting('mute_audio', quickPrefs.muteAudio);
      await saveSetting('api_fallback_enabled', quickPrefs.apiFallback);
      if (quickPrefs.autostart) await invoke('set_autostart', { enabled: true });
      await saveSetting('setup_complete', true);
    } catch {}
    setupComplete.set(true);
  }

  function copyUrl(url: string) {
    navigator.clipboard.writeText('https://' + url).catch(() => {});
  }

  function delay(ms: number) { return new Promise(r => setTimeout(r, ms)); }

  // ── Intro animation ───────────────────────────────────────────────────────────
  let introReady = false;

  // ── Done animation ────────────────────────────────────────────────────────────
  let checkAnimating = false;
  $: if (step === 7) { setTimeout(() => { checkAnimating = true; }, 200); }
</script>

<svelte:window onclick={closeDropdowns} />

<!-- Full-screen overlay -->
<div class="setup-overlay">
  <!-- Draggable title bar -->
  <div class="setup-titlebar" data-tauri-drag-region>
    <div></div>
    <div class="tb-right">
      <button class="tb-btn" title="Minimize" onclick={minimize}>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <path d="M5 12h14"/>
        </svg>
      </button>
      <button class="tb-btn close" title="Close" onclick={closeWindow}>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <path d="M6 6l12 12M6 18 18 6"/>
        </svg>
      </button>
    </div>
  </div>

  <!-- Progress dots (steps 1–6) -->
  {#if step > 0 && step < 7}
    <div class="progress">
      {#each Array(TOTAL_STEPS) as _, i}
        <button
          class="dot"
          class:active={i + 1 === step}
          class:done={i + 1 < step}
          onclick={() => { if (i + 1 < step) { direction = 'back'; step = i + 1; } }}
          aria-label="Step {i + 1}"
        ></button>
      {/each}
    </div>
  {/if}

  <!-- Step content -->
  <div
    class="step-wrap"
    class:visible
    class:slide-left={direction === 'forward'}
    class:slide-right={direction === 'back'}
  >
    <!-- ── Step 0: Intro ─────────────────────────────────── -->
    {#if step === 0}
      <div class="step intro-step">
        <div class="intro-brand" class:ready={introReady}>
          <div class="intro-lockup">
            <div class="intro-mark">
              <span style="height:35%"></span>
              <span style="height:70%"></span>
              <span style="height:100%"></span>
              <span style="height:55%"></span>
              <span style="height:25%"></span>
            </div>
            <div class="intro-wordmark">
              <h1 class="brand-name">Open Flow</h1>
              <p class="brand-tagline">open-source AI dictation for Windows</p>
            </div>
          </div>
        </div>

        <div class="how-it-works" class:ready={introReady}>
          <p class="how-label">How it works</p>
          <div class="how-steps">
            <div class="how-step">
              <div class="how-num">1</div>
              <div>
                <strong>Hold <kbd>Alt</kbd> + <kbd>Space</kbd></strong>
                <p>Start recording. A floating pill shows your audio level.</p>
              </div>
            </div>
            <div class="how-step">
              <div class="how-num">2</div>
              <div>
                <strong>Release to transcribe</strong>
                <p>Your speech is sent to the AI provider and converted to text.</p>
              </div>
            </div>
            <div class="how-step">
              <div class="how-num">3</div>
              <div>
                <strong>Text appears instantly</strong>
                <p>Cleaned text is injected into whatever app you're focused on.</p>
              </div>
            </div>
          </div>
        </div>

        <div class="intro-actions" class:ready={introReady}>
          <button class="btn-primary btn-lg" onclick={goNext}>Get Started</button>
          <p class="intro-note">Takes about 2 minutes · You can change anything later</p>
        </div>
      </div>

    <!-- ── Step 1: Pick Provider ──────────────────────────── -->
    {:else if step === 1}
      <div class="step">
        <div class="step-header">
          <h2>Choose your AI provider</h2>
          <p class="step-sub">This powers both transcription and text cleanup. You can switch anytime in Settings.</p>
        </div>
        <div class="provider-cards">
          {#each providers as p}
            <button
              class="provider-card"
              class:selected={selectedProvider === p.id}
              onclick={() => { selectedProvider = p.id; }}
            >
              <div class="provider-top">
                <div class="provider-icon">{@html p.icon}</div>
                <div class="provider-info">
                  <div class="provider-name-row">
                    <span class="provider-name">{p.name}</span>
                    {#if p.badge}
                      <span class="badge">{p.badge}</span>
                    {/if}
                  </div>
                  <span class="provider-tagline">{p.tagline}</span>
                </div>
                <div class="provider-radio" class:checked={selectedProvider === p.id}></div>
              </div>
              <p class="provider-desc">{p.desc}</p>
            </button>
          {/each}
        </div>
        <div class="step-footer">
          <div></div>
          <button class="btn-primary" onclick={goNext}>Next</button>
        </div>
      </div>

    <!-- ── Step 2: API Key ────────────────────────────────── -->
    {:else if step === 2}
      {@const guide = providerGuides[selectedProvider]}
      <div class="step">
        <div class="step-header">
          <h2>Enter your {providers.find(p => p.id === selectedProvider)?.name} API key</h2>
          <p class="step-sub">Keys are stored locally and never leave your machine.</p>
        </div>

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
            {#if showKey}
              <input
                class="key-input"
                type="text"
                bind:value={apiKeyDraft}
                placeholder="Paste your API key here…"
                spellcheck="false"
                autocomplete="off"
              />
            {:else}
              <input
                class="key-input"
                type="password"
                bind:value={apiKeyDraft}
                placeholder="Paste your API key here…"
                spellcheck="false"
                autocomplete="off"
              />
            {/if}
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
          {#if keySaved}
            <p class="key-saved">Key saved successfully.</p>
          {/if}
        </div>

        <div class="step-footer">
          <button class="btn-skip" onclick={skip}>Skip for now</button>
          <button class="btn-primary" onclick={saveKeyAndNext} disabled={keySaving}>
            {keySaving ? 'Saving…' : apiKeyDraft.trim() ? 'Save & Continue' : 'Continue'}
          </button>
        </div>
      </div>

    <!-- ── Step 3: Cleanup Intensity ─────────────────────── -->
    {:else if step === 3}
      <div class="step">
        <div class="step-header">
          <h2>How should your text be cleaned up?</h2>
          <p class="step-sub">The AI applies this after transcribing. You can override it per-app later.</p>
        </div>
        <div class="option-cards">
          {#each cleanupCards as c}
            <button
              class="option-card"
              class:selected={selectedIntensity === c.id}
              onclick={() => { selectedIntensity = c.id; }}
            >
              <div class="option-card-top">
                <span class="option-name">{c.name}</span>
                <div class="option-radio" class:checked={selectedIntensity === c.id}></div>
              </div>
              <p class="option-desc">{c.desc}</p>
            </button>
          {/each}
        </div>
        <div class="step-footer">
          <button class="btn-skip" onclick={skip}>Skip for now</button>
          <button class="btn-primary" onclick={goNext}>Next</button>
        </div>
      </div>

    <!-- ── Step 4: Personal Tone ─────────────────────────── -->
    {:else if step === 4}
      <div class="step">
        <div class="step-header">
          <h2>Pick your default tone</h2>
          <p class="step-sub">How should your dictations sound? This is the default — you can map specific apps to different tones.</p>
        </div>
        <div class="tone-grid">
          {#each toneCards as t}
            <button
              class="tone-card"
              class:selected={selectedTone === t.id}
              onclick={() => { selectedTone = t.id; }}
            >
              <div class="tone-name">{t.name}</div>
              <p class="tone-desc">{t.desc}</p>
              <div class="tone-check" class:visible={selectedTone === t.id}>
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
              </div>
            </button>
          {/each}
        </div>
        <div class="step-footer">
          <button class="btn-skip" onclick={skip}>Skip for now</button>
          <button class="btn-primary" onclick={goNext}>Next</button>
        </div>
      </div>

    <!-- ── Step 5: App Mappings ───────────────────────────── -->
    {:else if step === 5}
      <div class="step">
        <div class="step-header">
          <h2>Map apps to styles <span class="optional-badge">Optional</span></h2>
          <p class="step-sub">Select apps that should use a different tone or cleanup level. You can add more anytime from the Style page.</p>
        </div>

        <div class="app-search-wrap">
          <svg class="search-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
          <input class="app-search" type="text" bind:value={appSearch} placeholder="Search installed apps…" />
        </div>

        {#if !appsLoaded}
          <p class="apps-loading">Loading installed apps…</p>
        {:else if filteredApps.length === 0}
          <p class="apps-loading">No apps found.</p>
        {:else}
          <div class="apps-list">
            {#each filteredApps as app}
              {@const mapped = mappings.find(m => m.exe === app.exe)}
              <div class="app-row" class:mapped={!!mapped}>
                <button class="app-toggle" onclick={() => toggleMapping(app)}>
                  <div class="app-toggle-check" class:checked={!!mapped}>
                    {#if mapped}
                      <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
                    {/if}
                  </div>
                  <span class="app-name">{app.name}</span>
                  <span class="app-exe">{app.exe}</span>
                </button>
                {#if mapped}
                  <div class="profile-drop-wrap">
                    <button
                      class="profile-drop-btn"
                      onclick={(e) => toggleProfileDropdown(app.exe, e)}
                    >
                      {profileOptions.find(o => o.id === mapped.profile)?.label ?? 'Casual'}
                      <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><polyline points="6 9 12 15 18 9"/></svg>
                    </button>
                    {#if openDropdownExe === app.exe}
                      <div class="profile-drop-list">
                        {#each profileOptions as opt}
                          <button
                            class="profile-drop-item"
                            class:active={mapped.profile === opt.id}
                            onclick={() => pickProfile(app.exe, opt.id)}
                          >{opt.label}</button>
                        {/each}
                      </div>
                    {/if}
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        {/if}

        <div class="step-footer">
          <button class="btn-skip" onclick={skip}>I'll set this up later</button>
          <button class="btn-primary" onclick={goNext}>
            {mappings.length > 0 ? `Save ${mappings.length} mapping${mappings.length !== 1 ? 's' : ''} & Next` : 'Next'}
          </button>
        </div>
      </div>

    <!-- ── Step 6: Quick Settings ──────────────────────────── -->
    {:else if step === 6}
      <div class="step qs-step">
        <div class="step-header">
          <h2>A few things worth knowing about</h2>
          <p class="step-sub">Defaults that work for most people — change them anytime in Settings.</p>
        </div>

        <div class="qs-cards" class:ready={quickSettingsReady}>
          <!-- Card 1: Smart Processing -->
          <div class="qs-card qs-card-1">
            <div class="qs-card-header">
              <div class="qs-card-icon">
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                  <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/>
                </svg>
              </div>
              <div>
                <h3 class="qs-card-title">Smart Processing</h3>
                <p class="qs-card-sub">AI cleanup and on-device learning</p>
              </div>
            </div>
            <div class="qs-toggle-list">
              <div class="qs-toggle-row">
                <div>
                  <div class="qs-toggle-label">AI cleanup</div>
                  <div class="qs-toggle-desc">Refine every transcription with an LLM automatically</div>
                </div>
                <div class="qs-toggle" class:on={quickPrefs.cleanup} role="switch" aria-checked={quickPrefs.cleanup} tabindex="0"
                  onclick={() => { quickPrefs = { ...quickPrefs, cleanup: !quickPrefs.cleanup }; }}
                  onkeydown={(e) => e.key === 'Enter' && (quickPrefs = { ...quickPrefs, cleanup: !quickPrefs.cleanup })}
                ></div>
              </div>
              <div class="qs-toggle-row">
                <div>
                  <div class="qs-toggle-label">Noise reduction</div>
                  <div class="qs-toggle-desc">Suppress background noise before transcription</div>
                </div>
                <div class="qs-toggle" class:on={quickPrefs.noise} role="switch" aria-checked={quickPrefs.noise} tabindex="0"
                  onclick={() => { quickPrefs = { ...quickPrefs, noise: !quickPrefs.noise }; }}
                  onkeydown={(e) => e.key === 'Enter' && (quickPrefs = { ...quickPrefs, noise: !quickPrefs.noise })}
                ></div>
              </div>
              <div class="qs-toggle-row">
                <div>
                  <div class="qs-toggle-label">Contextual capitalization</div>
                  <div class="qs-toggle-desc">Lowercase the first word when injecting mid-sentence</div>
                </div>
                <div class="qs-toggle" class:on={quickPrefs.caps} role="switch" aria-checked={quickPrefs.caps} tabindex="0"
                  onclick={() => { quickPrefs = { ...quickPrefs, caps: !quickPrefs.caps }; }}
                  onkeydown={(e) => e.key === 'Enter' && (quickPrefs = { ...quickPrefs, caps: !quickPrefs.caps })}
                ></div>
              </div>
              <div class="qs-toggle-row">
                <div>
                  <div class="qs-toggle-label">Auto-learn corrections</div>
                  <div class="qs-toggle-desc">Add confirmed corrections to your dictionary automatically</div>
                </div>
                <div class="qs-toggle" class:on={quickPrefs.autoLearn} role="switch" aria-checked={quickPrefs.autoLearn} tabindex="0"
                  onclick={() => { quickPrefs = { ...quickPrefs, autoLearn: !quickPrefs.autoLearn }; }}
                  onkeydown={(e) => e.key === 'Enter' && (quickPrefs = { ...quickPrefs, autoLearn: !quickPrefs.autoLearn })}
                ></div>
              </div>
            </div>
          </div>

          <!-- Card 2: System -->
          <div class="qs-card qs-card-2">
            <div class="qs-card-header">
              <div class="qs-card-icon">
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                  <rect x="2" y="3" width="20" height="14" rx="2"/><path d="M8 21h8M12 17v4"/>
                </svg>
              </div>
              <div>
                <h3 class="qs-card-title">System</h3>
                <p class="qs-card-sub">Launch and recording preferences</p>
              </div>
            </div>
            <div class="qs-toggle-list">
              <div class="qs-toggle-row">
                <div>
                  <div class="qs-toggle-label">Start on boot</div>
                  <div class="qs-toggle-desc">Launch Open Flow with Windows</div>
                </div>
                <div class="qs-toggle" class:on={quickPrefs.autostart} role="switch" aria-checked={quickPrefs.autostart} tabindex="0"
                  onclick={() => { quickPrefs = { ...quickPrefs, autostart: !quickPrefs.autostart }; }}
                  onkeydown={(e) => e.key === 'Enter' && (quickPrefs = { ...quickPrefs, autostart: !quickPrefs.autostart })}
                ></div>
              </div>
              <div class="qs-toggle-row">
                <div>
                  <div class="qs-toggle-label">Mute while recording</div>
                  <div class="qs-toggle-desc">Silence other audio during dictation</div>
                </div>
                <div class="qs-toggle" class:on={quickPrefs.muteAudio} role="switch" aria-checked={quickPrefs.muteAudio} tabindex="0"
                  onclick={() => { quickPrefs = { ...quickPrefs, muteAudio: !quickPrefs.muteAudio }; }}
                  onkeydown={(e) => e.key === 'Enter' && (quickPrefs = { ...quickPrefs, muteAudio: !quickPrefs.muteAudio })}
                ></div>
              </div>
              <div class="qs-toggle-row">
                <div>
                  <div class="qs-toggle-label">Auto-retry on quota errors</div>
                  <div class="qs-toggle-desc">Switch to another provider if the primary hits its limit</div>
                </div>
                <div class="qs-toggle" class:on={quickPrefs.apiFallback} role="switch" aria-checked={quickPrefs.apiFallback} tabindex="0"
                  onclick={() => { quickPrefs = { ...quickPrefs, apiFallback: !quickPrefs.apiFallback }; }}
                  onkeydown={(e) => e.key === 'Enter' && (quickPrefs = { ...quickPrefs, apiFallback: !quickPrefs.apiFallback })}
                ></div>
              </div>
            </div>
          </div>
        </div>

        <div class="step-footer">
          <button class="btn-skip" onclick={skip}>Skip</button>
          <button class="btn-primary" onclick={goNext}>Next</button>
        </div>
      </div>

    <!-- ── Step 7: Done ───────────────────────────────────── -->
    {:else if step === 7}
      <div class="step done-step">
        <div class="done-check-wrap">
          <svg class="done-check" class:animate={checkAnimating} width="64" height="64" viewBox="0 0 64 64" fill="none">
            <circle cx="32" cy="32" r="28" stroke="var(--accent-soft)" stroke-width="6"/>
            <circle cx="32" cy="32" r="28" stroke="var(--accent)" stroke-width="6"
              stroke-dasharray="176"
              stroke-dashoffset={checkAnimating ? '0' : '176'}
              stroke-linecap="round"
              style="transition: stroke-dashoffset 0.6s cubic-bezier(0.4,0,0.2,1); transform: rotate(-90deg); transform-origin: 32px 32px;"
            />
            <polyline
              points="20,33 28,41 44,24"
              stroke="var(--accent)"
              stroke-width="4.5"
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-dasharray="36"
              stroke-dashoffset={checkAnimating ? '0' : '36'}
              style="transition: stroke-dashoffset 0.4s 0.5s cubic-bezier(0.4,0,0.2,1);"
            />
          </svg>
        </div>
        <h2 class="done-title">You're all set.</h2>
        <p class="done-sub">
          Hold <kbd>Alt</kbd> + <kbd>Space</kbd> anywhere to start dictating.
          Open Flow lives in your system tray and is always ready.
        </p>
        <div class="done-summary">
          <div class="summary-item">
            <span class="summary-label">Provider</span>
            <span class="summary-val">{providers.find(p => p.id === selectedProvider)?.name}</span>
          </div>
          <div class="summary-item">
            <span class="summary-label">Cleanup</span>
            <span class="summary-val">{cleanupCards.find(c => c.id === selectedIntensity)?.name}</span>
          </div>
          <div class="summary-item">
            <span class="summary-label">Tone</span>
            <span class="summary-val">{toneCards.find(t => t.id === selectedTone)?.name}</span>
          </div>
          {#if mappings.length > 0}
            <div class="summary-item">
              <span class="summary-label">App mappings</span>
              <span class="summary-val">{mappings.length} app{mappings.length !== 1 ? 's' : ''}</span>
            </div>
          {/if}
        </div>
        <button class="btn-primary btn-lg" onclick={finish}>Start dictating</button>
        <p class="done-note">Everything can be changed in Settings or the Style page.</p>
      </div>
    {/if}
  </div>
</div>

<style>
  /* ── Overlay ───────────────────────────────────────────────────────── */
  .setup-overlay {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: var(--paper);
    display: flex;
    flex-direction: column;
    align-items: center;
    overflow: hidden;
  }

  /* ── Title bar ─────────────────────────────────────────────────────── */
  .setup-titlebar {
    width: 100%;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 10px;
    flex-shrink: 0;
  }

  .tb-right {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .tb-btn {
    width: 24px;
    height: 24px;
    display: grid;
    place-items: center;
    background: transparent;
    border: 0;
    border-radius: 6px;
    color: var(--ink-mute);
    cursor: pointer;
  }

  .tb-btn:hover { background: var(--paper-2); color: var(--ink-strong); }
  .tb-btn.close:hover { background: var(--danger); color: var(--on-accent); }

  /* ── Progress dots ─────────────────────────────────────────────────── */
  .progress {
    display: flex;
    gap: 6px;
    padding: 20px 0 0;
    flex-shrink: 0;
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--line-strong);
    border: none;
    padding: 0;
    cursor: default;
    transition: background 0.25s, width 0.25s, border-radius 0.25s;
  }

  .dot.active {
    width: 20px;
    border-radius: 4px;
    background: var(--accent);
    cursor: default;
  }

  .dot.done {
    background: var(--accent);
    opacity: 0.45;
    cursor: pointer;
  }

  /* ── Step container / transitions ─────────────────────────────────── */
  .step-wrap {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    padding: 24px 0 32px;
    opacity: 0;
    transform: translateX(28px);
    transition: opacity 0.22s ease, transform 0.22s ease;
  }

  .step-wrap.visible {
    opacity: 1;
    transform: translateX(0);
  }

  .step-wrap.slide-right {
    transform: translateX(-28px);
  }

  .step-wrap.slide-right.visible {
    transform: translateX(0);
  }

  /* ── Generic step layout ───────────────────────────────────────────── */
  .step {
    width: 100%;
    max-width: 560px;
    padding: 0 28px;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  .step-header h2 {
    font-family: var(--serif);
    font-size: 22px;
    font-weight: 500;
    color: var(--ink-strong);
    margin: 0 0 6px;
    line-height: 1.25;
  }

  .step-sub {
    font-size: 13px;
    color: var(--ink-mute);
    margin: 0;
    line-height: 1.5;
  }

  .step-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-top: 4px;
  }

  /* ── Buttons ───────────────────────────────────────────────────────── */
  .btn-primary {
    background: var(--accent);
    color: var(--on-accent);
    border: none;
    border-radius: var(--r-sm);
    padding: 9px 22px;
    font-family: var(--sans);
    font-size: 13.5px;
    font-weight: 500;
    cursor: pointer;
    transition: opacity 0.15s, transform 0.1s;
  }

  .btn-primary:hover { opacity: 0.88; }
  .btn-primary:active { transform: scale(0.98); }
  .btn-primary:disabled { opacity: 0.45; cursor: not-allowed; }

  .btn-primary.btn-lg {
    padding: 11px 32px;
    font-size: 14.5px;
    border-radius: var(--r-md);
  }

  .btn-skip {
    background: transparent;
    border: none;
    color: var(--ink-faint);
    font-family: var(--sans);
    font-size: 12.5px;
    cursor: pointer;
    padding: 0;
    transition: color 0.15s;
  }

  .btn-skip:hover { color: var(--ink-mute); }

  /* ── Intro step ────────────────────────────────────────────────────── */
  .intro-step {
    align-items: center;
    text-align: center;
    max-width: 480px;
  }

  .intro-brand {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    opacity: 0;
    transform: translateY(14px);
    transition: opacity 0.5s ease, transform 0.5s ease;
  }

  .intro-brand.ready { opacity: 1; transform: none; }

  .intro-lockup {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .intro-mark {
    width: 42px;
    height: 36px;
    display: flex;
    align-items: flex-end;
    gap: 3px;
    flex-shrink: 0;
  }

  .intro-mark span {
    flex: 1;
    background: var(--accent);
    border-radius: 999px;
    display: block;
  }

  .intro-wordmark {
    display: flex;
    flex-direction: column;
    gap: 2px;
    text-align: left;
  }

  .brand-name {
    font-family: var(--serif);
    font-size: 28px;
    font-weight: 500;
    color: var(--ink-strong);
    margin: 0;
    letter-spacing: -0.3px;
    line-height: 1.1;
  }

  .brand-tagline {
    font-size: 13px;
    color: var(--ink-mute);
    margin: 0;
    line-height: 1.3;
  }

  .how-it-works {
    background: var(--paper-2);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    padding: 18px 22px;
    text-align: left;
    opacity: 0;
    transform: translateY(10px);
    transition: opacity 0.5s 0.15s ease, transform 0.5s 0.15s ease;
  }

  .how-it-works.ready { opacity: 1; transform: none; }

  .how-label {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--ink-faint);
    margin: 0 0 14px;
  }

  .how-steps { display: flex; flex-direction: column; gap: 14px; }

  .how-step {
    display: flex;
    gap: 14px;
    align-items: flex-start;
  }

  .how-num {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent-ink);
    font-size: 12px;
    font-weight: 600;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    margin-top: 1px;
  }

  .how-step strong {
    display: block;
    font-size: 13px;
    color: var(--ink-soft);
    margin-bottom: 2px;
  }

  .how-step p {
    margin: 0;
    font-size: 12.5px;
    color: var(--ink-mute);
    line-height: 1.4;
  }

  .intro-actions {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    opacity: 0;
    transform: translateY(8px);
    transition: opacity 0.5s 0.28s ease, transform 0.5s 0.28s ease;
  }

  .intro-actions.ready { opacity: 1; transform: none; }

  .intro-note {
    font-size: 12px;
    color: var(--ink-faint);
    margin: 0;
  }

  /* ── Provider cards ────────────────────────────────────────────────── */
  .provider-cards { display: flex; flex-direction: column; gap: 10px; }

  .provider-card {
    background: var(--bg-elev);
    border: 1.5px solid var(--line);
    border-radius: var(--r-md);
    padding: 14px 16px;
    text-align: left;
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .provider-card:hover { border-color: var(--line-strong); }

  .provider-card.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .provider-top {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .provider-icon { color: var(--ink-mute); flex-shrink: 0; }

  .provider-card.selected .provider-icon { color: var(--accent-ink); }

  .provider-info { flex: 1; }

  .provider-name-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 2px;
  }

  .provider-name {
    font-size: 14px;
    font-weight: 500;
    color: var(--ink-strong);
  }

  .badge {
    font-size: 10.5px;
    font-weight: 600;
    background: var(--accent);
    color: var(--on-accent);
    border-radius: 20px;
    padding: 1px 8px;
    letter-spacing: 0.02em;
  }

  .provider-tagline {
    font-size: 12px;
    color: var(--ink-mute);
  }

  .provider-desc {
    font-size: 12.5px;
    color: var(--ink-mute);
    margin: 0;
    line-height: 1.45;
    padding-left: 40px;
  }

  .provider-radio {
    width: 17px;
    height: 17px;
    border-radius: 50%;
    border: 2px solid var(--line-strong);
    flex-shrink: 0;
    transition: border-color 0.15s;
    position: relative;
  }

  .provider-radio.checked {
    border-color: var(--accent);
  }

  .provider-radio.checked::after {
    content: '';
    position: absolute;
    inset: 3px;
    border-radius: 50%;
    background: var(--accent);
  }

  /* ── API Key step ──────────────────────────────────────────────────── */
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

  .guide-steps {
    margin: 0;
    padding-left: 20px;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .guide-steps li {
    font-size: 13px;
    color: var(--ink-soft);
    line-height: 1.45;
  }

  .url-row {
    display: flex;
    align-items: center;
    gap: 10px;
    background: var(--paper);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    padding: 8px 12px;
  }

  .url-display {
    flex: 1;
    font-family: var(--mono);
    font-size: 12px;
    color: var(--accent-ink);
    word-break: break-all;
  }

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
  .key-saved { font-size: 12px; color: var(--accent-ink); margin: 0; }

  /* ── Option cards (cleanup intensity) ─────────────────────────────── */
  .option-cards {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .option-card {
    background: var(--bg-elev);
    border: 1.5px solid var(--line);
    border-radius: var(--r-md);
    padding: 14px 14px;
    text-align: left;
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .option-card:hover { border-color: var(--line-strong); }

  .option-card.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .option-card-top {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .option-name {
    font-size: 13.5px;
    font-weight: 500;
    color: var(--ink-strong);
  }

  .option-desc {
    font-size: 12px;
    color: var(--ink-mute);
    margin: 0;
    line-height: 1.4;
  }

  .option-radio {
    width: 15px;
    height: 15px;
    border-radius: 50%;
    border: 2px solid var(--line-strong);
    flex-shrink: 0;
    position: relative;
    transition: border-color 0.15s;
  }

  .option-radio.checked { border-color: var(--accent); }
  .option-radio.checked::after {
    content: '';
    position: absolute;
    inset: 2.5px;
    border-radius: 50%;
    background: var(--accent);
  }

  /* ── Tone grid ─────────────────────────────────────────────────────── */
  .tone-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }

  .tone-card {
    background: var(--bg-elev);
    border: 1.5px solid var(--line);
    border-radius: var(--r-sm);
    padding: 12px 12px 10px;
    text-align: left;
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .tone-card:hover { border-color: var(--line-strong); }

  .tone-card.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .tone-name {
    font-size: 13px;
    font-weight: 500;
    color: var(--ink-strong);
  }

  .tone-desc {
    font-size: 11.5px;
    color: var(--ink-mute);
    margin: 0;
    line-height: 1.35;
  }

  .tone-check {
    position: absolute;
    top: 8px;
    right: 8px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--on-accent);
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    transform: scale(0.6);
    transition: opacity 0.15s, transform 0.15s;
  }

  .tone-check.visible { opacity: 1; transform: scale(1); }

  /* ── App mappings ──────────────────────────────────────────────────── */
  .optional-badge {
    font-family: var(--sans);
    font-size: 11px;
    font-weight: 500;
    background: var(--paper-2);
    border: 1px solid var(--line);
    border-radius: 20px;
    padding: 1px 9px;
    color: var(--ink-faint);
    vertical-align: middle;
    margin-left: 8px;
  }

  .app-search-wrap {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--bg-elev);
    border: 1.5px solid var(--line-strong);
    border-radius: var(--r-sm);
    padding: 8px 12px;
    transition: border-color 0.15s;
  }

  .app-search-wrap:focus-within { border-color: var(--accent); }

  .search-icon { color: var(--ink-faint); flex-shrink: 0; }

  .app-search {
    flex: 1;
    border: none;
    background: transparent;
    font-family: var(--sans);
    font-size: 13px;
    color: var(--ink);
    outline: none;
  }

  .app-search::placeholder { color: var(--ink-faint); }

  .apps-loading {
    font-size: 12.5px;
    color: var(--ink-faint);
    text-align: center;
    margin: 8px 0;
  }

  .apps-list {
    max-height: 220px;
    overflow-y: auto;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--bg-elev);
  }

  .app-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px 0 0;
    border-bottom: 1px solid var(--line-soft);
    transition: background 0.12s;
  }

  .app-row:last-child { border-bottom: none; }
  .app-row.mapped { background: var(--accent-soft); }

  .app-toggle {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    background: transparent;
    border: none;
    cursor: pointer;
    text-align: left;
  }

  .app-toggle-check {
    width: 16px;
    height: 16px;
    border-radius: 4px;
    border: 1.5px solid var(--line-strong);
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.12s, border-color 0.12s;
    color: var(--on-accent);
  }

  .app-toggle-check.checked {
    background: var(--accent);
    border-color: var(--accent);
  }

  .app-name { font-size: 13px; color: var(--ink-soft); flex: 1; }
  .app-exe { font-size: 11px; color: var(--ink-faint); font-family: var(--mono); }

  .profile-drop-wrap {
    position: relative;
    flex-shrink: 0;
  }

  .profile-drop-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    background: transparent;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    padding: 5px 12px;
    font-size: 12px;
    font-family: var(--sans);
    color: var(--ink-strong);
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
  }

  .profile-drop-btn:hover { background: var(--paper); }

  .profile-drop-list {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    box-shadow: var(--shadow-popover);
    min-width: 130px;
    max-height: 200px;
    overflow-y: auto;
    z-index: 10;
  }

  .profile-drop-item {
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

  .profile-drop-item:last-child { border-bottom: none; }
  .profile-drop-item:hover { background: var(--paper); }
  .profile-drop-item.active { background: var(--accent-soft); color: var(--ink); font-weight: 500; }

  /* ── Done step ─────────────────────────────────────────────────────── */
  .done-step {
    align-items: center;
    text-align: center;
    max-width: 440px;
  }

  .done-check-wrap { margin-bottom: 4px; }

  .done-title {
    font-family: var(--serif);
    font-size: 26px;
    font-weight: 500;
    color: var(--ink-strong);
    margin: 0;
  }

  .done-sub {
    font-size: 13.5px;
    color: var(--ink-mute);
    margin: 0;
    line-height: 1.5;
  }

  .done-summary {
    display: flex;
    gap: 16px;
    background: var(--paper-2);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    padding: 14px 20px;
    flex-wrap: wrap;
    justify-content: center;
  }

  .summary-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }

  .summary-label {
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--ink-faint);
  }

  .summary-val {
    font-size: 13px;
    font-weight: 500;
    color: var(--ink-soft);
  }

  .done-note {
    font-size: 12px;
    color: var(--ink-faint);
    margin: 0;
  }

  /* ── Quick Settings step ───────────────────────────────────────────── */
  .qs-step { max-width: 760px; }

  .qs-cards {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
    align-items: stretch;
  }

  .qs-card {
    background: var(--bg-elev);
    border: 1.5px solid var(--line);
    border-radius: var(--r-md);
    padding: 16px 18px;
    opacity: 0;
    transform: translateY(12px);
    transition: opacity 0.3s ease, transform 0.3s ease, border-color 0.15s;
  }

  .qs-cards.ready .qs-card-1 {
    opacity: 1;
    transform: none;
  }

  .qs-cards.ready .qs-card-2 {
    opacity: 1;
    transform: none;
    transition-delay: 0.1s;
  }

  .qs-card-header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 12px;
  }

  .qs-card-icon {
    width: 34px;
    height: 34px;
    border-radius: var(--r-sm);
    background: var(--accent-soft);
    color: var(--accent-ink);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .qs-card-title {
    font-family: var(--serif);
    font-size: 15px;
    font-weight: 500;
    color: var(--ink-strong);
    margin: 0 0 2px;
    line-height: 1.2;
  }

  .qs-card-sub {
    font-size: 11.5px;
    color: var(--ink-mute);
    margin: 0;
  }

  .qs-toggle-list {
    display: flex;
    flex-direction: column;
  }

  .qs-toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 0;
    border-top: 1px solid var(--line);
    gap: 16px;
  }

  .qs-toggle-label {
    font-size: 13px;
    font-weight: 500;
    color: var(--ink-strong);
    margin-bottom: 2px;
  }

  .qs-toggle-desc {
    font-size: 11.5px;
    color: var(--ink-mute);
    line-height: 1.4;
  }

  .qs-toggle {
    width: 30px;
    height: 16px;
    background: var(--line-strong);
    border-radius: 999px;
    position: relative;
    cursor: pointer;
    transition: background 0.3s ease-out;
    flex-shrink: 0;
  }

  .qs-toggle::after {
    content: '';
    position: absolute;
    width: 12px;
    height: 12px;
    background: var(--bg-elev);
    border-radius: 50%;
    top: 2px;
    left: 2px;
    transition: left 0.35s cubic-bezier(0.22, 1, 0.36, 1);
    box-shadow: 0 1px 2px color-mix(in srgb, var(--ink) 15%, transparent);
  }

  .qs-toggle.on { background: var(--accent); }
  .qs-toggle.on::after { left: 16px; }
</style>
