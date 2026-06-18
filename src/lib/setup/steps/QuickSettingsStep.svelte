<script lang="ts">
  import { onMount } from 'svelte';
  import { transcriptionLanguages, type TranscriptionLanguageCode } from '../../transcriptionLanguages';

  type QuickPrefs = {
    cleanup: boolean;
    noise: boolean;
    caps: boolean;
    autoSpacing: boolean;
    appContextHint: boolean;
    autoLearn: boolean;
    autostart: boolean;
    muteAudio: boolean;
  };
  type QuickPrefKey = keyof QuickPrefs;

  let {
    quickPrefs = $bindable(),
    language = $bindable(),
  }: { quickPrefs: QuickPrefs; language: TranscriptionLanguageCode } = $props();

  const onboardingLanguageSet = new Set<TranscriptionLanguageCode>(['en', 'es', 'fr', 'de', 'pt', 'zh']);
  const onboardingLanguages = transcriptionLanguages.filter((option) => onboardingLanguageSet.has(option.code));

  let ready = $state(false);
  onMount(() => {
    const t = setTimeout(() => { ready = true; }, 60);
    return () => clearTimeout(t);
  });

  function toggle(key: QuickPrefKey) {
    quickPrefs = { ...quickPrefs, [key]: !quickPrefs[key] };
  }
  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' || event.key === ' ') event.preventDefault();
  }
  function handleKeyup(event: KeyboardEvent, key: QuickPrefKey) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      toggle(key);
    }
  }
</script>

