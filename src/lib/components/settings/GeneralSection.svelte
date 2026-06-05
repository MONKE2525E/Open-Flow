<script lang="ts">
  import { emit, invoke } from '../../tauri';
  import { fly, fade } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { isMac } from '../../platform';
  import Toggle from '../Toggle.svelte';
  import { appStore } from '../../stores';
  import { saveSetting, type AppearanceMode } from '../../settings';
  import { MOTION_MS, MOTION_PX, motionMs, motionPx, animateWidth } from '../../motion';
  import {
    getTranscriptionLanguageLabel,
    transcriptionLanguages,
    type TranscriptionLanguageCode,
  } from '../../transcriptionLanguages';
  import { getAudioCalibrationCopy } from '../../calibrationCopy';

  let selectedLanguage = $state<TranscriptionLanguageCode>('en');
  let languageDropdownOpen = $state(false);
  let languageTouched = false;
  let microphones = $state<string[]>([]);
  let selectedMic = $state('');
  let micDropdownOpen = $state(false);
  const audioCopy = $derived(getAudioCalibrationCopy(selectedLanguage));
  let muteAudio = $state(false);
  let autostart = $state(false);
  let cleanup = $state(true);
  let contextualCaps = $state(true);
  let autoSpacing = $state(true);
  let hotkey = $state(['ControlLeft', 'MetaLeft']);
  let recordingHotkey = $state(false);
  let capturedKeys = $state<string[]>([]);
  let hotkeyState = $state<'idle' | 'armed' | 'first' | 'saving' | 'success' | 'error'>('idle');
  const HOTKEY_SUCCESS_MS = 700;
  const HOTKEY_ERROR_MS   = 900;
  const LANGUAGE_MENU_ID = 'spoken-language-menu';
  const MIC_MENU_ID = 'microphone-menu';
  let keybindEl: HTMLElement | null = $state(null);
  let capturedWidth = 0;
  let segmentEl: HTMLElement | null = $state(null);
  let indicatorStyle = $state('');

  $effect(() => {
    const idx = appearanceOptions.findIndex(o => o.id === appStore.appearanceMode);
    if (!segmentEl) return;
    const btn = segmentEl.querySelectorAll<HTMLElement>('.appearance-option')[idx];
    if (!btn) return;
    indicatorStyle = `left:${btn.offsetLeft}px;width:${btn.offsetWidth}px`;
  });

  let buttonText = $derived(
    recordingHotkey
      ? capturedKeys[0] === '__bad__'
        ? 'Must be Alt/Ctrl/Shift/Win'
        : capturedKeys.length === 0
          ? 'Press Alt/Ctrl/Shift/Win...'
          : 'Press 2nd key...'
      : `${formatKey(hotkey[0])} + ${formatKey(hotkey[1])}`
  );

  $effect.pre(() => {
    void buttonText;
    if (keybindEl) capturedWidth = keybindEl.getBoundingClientRect().width;
  });

  $effect(() => {
    void buttonText;
    if (!keybindEl || capturedWidth === 0) return;
    const el = keybindEl;
    const prevW = capturedWidth;
    el.style.transition = 'none';
    el.style.width = 'max-content';
    const newW = Math.ceil(el.getBoundingClientRect().width);
    el.style.width = `${prevW}px`;
    void el.offsetWidth;
    el.style.transition = '';
    el.style.width = `${newW}px`;
  });

  const appearanceOptions: { id: AppearanceMode; label: string }[] = [
    { id: 'system', label: 'System' },
    { id: 'light', label: 'Light' },
    { id: 'dark', label: 'Dark' },
  ];

  const MODIFIER_CODES = new Set([
    'ShiftLeft', 'ShiftRight', 'ControlLeft', 'ControlRight',
    'AltLeft', 'AltRight', 'MetaLeft', 'MetaRight',
  ]);

  async function loadSettings() {
    const results = await Promise.allSettled([
      invoke<boolean | null>('get_setting', { key: 'mute_audio' }),
      invoke<boolean | null>('get_setting', { key: 'autostart_enabled' }),
      invoke<string[] | null>('get_setting', { key: 'hotkey' }),
      invoke<AppearanceMode | null>('get_setting', { key: 'appearance_mode' }),
      invoke<TranscriptionLanguageCode | null>('get_setting', { key: 'transcription_language' }),
      invoke<boolean | null>('get_setting', { key: 'cleanup_enabled' }),
      invoke<boolean | null>('get_setting', { key: 'contextual_caps_enabled' }),
      invoke<boolean | null>('get_setting', { key: 'auto_spacing_enabled' }),
      invoke<string[]>('get_microphones'),
      invoke<string | null>('get_setting', { key: 'microphone_device' }),
    ]);

    const val = <T>(i: number, fallback: T): T =>
      results[i].status === 'fulfilled' ? (results[i] as PromiseFulfilledResult<T>).value ?? fallback : fallback;

    muteAudio = val<boolean | null>(0, null) ?? false;
    autostart = val<boolean | null>(1, null) ?? false;
    cleanup = val<boolean | null>(5, null) ?? true;
    contextualCaps = val<boolean | null>(6, null) ?? true;
    autoSpacing = val<boolean | null>(7, null) ?? true;

    const hk = val<string[] | null>(2, null);
    if (hk && hk.length === 2) hotkey = hk;

    const appearance = val<AppearanceMode | null>(3, null);
    if (appearance === 'system' || appearance === 'light' || appearance === 'dark') {
      appStore.appearanceMode = appearance;
    }

    const language = val<TranscriptionLanguageCode | null>(4, null);
    if (!languageTouched && language && transcriptionLanguages.some((option) => option.code === language)) {
      selectedLanguage = language;
    }

    microphones = val<string[]>(8, []);
    selectedMic = val<string | null>(9, null) ?? '';

    results.forEach((r, i) => {
      if (r.status === 'rejected') console.error(`GeneralSection: invoke[${i}] failed:`, r.reason);
    });
  }

  function handleWindowClick(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (micDropdownOpen && !target.closest('.mic-dropdown')) micDropdownOpen = false;
    if (languageDropdownOpen && !target.closest('.language-dropdown')) languageDropdownOpen = false;
  }

  async function saveMic(name: string) {
    selectedMic = name;
    micDropdownOpen = false;
    try {
      await saveSetting('microphone_device', name || null);
    } catch (err) {
      console.error('saveMic failed:', err);
    }
  }

  async function saveLanguage(code: TranscriptionLanguageCode) {
    languageTouched = true;
    selectedLanguage = code;
    languageDropdownOpen = false;
    try {
      await saveSetting('transcription_language', code);
    } catch (err) {
      console.error('save transcription_language failed:', err);
    }
  }

  async function handleMuteAudio(value: boolean) {
    muteAudio = value;
    try {
      await saveSetting('mute_audio', value);
    } catch (err) {
      muteAudio = !value;
      console.error('save mute_audio failed:', err);
    }
  }

  async function handleAutostart(value: boolean) {
    autostart = value;
    try {
      await invoke('set_autostart', { enabled: value });
    } catch (err) {
      autostart = !value;
      console.error('set_autostart failed:', err);
    }
  }

  async function handleCleanup(value: boolean) {
    cleanup = value;
    try {
      await saveSetting('cleanup_enabled', value);
    } catch (err) {
      cleanup = !value;
      console.error('save cleanup_enabled failed:', err);
    }
  }

  async function handleContextualCaps(value: boolean) {
    contextualCaps = value;
    try {
      await saveSetting('contextual_caps_enabled', value);
    } catch (err) {
      contextualCaps = !value;
      console.error('save contextual_caps_enabled failed:', err);
    }
  }

  async function handleAutoSpacing(value: boolean) {
    autoSpacing = value;
    try {
      await saveSetting('auto_spacing_enabled', value);
    } catch (err) {
      autoSpacing = !value;
      console.error('save auto_spacing_enabled failed:', err);
    }
  }


  async function handleAppearance(mode: AppearanceMode) {
    const previous = appStore.appearanceMode;
    appStore.appearanceMode = mode;
    try {
      await saveSetting('appearance_mode', mode);
    } catch (err) {
      appStore.appearanceMode = previous;
      console.error('save appearance_mode failed:', err);
    }
  }

  function formatKey(code: string) {
    const labels: Record<string, string> = {
      ControlLeft: 'Ctrl',
      ControlRight: 'Ctrl',
      MetaLeft: 'Windows',
      MetaRight: 'Windows',
      AltLeft: 'Alt',
      AltRight: 'Alt',
      Space: 'Space',
    };
    return labels[code] ?? code.replace('Left', '').replace('Right', '').replace('Key', '').replace('Digit', '');
  }

  function startRecordingHotkey(e: MouseEvent | KeyboardEvent) {
    e.stopPropagation();
    if (recordingHotkey) return;
    recordingHotkey = true;
    hotkeyState = 'armed';
    capturedKeys = [];
    window.addEventListener('keydown', handleHotkeyKeydown, { capture: true });
    window.addEventListener('keyup', handleHotkeyKeyup, { capture: true });
    window.addEventListener('mousedown', cancelRecordingHotkey, { capture: true });
  }

  function cancelRecordingHotkey(e?: MouseEvent | KeyboardEvent) {
    if (e && (e.target as HTMLElement).closest('.keybind-btn')) return;
    if (recordingHotkey) {
      window.removeEventListener('keydown', handleHotkeyKeydown, { capture: true });
      window.removeEventListener('keyup', handleHotkeyKeyup, { capture: true });
      window.removeEventListener('mousedown', cancelRecordingHotkey, { capture: true });
      recordingHotkey = false;
      hotkeyState = 'idle';
      capturedKeys = [];
    }
  }

  function handleHotkeyKeydown(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (e.repeat) return;
    if (capturedKeys.length === 0) {
      if (e.code === 'Escape') { cancelRecordingHotkey(); return; }
      if (!MODIFIER_CODES.has(e.code)) {
        capturedKeys = ['__bad__'];
        setTimeout(() => { capturedKeys = []; }, 800);
        return;
      }
      capturedKeys = [e.code];
      hotkeyState = 'first';
    } else if (capturedKeys.length === 1 && e.code !== capturedKeys[0]) {
      capturedKeys = [...capturedKeys, e.code];
      hotkeyState = 'saving';
      finishRecordingHotkey();
    }
  }

  function handleHotkeyKeyup(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();
  }

  async function finishRecordingHotkey() {
    window.removeEventListener('keydown', handleHotkeyKeydown, { capture: true });
    window.removeEventListener('keyup', handleHotkeyKeyup, { capture: true });
    window.removeEventListener('mousedown', cancelRecordingHotkey, { capture: true });
    recordingHotkey = false;
    if (capturedKeys.length === 2) {
      try {
        let available = true;
        try {
          available = await invoke<boolean>('check_hotkey', { key1: capturedKeys[0], key2: capturedKeys[1] });
        } catch (e) {
          console.warn('check_hotkey failed (likely running in browser dev mode)', e);
        }
        if (!available) {
          hotkeyState = 'error';
          await emit('open-flow:error', 'Hotkey may already be in use by another application');
          setTimeout(() => { hotkeyState = 'idle'; }, HOTKEY_ERROR_MS);
          return;
        }
        await invoke('save_hotkey', { key1: capturedKeys[0], key2: capturedKeys[1] });
        hotkey = capturedKeys;
        hotkeyState = 'success';
        setTimeout(() => { hotkeyState = 'idle'; }, HOTKEY_SUCCESS_MS);
      } catch (e) {
        console.error('Failed to save hotkey', e);
        hotkeyState = 'error';
        await emit('open-flow:error', 'Failed to save hotkey - key may not be recognized');
        setTimeout(() => { hotkeyState = 'idle'; }, HOTKEY_ERROR_MS);
      }
    }
  }

  loadSettings();
