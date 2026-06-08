<script lang="ts">
  import { invoke } from '../tauri';
  import { onMount, onDestroy } from 'svelte';
  import { fly, fade, slide } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { appStore } from '../stores';
  import { animateWidth, motionMs } from '../motion';
  import { getSetupCalibrationCopy } from '../calibrationCopy';
  import { saveSetting, type AppearanceMode, type CleanupIntensity, type ToneId } from '../settings';
  import {
    getTranscriptionLanguageLabel,
    transcriptionLanguages,
    type TranscriptionLanguageCode,
  } from '../transcriptionLanguages';
  import { isMac } from '../platform';
  import MacPermissions from '../components/MacPermissions.svelte';

  // Platform-aware labels for the dictation hotkey and copy.
  const hkKey1 = isMac ? 'fn' : 'Ctrl';
  const hkKey2 = isMac ? 'Control' : 'Windows';
  const platformTagline = isMac ? 'macOS' : 'Windows';
  const TOTAL_STEPS = isMac ? 8 : 7; // steps 1–8 show progress dots (done is step 8 on Windows / 9 on macOS)
  const permissionStep = isMac ? 3 : -1;
  const cleanupStep = isMac ? 4 : 3;
  const toneStep = isMac ? 5 : 4;
  const appearanceStep = isMac ? 6 : 5;
  const quickSettingsStep = isMac ? 7 : 6;
  const calibrationStep = isMac ? 8 : 7;
  const doneStep = isMac ? 9 : 8;

  type AppWindow = {
    minimize: () => Promise<void>;
  };

  let win = $state<AppWindow | null>(null);
  onMount(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      win = getCurrentWindow();
    } catch {}
    try {
      const [savedAppearance, savedLanguage] = await Promise.all([
        invoke<AppearanceMode | null>('get_setting', { key: 'appearance_mode' }),
        invoke<TranscriptionLanguageCode | null>('get_setting', { key: 'transcription_language' }),
      ]);
      if (savedAppearance === 'system' || savedAppearance === 'light' || savedAppearance === 'dark') {
        selectedAppearance = savedAppearance;
      }
      if (savedLanguage && transcriptionLanguages.some((option) => option.code === savedLanguage)) {
        selectedLanguage = savedLanguage;
      }
    } catch {}
    setTimeout(() => { introReady = true; }, 60);
  });

  function minimize() { win?.minimize(); }
  async function closeWindow() {
    try { await invoke('hide_main'); } catch {}
  }

  // ── Step state ──────────────────────────────────────────────────────────────
  let step = $state(0);

  // ── Microphone Calibration ──────────────────────────────────────────────────
  import {
    isCalibrating,
    calibrationCountdown,
    calibratedGain,
    micLevel,
    startCalibration,
    cancelCalibration,
    speechDetected,
    calibrationPhase
  } from '../calibration';

  onDestroy(() => {
    cancelCalibration();
  });

  let direction = $state<'forward' | 'back'>('forward');
  let animating = $state(false);
  let visible = $state(true);

  // ── Quick Settings ──────────────────────────────────────────────────────────
  let quickPrefs = $state({
    cleanup: true,
    noise: true,
    caps: true,
    autoLearn: false,
    autostart: false,
    muteAudio: false,
  });
  let quickSettingsReady = $state(false);
  type QuickPrefKey = keyof typeof quickPrefs;
  function toggleQuickPref(key: QuickPrefKey) {
    quickPrefs = { ...quickPrefs, [key]: !quickPrefs[key] };
  }
  function handleQuickSwitchKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
    }
  }
  function handleQuickSwitchKeyup(event: KeyboardEvent, key: QuickPrefKey) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      toggleQuickPref(key);
    }
  }
  let selectedLanguage = $state<TranscriptionLanguageCode>('en');
  let setupCalibrationCopy = $derived(getSetupCalibrationCopy(selectedLanguage));
  const onboardingLanguageSet = new Set<TranscriptionLanguageCode>(['en', 'es', 'fr', 'de', 'pt', 'zh']);
  const onboardingLanguages = transcriptionLanguages.filter((option) => onboardingLanguageSet.has(option.code));

  // ── Provider ─────────────────────────────────────────────────────────────────
  let selectedProvider = $state<'groq' | 'openai' | 'google'>('groq');

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
  let apiKeyDraft = $state('');
  let keySaved = $state(false);
  let keySaving = $state(false);
  let keyError = $state('');
  let showKey = $state(false);

  // macOS permission handling lives in the shared <MacPermissions> component;
  // it binds `permsGranted` (true once Accessibility + Input Monitoring + Mic
  // are all authorized) so we can advance/glow the Next button accordingly.
  let permsGranted = $state(!isMac);

  // ── Cleanup intensity ─────────────────────────────────────────────────────────
  let selectedIntensity = $state('medium');
  const cleanupCards = [
    { id: 'none',   name: 'Verbatim', desc: 'Raw transcription. No AI cleanup at all.' },
    { id: 'light',  name: 'Light',    desc: 'Removes filler words and repeated phrases. Keeps everything else.' },
    { id: 'medium', name: 'Medium',   desc: 'Removes fillers, cuts repetition, tightens phrasing. Keeps your detail.' },
    { id: 'high',   name: 'Direct',   desc: 'Aggressive rewrite. Punchy and concise — about half the words.' },
  ];

  // ── Personal tone ─────────────────────────────────────────────────────────────
  let selectedTone = $state('casual');
  const toneCards = [
    { id: 'casual',      name: 'Casual',      desc: 'Conversational. Light caps and punctuation — reads like a Slack message.' },
    { id: 'formal',      name: 'Formal',      desc: 'Professional prose. Full punctuation, expanded contractions, formal vocabulary. No em dashes.' },
    { id: 'very_casual', name: 'Very Casual', desc: 'All lowercase, almost no punctuation. Like a quick text typed without thinking.' },
  ];

  // ── Appearance ──────────────────────────────────────────────────────────────
  let selectedAppearance = $state<AppearanceMode>('system');
  const appearanceModes: { id: AppearanceMode; name: string; desc: string }[] = [
    { id: 'system', name: 'System', desc: 'Match your system theme automatically.' },
    { id: 'dark', name: 'Dark', desc: 'Lower glare for night work and dark desktops.' },
    { id: 'light', name: 'Light', desc: 'Brighter surfaces with higher daylight contrast.' },
  ];

  function pickLanguage(code: TranscriptionLanguageCode) {
    selectedLanguage = code;
  }

  // ── Navigation ────────────────────────────────────────────────────────────────
  async function goNext() {
    if (animating) return;
    if (step === doneStep) { await finish(); return; }
    direction = 'forward';
    animating = true;
    visible = false;
    await delay(220);
    step++;
    if (step === quickSettingsStep) setTimeout(() => { quickSettingsReady = true; }, 60);
    visible = true;
    await delay(220);
    animating = false;
  }

  async function goBack() {
    if (animating || step === 0) return;
    if (step === quickSettingsStep || step === doneStep) quickSettingsReady = false;
    direction = 'back';
    animating = true;
    visible = false;
    await delay(220);
    step--;
    if (step === quickSettingsStep) setTimeout(() => { quickSettingsReady = true; }, 60);
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
      await saveSetting('transcription_language', selectedLanguage);
      await saveSetting('cleanup_provider', selectedProvider);
      await saveSetting('appearance_mode', selectedAppearance);
      await saveSetting('cleanup_enabled', quickPrefs.cleanup);
      await saveSetting('noise_reduction', quickPrefs.noise);
      await saveSetting('contextual_caps_enabled', quickPrefs.caps);
      await saveSetting('auto_learn_enabled', quickPrefs.autoLearn);
      await saveSetting('mute_audio', quickPrefs.muteAudio);
      if (quickPrefs.autostart) await invoke('set_autostart', { enabled: true });
      await saveSetting('setup_complete', true);
    } catch {}
    appStore.appearanceMode = selectedAppearance;
    appStore.setupComplete = true;
  }

  function copyUrl(url: string) {
    navigator.clipboard.writeText('https://' + url).catch(() => {});
  }

  function delay(ms: number) { return new Promise(r => setTimeout(r, ms)); }

  // ── Intro animation ───────────────────────────────────────────────────────────
  let introReady = $state(false);

  // ── Done animation ────────────────────────────────────────────────────────────
  let checkAnimating = $state(false);
  $effect(() => {
    if (step !== doneStep) {
      checkAnimating = false;
      return;
    }

    checkAnimating = false;
    const timeout = setTimeout(() => {
      checkAnimating = true;
    }, 200);
    return () => clearTimeout(timeout);
  });
