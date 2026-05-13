<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy, tick } from 'svelte';
  import { fly, slide, crossfade } from 'svelte/transition';
  import { flip } from 'svelte/animate';
  import { expoOut } from 'svelte/easing';

  const [send, receive] = crossfade({
    duration: 300,
    easing: expoOut,
  });

  let tab = $state('cleanup');
  let intensity = $state('medium');
  let tone = $state('casual');

  const tabs = [
    { id: 'cleanup',  label: 'Auto-cleanup', pill: '' },
    { id: 'personal', label: 'Personal Tone',pill: '' },
    { id: 'apps',     label: 'App Mappings', pill: 'New' },
  ];

  const cleanupCards = [
    { id: 'none',   name: 'Verbatim', desc: 'Exactly what you said, word for word.',  sample: "so um i was thinking like we should probably leave a bit earlier you know cause there's gonna be traffic i think" },
    { id: 'light',  name: 'Light',    desc: 'Removes filler words, nothing else.',    sample: "i was thinking we should probably leave a bit earlier, cause there's gonna be traffic i think" },
    { id: 'medium', name: 'Medium',   desc: 'Cleans it up, keeps your words.',        sample: "I think we should leave a bit earlier — there's going to be traffic." },
    { id: 'high',   name: 'Direct',   desc: 'Rewrites for max brevity.',              sample: "Leave early. Traffic." },
  ];

  const personalCards = [
    { id: 'formal', name: 'Formal', desc: 'Proper capitalization. Full punctuation.', sample: "Hey, are you free for lunch tomorrow? Let's do 12 if that works." },
    { id: 'casual', name: 'Casual', desc: 'Caps and light punctuation.',              sample: "Hey, are you free for lunch tomorrow? Let's do 12 if that works" },
    { id: 'plain',  name: 'Plain',  desc: 'No caps, minimal punctuation.',            sample: "hey are you free for lunch tomorrow let's do 12 if that works" },
    { id: 'code',   name: 'Code',   desc: 'No conversational filler. Raw syntax.',    sample: "def hello_world():\n    print('hello')" },
  ];

  const profileOptions = [
    { id: 'casual', label: 'Casual' },
    { id: 'formal', label: 'Formal' },
    { id: 'plain',  label: 'Plain' },
    { id: 'code',   label: 'Code' },
  ];

  interface InstalledApp { name: string; exe: string; }

  let mappings = $state<{ exe: string; profile: string }[]>([]);
  let newExe = $state('');
  let newProfile = $state('casual');
  let profileDropdownOpen = $state(false);

  let installedApps = $state<InstalledApp[]>([]);
  let areAppsLoaded = $state(false);
  let appSearch = $state('');
  let appPickerOpen = $state(false);

  let filteredApps = $derived(appSearch
    ? installedApps.filter(a =>
        a.name.toLowerCase().includes(appSearch.toLowerCase()) ||
        a.exe.toLowerCase().includes(appSearch.toLowerCase())
      ).slice(0, 40)
    : installedApps.slice(0, 40));

  async function loadInstalledApps() {
    if (areAppsLoaded) return;
    try {
      installedApps = await invoke<InstalledApp[]>('get_installed_apps');
      areAppsLoaded = true;
    } catch { /* dev mode */ }
  }

  function pickApp(app: InstalledApp) {
    newExe = app.exe;
    appSearch = app.name;
    appPickerOpen = false;
  }

  function closeAppPicker(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (!target.closest('.app-picker-wrap')) appPickerOpen = false;
  }

  $effect(() => {
    if (appPickerOpen) {
      tick().then(() => window.addEventListener('click', closeAppPicker, { once: true }));
    }
  });

  function handleWindowClick() { profileDropdownOpen = false; }

  onMount(async () => {
    window.addEventListener('click', handleWindowClick);
    loadInstalledApps();
    try {
      const [savedTone, savedIntensity, savedMappings] = await Promise.all([
        invoke<string | null>('get_setting', { key: 'default_tone' }),
        invoke<string | null>('get_setting', { key: 'cleanup_intensity' }),
        invoke<{ exe: string; profile: string }[] | null>('get_setting', { key: 'app_mappings' }),
      ]);
      if (savedTone) tone = savedTone as string;
      if (savedIntensity) intensity = savedIntensity as string;
      if (savedMappings) mappings = savedMappings as { exe: string; profile: string }[];
    } catch { /* dev mode without Tauri */ }
  });

  onDestroy(() => { window.removeEventListener('click', handleWindowClick); });

  function selectIntensity(id: string) {
    intensity = id;
    invoke('save_setting', { key: 'cleanup_intensity', value: id });
  }

  function selectTone(id: string) {
    tone = id;
    invoke('save_setting', { key: 'default_tone', value: id });
  }

  function saveMappings(updated: { exe: string; profile: string }[]) {
    mappings = updated;
    invoke('save_setting', { key: 'app_mappings', value: updated });
  }

  function addMapping() {
    if (newExe.trim()) {
      saveMappings([...mappings, { exe: newExe.trim().toLowerCase(), profile: newProfile }]);
      newExe = '';
      appSearch = '';
      appPickerOpen = false;
    }
  }

  function removeMapping(index: number) {
    saveMappings(mappings.filter((_, i) => i !== index));
  }