</script>
<svelte:window onclick={handleWindowClick} />

<h2 class="settings-h">General</h2>
<div class="setting-row">
  <div><div class="label">Hotkey</div><div class="desc">Hold to record, release to transcribe</div></div>
  <button
    bind:this={keybindEl}
    class="badge key-badge keybind-btn"
    onclick={startRecordingHotkey}
    class:recording={recordingHotkey}
    class:armed={hotkeyState === 'armed'}
    class:first={hotkeyState === 'first'}
    class:saving={hotkeyState === 'saving'}
    class:success={hotkeyState === 'success'}
    class:error={hotkeyState === 'error'}
  >
    {#key buttonText}
      <span in:fade={{ duration: motionMs(MOTION_MS.fast) }}>{buttonText}</span>
    {/key}
  </button>
</div>
<div class="setting-row">
  <div><div class="label">Spoken Language</div><div class="desc">Tells transcription what language to expect</div></div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="language-dropdown" onkeydown={(e) => { if (e.key === 'Escape' && languageDropdownOpen) { languageDropdownOpen = false; e.stopPropagation(); } }}>
    <button
      class="btn-ghost language-btn"
      use:animateWidth={{ text: getTranscriptionLanguageLabel(selectedLanguage) }}
      onclick={() => (languageDropdownOpen = !languageDropdownOpen)}
      aria-haspopup="true"
      aria-expanded={languageDropdownOpen}
      aria-controls={LANGUAGE_MENU_ID}
      aria-label="Spoken language"
    >
      <span>{getTranscriptionLanguageLabel(selectedLanguage)}</span>
      <span class="language-code">{selectedLanguage}</span>
      <svg class:open={languageDropdownOpen} width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="m6 9 6 6 6-6"/>
      </svg>
    </button>
    {#if languageDropdownOpen}
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <div
        id={LANGUAGE_MENU_ID}
        class="language-menu scroll-styled scroll-thumb-elev"
        aria-label="Spoken language options"
        onclick={(e) => e.stopPropagation()}
        in:fly={{ y: -motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.panel), easing: expoOut }}
        out:fade={{ duration: motionMs(MOTION_MS.fast) }}
      >
        {#each transcriptionLanguages as language}
          <button
            class="language-item"
            class:active={selectedLanguage === language.code}
            onclick={() => saveLanguage(language.code)}
          >
            <span>{language.label}</span>
            <span>{language.code}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
</div>
<div class="setting-row">
  <div>
    <div class="label">{audioCopy.inputDeviceLabel}</div>
    <div class="desc">{audioCopy.inputDeviceDescription}</div>
  </div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="mic-dropdown" onkeydown={(e) => { if (e.key === 'Escape' && micDropdownOpen) { micDropdownOpen = false; e.stopPropagation(); } }}>
    <button
      class="btn-ghost mic-btn"
      use:animateWidth={{ text: selectedMic || audioCopy.defaultDevice, max: 180 }}
      onclick={() => (micDropdownOpen = !micDropdownOpen)}
      aria-haspopup="true"
      aria-expanded={micDropdownOpen}
      aria-controls={MIC_MENU_ID}
      aria-label="Microphone device"
    >
      <span class="mic-btn-label">{selectedMic || audioCopy.defaultDevice}</span>
      <svg class:open={micDropdownOpen} width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="m6 9 6 6 6-6"/>
      </svg>
    </button>
    {#if micDropdownOpen}
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <div
        id={MIC_MENU_ID}
        class="mic-menu scroll-styled scroll-thumb-elev"
        aria-label="Microphone device options"
        onclick={(e) => e.stopPropagation()}
        in:fly={{ y: -motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.panel), easing: expoOut }}
        out:fade={{ duration: motionMs(MOTION_MS.fast) }}
      >
        <button class="mic-item" class:active={!selectedMic} onclick={() => saveMic('')}>{audioCopy.defaultDevice}</button>
        {#each microphones as m}
          <button class="mic-item" class:active={selectedMic === m} onclick={() => saveMic(m)}>{m}</button>
        {/each}
        {#if microphones.length === 0}
          <div class="mic-empty">{audioCopy.noDevicesFound}</div>
        {/if}
      </div>
    {/if}
  </div>
</div>
<div class="setting-row">
  <div><div class="label">Mute PC Audio</div><div class="desc">Mutes Windows volume while dictating to prevent audio interference</div></div>
  <Toggle checked={muteAudio} onchange={handleMuteAudio} label="Mute PC audio" />
</div>
<div class="setting-row">
  <div><div class="label">Appearance</div><div class="desc">Follow Windows or force a specific theme</div></div>
  <div class="appearance-segment" role="radiogroup" aria-label="Appearance" bind:this={segmentEl}>
    {#if indicatorStyle}
      <div class="appearance-indicator" style={indicatorStyle} aria-hidden="true"></div>
    {/if}
    {#each appearanceOptions as option}
      <button
        class="appearance-option"
        class:active={appStore.appearanceMode === option.id}
        role="radio"
        aria-checked={appStore.appearanceMode === option.id}
        onclick={() => handleAppearance(option.id)}
      >{option.label}</button>
    {/each}
  </div>
</div>
<div class="setting-row">
  <div><div class="label">Start on Boot</div><div class="desc">Launch Open Flow when Windows starts</div></div>
  <Toggle checked={autostart} onchange={handleAutostart} label="Start on boot" />
</div>
<div class="setting-row">
  <div><div class="label">Auto-cleanup</div><div class="desc">Run LLM cleanup on every transcription</div></div>
  <Toggle checked={cleanup} onchange={handleCleanup} label="Auto-cleanup" />
</div>
<div class="setting-row">
  <div><div class="label">Contextual capitalization</div><div class="desc">Lowercases the first word when injecting mid-sentence</div></div>
  <Toggle checked={contextualCaps} onchange={handleContextualCaps} label="Contextual capitalization" />
</div>
<div class="setting-row">
  <div><div class="label">Automatic spacing</div><div class="desc">Adds a space before injected text when the cursor is after existing text</div></div>
  <Toggle checked={autoSpacing} onchange={handleAutoSpacing} label="Automatic spacing" />
</div>
{#if isMac}
  <div class="setting-row setting-row-note">
    <div>
      <div class="label">macOS behavior</div>
      <div class="desc">Supported editors use caret-local context. If an editor is unreadable or unsupported, Open Flow degrades conservatively and pastes without guessing capitalization or leading spaces.</div>
    </div>
  </div>
{/if}

<style>
  .keybind-btn {
    cursor: pointer;
    border: 1px solid transparent;
    transition:
      width 240ms cubic-bezier(0.22, 1, 0.36, 1),
      background 0.18s cubic-bezier(0.22, 1, 0.36, 1),
      color 0.18s cubic-bezier(0.22, 1, 0.36, 1),
      transform 0.18s cubic-bezier(0.22, 1, 0.36, 1),
      box-shadow 0.18s cubic-bezier(0.22, 1, 0.36, 1),
      border-color 0.18s cubic-bezier(0.22, 1, 0.36, 1),
      opacity 0.18s cubic-bezier(0.22, 1, 0.36, 1);
    user-select: none;
    transform-origin: center;
    white-space: nowrap;
    overflow: hidden;
  }
  .keybind-btn:hover { background: var(--control-hover); }
  .keybind-btn.recording { background: var(--accent); color: var(--on-accent); animation: pulse 1.5s infinite; }
  .keybind-btn.armed { transform: scale(1.02); }
  .keybind-btn.first { transform: scale(1.03); box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent) 40%, transparent); }
  .keybind-btn.saving { opacity: 0.9; }
  .keybind-btn.success { background: color-mix(in srgb, var(--accent) 82%, white 18%); color: var(--on-accent); transform: scale(1.03); }
  .keybind-btn.error { background: var(--danger-bg); color: var(--danger); border-color: var(--danger-line); animation: none; }
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.7; } }
  .mic-dropdown { position: relative; flex-shrink: 0; }
  .mic-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 12px;
    border-radius: var(--r-md);
    background: var(--paper-2);
    border: 1px solid var(--line);
    color: var(--ink);
    font-size: 13px;
    font-weight: 500;
    max-width: 180px;
  }
  .mic-btn svg { transition: transform 0.2s; }
  .mic-btn svg.open { transform: rotate(180deg); }
  .language-btn svg { transition: transform 150ms; }
  .language-btn svg.open { transform: rotate(180deg); }
  .mic-btn-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    text-align: left;
  }
  .mic-menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    width: 220px;
    max-height: 240px;
    overflow-y: auto;
    background: var(--bg-elev);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    box-shadow: 0 4px 16px var(--shadow-md);
    z-index: 10;
    padding: 4px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .mic-item {
    width: 100%;
    text-align: left;
    padding: 6px 10px;
    border-radius: var(--r-sm);
    font-size: 12.5px;
    color: var(--ink-soft);
    background: transparent;
    border: none;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mic-item:hover { background: var(--paper-2); color: var(--ink); }
  .mic-item.active { background: var(--accent-soft); color: var(--accent-ink); font-weight: 500; }
  .mic-empty { padding: 8px 10px; font-size: 12px; color: var(--ink-mute); text-align: center; }
  .language-dropdown { position: relative; flex-shrink: 0; }
  .language-btn { max-width: 210px; }
  .language-btn span:first-child { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 140px; }
  .language-code {
    font-family: var(--mono);
    font-size: 10.5px;
    color: var(--ink-faint);
    text-transform: uppercase;
  }
  .language-menu {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    box-shadow: var(--shadow-popover);
    min-width: 220px;
    max-width: 280px;
    max-height: 260px;
    overflow-y: auto;
    z-index: 10;
  }
  .language-item {
    display: flex;
    justify-content: space-between;
    gap: 12px;
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
  .language-item span:last-child {
    color: var(--ink-faint);
    font-family: var(--mono);
    font-size: 10.5px;
    text-transform: uppercase;
  }
  .language-item:last-child { border-bottom: none; }
  .language-item:hover { background: var(--paper); }
  .language-item.active { background: var(--accent-soft); color: var(--ink); font-weight: 500; }
  .appearance-segment {
    position: relative;
    display: inline-flex;
    align-items: center;
    padding: 2px;
    background: var(--paper);
    border: 1px solid var(--line);
    border-radius: 7px;
    gap: 2px;
  }
  .appearance-indicator {
    position: absolute;
    top: 2px;
    height: calc(100% - 4px);
    background: var(--bg-elev);
    border-radius: 5px;
    box-shadow: 0 0 0 1px var(--line-soft);
    pointer-events: none;
    transition: left 180ms cubic-bezier(0.22, 1, 0.36, 1), width 180ms cubic-bezier(0.22, 1, 0.36, 1);
  }
  .appearance-option {
    position: relative;
    z-index: 1;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--ink-mute);
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 500;
    padding: 4px 9px;
    cursor: pointer;
    transition: color 0.12s;
  }
  .appearance-option:hover { color: var(--ink-strong); }
  .appearance-option.active { color: var(--ink); }
</style>
