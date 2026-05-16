<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { fly, crossfade } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { saveSetting, type CleanupIntensity, type ToneId } from '../settings';
  import AppMappingsEditor from '../components/AppMappingsEditor.svelte';

  const [send, receive] = crossfade({
    duration: 300,
    easing: expoOut,
  });

  let tab = $state('cleanup');
  let intensity = $state('medium');
  let tone = $state('casual');

  const tabs = [
    { id: 'cleanup', label: 'Auto-cleanup', pill: '' },
    { id: 'personal', label: 'Personal Tone', pill: '' },
    { id: 'apps', label: 'App Mappings', pill: 'New' },
  ];

  const cleanupCards = [
    { id: 'none', name: 'Verbatim', desc: 'Exactly what you said, word for word.', sample: "so um i was thinking like we should probably leave a bit earlier you know cause there's gonna be traffic i think" },
    { id: 'light', name: 'Light', desc: 'Removes filler words, nothing else.', sample: "i was thinking we should probably leave a bit earlier, cause there's gonna be traffic i think" },
    { id: 'medium', name: 'Medium', desc: 'Cleans it up, keeps your words.', sample: "I think we should leave a bit earlier, there's going to be traffic." },
    { id: 'high', name: 'Direct', desc: 'Rewrites for max brevity.', sample: 'Leave early. Traffic.' },
  ];

  const personalCards = [
    { id: 'casual', name: 'Casual', desc: 'Conversational. Light caps and punctuation.', sample: "Hey, are you free for lunch tomorrow? Let's do 12 if that works" },
    { id: 'formal', name: 'Formal', desc: 'Professional prose. Full punctuation, formal vocabulary.', sample: 'Hey, are you free for lunch tomorrow? I would love to do 12 if that works for you.' },
    { id: 'very_casual', name: 'Very Casual', desc: 'All lowercase, almost no punctuation.', sample: "hey are you free for lunch tomorrow let's do 12 if that works" },
  ];

  onMount(async () => {
    try {
      const [savedTone, savedIntensity] = await Promise.all([
        invoke<string | null>('get_setting', { key: 'default_tone' }),
        invoke<string | null>('get_setting', { key: 'cleanup_intensity' }),
      ]);
      if (savedTone) tone = savedTone as string;
      if (savedIntensity) intensity = savedIntensity as string;
    } catch {
      // Dev mode without Tauri.
    }
  });

  function selectIntensity(id: string) {
    intensity = id;
    saveSetting('cleanup_intensity', id as CleanupIntensity);
  }

  function selectTone(id: string) {
    tone = id;
    saveSetting('default_tone', id as ToneId);
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
              <div
                class="style-card"
                class:active={intensity === c.id}
                role="button"
                tabindex="0"
                onclick={() => selectIntensity(c.id)}
                onkeydown={(e) => e.key === 'Enter' && selectIntensity(c.id)}
              >
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
              <div
                class="style-card"
                class:active={tone === c.id}
                role="button"
                tabindex="0"
                onclick={() => selectTone(c.id)}
                onkeydown={(e) => e.key === 'Enter' && selectTone(c.id)}
              >
                <h4>{c.name}</h4>
                <p class="desc">{c.desc}</p>
                <div class="style-sample" style="white-space: pre-wrap;">"{c.sample}"</div>
              </div>
            {/each}
          </div>
        {:else if tab === 'apps'}
          <AppMappingsEditor
            showHeading={false}
            intro="Give specific apps their own tone. Open Flow switches automatically while you type."
            emptyText="No app tones yet."
            addLabel="Add App Tone"
          />
        {/if}
      </div>
    {/key}
  </div>
</div>

<style>
  .content-inner {
    width: min(100%, var(--page-max));
    margin-inline: auto;
    padding: var(--page-pad-y) var(--page-pad-x) 36px;
    min-width: 0;
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
    letter-spacing: 0;
    margin: 0 0 4px;
    line-height: 1.1;
    color: var(--ink);
  }

  .page-sub {
    color: var(--ink-mute);
    font-size: 12.5px;
    margin: 0 0 22px;
  }

  .tabs {
    display: flex;
    flex-wrap: wrap;
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
    grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
    gap: 10px;
  }

  .style-grid.four {
    grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
  }

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
    background: var(--control-hover);
    transform: translateY(-2px);
    box-shadow: var(--shadow-popover);
  }

  .style-card:active {
    transform: translateY(0) scale(0.98);
    transition: all 0.1s cubic-bezier(0.22, 1, 0.36, 1);
  }

  .style-card.active {
    background: var(--accent-soft);
    border-color: var(--accent);
  }

  .style-card h4 {
    font-family: var(--serif);
    font-size: 16px;
    font-weight: 500;
    margin: 0 0 2px;
    letter-spacing: 0;
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

  @media (max-width: 720px) {
    .tabs {
      gap: 14px;
    }
  }
</style>
