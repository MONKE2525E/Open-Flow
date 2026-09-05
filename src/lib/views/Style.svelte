<script lang="ts">
  import { invoke, emit } from '../tauri';
  import { onMount } from 'svelte';
  import { crossfade, fly } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { saveSetting, type CleanupIntensity, type ToneId } from '../settings';
  import { appStore } from '../stores';
  import { MOTION_MS, MOTION_PX, STYLE_TAB_ORDER, directionFromOrder, motionMs, motionPx, pageSwap } from '../motion';

  const [send, receive] = crossfade({
    duration: motionMs(MOTION_MS.base),
    easing: expoOut,
  });

  let tab = $state('cleanup');
  let tabDir = $state<1 | -1>(1);
  let mountedTabs = $state<Record<string, boolean>>({});
  let intensity = $state('medium');
  let tone = $state('casual');

  const tabs = [
    { id: 'cleanup', label: 'Cleanup', pill: '' },
    { id: 'personal', label: 'Personal Tone', pill: '' },
  ];

  const cleanupCards = [
    { id: 'none', name: 'Off', desc: 'Keep the raw transcript. Dual transcription may still reconcile candidates.', sample: "so um i was thinking like we should probably leave a bit earlier you know cause there's gonna be traffic i think" },
    { id: 'light', name: 'Light', desc: 'Remove non-semantic speech artifacts and fix basics. Keep wording, order, and detail.', sample: "I was thinking we should probably leave a bit earlier because there's going to be traffic, I think." },
    { id: 'medium', name: 'Medium', desc: 'Improve flow and remove redundancy with light restructuring. Preserve every distinct detail.', sample: "I think we should leave a bit earlier. There's going to be traffic." },
    { id: 'high', name: 'Strong', desc: 'Rewrite concisely and directly. Preserve facts, constraints, qualifiers, and emphasis.', sample: 'Leave early. There will be traffic.' },
  ];

  const personalCards = [
    { id: 'casual', name: 'Casual', desc: 'Contractions, normal casing and punctuation, and the speaker’s casual voice.', sample: "Hey, are you free for lunch tomorrow? Let's do 12 if that works." },
    { id: 'formal', name: 'Formal', desc: 'Professional wording and punctuation. No invented politeness or extra content.', sample: 'Are you available for lunch tomorrow? Let us meet at 12 if that works for you.' },
    { id: 'very_casual', name: 'Very Casual', desc: 'Mostly lowercase, contractions, and minimal readable punctuation. Preserve profanity and emphasis.', sample: "hey are you free for lunch tomorrow let's do 12 if that works" },
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

  function selectTab(id: string) {
    if (id === tab) return;
    tabDir = directionFromOrder(tab, id, STYLE_TAB_ORDER);
    tab = id;
  }

  // The tabs declare role="tab", so they get the full APG pattern: only the
  // selected tab is in the tab order, and Left/Right/Home/End move and select
  // with automatic activation (the panels are lightweight).
  let tablistEl = $state<HTMLDivElement | null>(null);

  function handleTablistKeydown(event: KeyboardEvent) {
    const tabButtons = tablistEl?.querySelectorAll<HTMLButtonElement>('.tab') ?? [];
    if (tabButtons.length === 0) return;
    const index = Array.from(tabButtons).indexOf(document.activeElement as HTMLButtonElement);
    if (index === -1) return;

    let next: number | null = null;
    if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = tabButtons.length - 1;
    else if (event.key === 'ArrowLeft') next = (index - 1 + tabButtons.length) % tabButtons.length;
    else if (event.key === 'ArrowRight') next = (index + 1) % tabButtons.length;
    if (next === null) return;

    event.preventDefault();
    const target = tabButtons[next];
    target.focus();
    const id = target.id.replace('style-tab-', '');
    if (id !== tab) {
      tabDir = directionFromOrder(tab, id, STYLE_TAB_ORDER);
      tab = id;
    }
  }

  $effect(() => {
    if (!mountedTabs[tab]) {
      mountedTabs = { ...mountedTabs, [tab]: true };
    }
  });
</script>

<div class="content-inner">
  <h1 class="page-h">Style</h1>
  <p class="page-sub">How Verenu shapes your dictation.</p>

  {#if !appStore.cleanupEnabled}
    <div class="cleanup-off-banner">
      <p>
        <strong>Cleanup is turned off</strong>, so nothing on this page has any effect right now —
        tone, intensity, and app-specific overrides only apply during the cleanup step. Your
        choices below are kept, just not used.
      </p>
      <button type="button" class="cleanup-off-link ui-focus-ring" onclick={() => emit('open-flow:open-settings-section', 'general')}>
        Turn Cleanup back on in Settings → General
      </button>
    </div>
  {/if}

  {#if appStore.legacyFeaturesEnabled}
    <div class="tabs" role="tablist" tabindex="-1" bind:this={tablistEl} onkeydown={handleTablistKeydown}>
      {#each tabs as t}
        <button
          class="tab ui-focus-ring"
          class:active={tab === t.id}
          role="tab"
          id="style-tab-{t.id}"
          tabindex={tab === t.id ? 0 : -1}
          aria-selected={tab === t.id}
          aria-controls="style-panel-{t.id}"
          onclick={() => selectTab(t.id)}
        >
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

    <div class="tab-content-area" class:tab-content-disabled={!appStore.cleanupEnabled} aria-disabled={!appStore.cleanupEnabled} inert={!appStore.cleanupEnabled}>
      {#key tab}
        <div
          class="tab-wrapper"
          role="tabpanel"
          id="style-panel-{tab}"
          aria-labelledby="style-tab-{tab}"
          in:pageSwap={{ axis: 'x', distance: tabDir * motionPx(MOTION_PX.panel), duration: motionMs(MOTION_MS.panel) }}
          out:pageSwap={{ axis: 'x', distance: -tabDir * motionPx(MOTION_PX.panel), duration: motionMs(MOTION_MS.base + 40) }}
        >
          {#if tab === 'cleanup'}
            <p class="style-intro">Cleanup runs after transcription unless it is turned Off. <span>Choose how much rewriting Verenu does.</span></p>
            <div class="style-grid four">
              {#each cleanupCards as c}
                <button
                  type="button"
                  class="style-card"
                  class:active={intensity === c.id}
                  aria-pressed={intensity === c.id}
                  onclick={() => selectIntensity(c.id)}
                  in:fly={!mountedTabs.cleanup ? { y: motionPx(MOTION_PX.lift), duration: motionMs(MOTION_MS.panel), easing: expoOut } : undefined}
                >
                  <span class="style-card-title">{c.name}</span>
                  <span class="desc">{c.desc}</span>
                  <span class="style-sample">"{c.sample}"</span>
                </button>
              {/each}
            </div>
          {:else if tab === 'personal'}
            <p class="style-intro">Default tone. <span>Applies to any app not explicitly mapped.</span></p>
            <div class="style-grid">
              {#each personalCards as c}
                <button
                  type="button"
                  class="style-card"
                  class:active={tone === c.id}
                  aria-pressed={tone === c.id}
                  onclick={() => selectTone(c.id)}
                  in:fly={!mountedTabs.personal ? { y: motionPx(MOTION_PX.lift), duration: motionMs(MOTION_MS.panel), easing: expoOut } : undefined}
                >
                  <span class="style-card-title">{c.name}</span>
                  <span class="desc">{c.desc}</span>
                  <span class="style-sample" style="white-space: pre-wrap;">"{c.sample}"</span>
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {/key}
    </div>
  {:else}
    <div class="style-sections" class:tab-content-disabled={!appStore.cleanupEnabled} aria-disabled={!appStore.cleanupEnabled} inert={!appStore.cleanupEnabled}>
      <section class="style-section">
        <h2 class="style-section-h">Cleanup</h2>
        <p class="style-intro">Cleanup runs after transcription unless it is turned Off. <span>Choose how much rewriting Verenu does.</span></p>
        <div class="style-grid four">
          {#each cleanupCards as c}
            <button
              type="button"
              class="style-card"
              class:active={intensity === c.id}
              aria-pressed={intensity === c.id}
              onclick={() => selectIntensity(c.id)}
              in:fly={{ y: motionPx(MOTION_PX.lift), duration: motionMs(MOTION_MS.panel), easing: expoOut }}
            >
              <span class="style-card-title">{c.name}</span>
              <span class="desc">{c.desc}</span>
              <span class="style-sample">"{c.sample}"</span>
            </button>
          {/each}
        </div>
      </section>

      <hr class="style-divider" />

      <section class="style-section">
        <h2 class="style-section-h">Personal Tone</h2>
        <p class="style-intro">Default tone. <span>Applies to any app not explicitly mapped.</span></p>
        <div class="style-grid">
          {#each personalCards as c}
            <button
              type="button"
              class="style-card"
              class:active={tone === c.id}
              aria-pressed={tone === c.id}
              onclick={() => selectTone(c.id)}
              in:fly={{ y: motionPx(MOTION_PX.lift), duration: motionMs(MOTION_MS.panel), easing: expoOut }}
            >
              <span class="style-card-title">{c.name}</span>
              <span class="desc">{c.desc}</span>
              <span class="style-sample" style="white-space: pre-wrap;">"{c.sample}"</span>
            </button>
          {/each}
        </div>
      </section>
    </div>
  {/if}
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
    container-type: inline-size;
    container-name: style-page;
  }

  .tab-content-area {
    position: relative;
    flex: 1;
    display: grid;
  }

  .tab-content-disabled {
    opacity: 0.45;
    pointer-events: none;
    user-select: none;
  }

  .cleanup-off-banner {
    padding: 12px 14px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: color-mix(in srgb, var(--paper) 55%, var(--bg-elev));
    margin-bottom: 20px;
  }

  .cleanup-off-banner p {
    margin: 0 0 8px;
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--ink-mute);
  }

  .cleanup-off-banner strong {
    color: var(--ink-soft);
  }

  .cleanup-off-link {
    font-size: 12.5px;
    font-weight: 500;
    color: var(--accent-ink);
    background: transparent;
    border: 0;
    padding: 0;
    cursor: pointer;
    text-decoration: underline;
  }

  .cleanup-off-link:hover {
    opacity: 0.8;
  }

  .tab-wrapper {
    grid-area: 1 / 1;
  }

  .style-sections {
    display: flex;
    flex-direction: column;
    gap: 28px;
  }

  .style-divider {
    border: 0;
    border-top: 1px solid var(--line);
    margin: 0;
  }

  .style-section-h {
    font-family: var(--sans);
    font-size: 14px;
    font-weight: 500;
    letter-spacing: -0.01em;
    color: var(--ink-soft);
    margin: 0 0 4px;
  }

  .page-h {
    font-family: var(--sans);
    font-size: 23px;
    font-weight: 600;
    letter-spacing: -0.025em;
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
    font-family: var(--sans);
    font-size: 9px;
    background: transparent;
    color: var(--ink-mute);
    padding: 1px 6px;
    border-radius: 999px;
    text-transform: none;
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

  @container style-page (max-width: 760px) {
    .style-grid,
    .style-grid.four {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  @container style-page (max-width: 500px) {
    .style-grid,
    .style-grid.four {
      grid-template-columns: 1fr;
    }
  }

  .style-card {
    border: 1px solid var(--line);
    width: 100%;
    text-align: left;
    padding: 14px;
    background: var(--bg-elev);
    border-radius: var(--r-md);
    display: flex;
    flex-direction: column;
    min-height: 140px;
    cursor: pointer;
    transition: background 0.15s var(--ui-ease-out),
      border-color 0.15s var(--ui-ease-out);
  }

  .style-card:hover {
    background: var(--control-hover);
  }
  .style-card:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .style-card:active {
    opacity: 0.82;
  }

  .style-card.active {
    background: var(--control-active);
    border-color: var(--line-strong);
  }

  .style-card-title {
    display: block;
    font-family: var(--sans);
    font-size: 14px;
    font-weight: 500;
    margin: 0 0 2px;
    letter-spacing: 0;
    color: var(--ink);
  }

  .style-card .desc {
    display: block;
    font-size: 12px;
    color: var(--ink-mute);
    margin-bottom: 14px;
    line-height: 1.45;
  }

  .style-sample {
    display: block;
    margin-top: auto;
    font-family: var(--sans);
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
