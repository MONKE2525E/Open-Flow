<script lang="ts">
  import { onDestroy } from 'svelte';
  import { emit, invoke } from '../../tauri';
  import { fly, fade } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { isMac, formatKeyLabel, defaultHotkey } from '../../platform';
  import Toggle from '../Toggle.svelte';
  import Dropdown from '../Dropdown.svelte';
  import { appStore } from '../../stores';
  import { saveSetting, type AppearanceMode } from '../../settings';
  import { modalFocusTrap } from '../../modalFocus';
  import { MOTION_MS, MOTION_PX, modalBackdrop, modalCard, motionMs, motionPx, animateWidth } from '../../motion';
  import {
    getTranscriptionLanguageLabel,
    transcriptionLanguages,
    type TranscriptionLanguageCode,
  } from '../../transcriptionLanguages';
  import { getLanguageSupport } from '../../transcriptionLanguageSupport';
  import { transcriptionModelStore } from '../../transcriptionModelStore.svelte';
  import { modelDisplayLabel, splitModelId } from './models';

  let selectedLanguage = $state<TranscriptionLanguageCode>('en');
  let languageDropdownOpen = $state(false);
  let languageTouched = false;
  // Guards the language auto-correct effect below until the real persisted
  // selection has loaded — without this, the effect could see the placeholder
  // initial state (selectedLanguage='en', transcriptionModelStore at its
  // default) as "unsupported," call saveLanguage('en'), and that call's
  // languageTouched=true side effect would then make loadSettings() skip
  // applying the user's actual saved language once it resolves.
  //
  // Must be $state, not a plain let: the auto-correct effect's first line is
  // `if (!initialLanguageLoaded) return`, which on the mount run short-circuits
  // before reading any reactive value — so unless THIS flag is reactive, the
  // effect registers no dependencies and never re-runs when loadSettings later
  // flips it true.
  let initialLanguageLoaded = $state(false);
  let microphones = $state<string[]>([]);
  let selectedMic = $state('');
  let micDropdownOpen = $state(false);
  const microphoneCopy = {
    inputDeviceLabel: 'Input device',
    inputDeviceDescription: 'Choose which microphone Verenu should record from',
    defaultDevice: 'Default Device',
    noDevicesFound: 'No devices found',
  };
  let autostart = $state(false);
  let contextualCaps = $state(true);
  let autoSpacing = $state(true);
  let capsLockUppercase = $state(false);
  let clipboardPhraseEnabled = $state(false);
  let clipboardPhrase = $state('paste clipboard here');
  let clipboardPhraseError = $state('');
  let hotkey = $state(defaultHotkey);
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

    const measure = () => {
      const btn = segmentEl?.querySelectorAll<HTMLElement>('.appearance-option')[idx];
      if (!btn) return;
      indicatorStyle = `left:${btn.offsetLeft}px;width:${btn.offsetWidth}px`;
    };

    measure();
    // The settings column is fluid now, so a one-shot measurement goes stale as
    // soon as the window is resized.
    const observer = new ResizeObserver(measure);
    observer.observe(segmentEl);
    return () => observer.disconnect();
  });

  const readableMac: Record<string, string> = {
    MetaLeft: 'Cmd',
    MetaRight: 'Cmd',
    ControlLeft: 'Ctrl',
    ControlRight: 'Ctrl',
    AltLeft: 'Option',
    AltRight: 'Option',
    ShiftLeft: 'Shift',
    ShiftRight: 'Shift',
    Fn: 'Fn',
  };

  function formatHotkeyBadgeLabel(code: string): string {
    if (isMac && readableMac[code]) {
      return readableMac[code];
    }
    return formatKeyLabel(code);
  }

  // A macOS hotkey can be a single key (e.g. F5) — its second slot is empty.
  function formatHotkeyDisplay(hk: string[]): string {
    const first = formatHotkeyBadgeLabel(hk[0] ?? '');
    return hk[1] ? `${first} + ${formatHotkeyBadgeLabel(hk[1])}` : first;
  }

  let buttonText = $derived(
    recordingHotkey
      ? capturedKeys[0] === '__bad__'
        ? isMac ? 'Pick a key like F5' : 'Must be Alt/Ctrl/Shift/Win'
        : capturedKeys.length === 0
          ? isMac ? 'Press a key (e.g. F5)…' : 'Press Alt/Ctrl/Shift/Win...'
          : 'Press 2nd key...'
      : formatHotkeyDisplay(hotkey)
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
      invoke<boolean | null>('get_setting', { key: 'autostart_enabled' }),
      invoke<string[] | null>('get_setting', { key: 'hotkey' }),
      invoke<AppearanceMode | null>('get_setting', { key: 'appearance_mode' }),
      invoke<TranscriptionLanguageCode | null>('get_setting', { key: 'transcription_language' }),
      invoke<boolean | null>('get_setting', { key: 'cleanup_enabled' }),
      invoke<boolean | null>('get_setting', { key: 'contextual_caps_enabled' }),
      invoke<boolean | null>('get_setting', { key: 'auto_spacing_enabled' }),
      invoke<boolean | null>('get_setting', { key: 'caps_lock_uppercase_enabled' }),
      invoke<boolean | null>('get_setting', { key: 'clipboard_phrase_enabled' }),
      invoke<string | null>('get_setting', { key: 'clipboard_phrase' }),
      invoke<string[]>('get_microphones'),
      invoke<string | null>('get_setting', { key: 'microphone_device' }),
    ]);

    const val = <T>(i: number, fallback: T): T =>
      results[i].status === 'fulfilled' ? (results[i] as PromiseFulfilledResult<T>).value ?? fallback : fallback;

    autostart = val<boolean | null>(0, null) ?? false;
    appStore.cleanupEnabled = val<boolean | null>(4, null) ?? true;
    contextualCaps = val<boolean | null>(5, null) ?? true;
    autoSpacing = val<boolean | null>(6, null) ?? true;
    capsLockUppercase = val<boolean | null>(7, null) ?? false;
    clipboardPhraseEnabled = val<boolean | null>(8, null) ?? false;
    clipboardPhrase = val<string | null>(9, null) ?? 'paste clipboard here';

    const hk = val<string[] | null>(1, null);
    if (hk && hk.length === 2) hotkey = hk;

    const appearance = val<AppearanceMode | null>(2, null);
    if (appearance === 'system' || appearance === 'light' || appearance === 'dark') {
      appStore.appearanceMode = appearance;
    }

    const language = val<TranscriptionLanguageCode | null>(3, null);
    if (!languageTouched && language && transcriptionLanguages.some((option) => option.code === language)) {
      selectedLanguage = language;
    }
    initialLanguageLoaded = true;

    microphones = val<string[]>(10, []);
    selectedMic = val<string | null>(11, null) ?? '';

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

  // Which of the 57 Spoken Language options the active transcription model
  // actually supports — 'all' for every current cloud model (Verenu's full
  // list already matches Whisper's official language support exactly, and
  // Gemini publishes no narrower restriction), a real subset for most local
  // models (e.g. Moonshine is English-only).
  const languageScope = $derived.by(() => {
    const parsed = splitModelId(transcriptionModelStore.defaultModel);
    return parsed ? getLanguageSupport(parsed.provider, parsed.model) : 'all';
  });
  const visibleLanguages = $derived(
    languageScope === 'all'
      ? transcriptionLanguages
      : transcriptionLanguages.filter((language) => languageScope.includes(language.code)),
  );
  const languageScopeNote = $derived.by(() => {
    if (languageScope === 'all') return '';
    const parsed = splitModelId(transcriptionModelStore.defaultModel);
    const modelName = parsed ? modelDisplayLabel(parsed.provider, parsed.model) : 'this model';
    const count = visibleLanguages.length;
    return ` · ${count} ${count === 1 ? 'language' : 'languages'} for ${modelName}`;
  });

  // If switching models drops the currently selected language out of the
  // now-narrower list, snap back to a supported one rather than leaving a
  // silently unsupported selection in place. Prefer English (most models
  // include it), but fall back to the model's first supported language for
  // the English-excluding ones (e.g. GigaAM is Russian-only). Gated on
  // initialLanguageLoaded so this never fires during the initial hydration
  // race (see the flag's declaration comment).
  $effect(() => {
    if (!initialLanguageLoaded) return;
    if (languageScope === 'all') return;
    if (visibleLanguages.some((language) => language.code === selectedLanguage)) return;
    const fallback = visibleLanguages.some((language) => language.code === 'en')
      ? 'en'
      : visibleLanguages[0]?.code;
    if (fallback) {
      saveLanguage(fallback).catch((err) => console.error('auto-correct transcription_language failed:', err));
    }
  });

  async function handleAutostart(value: boolean) {
    autostart = value;
    try {
      await invoke('set_autostart', { enabled: value });
    } catch (err) {
      autostart = !value;
      console.error('set_autostart failed:', err);
    }
  }

  async function applyCleanup(value: boolean) {
    appStore.cleanupEnabled = value;
    try {
      await saveSetting('cleanup_enabled', value);
    } catch (err) {
      appStore.cleanupEnabled = !value;
      console.error('save cleanup_enabled failed:', err);
    }
  }

  // Turning Cleanup off is a bigger behavioral change than most toggles here
  // (it silently makes the Style and App Mappings pages inert), so it gets a
  // confirmation instead of taking effect immediately. Turning it back on
  // needs no confirmation — that's just restoring the default.
  let confirmCleanupOff = $state(false);
  let cleanupCancelButton: HTMLButtonElement | null = $state(null);

  function handleCleanup(value: boolean) {
    if (!value) {
      confirmCleanupOff = true;
      return;
    }
    applyCleanup(true);
  }

  async function confirmCleanupOffAction() {
    confirmCleanupOff = false;
    await applyCleanup(false);
  }

  function handleCleanupModalKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && confirmCleanupOff) confirmCleanupOff = false;
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

  async function handleCapsLockUppercase(value: boolean) {
    capsLockUppercase = value;
    try {
      await saveSetting('caps_lock_uppercase_enabled', value);
    } catch (err) {
      capsLockUppercase = !value;
      console.error('save caps_lock_uppercase_enabled failed:', err);
    }
  }

  function normalizedClipboardPhrase(value: string) {
    return value.trim().replace(/\s+/g, ' ');
  }

  function clipboardPhraseValidation(value: string) {
    const normalized = normalizedClipboardPhrase(value);
    const length = [...normalized].length;
    if (length < 5 || length > 80 || !/[\p{L}\p{N}]/u.test(normalized)) {
      return 'Use 5–80 characters and include a letter or number.';
    }
    return '';
  }

  async function saveClipboardPhrase() {
    const normalized = normalizedClipboardPhrase(clipboardPhrase);
    clipboardPhraseError = clipboardPhraseValidation(normalized);
    if (clipboardPhraseError) return false;
    clipboardPhrase = normalized;
    try {
      await saveSetting('clipboard_phrase', normalized);
      return true;
    } catch (err) {
      clipboardPhraseError = 'Could not save this phrase.';
      console.error('save clipboard_phrase failed:', err);
      return false;
    }
  }

  async function handleClipboardPhrase(value: boolean) {
    if (isMac) return;
    if (value && !(await saveClipboardPhrase())) return;
    clipboardPhraseEnabled = value;
    try {
      await saveSetting('clipboard_phrase_enabled', value);
    } catch (err) {
      clipboardPhraseEnabled = !value;
      console.error('save clipboard_phrase_enabled failed:', err);
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

  function removeHotkeyCaptureListeners() {
    window.removeEventListener('keydown', handleHotkeyKeydown, { capture: true });
    window.removeEventListener('keyup', handleHotkeyKeyup, { capture: true });
    window.removeEventListener('mousedown', cancelRecordingHotkey, { capture: true });
  }

  function cancelRecordingHotkey(e?: MouseEvent | KeyboardEvent) {
    if (e && (e.target as HTMLElement).closest('.keybind-btn')) return;
    if (recordingHotkey) {
      removeHotkeyCaptureListeners();
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
      if (MODIFIER_CODES.has(e.code)) {
        // A modifier first — wait for the key it pairs with (modifier+key chord).
        capturedKeys = [e.code];
        hotkeyState = 'first';
      } else if (isMac && /^F([1-9]|1[0-2])$/.test(e.code)) {
        // macOS allows a single-key hotkey, but only function keys (F1–F12):
        // a bare letter/Space would be consumed system-wide and hijack typing.
        capturedKeys = [e.code, ''];
        hotkeyState = 'saving';
        finishRecordingHotkey();
      } else {
        capturedKeys = ['__bad__'];
        setTimeout(() => { capturedKeys = []; }, 800);
      }
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
    removeHotkeyCaptureListeners();
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
          await emit('verenu:error', 'Hotkey may already be in use by another application');
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
        await emit('verenu:error', 'Failed to save hotkey - key may not be recognized');
        setTimeout(() => { hotkeyState = 'idle'; }, HOTKEY_ERROR_MS);
      }
    }
  }

  onDestroy(() => {
    removeHotkeyCaptureListeners();
    recordingHotkey = false;
  });

  loadSettings();

</script>
<svelte:window onclick={handleWindowClick} onkeydown={handleCleanupModalKeydown} />

<h2 class="settings-h">General</h2>
<h3 class="settings-subhead first">Dictation</h3>
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
{#if isMac && hotkey[0] === 'F5'}
  <p class="hotkey-tip">
    F5 is the 🎤 key on Mac keyboards. If pressing it opens macOS Dictation instead of
    Verenu, turn off Dictation in <strong>System Settings → Keyboard → Dictation</strong>
    (or hold <strong>Fn</strong> with F5). You can also pick any other key above.
  </p>
{/if}
<div class="setting-row">
  <div><div class="label">Copy last dictation</div><div class="desc">Always available — re-copies your last dictation to the clipboard, in case a paste didn't land</div></div>
  <span class="badge key-badge">{isMac ? '⌥⌘C' : 'Ctrl+Alt+C'}</span>
</div>
<div class="setting-row">
  <div class="lang-setting-text"><div class="label">Spoken Language</div><div class="desc">Tells transcription what language to expect{languageScopeNote}</div></div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="ui-dropdown language-dropdown" onkeydown={(e) => { if (e.key === 'Escape' && languageDropdownOpen) { languageDropdownOpen = false; e.stopPropagation(); } }}>
    <button
      class="btn-ghost ui-dropdown-trigger language-btn"
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
        class="ui-dropdown-menu language-menu scroll-styled scroll-thumb-elev"
        aria-label="Spoken language options"
        onclick={(e) => e.stopPropagation()}
        in:fly={{ y: -motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.panel), easing: expoOut }}
        out:fade={{ duration: motionMs(MOTION_MS.fast) }}
      >
        {#each visibleLanguages as language}
          <button
            class="ui-dropdown-option language-item"
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
    <div class="label">{microphoneCopy.inputDeviceLabel}</div>
    <div class="desc">{microphoneCopy.inputDeviceDescription}</div>
  </div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="ui-dropdown mic-dropdown" onkeydown={(e) => { if (e.key === 'Escape' && micDropdownOpen) { micDropdownOpen = false; e.stopPropagation(); } }}>
    <button
      class="btn-ghost ui-dropdown-trigger mic-btn"
      use:animateWidth={{ text: selectedMic || microphoneCopy.defaultDevice, max: 180 }}
      onclick={() => (micDropdownOpen = !micDropdownOpen)}
      aria-haspopup="true"
      aria-expanded={micDropdownOpen}
      aria-controls={MIC_MENU_ID}
      aria-label="Microphone device"
    >
      <span class="mic-btn-label">{selectedMic || microphoneCopy.defaultDevice}</span>
      <svg class:open={micDropdownOpen} width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="m6 9 6 6 6-6"/>
      </svg>
    </button>
    {#if micDropdownOpen}
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <div
        id={MIC_MENU_ID}
        class="ui-dropdown-menu ui-dropdown-menu--padded mic-menu scroll-styled scroll-thumb-elev"
        aria-label="Microphone device options"
        onclick={(e) => e.stopPropagation()}
        in:fly={{ y: -motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.panel), easing: expoOut }}
        out:fade={{ duration: motionMs(MOTION_MS.fast) }}
      >
        <button class="ui-dropdown-option mic-item" class:active={!selectedMic} onclick={() => saveMic('')}>{microphoneCopy.defaultDevice}</button>
        {#each microphones as m}
          <button class="ui-dropdown-option mic-item" class:active={selectedMic === m} onclick={() => saveMic(m)}>{m}</button>
        {/each}
        {#if microphones.length === 0}
          <div class="mic-empty">{microphoneCopy.noDevicesFound}</div>
        {/if}
      </div>
    {/if}
  </div>
</div>
<h3 class="settings-subhead">Appearance & System</h3>
<div class="setting-row">
  <div><div class="label">Appearance</div><div class="desc">{isMac ? 'Follow macOS or force a specific theme' : 'Follow Windows or force a specific theme'}</div></div>
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
  <div><div class="label">Start on Boot</div><div class="desc">{isMac ? 'Launch Verenu when macOS starts' : 'Launch Verenu when Windows starts'}</div></div>
  <Toggle checked={autostart} onchange={handleAutostart} label="Start on boot" />
</div>
<h3 class="settings-subhead">Text processing</h3>
<div class="setting-row">
  <div><div class="label">Cleanup</div><div class="desc">Runs an LLM-powered cleanup pass after transcription for tone and formatting.</div></div>
  <Toggle checked={appStore.cleanupEnabled} onchange={handleCleanup} label="Cleanup" />
</div>
<div class="setting-row">
  <div><div class="label">Contextual capitalization</div><div class="desc">Lowercases the first word when injecting mid-sentence</div></div>
  <Toggle checked={contextualCaps} onchange={handleContextualCaps} label="Contextual capitalization" />
</div>
<div class="setting-row">
  <div><div class="label">Automatic spacing</div><div class="desc">Adds a space before injected text when the cursor is after existing text</div></div>
  <Toggle checked={autoSpacing} onchange={handleAutoSpacing} label="Automatic spacing" />
</div>
<div class="setting-row">
  <div><div class="label">Automatic caps lock detection</div><div class="desc">When Caps Lock is on, output your dictation in ALL CAPS</div></div>
  <Toggle checked={capsLockUppercase} onchange={handleCapsLockUppercase} label="Automatic caps lock detection" />
</div>
<div class="setting-row clipboard-phrase-row">
  <div>
    <div class="label">Clipboard phrase</div>
    <div class="desc">{isMac ? 'Available on Windows. Clipboard text stays private.' : 'Insert current clipboard text. It stays private.'}</div>
    {#if clipboardPhraseError}<div class="clipboard-phrase-error">{clipboardPhraseError}</div>{/if}
  </div>
  <div class="clipboard-phrase-controls">
    <input id="clipboard-phrase" class="ui-input" bind:value={clipboardPhrase} onblur={saveClipboardPhrase} disabled={isMac} aria-label="Phrase to insert clipboard text" aria-invalid={clipboardPhraseError ? 'true' : undefined} />
    <div aria-disabled={isMac} inert={isMac || undefined}><Toggle checked={clipboardPhraseEnabled} onchange={handleClipboardPhrase} label="Clipboard phrase" /></div>
  </div>
</div>

{#if confirmCleanupOff}
  <button class="ui-modal-backdrop" aria-label="Close dialog" onclick={() => (confirmCleanupOff = false)} in:modalBackdrop={{ duration: 180 }} out:modalBackdrop={{ duration: 160 }}></button>
  <div
    class="modal-card ui-modal-card"
    use:modalFocusTrap={{
      active: confirmCleanupOff,
      initialFocus: () => cleanupCancelButton,
    }}
    role="dialog"
    aria-modal="true"
    aria-labelledby="cleanup-off-confirm-title"
    tabindex="-1"
    in:modalCard={{ duration: 220, distance: motionPx(MOTION_PX.panel), scaleFrom: 0.97 }}
    out:modalCard={{ duration: 160, distance: motionPx(MOTION_PX.nudge), scaleFrom: 0.985 }}
  >
    <div class="modal-header">
      <h2 id="cleanup-off-confirm-title" class="modal-title">Turn Cleanup off?</h2>
    </div>
    <div class="modal-body">
      <p class="confirm-copy">
        Dictation will keep the raw transcript as-is — faster, but tone, formatting, and the
        Style and App Mappings pages stop having any effect. You can turn this back on anytime.
      </p>
    </div>
    <div class="modal-footer">
      <div class="footer-actions">
        <button bind:this={cleanupCancelButton} class="btn-ghost" onclick={() => (confirmCleanupOff = false)}>Cancel</button>
        <button class="btn-primary" onclick={confirmCleanupOffAction}>Turn off</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .hotkey-tip {
    margin: -2px 0 2px;
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--ink-mute);
    max-width: 52ch;
  }
  .clipboard-phrase-row { gap: 16px; }
  .clipboard-phrase-controls { display: flex; align-items: center; gap: 12px; flex-shrink: 0; }
  .clipboard-phrase-row .ui-input { width: 220px; }
  .clipboard-phrase-error { margin-top: 5px; font-size: 11px; color: var(--danger); }
  @container settings-column (max-width: 620px) {
    .clipboard-phrase-row { align-items: flex-start; flex-direction: column; }
    .clipboard-phrase-controls { width: 100%; justify-content: space-between; }
    .clipboard-phrase-row .ui-input { width: min(320px, calc(100% - 42px)); }
  }
  .hotkey-tip strong { color: var(--ink-soft); font-weight: 600; }

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
  .mic-btn {
    max-width: 180px;
  }
  .mic-btn-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    text-align: left;
  }
  .mic-menu {
    width: 220px;
  }
  .mic-empty { padding: 8px 10px; font-size: 12px; color: var(--ink-mute); text-align: center; }
  /* Let the label+desc column take remaining width and wrap within itself,
     so the (sometimes long) language-scope note never runs under the
     fixed-width dropdown button to its right. */
  .lang-setting-text { min-width: 0; flex: 1; padding-right: 14px; }
  .language-btn { max-width: 210px; }
  .language-btn span:first-child { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 140px; }
  .language-code {
    font-family: var(--mono);
    font-size: 10.5px;
    color: var(--ink-faint);
    text-transform: uppercase;
  }
  .language-menu {
    min-width: 220px;
    max-width: 280px;
    max-height: 260px;
  }
  .language-item {
    display: flex;
    gap: 12px;
    justify-content: space-between;
  }
  .language-item span:last-child {
    color: var(--ink-faint);
    font-family: var(--mono);
    font-size: 10.5px;
    text-transform: uppercase;
  }
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

  /* ── cleanup-off confirm modal ── */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    border: 0;
    padding: 0;
    appearance: none;
    background: var(--overlay);
    z-index: 50;
    outline: none;
  }
  .modal-card {
    position: fixed;
    top: 50%;
    left: 50%;
    translate: -50% -50%;
    z-index: 51;
    isolation: isolate;
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    width: min(420px, calc(100vw - 40px));
    box-shadow: var(--shadow-elev);
    overflow: hidden;
  }
  .modal-header {
    padding: 20px 20px 0;
  }
  .modal-title {
    font-family: var(--serif);
    font-size: 18px;
    font-weight: 500;
    letter-spacing: -0.015em;
    color: var(--ink);
    margin: 0;
  }
  .modal-body { padding: 10px 20px 18px; }
  .confirm-copy {
    margin: 0;
    font-size: 13px;
    line-height: 1.5;
    color: var(--ink-soft);
  }
  .modal-footer {
    padding: 0 20px 20px;
  }
  .footer-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