</script>

<!-- Full-screen overlay -->
<div class="setup-overlay">
  {#if !isMac}
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
  {/if}

  <!-- Progress dots -->
  {#if step > 0 && step < doneStep}
    <div class="progress">
      {#each Array.from({ length: TOTAL_STEPS }) as _, i}
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
    class="step-wrap scroll-styled"
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
              <p class="brand-tagline">open-source AI dictation for {platformTagline}</p>
            </div>
          </div>
        </div>

        <div class="how-it-works" class:ready={introReady}>
          <p class="how-label">How it works</p>
          <div class="how-steps">
            <div class="how-step">
              <div class="how-num">1</div>
              <div>
                <strong>Hold <kbd>{hkKey1}</kbd> + <kbd>{hkKey2}</kbd></strong>
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
          {#if isMac}
            <div class="keychain-note">
              <strong>macOS note:</strong>
              If Keychain asks for your login password, choose <span>Always Allow</span>.
              That keeps your API key stored securely without repeating the prompt.
            </div>
          {/if}
        </div>

        <div class="step-footer">
          <button class="btn-skip" onclick={skip}>Skip for now</button>
          <button class="btn-primary" onclick={saveKeyAndNext} disabled={keySaving}>
            {keySaving ? 'Saving…' : apiKeyDraft.trim() ? 'Save & Continue' : 'Continue'}
          </button>
        </div>
      </div>

    <!-- ── Step 3: macOS permissions ─────────────────────── -->
    {:else if isMac && step === permissionStep}
      <div class="step">
        <div class="step-header">
          <h2>Grant macOS permissions</h2>
          <p class="step-sub">Open Flow needs these to hear your voice and type into other apps. This list updates itself as you grant each one.</p>
        </div>

        <MacPermissions variant="setup" provider={selectedProvider} bind:allGranted={permsGranted} />

        <div class="step-footer">
          <button class="btn-skip" onclick={skip}>Skip for now</button>
          <button
            class="btn-primary"
            class:btn-primary--glow={permsGranted}
            onclick={goNext}
          >
            {permsGranted ? 'Continue' : 'Next'}
          </button>
        </div>
      </div>

    <!-- ── Step 3/4: Cleanup Intensity ───────────────────── -->
    {:else if step === cleanupStep}
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

    <!-- ── Step 4/5: Personal Tone ───────────────────────── -->
    {:else if step === toneStep}
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

    <!-- ── Step 5/6: Appearance ───────────────────────────── -->
    {:else if step === appearanceStep}
      <div class="step appearance-step">
        <div class="step-header">
          <h2>Choose your appearance</h2>
          <p class="step-sub">Choose your default theme mode. You can change this later in Settings.</p>
        </div>

        <div class="appearance-mode-grid">
          {#each appearanceModes as mode}
            <button
              class="appearance-mode-card"
              class:selected={selectedAppearance === mode.id}
              onclick={() => {
                selectedAppearance = mode.id;
                appStore.appearanceMode = mode.id;
              }}
            >
              <div class="appearance-mode-title-row">
                <span class="appearance-mode-name">{mode.name}</span>
                <span class="appearance-mode-radio" class:checked={selectedAppearance === mode.id}></span>
              </div>
              <p class="appearance-mode-desc">{mode.desc}</p>
            </button>
          {/each}
        </div>

        <div class="step-footer">
          <button class="btn-skip" onclick={skip}>Skip for now</button>
          <button class="btn-primary" onclick={goNext}>Next</button>
        </div>
      </div>

    <!-- ── Step 6/7: Microphone Calibration ───────────────── -->
    {:else if step === calibrationStep}
      <div class="step">
        <div class="step-header">
          <h2>{setupCalibrationCopy.title}</h2>
          <p class="step-sub">{setupCalibrationCopy.subtitle}</p>
        </div>

        <div class="calibration-box">
          {#if !$isCalibrating && $calibratedGain === null}
            <div class="cal-start-state"
              out:fade={{ duration: motionMs(160) }}>
              <div class="cal-steps-preview">
                <div class="cal-step-row">
                  <span class="cal-step-num">1</span>
                  <span class="cal-step-text">{setupCalibrationCopy.step1Text}</span>
                </div>
                <div class="cal-step-row">
                  <span class="cal-step-num">2</span>
                  <span class="cal-step-text">{setupCalibrationCopy.step2Text}</span>
                </div>
              </div>
              <button class="btn-primary" onclick={startCalibration}>{setupCalibrationCopy.startButton}</button>
            </div>
          {:else if $isCalibrating}
            <div class="cal-active-state"
              in:slide={{ duration: motionMs(260), easing: expoOut }}
              out:fade={{ duration: motionMs(160) }}>
              <div class="cal-phase-header">
                <div class="cal-label-stack">
                  {#key $calibrationPhase}
                    <span class="cal-phase-label"
                      in:fade={{ duration: motionMs(200), delay: motionMs(80) }}
                      out:fade={{ duration: motionMs(80) }}>
                      {$calibrationPhase === 'loud' ? setupCalibrationCopy.phase1Label : setupCalibrationCopy.phase2Label}
                    </span>
                  {/key}
                </div>
                <div class="cal-timer-ring">
                  <span class="cal-countdown">{$calibrationCountdown}s</span>
                </div>
              </div>
              <div class="cal-content-stack">
                {#key $calibrationPhase}
                  <div class="cal-phase-content"
                    in:fade={{ duration: motionMs(200), delay: motionMs(80) }}
                    out:fade={{ duration: motionMs(80) }}>
                    <p class="cal-prompt">
                      {$calibrationPhase === 'loud' ? setupCalibrationCopy.readPrompt : setupCalibrationCopy.whisperPrompt}
                    </p>
                    <blockquote class="cal-phrase">
                      "{$calibrationPhase === 'loud' ? setupCalibrationCopy.readPhrase : setupCalibrationCopy.whisperPhrase}"
                    </blockquote>
                  </div>
                {/key}
              </div>

              <!-- Live Level Visualizer -->
              <div class="cal-meter-container">
                <div class="cal-meter-track">
                  <div class="cal-meter-fill" style="width: {($micLevel * 100).toFixed(0)}%"></div>
                </div>
              </div>
              <button class="cal-cancel-btn" onclick={cancelCalibration}
                in:fly={{ y: 6, duration: motionMs(200), delay: motionMs(240), easing: expoOut }}>
                {setupCalibrationCopy.cancelButton}
              </button>
            </div>
          {:else if $calibratedGain !== null}
            <div class="cal-result-state"
              in:slide={{ duration: motionMs(280), easing: expoOut }}>
              {#if $speechDetected === false}
                <div class="cal-warning-icon">
                  <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <circle cx="12" cy="12" r="10"/>
                    <line x1="12" x2="12" y1="8" y2="12"/>
                    <line x1="12" x2="12" y1="16" y2="16"/>
                  </svg>
                </div>
                <h3 class="cal-result-title">{setupCalibrationCopy.silenceTitle}</h3>
                <p class="cal-result-desc">
                  {setupCalibrationCopy.silenceDescription} <strong>{$calibratedGain.toFixed(1)}×</strong>.
                </p>
              {:else}
                <div class="cal-success-icon">
                  <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/>
                    <polyline points="22 4 12 14.01 9 11.01"/>
                  </svg>
                </div>
                <h3 class="cal-result-title">{setupCalibrationCopy.successTitle}</h3>
                <p class="cal-result-desc">
                  {setupCalibrationCopy.successDescription} <strong>{$calibratedGain.toFixed(1)}×</strong>.
                  {setupCalibrationCopy.successTail}
                </p>
              {/if}
            </div>
          {/if}
        </div>

        <div class="step-footer">
          {#if $calibratedGain !== null}
            <button class="cal-recalibrate-btn" onclick={startCalibration}
              in:fade={{ duration: motionMs(200) }}>
              {setupCalibrationCopy.recalibrateButton}
            </button>
          {:else}
            <button class="btn-skip" onclick={skip} disabled={$isCalibrating}>{setupCalibrationCopy.skipButton}</button>
          {/if}
          <button class="btn-primary" onclick={goNext} disabled={$isCalibrating}
            style="min-width: 128px; text-align: center;">
            {$calibratedGain !== null ? setupCalibrationCopy.continueButton : setupCalibrationCopy.skipCalibrationButton}
          </button>
        </div>
      </div>

    <!-- ── Step 7/8: Quick Settings ─────────────────────────── -->
    {:else if step === quickSettingsStep}
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
                <div class="qs-toggle" class:on={quickPrefs.cleanup} role="switch" aria-checked={quickPrefs.cleanup} aria-label="AI cleanup" tabindex="0"
                  onclick={() => toggleQuickPref('cleanup')}
                  onkeydown={handleQuickSwitchKeydown}
                  onkeyup={(e) => handleQuickSwitchKeyup(e, 'cleanup')}
                ></div>
              </div>
              <div class="qs-toggle-row">
                <div>
                  <div class="qs-toggle-label">Noise reduction</div>
                  <div class="qs-toggle-desc">Suppress background noise before transcription</div>
                </div>
                <div class="qs-toggle" class:on={quickPrefs.noise} role="switch" aria-checked={quickPrefs.noise} aria-label="Noise reduction" tabindex="0"
                  onclick={() => toggleQuickPref('noise')}
                  onkeydown={handleQuickSwitchKeydown}
                  onkeyup={(e) => handleQuickSwitchKeyup(e, 'noise')}
                ></div>
              </div>
              <div class="qs-toggle-row">
                <div>
                  <div class="qs-toggle-label">Contextual capitalization</div>
                  <div class="qs-toggle-desc">Lowercase the first word when injecting mid-sentence</div>
                </div>
                <div class="qs-toggle" class:on={quickPrefs.caps} role="switch" aria-checked={quickPrefs.caps} aria-label="Contextual capitalization" tabindex="0"
                  onclick={() => toggleQuickPref('caps')}
                  onkeydown={handleQuickSwitchKeydown}
                  onkeyup={(e) => handleQuickSwitchKeyup(e, 'caps')}
                ></div>
              </div>
              <div class="qs-toggle-row">
                <div>
                  <div class="qs-toggle-label">Auto-learn corrections</div>
                  <div class="qs-toggle-desc">Add confirmed corrections to your dictionary automatically</div>
                </div>
                <div class="qs-toggle" class:on={quickPrefs.autoLearn} role="switch" aria-checked={quickPrefs.autoLearn} aria-label="Auto-learn corrections" tabindex="0"
                  onclick={() => toggleQuickPref('autoLearn')}
                  onkeydown={handleQuickSwitchKeydown}
                  onkeyup={(e) => handleQuickSwitchKeyup(e, 'autoLearn')}
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
                  <div class="qs-toggle-desc">Launch Open Flow at login</div>
                </div>
                <div class="qs-toggle" class:on={quickPrefs.autostart} role="switch" aria-checked={quickPrefs.autostart} aria-label="Start on boot" tabindex="0"
                  onclick={() => toggleQuickPref('autostart')}
                  onkeydown={handleQuickSwitchKeydown}
                  onkeyup={(e) => handleQuickSwitchKeyup(e, 'autostart')}
                ></div>
              </div>
              <div class="qs-toggle-row">
                <div>
                  <div class="qs-toggle-label">Mute while recording</div>
                  <div class="qs-toggle-desc">Silence other audio during dictation</div>
                </div>
                <div class="qs-toggle" class:on={quickPrefs.muteAudio} role="switch" aria-checked={quickPrefs.muteAudio} aria-label="Mute while recording" tabindex="0"
                  onclick={() => toggleQuickPref('muteAudio')}
                  onkeydown={handleQuickSwitchKeydown}
                  onkeyup={(e) => handleQuickSwitchKeyup(e, 'muteAudio')}
                ></div>
              </div>
            </div>
          </div>

          <!-- Card 3: Language -->
          <div class="qs-card qs-card-3">
            <div class="qs-card-header">
              <div class="qs-card-icon">
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                  <circle cx="12" cy="12" r="10"/>
                  <path d="M2 12h20M12 2a15 15 0 0 1 0 20M12 2a15 15 0 0 0 0 20"/>
                </svg>
              </div>
              <div>
                <h3 class="qs-card-title">Spoken Language</h3>
                <p class="qs-card-sub">Language expected in your dictation</p>
              </div>
            </div>
            <div class="setup-language-chip-grid">
              {#each onboardingLanguages as language}
                <button
                  class="setup-language-chip"
                  class:active={selectedLanguage === language.code}
                  onclick={() => pickLanguage(language.code)}
                >
                  <span>{language.label}</span>
                  <span>{language.code}</span>
                </button>
              {/each}
            </div>
            <p class="setup-language-note">More languages are available in Settings > General.</p>
          </div>
        </div>

        <div class="step-footer">
          <button class="btn-skip" onclick={skip}>Skip</button>
          <button class="btn-primary" onclick={goNext}>Next</button>
        </div>
      </div>

    <!-- ── Step 8/9: Done ───────────────────────────────────── -->
    {:else if step === doneStep}
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
          Hold <kbd>{hkKey1}</kbd> + <kbd>{hkKey2}</kbd> anywhere to start dictating.
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
          <div class="summary-item">
            <span class="summary-label">Language</span>
            <span class="summary-val">{getTranscriptionLanguageLabel(selectedLanguage)}</span>
          </div>
          <div class="summary-item">
            <span class="summary-label">Theme</span>
            <span class="summary-val">{appearanceModes.find((mode) => mode.id === selectedAppearance)?.name ?? 'System'}</span>
          </div>
        </div>
        <button class="btn-primary btn-lg" onclick={finish}>Start dictating</button>
        <p class="done-note">Everything can be changed in Settings or the Style page.</p>
      </div>
    {/if}
  </div>
</div>

<style>
  /* ── Calibration Step ──────────────────────────────────────────────── */
  .calibration-box {
    background: var(--paper-2);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    padding: 20px 24px;
    display: grid;
    grid-template-columns: 1fr;
    align-items: center;
    justify-items: center;
    text-align: center;
    width: 100%;
    overflow: hidden;
  }

  /* All direct children stack in the same grid cell — overlap during transitions */
  .calibration-box > * {
    grid-column: 1;
    grid-row: 1;
  }

  /* Grid-stacking: lets keyed children overlap so height stays stable during crossfade */
  .cal-label-stack,
  .cal-content-stack {
    display: grid;
    width: 100%;
  }
  .cal-label-stack > *,
  .cal-content-stack > * {
    grid-column: 1;
    grid-row: 1;
  }

  .cal-start-state, .cal-active-state, .cal-result-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    width: 100%;
  }

  .cal-steps-preview {
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 100%;
    text-align: left;
  }

  .cal-step-row {
    display: flex;
    align-items: center;
    gap: 10px;
    animation: calStepIn 0.28s ease both;
  }

  .cal-step-row:nth-child(2) {
    animation-delay: 0.07s;
  }

  @keyframes calStepIn {
    from { opacity: 0; transform: translateY(6px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  @media (prefers-reduced-motion: reduce) {
    .cal-step-row,
    .cal-success-icon,
    .cal-warning-icon { animation: none; }
  }

  .cal-phase-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    width: 100%;
  }

  .cal-step-num {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent-ink);
    font-size: 11px;
    font-weight: 600;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .cal-step-text {
    font-size: 13px;
    color: var(--ink-soft);
  }

  .cal-phase-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
  }

  .cal-phase-label {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--ink-faint);
  }

  .cal-timer-ring {
    width: 44px;
    height: 44px;
    aspect-ratio: 1;
    flex-shrink: 0;
    border-radius: 50%;
    border: 3px solid var(--accent);
    display: flex;
    align-items: center;
    justify-content: center;
    animation: pulseCal 1.5s infinite;
  }

  @keyframes pulseCal {
    0%, 100% { border-color: var(--accent); transform: scale(1); }
    50% { border-color: color-mix(in srgb, var(--accent) 50%, transparent); transform: scale(1.03); }
  }

  .cal-countdown {
    font-size: 15px;
    font-weight: 600;
    color: var(--accent-ink);
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    text-align: center;
  }

  .cal-prompt {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--ink-faint);
    margin: 0;
  }

  .cal-phrase {
    font-family: var(--serif);
    font-size: 18px;
    font-weight: 500;
    font-style: italic;
    color: var(--ink-strong);
    margin: 0;
    line-height: 1.4;
  }

  .cal-meter-container {
    width: 100%;
    max-width: 280px;
    margin-top: 8px;
  }

  .cal-meter-track {
    width: 100%;
    height: 6px;
    background: var(--line-strong);
    border-radius: 999px;
    overflow: hidden;
    position: relative;
  }

  .cal-meter-fill {
    height: 100%;
    background: linear-gradient(90deg, var(--accent) 0%, color-mix(in srgb, var(--accent) 70%, white 30%) 100%);
    border-radius: 999px;
    transition: width 0.05s ease-out;
  }

  .cal-success-icon {
    width: 56px;
    height: 56px;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent);
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 4px;
    animation: iconPop 0.38s 0.18s cubic-bezier(0.16, 1, 0.3, 1) both;
  }

  .cal-warning-icon {
    width: 56px;
    height: 56px;
    border-radius: 50%;
    background: var(--warning-bg);
    color: var(--warning);
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 4px;
    animation: iconShake 0.42s 0.18s cubic-bezier(0.16, 1, 0.3, 1) both;
  }

  @keyframes iconPop {
    0%   { transform: scale(0.5); opacity: 0; }
    60%  { transform: scale(1.08); opacity: 1; }
    100% { transform: scale(1); opacity: 1; }
  }

  @keyframes iconShake {
    0%   { transform: translateY(-10px) scale(0.8); opacity: 0; }
    45%  { transform: translateY(4px) scale(1.02); opacity: 1; }
    70%  { transform: translateY(-2px) scale(1); }
    100% { transform: translateY(0) scale(1); opacity: 1; }
  }

  .cal-result-title {
    font-family: var(--serif);
    font-size: 18px;
    font-weight: 500;
    color: var(--ink-strong);
    margin: 0;
  }

  .cal-result-desc {
    font-size: 13px;
    color: var(--ink-soft);
    line-height: 1.5;
    margin: 0;
    max-width: 360px;
  }

  .cal-result-desc strong {
    color: var(--accent);
    font-family: var(--mono);
    font-size: 13.5px;
  }

  .cal-recalibrate-btn {
    padding: 7px 18px;
    border-radius: var(--r-md);
    font-size: 13px;
    font-weight: 500;
    font-family: var(--sans);
    border: 1px solid var(--line-strong);
    color: var(--ink-mute);
    background: transparent;
    cursor: pointer;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }

  .cal-recalibrate-btn:hover {
    background: var(--paper-2);
    color: var(--ink-strong);
    border-color: var(--accent);
  }

  .cal-cancel-btn {
    margin-top: 4px;
    padding: 6px 16px;
    border-radius: var(--r-md);
    font-size: 13px;
    font-weight: 500;
    font-family: var(--sans);
    color: var(--ink-mute);
    cursor: pointer;
    background: transparent;
    border: 1px solid var(--line-strong);
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }
  .cal-cancel-btn:hover {
    color: var(--ink-strong);
    background: var(--paper-3);
    border-color: var(--line-strong);
  }

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
    min-height: 0;
    display: flex;
    /* align-items: safe center is not supported on macOS 11/12 WebKit — use
       margin: auto on the .step child instead for vertical centering that
       gracefully degrades to top-aligned when the step is taller than the
       viewport, keeping the footer and Next button reachable. */
    align-items: flex-start;
    justify-content: center;
    width: 100%;
    padding: 24px 0 32px;
    overflow-y: auto;
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
    margin: auto;
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

  .keychain-note {
    padding: 11px 12px;
    border-radius: var(--r-sm);
    border: 1px solid color-mix(in srgb, var(--accent) 25%, var(--line));
    background: color-mix(in srgb, var(--accent-soft) 42%, var(--paper-2));
    color: var(--ink-soft);
    font-size: 12.5px;
    line-height: 1.45;
  }

  .keychain-note strong {
    color: var(--ink-strong);
    font-weight: 600;
  }

  .keychain-note span {
    color: var(--accent-ink);
    font-weight: 600;
  }

  @keyframes glow-pulse {
    0%   { box-shadow: 0 0 0 0   color-mix(in srgb, var(--accent) 45%, transparent); }
    55%  { box-shadow: 0 0 0 7px color-mix(in srgb, var(--accent) 0%,  transparent); }
    100% { box-shadow: 0 0 0 0   transparent; }
  }

  .btn-primary--glow { animation: glow-pulse 0.85s ease-out forwards; }

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

  /* ── Appearance ──────────────────────────────────────────────────── */
  .appearance-step {
    max-width: 640px;
    gap: 16px;
  }

  .appearance-mode-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
  }

  .appearance-mode-card {
    background: var(--bg-elev);
    border: 1.5px solid var(--line);
    border-radius: var(--r-md);
    padding: 12px;
    text-align: left;
    cursor: pointer;
    display: grid;
    gap: 6px;
    transition: border-color 0.15s, background 0.15s;
  }

  .appearance-mode-card:hover { border-color: var(--line-strong); }
  .appearance-mode-card.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .appearance-mode-title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .appearance-mode-name {
    margin: 0;
    font-size: 13px;
    font-weight: 500;
    color: var(--ink-strong);
  }

  .appearance-mode-desc {
    margin: 0;
    font-size: 11.5px;
    color: var(--ink-mute);
    line-height: 1.35;
  }

  .appearance-mode-radio {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid var(--line-strong);
    position: relative;
    flex-shrink: 0;
  }

  .appearance-mode-radio.checked { border-color: var(--accent); }
  .appearance-mode-radio.checked::after {
    content: '';
    position: absolute;
    inset: 2px;
    border-radius: 50%;
    background: var(--accent);
  }

  @media (max-width: 960px) {
    .appearance-mode-grid {
      grid-template-columns: 1fr;
    }
  }
  /* Done step */
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
  .qs-step { max-width: 920px; }

  .qs-cards {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
    align-items: stretch;
  }

  .qs-card {
    background: var(--bg-elev);
    border: 1.5px solid var(--line);
    border-radius: var(--r-md);
    padding: 14px 14px;
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

  .qs-cards.ready .qs-card-3 {
    opacity: 1;
    transform: none;
    transition-delay: 0.2s;
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

  .setup-language-chip-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }

  .setup-language-chip {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    border: 1px solid var(--line-strong);
    background: var(--paper);
    border-radius: 8px;
    padding: 7px 9px;
    color: var(--ink-strong);
    font-family: var(--sans);
    font-size: 12px;
    text-align: left;
  }

  .setup-language-chip span:last-child {
    color: var(--ink-faint);
    font-family: var(--mono);
    font-size: 10.5px;
    text-transform: uppercase;
  }

  .setup-language-chip.active {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--ink);
    font-weight: 500;
  }

  .setup-language-note {
    margin: 10px 0 0;
    color: var(--ink-mute);
    font-size: 11.5px;
    line-height: 1.4;
  }
</style>
