<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { tick } from 'svelte';
  import Toggle from '../Toggle.svelte';
  import { appearanceMode } from '../../stores';
  import { saveSetting, type AppearanceMode } from '../../settings';
  import {
    getTranscriptionLanguageLabel,
    transcriptionLanguages,
    type TranscriptionLanguageCode,
  } from '../../transcriptionLanguages';

  let microphones = $state<string[]>([]);
  let selectedMic = $state('');
  let micDropdownOpen = $state(false);
  let selectedLanguage = $state<TranscriptionLanguageCode>('en');
  let languageDropdownOpen = $state(false);
  let languageTouched = false;
  let muteAudio = $state(false);
  let autostart = $state(false);
  let hotkey = $state(['ControlLeft', 'MetaLeft']);
  let recordingHotkey = $state(false);
  let capturedKeys = $state<string[]>([]);
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
    try {
      const [mics, muteVal, autostartVal, mic, hk, appearance, language] = await Promise.all([
        invoke<string[]>('get_microphones'),
        invoke<boolean | null>('get_setting', { key: 'mute_audio' }),
        invoke<boolean | null>('get_setting', { key: 'autostart_enabled' }),
        invoke<string | null>('get_setting', { key: 'microphone_device' }),
        invoke<string[] | null>('get_setting', { key: 'hotkey' }),
        invoke<AppearanceMode | null>('get_setting', { key: 'appearance_mode' }),
        invoke<TranscriptionLanguageCode | null>('get_setting', { key: 'transcription_language' }),
      ]);
      microphones = mics;
      muteAudio = muteVal ?? false;
      autostart = autostartVal ?? false;
      selectedMic = mic ?? '';
      if (hk && hk.length === 2) hotkey = hk;
      if (appearance === 'system' || appearance === 'light' || appearance === 'dark') {
        appearanceMode.set(appearance);
      }
      if (!languageTouched && language && transcriptionLanguages.some((option) => option.code === language)) {
        selectedLanguage = language;
      }
    } catch (err) {
      console.error('GeneralSection load failed:', err);
    }
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

  function closeMicDropdown(e: MouseEvent) {
    if (!(e.target as HTMLElement).closest('.mic-dropdown')) micDropdownOpen = false;
  }

  function closeLanguageDropdown(e: MouseEvent) {
    if (!(e.target as HTMLElement).closest('.language-dropdown')) languageDropdownOpen = false;
  }

  $effect(() => {
    if (micDropdownOpen) {
      tick().then(() => window.addEventListener('click', closeMicDropdown, { once: true }));
    }
  });

  $effect(() => {
    if (languageDropdownOpen) {
      tick().then(() => window.addEventListener('click', closeLanguageDropdown, { once: true }));
    }
  });

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

  async function handleAppearance(mode: AppearanceMode) {
    const previous = $appearanceMode;
    appearanceMode.set(mode);
    try {
      await saveSetting('appearance_mode', mode);
    } catch (err) {
      appearanceMode.set(previous);
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

  function micLabel(name: string) {
    return name.length > 32 ? name.slice(0, 32) + '…' : name;
  }

  function startRecordingHotkey(e: MouseEvent | KeyboardEvent) {
    e.stopPropagation();
    if (recordingHotkey) return;
    recordingHotkey = true;
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
    } else if (capturedKeys.length === 1 && e.code !== capturedKeys[0]) {
      capturedKeys = [...capturedKeys, e.code];
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
          const { emit } = await import('@tauri-apps/api/event');
          await emit('open-flow:error', 'Hotkey may already be in use by another application');
          return;
        }
        await invoke('save_hotkey', { key1: capturedKeys[0], key2: capturedKeys[1] });
        hotkey = capturedKeys;
      } catch (e) {
        console.error('Failed to save hotkey', e);
        const { emit } = await import('@tauri-apps/api/event');
        await emit('open-flow:error', 'Failed to save hotkey — key may not be recognized');
      }
    }
  }

  loadSettings();
</script>

