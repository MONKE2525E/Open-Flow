<script lang="ts">
  import { settingsOpen, updateInfo, type UpdateInfo } from '../stores';
  import { icons } from '../icons';
  import { tick, onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { getVersion } from '@tauri-apps/api/app';
  import { fly, slide, fade } from 'svelte/transition';
  import { flip } from 'svelte/animate';
  import { expoOut } from 'svelte/easing';

  let section = 'general';

  let animDir: 'up' | 'down' | null = null;

  let appVersion = '';

  // About page update state
  type UpdateCheckState = 'idle' | 'checking' | 'up-to-date' | 'available';
  let updateCheckState: UpdateCheckState = 'idle';
  let installingFromAbout = false;

  // Hotkey state
  let hotkey = ['ControlLeft', 'MetaLeft'];
  let recordingHotkey = false;
  let capturedKeys: string[] = [];

  // API key status — true means a key is saved; never expose the value
  let keyStatus = { groq: false, openai: false, google: false };

  // Draft key inputs (only held in memory while settings is open, never read back from store)
  let draftKeys = { groq: '', openai: '', google: '' };

  // App mappings state
  interface InstalledApp { name: string; exe: string; }
  interface AppMapping { exe: string; profile: string; name: string; }

  let mappings: AppMapping[] = [];
  let installedApps: InstalledApp[] = [];
  let areAppsLoaded = false;
  let addExe = '';
  let addName = '';
  let addProfile = 'casual';
  let appSearch = '';
  let appPickerOpen = false;

  // Microphone state
  let microphones: string[] = [];
  let selectedMic = '';
  let micDropdownOpen = false;

  // Model selection state
  let transcriptionModel = 'groq/whisper-large-v3-turbo';
  let cleanupModel = 'groq/llama-3.3-70b-versatile';

  // Toggle states
  let toggleState = { cleanup: true, noiseReduction: true, muteAudio: false, autostart: false, appContextHint: false, apiFallback: false, autoLearn: false, contextualCaps: true };

  // Mic gain (1.0–8.0, default 3.5)
  let micGain = 3.5;

  // Transcription history retention dropdown
  let historyRetention = '30 days';
  let historyDropdownOpen = false;
  const historyOptions = ['7 days', '30 days', '90 days', 'Forever'];

  const sectionOrder = ['general','apps','keys','models','privacy','advanced','about'];
  const profiles = [
    { id: 'casual',      label: 'Casual'      },
    { id: 'formal',      label: 'Formal'      },
    { id: 'very_casual', label: 'Very Casual' },
  ];

  $: if ($settingsOpen) {
    loadSettings();
    // If an update was already detected (e.g. from Home), surface it in About
    if ($updateInfo) updateCheckState = 'available';
  } else {
    cancelRecordingHotkey();
    draftKeys = { groq: '', openai: '', google: '' };
    micDropdownOpen = false;
    appPickerOpen = false;
    updateCheckState = 'idle';
  }

  onDestroy(() => {
    cancelRecordingHotkey();
  });

  onMount(async () => {
    appVersion = await getVersion();
  });

  async function loadMappings() {
    try {
      mappings = await invoke<AppMapping[]>('get_app_mappings');
    } catch { /* dev mode */ }
  }

  async function loadInstalledApps() {
    if (areAppsLoaded) return;
    try {
      installedApps = await invoke<InstalledApp[]>('get_installed_apps');
      areAppsLoaded = true;
    } catch { /* dev mode */ }
  }

  async function deleteMapping(exe: string) {
    mappings = mappings.filter(m => m.exe !== exe);
    try {
      await invoke('save_app_mappings', { mappings });
    } catch {}
  }

  async function addMapping() {
    if (!addExe) return;
    const existing = mappings.findIndex(m => m.exe === addExe);
    const entry: AppMapping = { exe: addExe, profile: addProfile, name: addName || addExe };
    if (existing >= 0) {
      mappings = mappings.map((m, i) => i === existing ? entry : m);
    } else {
      mappings = [...mappings, entry];
    }
    try {
      await invoke('save_app_mappings', { mappings });
    } catch {}
    addExe = '';
    addName = '';
    addProfile = 'casual';
    appSearch = '';
    appPickerOpen = false;
  }

  function pickApp(app: InstalledApp) {
    addExe = app.exe;
    addName = app.name;
    appSearch = app.name;
    appPickerOpen = false;
  }

  $: filteredApps = appSearch
    ? installedApps.filter(a =>
        a.name.toLowerCase().includes(appSearch.toLowerCase()) ||
        a.exe.toLowerCase().includes(appSearch.toLowerCase())
      ).slice(0, 40)
    : installedApps.slice(0, 40);

  function closeAppPicker(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (!target.closest('.app-picker-wrap')) appPickerOpen = false;
  }

  $: if (appPickerOpen) {
    tick().then(() => window.addEventListener('click', closeAppPicker, { once: true }));
  }

  $: if (section === 'apps' && $settingsOpen) {
    loadMappings();
    loadInstalledApps();
  }

  async function loadSettings() {
    try {
      const [
        keyStatusResult,
        microphonesResult,
        tModel,
        cModel,
        cleanupEnabled,
        noiseReduction,
        muteAudio,
        autostartEnabled,
        appContextHint,
        apiFallback,
        autoLearn,
        contextualCaps,
        savedGain,
        retention,
        mic,
        hk,
      ] = await Promise.all([
        invoke('get_api_key_status'),
        invoke<string[]>('get_microphones'),
        invoke<string | null>('get_setting', { key: 'transcription_model' }),
        invoke<string | null>('get_setting', { key: 'cleanup_model' }),
        invoke<boolean | null>('get_setting', { key: 'cleanup_enabled' }),
        invoke<boolean | null>('get_setting', { key: 'noise_reduction' }),
        invoke<boolean | null>('get_setting', { key: 'mute_audio' }),
        invoke<boolean | null>('get_setting', { key: 'autostart_enabled' }),
        invoke<boolean | null>('get_setting', { key: 'app_context_hint' }),
        invoke<boolean | null>('get_setting', { key: 'api_fallback_enabled' }),
        invoke<boolean | null>('get_setting', { key: 'auto_learn_enabled' }),
        invoke<boolean | null>('get_setting', { key: 'contextual_caps_enabled' }),
        invoke<number | null>('get_setting', { key: 'mic_gain' }),
        invoke<string | null>('get_setting', { key: 'history_retention' }),
        invoke<string | null>('get_setting', { key: 'microphone_device' }),
        invoke<string[] | null>('get_setting', { key: 'hotkey' }),
      ]);

      keyStatus = keyStatusResult as typeof keyStatus;
      microphones = microphonesResult as string[];
      if (tModel) transcriptionModel = tModel as string;
      if (cModel) cleanupModel = cModel as string;
      if ((savedGain as number | null) !== null && savedGain !== undefined) {
        micGain = Math.max(1, Math.min(8, savedGain as number));
      }
      toggleState = {
        cleanup: (cleanupEnabled as boolean | null) ?? true,
        noiseReduction: (noiseReduction as boolean | null) ?? true,
        muteAudio: (muteAudio as boolean | null) ?? false,
        autostart: (autostartEnabled as boolean | null) ?? false,
        appContextHint: (appContextHint as boolean | null) ?? false,
        apiFallback: (apiFallback as boolean | null) ?? false,
        autoLearn: (autoLearn as boolean | null) ?? false,
        contextualCaps: (contextualCaps as boolean | null) ?? true,
      };
      if (retention) historyRetention = retention as string;
      selectedMic = (mic as string | null) ?? '';
      if (hk && (hk as string[]).length === 2) hotkey = hk as string[];
    } catch {
      // dev mode without Tauri — best-effort
    }
  }

  const MODIFIER_CODES = new Set([
    'ShiftLeft', 'ShiftRight',
    'ControlLeft', 'ControlRight',
    'AltLeft', 'AltRight',
    'MetaLeft', 'MetaRight',
  ]);

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
    if (e && (e.target as HTMLElement).closest('.keybind-btn')) {
      // Don't cancel if they click the button itself again
      return;
    }
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
      if (e.code === 'Escape') {
        cancelRecordingHotkey();
        return;
      }
      // First key must be a modifier to avoid suppressing arbitrary typing system-wide.
      if (!MODIFIER_CODES.has(e.code)) {
        // Flash the button label briefly so the user understands why nothing happened.
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
          console.warn("check_hotkey failed (likely running in browser dev mode)", e);
        }

        if (!available) {
          const { emit } = await import('@tauri-apps/api/event');
          // RegisterHotKey only catches apps using the same Win32 API — treat
          // this as a best-effort signal, not a definitive conflict.
          await emit('open-flow:error', "Hotkey may already be in use by another application");
          return;
        }

        // Save first; only commit to the UI if the backend call succeeds.
        await invoke('save_hotkey', { key1: capturedKeys[0], key2: capturedKeys[1] });
        hotkey = capturedKeys;
      } catch (e) {
        console.error("Failed to save hotkey", e);
        const { emit } = await import('@tauri-apps/api/event');
        await emit('open-flow:error', "Failed to save hotkey — key may not be recognized");
      }
    }
  }

  function formatKey(code: string) {
    if (!code) return '';
    return code.replace('Left', '').replace('Right', '').replace('Key', '').replace('Digit', '');
  }

  async function saveKey(provider: 'groq' | 'openai' | 'google') {
    const key = draftKeys[provider].trim();
    if (!key) return;
    try {
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
      await invoke('save_setting', { key: 'transcription_model', value: id });
      await invoke('save_setting', { key: 'transcription_provider', value: provider });
    } catch {}
  }

  async function setCleanupModel(id: string) {
    cleanupModel = id;
    const provider = id.split('/')[0];
    try {
      await invoke('save_setting', { key: 'cleanup_model', value: id });
      await invoke('save_setting', { key: 'cleanup_provider', value: provider });
    } catch {}
  }

  async function toggleCleanup() {
    toggleState = { ...toggleState, cleanup: !toggleState.cleanup };
    try {
      await invoke('save_setting', { key: 'cleanup_enabled', value: toggleState.cleanup });
    } catch {}
  }

  async function toggleNoiseReduction() {
    toggleState = { ...toggleState, noiseReduction: !toggleState.noiseReduction };
    try {
      await invoke('save_setting', { key: 'noise_reduction', value: toggleState.noiseReduction });
    } catch {}
  }

  async function saveMicGain() {
    try {
      await invoke('save_setting', { key: 'mic_gain', value: micGain });
    } catch {}
  }

  async function toggleMuteAudio() {
    toggleState = { ...toggleState, muteAudio: !toggleState.muteAudio };
    try {
      await invoke('save_setting', { key: 'mute_audio', value: toggleState.muteAudio });
    } catch {}
  }

  async function toggleAutostart() {
    toggleState = { ...toggleState, autostart: !toggleState.autostart };
    try {
      await invoke('set_autostart', { enabled: toggleState.autostart });
    } catch {}
  }

  async function toggleAppContextHint() {
    toggleState = { ...toggleState, appContextHint: !toggleState.appContextHint };
    try {
      await invoke('save_setting', { key: 'app_context_hint', value: toggleState.appContextHint });
    } catch {}
  }

  async function toggleApiFallback() {
    toggleState = { ...toggleState, apiFallback: !toggleState.apiFallback };
    try {
      await invoke('save_setting', { key: 'api_fallback_enabled', value: toggleState.apiFallback });
    } catch {}
  }

  async function toggleAutoLearn() {
    toggleState = { ...toggleState, autoLearn: !toggleState.autoLearn };
    try {
      await invoke('save_setting', { key: 'auto_learn_enabled', value: toggleState.autoLearn });
    } catch {}
  }

  async function toggleContextualCaps() {
    toggleState = { ...toggleState, contextualCaps: !toggleState.contextualCaps };
    try {
      await invoke('save_setting', { key: 'contextual_caps_enabled', value: toggleState.contextualCaps });
    } catch {}
  }

  async function saveMic(name: string) {
    selectedMic = name;
    micDropdownOpen = false;
    try {
      await invoke('save_setting', { key: 'microphone_device', value: name || null });
    } catch {}
  }

  function closeMicDropdown(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (!target.closest('.mic-dropdown')) micDropdownOpen = false;
  }

  function closeHistoryDropdown(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (!target.closest('.history-dropdown')) historyDropdownOpen = false;
  }

  $: if (micDropdownOpen) {
    tick().then(() => window.addEventListener('click', closeMicDropdown, { once: true }));
  }

  $: if (historyDropdownOpen) {
    tick().then(() => window.addEventListener('click', closeHistoryDropdown, { once: true }));
  }

  async function saveHistoryRetention(value: string) {
    historyRetention = value;
    historyDropdownOpen = false;
    try {
      await invoke('save_setting', { key: 'history_retention', value });
    } catch {}
  }

  async function openRepo() {
    try {
      const { open } = await import('@tauri-apps/plugin-shell');
      await open('https://github.com/MONKE2525E/Open-Flow');
    } catch {
      window.open('https://github.com/MONKE2525E/Open-Flow', '_blank');
    }
  }

  async function checkForUpdateManual() {
    updateCheckState = 'checking';
    try {
      const update = await invoke<any>('check_for_update');
      if (update) {
        // Clear any dismissed flag so the install flow works from here
        try { await invoke('save_setting', { key: 'update_dismissed_version', value: null }); } catch {}
        updateInfo.set(update as UpdateInfo);
        updateCheckState = 'available';
      } else {
        updateCheckState = 'up-to-date';
      }
    } catch {
      updateCheckState = 'idle';
    }
  }

  async function handleInstallFromAbout() {
    if (!$updateInfo) return;
    installingFromAbout = true;
    try {
      await invoke('install_update', { downloadUrl: $updateInfo.downloadUrl });
    } catch (e) {
      console.error('Install failed:', e);
    } finally {
      installingFromAbout = false;
    }
  }

  function close() { $settingsOpen = false; }

  function goTo(id: string) {
    if (id === section) return;
    const oldIdx = sectionOrder.indexOf(section);
    const newIdx = sectionOrder.indexOf(id);
    animDir = newIdx > oldIdx ? 'up' : 'down';
    section = id;
  }

  const navSections = [
    { group: 'Settings', items: [
      { id: 'general',  label: 'General',      icon: 'sliders'  as keyof typeof icons },
      { id: 'apps',     label: 'App Mappings', icon: 'apps'     as keyof typeof icons },
      { id: 'keys',     label: 'API Keys',     icon: 'key'      as keyof typeof icons },
      { id: 'models',   label: 'Models',       icon: 'command'  as keyof typeof icons },
      { id: 'privacy',  label: 'Privacy',      icon: 'lock'     as keyof typeof icons },
      { id: 'advanced', label: 'Advanced',     icon: 'settings' as keyof typeof icons },
    ]},
    { group: 'Account', items: [
      { id: 'about', label: 'About', icon: 'help' as keyof typeof icons },
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
    transition:fade={{ duration: 200 }}
    onclick={close}
  >
    <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
    <div
      class="settings-modal"
      transition:fly={{ y: 40, duration: 400, easing: expoOut }}
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
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">{@html icons[it.icon]}</svg>
              <span>{it.label}</span>
            </div>
          {/each}
        {/each}
        <div style="flex:1"></div>
        <div class="settings-foot">Open Flow v{appVersion} · MIT</div>
      </div>

      <!-- Right panel -->
      <div class="settings-body">
        {#key section}
          <div
            class="panel"
            in:fly={{ y: animDir === 'up' ? 20 : -20, duration: 350, delay: 150, easing: expoOut }}
            out:fly={{ y: animDir === 'up' ? -20 : 20, duration: 150, easing: expoOut }}
          >
            {#if section === 'general'}
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
              <div><div class="label">Mute PC Audio</div><div class="desc">Mutes Windows volume while dictating to prevent audio interference</div></div>
              <div class="toggle" class:on={toggleState.muteAudio} role="switch" aria-checked={toggleState.muteAudio} tabindex="0"
                onclick={toggleMuteAudio}
                onkeydown={(e) => e.key === 'Enter' && toggleMuteAudio()}
              ></div>
            </div>
            <div class="setting-row">
              <div><div class="label">Start on Boot</div><div class="desc">Launch Open Flow when Windows starts</div></div>
              <div class="toggle" class:on={toggleState.autostart} role="switch" aria-checked={toggleState.autostart} tabindex="0"
                onclick={toggleAutostart}
                onkeydown={(e) => e.key === 'Enter' && toggleAutostart()}
              ></div>
            </div>

          {:else if section === 'apps'}
            <h2 class="settings-h">App Mappings</h2>
            <p class="panel-note">Switch profiles automatically based on the active window.</p>

            {#if mappings.length > 0}
              <div class="mapping-list">
                {#each mappings as m (m.exe)}
                  <div class="mapping-row" animate:flip={{duration: 300, easing: expoOut}} in:fly={{y: 10, duration: 300, easing: expoOut}} out:slide={{duration: 200, easing: expoOut}}>
                    <div class="mapping-app-info">
                      <span class="mapping-app-name">{m.name || m.exe.replace(/\.exe$/i, '')}</span>
                      <span class="mapping-exe-pill">{m.exe}</span>
                    </div>
                    <svg class="mapping-arrow-icon" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14M12 5l7 7-7 7"/></svg>
                    <span class="mapping-profile-badge">{m.profile}</span>
                    <button class="mapping-delete-btn" onclick={() => deleteMapping(m.exe)} title="Remove">
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
                    </button>
                  </div>
                {/each}
              </div>
            {:else}
              <div class="mapping-empty">No app mappings yet. Add one below to get started.</div>
            {/if}

            <div class="add-mapping-section">
              <div class="add-mapping-label">Add Mapping</div>
              <div class="add-mapping-row">
                <div class="app-picker-wrap">
                  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
                  <input
                    class="app-search-input"
                    placeholder={areAppsLoaded ? 'Search apps…' : 'Loading apps…'}
                    bind:value={appSearch}
                    onfocus={() => { appPickerOpen = true; }}
                    oninput={() => { appPickerOpen = true; }}
                  />
                  {#if appPickerOpen && filteredApps.length > 0}
                    <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
                    <div class="app-picker-menu" onclick={(e) => e.stopPropagation()}>
                      {#each filteredApps as app}
                        <button class="app-picker-item" onclick={() => pickApp(app)}>
                          <span class="app-picker-name">{app.name}</span>
                          <span class="app-picker-exe">{app.exe}</span>
                        </button>
                      {/each}
                    </div>
                  {/if}
                </div>
                <select class="profile-select" bind:value={addProfile}>
                  {#each profiles as p}
                    <option value={p.id}>{p.label}</option>
                  {/each}
                </select>
                <button
                  class="btn-ghost add-btn"
                  onclick={addMapping}
                  disabled={!addExe}
                >Add</button>
              </div>
              {#if addExe}
                <div class="add-preview">
                  <span class="mapping-exe-pill">{addExe}</span>
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M5 12h14M12 5l7 7-7 7"/></svg>
                  <span class="mapping-profile-badge">{addProfile}</span>
                </div>
              {/if}
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
              <div><div class="label">Transcription history</div><div class="desc">How long to keep past dictations</div></div>
              <div class="history-dropdown">
                <button class="btn-ghost mic-btn" onclick={() => (historyDropdownOpen = !historyDropdownOpen)}>
                  <span>{historyRetention}</span>
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                    <path d="m6 9 6 6 6-6"/>
                  </svg>
                </button>
                {#if historyDropdownOpen}
                  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
                  <div class="mic-menu" onclick={(e) => e.stopPropagation()}>
                    {#each historyOptions as opt}
                      <button class="mic-item" class:active={historyRetention === opt} onclick={() => saveHistoryRetention(opt)}>
                        {opt}
                      </button>
                    {/each}
                  </div>
                {/if}
              </div>
            </div>
            <div class="setting-row">
              <div><div class="label">App context hint</div><div class="desc">Passes the active app to the cleanup model to tailor formatting</div></div>
              <div class="toggle" class:on={toggleState.appContextHint} role="switch" aria-checked={toggleState.appContextHint} tabindex="0"
                onclick={toggleAppContextHint}
                onkeydown={(e) => e.key === 'Enter' && toggleAppContextHint()}
              ></div>
            </div>

          {:else if section === 'advanced'}
            <h2 class="settings-h">Advanced</h2>
            <div class="setting-row">
              <div><div class="label">Auto-cleanup</div><div class="desc">Run LLM cleanup on every transcription</div></div>
              <div class="toggle" class:on={toggleState.cleanup} role="switch" aria-checked={toggleState.cleanup} tabindex="0"
                onclick={toggleCleanup}
                onkeydown={(e) => e.key === 'Enter' && toggleCleanup()}
              ></div>
            </div>
            <div class="setting-row">
              <div><div class="label">Contextual capitalization</div><div class="desc">Lowercases the first word when injecting mid-sentence</div></div>
              <div class="toggle" class:on={toggleState.contextualCaps} role="switch" aria-checked={toggleState.contextualCaps} tabindex="0"
                onclick={toggleContextualCaps}
                onkeydown={(e) => e.key === 'Enter' && toggleContextualCaps()}
              ></div>
            </div>
            <div class="setting-row gain-row">
              <div class="gain-header">
                <div>
                  <div class="label">Microphone gain</div>
                  <div class="desc">Boost signal strength before sending audio to the voice model</div>
                </div>
                <span class="gain-value">{micGain.toFixed(1)}×</span>
              </div>
              <div class="gain-slider-wrap">
                <input
                  type="range"
                  class="gain-slider"
                  min="1" max="8" step="0.1"
                  bind:value={micGain}
                  oninput={saveMicGain}
                  style="--pct: {((micGain - 1) / 7 * 100).toFixed(1)}%"
                  aria-label="Microphone gain"
                />
                <div class="gain-ticks">
                  <span>1×</span>
                  <span>4×</span>
                  <span>8×</span>
                </div>
              </div>
              {#if micGain >= 5}
                <div class="gain-tip" transition:slide={{ duration: 220 }}>
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
                  At high gain, enable <strong>noise reduction</strong> to avoid amplifying background noise.
                </div>
              {/if}
            </div>
            <div class="setting-row">
              <div><div class="label">Noise reduction</div><div class="desc">Suppress background noise before transcription (RNNoise)</div></div>
              <div class="toggle" class:on={toggleState.noiseReduction} role="switch" aria-checked={toggleState.noiseReduction} tabindex="0"
                onclick={toggleNoiseReduction}
                onkeydown={(e) => e.key === 'Enter' && toggleNoiseReduction()}
              ></div>
            </div>
            <div class="setting-row">
              <div>
                <div class="label">API fallback</div>
                <div class="desc">If your primary provider hits its quota, automatically retry with another configured API key</div>
              </div>
              <div class="toggle" class:on={toggleState.apiFallback} role="switch" aria-checked={toggleState.apiFallback} tabindex="0"
                onclick={toggleApiFallback}
                onkeydown={(e) => e.key === 'Enter' && toggleApiFallback()}
              ></div>
            </div>
            <div class="setting-row">
              <div>
                <div class="label" style="display:flex;align-items:center;gap:7px;">
                  Auto-learn corrections
                  <span class="privacy-eye-wrap">
                    <svg class="privacy-eye" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                      <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
                      <circle cx="12" cy="12" r="3"/>
                    </svg>
                    <span class="privacy-tooltip">Entirely on-device — no text is sent to any API.</span>
                  </span>
                </div>
                <div class="desc">Add confirmed corrections to dictionary automatically</div>
              </div>
              <div class="toggle" class:on={toggleState.autoLearn} role="switch" aria-checked={toggleState.autoLearn} tabindex="0"
                onclick={toggleAutoLearn}
                onkeydown={(e) => e.key === 'Enter' && toggleAutoLearn()}
              ></div>
            </div>

          {:else if section === 'about'}
            <h2 class="settings-h">About</h2>
            <div class="setting-row">
              <div><div class="label">Version</div></div>
              <span class="desc">v{appVersion}</span>
            </div>
            <div class="setting-row">
              <div><div class="label">License</div></div>
              <span class="desc">MIT</span>
            </div>
            <div class="setting-row">
              <div><div class="label">Source</div></div>
              <button class="btn-ghost" onclick={openRepo}>github.com/MONKE2525E/Open-Flow</button>
            </div>
            <div class="setting-row">
              <div>
                <div class="label">Updates</div>
                {#if updateCheckState === 'up-to-date'}
                  <div class="desc update-ok">You're on the latest version</div>
                {:else if updateCheckState === 'available' && $updateInfo}
                  <div class="desc update-available">v{$updateInfo.version} is available</div>
                {/if}
              </div>
              <div class="update-controls">
                {#if updateCheckState === 'available' && $updateInfo}
                  <button class="btn-ghost" onclick={handleInstallFromAbout} disabled={installingFromAbout}>
                    {installingFromAbout ? 'Downloading…' : 'Install Now'}
                  </button>
                {:else}
                  <button
                    class="btn-ghost"
                    onclick={checkForUpdateManual}
                    disabled={updateCheckState === 'checking'}
                  >
                    {updateCheckState === 'checking' ? 'Checking…' : 'Check for Updates'}
                  </button>
                {/if}
              </div>
            </div>
          {/if}
        </div>
        {/key}
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
    transition: background 0.3s ease-out;
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
    transition: left 0.35s cubic-bezier(0.22, 1, 0.36, 1);
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

  .history-dropdown {
    position: relative;
    flex-shrink: 0;
  }

  .coming-soon-row { opacity: 0.6; }

  .coming-soon-label { display: flex; align-items: center; gap: 8px; }

  .coming-soon-badge {
    font-family: var(--mono);
    font-size: 9.5px;
    font-weight: 500;
    letter-spacing: 0.05em;
    color: var(--ink-mute);
    background: var(--paper);
    border: 1px solid var(--line-strong);
    border-radius: 4px;
    padding: 1px 6px;
    text-transform: uppercase;
    vertical-align: middle;
  }

  .privacy-eye-wrap {
    position: relative;
    display: inline-flex;
    align-items: center;
  }

  .privacy-eye {
    color: var(--ink-mute);
    cursor: default;
    flex-shrink: 0;
  }

  .privacy-tooltip {
    display: none;
    position: absolute;
    left: 50%;
    bottom: calc(100% + 6px);
    transform: translateX(-50%);
    background: var(--ink);
    color: var(--amber-50);
    font-size: 11px;
    font-family: var(--sans);
    font-weight: 400;
    white-space: nowrap;
    padding: 4px 9px;
    border-radius: 6px;
    pointer-events: none;
    z-index: 20;
    box-shadow: 0 2px 8px rgba(13,10,8,0.18);
  }

  .privacy-eye-wrap:hover .privacy-tooltip { display: block; }

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

  /* App Mappings */
  .mapping-list {
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    overflow: hidden;
    margin-bottom: 20px;
  }

  .mapping-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 12px;
    border-bottom: 1px solid var(--line);
    background: var(--bg-elev);
  }

  .mapping-row:last-child { border-bottom: none; }

  .mapping-app-info {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 7px;
  }

  .mapping-app-name {
    font-size: 12.5px;
    font-weight: 500;
    color: var(--ink-strong);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 130px;
  }

  .mapping-exe-pill {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--ink-mute);
    background: var(--paper);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 1px 5px;
    flex-shrink: 0;
  }

  .mapping-arrow-icon {
    color: var(--ink-mute);
    flex-shrink: 0;
  }

  .mapping-profile-badge {
    font-family: var(--mono);
    font-size: 10px;
    font-weight: 500;
    color: var(--accent);
    background: var(--accent-soft);
    border: 1px solid color-mix(in oklab, var(--accent) 30%, transparent);
    border-radius: 4px;
    padding: 2px 7px;
    flex-shrink: 0;
    text-transform: lowercase;
  }

  .mapping-delete-btn {
    background: none;
    border: none;
    padding: 3px;
    border-radius: 4px;
    color: var(--ink-mute);
    cursor: pointer;
    display: flex;
    align-items: center;
    flex-shrink: 0;
    margin-left: auto;
  }

  .mapping-delete-btn:hover { color: var(--ink-strong); background: var(--paper); }

  .mapping-empty {
    font-size: 12px;
    color: var(--ink-mute);
    padding: 16px 0 20px;
    font-style: italic;
  }

  .add-mapping-section {
    border-top: 1px solid var(--line);
    padding-top: 16px;
  }

  .add-mapping-label {
    font-size: 11px;
    font-weight: 500;
    color: var(--ink-mute);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-family: var(--mono);
    margin-bottom: 10px;
  }

  .add-mapping-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .app-picker-wrap {
    position: relative;
    flex: 1;
  }

  .app-search-input {
    width: 100%;
    box-sizing: border-box;
    font-family: var(--sans);
    font-size: 12px;
    background: transparent;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    padding: 5px 10px;
    color: var(--ink-strong);
  }

  .app-search-input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .app-picker-menu {
    position: absolute;
    left: 0;
    top: calc(100% + 4px);
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    box-shadow: 0 8px 24px rgba(13,10,8,0.14);
    width: 100%;
    max-height: 180px;
    overflow-y: auto;
    z-index: 10;
  }

  .app-picker-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 7px 10px;
    font-family: var(--sans);
    background: none;
    border: none;
    border-bottom: 1px solid var(--line);
    cursor: pointer;
    text-align: left;
    gap: 8px;
  }

  .app-picker-item:last-child { border-bottom: none; }
  .app-picker-item:hover { background: var(--paper); }

  .app-picker-name {
    font-size: 12px;
    color: var(--ink-strong);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
  }

  .app-picker-exe {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--ink-mute);
    flex-shrink: 0;
  }

  .profile-select {
    font-family: var(--sans);
    font-size: 12px;
    background: var(--bg-elev);
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    padding: 5px 8px;
    color: var(--ink-strong);
    cursor: pointer;
    flex-shrink: 0;
  }

  .profile-select:focus { outline: none; border-color: var(--accent); }

  .add-btn { flex-shrink: 0; }

  .add-preview {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 8px;
    padding: 4px 2px;
  }

  .keybind-btn {
    cursor: pointer;
    border: 1px solid transparent;
    transition: all 0.2s;
    user-select: none;
  }
  .keybind-btn:hover {
    background: rgba(255, 255, 255, 0.15);
  }
  .keybind-btn.recording {
    background: var(--accent);
    color: #fff;
    animation: pulse 1.5s infinite;
  }
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.7; }
  }

  /* Mic gain slider */
  .gain-row {
    flex-direction: column;
    align-items: stretch;
    gap: 0;
  }

  .gain-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
  }

  .gain-value {
    font-family: var(--mono);
    font-size: 13px;
    font-weight: 500;
    color: var(--jap-400);
    min-width: 36px;
    text-align: right;
    flex-shrink: 0;
  }

  .gain-slider-wrap {
    margin-top: 10px;
    width: 100%;
  }

  .gain-slider {
    -webkit-appearance: none;
    appearance: none;
    width: 100%;
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(
      to right,
      var(--jap-400) 0%,
      var(--jap-400) var(--pct),
      var(--line-strong) var(--pct),
      var(--line-strong) 100%
    );
    outline: none;
    cursor: pointer;
    border: none;
    display: block;
  }

  .gain-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: white;
    border: 2px solid var(--jap-400);
    box-shadow: 0 1px 4px rgba(217, 119, 87, 0.35);
    cursor: pointer;
    transition: box-shadow 0.15s ease, transform 0.15s ease;
  }

  .gain-slider::-webkit-slider-thumb:hover {
    box-shadow: 0 2px 8px rgba(217, 119, 87, 0.45);
    transform: scale(1.1);
  }

  .gain-slider::-webkit-slider-thumb:active {
    box-shadow: 0 2px 10px rgba(217, 119, 87, 0.55);
    transform: scale(1.15);
  }

  .gain-slider::-moz-range-thumb {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: white;
    border: 2px solid var(--jap-400);
    box-shadow: 0 1px 4px rgba(217, 119, 87, 0.35);
    cursor: pointer;
  }

  .gain-ticks {
    display: flex;
    justify-content: space-between;
    margin-top: 5px;
    font-size: 10px;
    color: var(--ink-mute);
    font-family: var(--mono);
    user-select: none;
  }

  .gain-tip {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 10px;
    padding: 7px 10px;
    background: var(--jap-50);
    border: 1px solid var(--jap-100);
    border-radius: 7px;
    font-size: 11.5px;
    color: var(--jap-700);
    line-height: 1.4;
  }

  .gain-tip svg {
    flex-shrink: 0;
    color: var(--jap-400);
  }

  .gain-tip strong {
    font-weight: 600;
  }

  .update-controls {
    flex-shrink: 0;
  }

  .update-ok {
    color: #5a8a52;
  }

  .update-available {
    color: var(--accent);
  }
</style>