</script>

<div class="content-inner">
  <h1 class="page-h">Style</h1>
  <p class="page-sub">How Open Flow shapes your dictation.</p>

  <div class="tabs">
    {#each tabs as t}
      <button class="tab" class:active={tab === t.id} onclick={() => (tab = t.id)}>
        {t.label}
        {#if t.pill}
          <span class="pill">{t.pill}</span>
        {/if}
        {#if tab === t.id}
          <div class="active-bar" in:receive={{key: 'tab'}} out:send={{key: 'tab'}}></div>
        {/if}
      </button>
    {/each}
  </div>

  <div class="tab-content-area">
    {#key tab}
      <div class="tab-wrapper" in:fly={{ y: 8, duration: 400, delay: 150, easing: expoOut }} out:fly={{ y: -8, duration: 150, easing: expoOut }}>
        {#if tab === 'cleanup'}
          <p class="style-intro">Auto-cleanup runs on every dictation. <span>Choose how much rewriting Open Flow does.</span></p>
          <div class="style-grid four">
            {#each cleanupCards as c}
              <div class="style-card" class:active={intensity === c.id} role="button" tabindex="0"
                onclick={() => selectIntensity(c.id)}
                onkeydown={(e) => e.key === 'Enter' && selectIntensity(c.id)}>
                <h4>{c.name}</h4>
                <p class="desc">{c.desc}</p>
                <div class="style-sample">"{c.sample}"</div>
              </div>
            {/each}
          </div>
        {:else if tab === 'personal'}
          <p class="style-intro">Default tone. <span>Applies to any app not explicitly mapped.</span></p>
          <div class="style-grid">
            {#each personalCards as c}
              <div class="style-card" class:active={tone === c.id} role="button" tabindex="0"
                onclick={() => selectTone(c.id)}
                onkeydown={(e) => e.key === 'Enter' && selectTone(c.id)}>
                <h4>{c.name}</h4>
                <p class="desc">{c.desc}</p>
                <div class="style-sample" style="white-space: pre-wrap;">"{c.sample}"</div>
              </div>
            {/each}
          </div>
        {:else if tab === 'apps'}
          <p class="style-intro">App Mappings. <span>Automatically switch tone based on the active window.</span></p>
          
          <div class="mapping-list">
            {#each mappings as m, i (m.exe)}
              <div class="mapping-item" animate:flip={{duration: 300, easing: expoOut}} in:fly={{y: 10, duration: 300, easing: expoOut}} out:slide={{duration: 200, easing: expoOut}}>
                <div class="mapping-info">
                  <span class="exe">{m.exe}</span>
                  <span class="arr">→</span>
                  <span class="prof">{m.profile}</span>
                </div>
                <button class="icon-btn del-btn" aria-label="Remove mapping" onclick={() => removeMapping(i)}>✕</button>
              </div>
            {/each}
          </div>

          <div class="add-mapping">
            <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
            <div class="app-picker-wrap" onclick={(e) => e.stopPropagation()}>
              <input
                class="app-search-input"
                placeholder={areAppsLoaded ? 'Search apps…' : 'Loading apps…'}
                bind:value={appSearch}
                onfocus={() => { appPickerOpen = true; }}
                oninput={() => { newExe = ''; appPickerOpen = true; }}
                onkeydown={(e) => e.key === 'Enter' && addMapping()}
              />
              {#if appPickerOpen && filteredApps.length > 0}
                <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
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
            <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
            <div class="profile-select" onclick={(e) => e.stopPropagation()}>
              <button class="profile-select-btn" onclick={() => (profileDropdownOpen = !profileDropdownOpen)}>
                <span>{profileOptions.find(p => p.id === newProfile)?.label ?? 'Casual'}</span>
                <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                  <path d="m6 9 6 6 6-6"/>
                </svg>
              </button>
              {#if profileDropdownOpen}
                <div class="profile-menu">
                  {#each profileOptions as opt}
                    <button
                      class="profile-item"
                      class:active={newProfile === opt.id}
                      onclick={() => { newProfile = opt.id; profileDropdownOpen = false; }}
                    >{opt.label}</button>
                  {/each}
                </div>
              {/if}
            </div>
            <button class="btn-primary" onclick={addMapping}>Add</button>
          </div>
        {/if}
      </div>
    {/key}
  </div>
</div>

<style>
  .content-inner {
    padding: 18px 28px 36px;
    max-width: 920px;
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 100%;
  }

  .tab-content-area {
    position: relative;
    flex: 1;
    display: grid;
  }

  .tab-wrapper {
    grid-area: 1 / 1;
  }

  .page-h {
    font-family: var(--serif);
    font-size: 26px;
    font-weight: 500;
    letter-spacing: -0.02em;
    margin: 0 0 4px;
    line-height: 1.1;
    color: var(--ink);
  }

  .page-sub { color: var(--ink-mute); font-size: 12.5px; margin: 0 0 22px; }

  .tabs {
    display: flex;
    gap: 22px;
    border-bottom: 1px solid var(--line);
    margin-bottom: 22px;
  }

  .tab {
    padding: 0 0 11px;
    font-size: 13px;
    color: var(--ink-mute);
    border: 0;
    background: transparent;
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    position: relative;
  }

  .tab:hover { color: var(--ink-soft); }
  .tab.active { color: var(--ink); font-weight: 500; }

  .active-bar {
    position: absolute;
    bottom: -1px;
    left: 0;
    right: 0;
    height: 1px;
    background: var(--ink);
  }

  .tab .pill {
    font-family: var(--mono);
    font-size: 9px;
    background: transparent;
    color: var(--ink-mute);
    padding: 1px 6px;
    border-radius: 999px;
    text-transform: uppercase;
    border: 1px solid var(--line);
  }

  .style-intro {
    font-size: 13px;
    color: var(--ink-soft);
    max-width: 540px;
    margin-bottom: 20px;
  }

  .style-intro span { color: var(--ink-mute); }

  .style-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }

  .style-grid.four { grid-template-columns: repeat(4, 1fr); }

  .style-card {
    padding: 14px;
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    display: flex;
    flex-direction: column;
    min-height: 160px;
    cursor: pointer;
    transition: all 0.2s cubic-bezier(0.22, 1, 0.36, 1);
  }

  .style-card:hover { 
    background: var(--amber-50); 
    transform: translateY(-2px);
    box-shadow: 0 4px 12px rgba(13, 10, 8, 0.05);
  }

  .style-card:active {
    transform: translateY(0) scale(0.98);
    transition: all 0.1s cubic-bezier(0.22, 1, 0.36, 1);
  }

  .style-card.active {
    background: var(--accent-soft);
    border-color: var(--jap-200);
  }

  .style-card h4 {
    font-family: var(--serif);
    font-size: 16px;
    font-weight: 500;
    margin: 0 0 2px;
    letter-spacing: -0.015em;
    color: var(--ink);
  }

  .style-card .desc {
    font-size: 12px;
    color: var(--ink-mute);
    margin-bottom: 14px;
    line-height: 1.45;
  }

  .style-sample {
    margin-top: auto;
    font-family: var(--serif);
    font-style: italic;
    font-size: 13.5px;
    line-height: 1.5;
    color: var(--ink-soft);
    transition: color 0.2s cubic-bezier(0.22, 1, 0.36, 1);
  }

  .style-card.active .style-sample { color: var(--accent-ink); }

  .mapping-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 16px;
    max-width: 480px;
  }

  .mapping-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
  }

  .mapping-info {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .exe {
    font-family: var(--mono);
    font-size: 13px;
    color: var(--ink);
  }

  .arr { color: var(--ink-mute); font-size: 12px; }

  .prof {
    font-size: 13px;
    color: var(--accent-ink);
    background: var(--accent-soft);
    padding: 2px 8px;
    border-radius: 4px;
    text-transform: capitalize;
  }

  .add-mapping {
    display: flex;
    gap: 10px;
    max-width: 480px;
    align-items: center;
  }

  .app-picker-wrap {
    position: relative;
    flex: 1;
  }

  .app-search-input {
    width: 100%;
    box-sizing: border-box;
    background: transparent;
    border: 1px solid var(--line);
    padding: 0 12px;
    height: 34px;
    border-radius: var(--r-sm);
    font-size: 13px;
    font-family: var(--sans);
    color: var(--ink);
    outline: none;
  }
  .app-search-input:focus { border-color: var(--ink-mute); }

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
    z-index: 20;
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
    color: var(--ink);
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
    position: relative;
    flex-shrink: 0;
  }

  .profile-select-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    padding: 0 12px;
    background: transparent;
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    font-size: 13px;
    font-family: var(--sans);
    color: var(--ink);
    cursor: pointer;
    white-space: nowrap;
  }

  .profile-select-btn:hover { background: var(--amber-50); border-color: var(--ink-mute); }

  .profile-menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    box-shadow: 0 8px 24px rgba(13,10,8,0.14);
    min-width: 110px;
    z-index: 20;
    overflow: hidden;
  }

  .profile-item {
    display: block;
    width: 100%;
    text-align: left;
    padding: 8px 12px;
    font-size: 13px;
    font-family: var(--sans);
    color: var(--ink);
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--line);
    cursor: pointer;
  }

  .profile-item:last-child { border-bottom: none; }
  .profile-item:hover { background: var(--paper); }
  .profile-item.active { background: var(--accent-soft); color: var(--ink); font-weight: 500; }

  .btn-primary {
    background: var(--ink);
    color: var(--paper);
    border: none;
    padding: 0 14px;
    height: 34px;
    border-radius: var(--r-sm);
    font-size: 13px;
    cursor: pointer;
  }
  .btn-primary:hover { background: var(--ink-soft); }

  .del-btn {
    background: transparent;
    border: none;
    color: var(--ink-mute);
    cursor: pointer;
    font-size: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: 4px;
  }
  .del-btn:hover { background: var(--amber-100); color: var(--ink); }
</style>