<h2 class="settings-h">General</h2>
<div class="setting-row">
  <div><div class="label">Hotkey</div><div class="desc">Hold to record, release to transcribe</div></div>
  <button class="badge key-badge keybind-btn" onclick={startRecordingHotkey} class:recording={recordingHotkey}>
    {#if recordingHotkey}
      {#if capturedKeys.length === 0 || capturedKeys[0] === '__bad__'}
        {capturedKeys[0] === '__bad__' ? 'Must be Alt/Ctrl/Shift/Win' : 'Press Alt/Ctrl/Shift/Win...'}
      {:else}
        Press 2nd key...
      {/if}
    {:else}
      {formatKey(hotkey[0])} + {formatKey(hotkey[1])}
    {/if}
  </button>
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
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <div class="mic-menu" role="presentation" onclick={(e) => e.stopPropagation()}>
        <button class="mic-item" class:active={!selectedMic} onclick={() => saveMic('')}>Default Device</button>
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
  <div><div class="label">Spoken Language</div><div class="desc">Tells transcription what language to expect</div></div>
  <div class="language-dropdown">
    <button class="btn-ghost language-btn" onclick={() => (languageDropdownOpen = !languageDropdownOpen)}>
      <span>{getTranscriptionLanguageLabel(selectedLanguage)}</span>
      <span class="language-code">{selectedLanguage}</span>
      <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="m6 9 6 6 6-6"/>
      </svg>
    </button>
    {#if languageDropdownOpen}
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <div class="language-menu" role="presentation" onclick={(e) => e.stopPropagation()}>
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
  <div><div class="label">Mute PC Audio</div><div class="desc">Mutes Windows volume while dictating to prevent audio interference</div></div>
  <Toggle checked={muteAudio} onchange={handleMuteAudio} />
</div>
<div class="setting-row">
  <div><div class="label">Appearance</div><div class="desc">Follow Windows or force a specific theme</div></div>
  <div class="appearance-segment" role="radiogroup" aria-label="Appearance">
    {#each appearanceOptions as option}
      <button
        class="appearance-option"
        class:active={$appearanceMode === option.id}
        role="radio"
        aria-checked={$appearanceMode === option.id}
        onclick={() => handleAppearance(option.id)}
      >{option.label}</button>
    {/each}
  </div>
</div>
<div class="setting-row">
  <div><div class="label">Start on Boot</div><div class="desc">Launch Open Flow when Windows starts</div></div>
  <Toggle checked={autostart} onchange={handleAutostart} />
</div>

<style>
  .keybind-btn { cursor: pointer; border: 1px solid transparent; transition: all 0.2s; user-select: none; }
  .keybind-btn:hover { background: var(--control-hover); }
  .keybind-btn.recording { background: var(--accent); color: var(--on-accent); animation: pulse 1.5s infinite; }
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.7; } }
  .mic-dropdown, .language-dropdown { position: relative; flex-shrink: 0; }
  .mic-btn, .language-btn { display: flex; align-items: center; gap: 6px; max-width: 180px; }
  .language-btn { max-width: 210px; }
  .mic-btn span, .language-btn span:first-child { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 140px; }
  .language-code {
    font-family: var(--mono);
    font-size: 10.5px;
    color: var(--ink-faint);
    text-transform: uppercase;
  }
  .mic-menu, .language-menu {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    box-shadow: var(--shadow-popover);
    min-width: 200px;
    max-width: 280px;
    max-height: 200px;
    overflow-y: auto;
    z-index: 10;
  }
  .language-menu { min-width: 220px; max-height: 260px; }
  .mic-item, .language-item {
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
  .language-item { display: flex; justify-content: space-between; gap: 12px; }
  .language-item span:last-child {
    color: var(--ink-faint);
    font-family: var(--mono);
    font-size: 10.5px;
    text-transform: uppercase;
  }
  .mic-item:last-child, .language-item:last-child { border-bottom: none; }
  .mic-item:hover, .language-item:hover { background: var(--paper); }
  .mic-item.active, .language-item.active { background: var(--accent-soft); color: var(--ink); font-weight: 500; }
  .mic-empty { padding: 10px 12px; font-size: 12px; color: var(--ink-mute); font-style: italic; }
  .appearance-segment {
    display: inline-flex;
    align-items: center;
    padding: 2px;
    background: var(--paper);
    border: 1px solid var(--line);
    border-radius: 7px;
    gap: 2px;
  }
  .appearance-option {
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--ink-mute);
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 500;
    padding: 4px 9px;
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }
  .appearance-option:hover { color: var(--ink-strong); background: var(--control-hover); }
  .appearance-option.active { color: var(--ink); background: var(--bg-elev); box-shadow: 0 0 0 1px var(--line-soft); }
</style>