<div class="step qs-step">
  <div class="qs-cards" class:ready>
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
            <div class="qs-toggle-desc">Refine transcriptions with an LLM</div>
          </div>
          <div class="qs-toggle" class:on={quickPrefs.cleanup} role="switch" aria-checked={quickPrefs.cleanup} aria-label="AI cleanup" tabindex="0"
            onclick={() => toggle('cleanup')} onkeydown={handleKeydown} onkeyup={(e) => handleKeyup(e, 'cleanup')}
          ></div>
        </div>
        <div class="qs-toggle-row">
          <div>
            <div class="qs-toggle-label">Noise reduction</div>
            <div class="qs-toggle-desc">Suppress background noise</div>
          </div>
          <div class="qs-toggle" class:on={quickPrefs.noise} role="switch" aria-checked={quickPrefs.noise} aria-label="Noise reduction" tabindex="0"
            onclick={() => toggle('noise')} onkeydown={handleKeydown} onkeyup={(e) => handleKeyup(e, 'noise')}
          ></div>
        </div>
        <div class="qs-toggle-row">
          <div>
            <div class="qs-toggle-label">Contextual capitalization</div>
            <div class="qs-toggle-desc">Lowercase when joining mid-sentence</div>
          </div>
          <div class="qs-toggle" class:on={quickPrefs.caps} role="switch" aria-checked={quickPrefs.caps} aria-label="Contextual capitalization" tabindex="0"
            onclick={() => toggle('caps')} onkeydown={handleKeydown} onkeyup={(e) => handleKeyup(e, 'caps')}
          ></div>
        </div>
        <div class="qs-toggle-row">
          <div>
            <div class="qs-toggle-label">Automatic spacing</div>
            <div class="qs-toggle-desc">Add a space when joining after existing text</div>
          </div>
          <div class="qs-toggle" class:on={quickPrefs.autoSpacing} role="switch" aria-checked={quickPrefs.autoSpacing} aria-label="Automatic spacing" tabindex="0"
            onclick={() => toggle('autoSpacing')} onkeydown={handleKeydown} onkeyup={(e) => handleKeyup(e, 'autoSpacing')}
          ></div>
        </div>
        <div class="qs-toggle-row">
          <div>
            <div class="qs-toggle-label">App context hint</div>
            <div class="qs-toggle-desc">Tailor formatting to the active app</div>
          </div>
          <div class="qs-toggle" class:on={quickPrefs.appContextHint} role="switch" aria-checked={quickPrefs.appContextHint} aria-label="App context hint" tabindex="0"
            onclick={() => toggle('appContextHint')} onkeydown={handleKeydown} onkeyup={(e) => handleKeyup(e, 'appContextHint')}
          ></div>
        </div>
        <div class="qs-toggle-row">
          <div>
            <div class="qs-toggle-label">Auto-learn corrections</div>
            <div class="qs-toggle-desc">Save confirmed corrections automatically</div>
          </div>
          <div class="qs-toggle" class:on={quickPrefs.autoLearn} role="switch" aria-checked={quickPrefs.autoLearn} aria-label="Auto-learn corrections" tabindex="0"
            onclick={() => toggle('autoLearn')} onkeydown={handleKeydown} onkeyup={(e) => handleKeyup(e, 'autoLearn')}
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
            <div class="qs-toggle-desc">Launch Verenu at login</div>
          </div>
          <div class="qs-toggle" class:on={quickPrefs.autostart} role="switch" aria-checked={quickPrefs.autostart} aria-label="Start on boot" tabindex="0"
            onclick={() => toggle('autostart')} onkeydown={handleKeydown} onkeyup={(e) => handleKeyup(e, 'autostart')}
          ></div>
        </div>
        <div class="qs-toggle-row">
          <div>
            <div class="qs-toggle-label">Mute while recording</div>
            <div class="qs-toggle-desc">Silence other audio during dictation</div>
          </div>
          <div class="qs-toggle" class:on={quickPrefs.muteAudio} role="switch" aria-checked={quickPrefs.muteAudio} aria-label="Mute while recording" tabindex="0"
            onclick={() => toggle('muteAudio')} onkeydown={handleKeydown} onkeyup={(e) => handleKeyup(e, 'muteAudio')}
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
        {#each onboardingLanguages as lang}
          <button
            class="setup-language-chip"
            class:active={language === lang.code}
            onclick={() => { language = lang.code; }}
          >
            <span>{lang.label}</span>
            <span>{lang.code}</span>
          </button>
        {/each}
      </div>
      <p class="setup-language-note">More languages are available in Settings &gt; General.</p>
    </div>
  </div>
</div>

<style>
  .qs-step { max-width: 920px; gap: 18px; }

  .qs-cards { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 8px; align-items: stretch; }

  .qs-card {
    background: var(--bg-elev);
    border: 1.5px solid var(--line);
    border-radius: var(--r-md);
    padding: 8px 10px;
    opacity: 0;
    transform: translateY(12px);
    transition: opacity 0.3s ease, transform 0.3s ease, border-color 0.15s;
  }

  .qs-cards.ready .qs-card-1 { opacity: 1; transform: none; }
  .qs-cards.ready .qs-card-2 { opacity: 1; transform: none; transition-delay: 0.1s; }
  .qs-cards.ready .qs-card-3 { opacity: 1; transform: none; transition-delay: 0.2s; }

  .qs-card-header { display: flex; align-items: center; gap: 10px; margin-bottom: 5px; }

  .qs-card-icon {
    width: 28px;
    height: 28px;
    border-radius: var(--r-sm);
    background: var(--accent-soft);
    color: var(--accent-ink);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .qs-card-title { font-family: var(--serif); font-size: 15px; font-weight: 500; color: var(--ink-strong); margin: 0 0 2px; line-height: 1.2; }
  .qs-card-sub { font-size: 11.5px; color: var(--ink-mute); margin: 0; }

  .qs-toggle-list { display: flex; flex-direction: column; }

  .qs-toggle-row { display: flex; align-items: center; justify-content: space-between; padding: 4px 0; border-top: 1px solid var(--line); gap: 12px; }

  .qs-toggle-label { font-size: 13px; font-weight: 500; color: var(--ink-strong); margin-bottom: 1px; }
  .qs-toggle-desc { font-size: 11.5px; color: var(--ink-mute); line-height: 1.3; }

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

  .setup-language-chip-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; }

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

  .setup-language-chip span:last-child { color: var(--ink-faint); font-family: var(--mono); font-size: 10.5px; text-transform: uppercase; }

  .setup-language-chip.active { border-color: var(--accent); background: var(--accent-soft); color: var(--ink); font-weight: 500; }

  .setup-language-note { margin: 10px 0 0; color: var(--ink-mute); font-size: 11.5px; line-height: 1.4; }
</style>
