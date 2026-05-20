<script lang="ts">
  import { devModeEnabled, settingsOpen } from '../stores';
  import { icons } from '../icons';
  import { onDestroy, onMount } from 'svelte';
  import { fly, fade } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { getVersion } from '@tauri-apps/api/app';
  import { MOTION_MS, MOTION_PX, SETTINGS_SECTION_ORDER, directionFromOrder, motionMs, motionPx } from '../motion';

  import GeneralSection from '../components/settings/GeneralSection.svelte';
  import AppMappingsSection from '../components/settings/AppMappingsSection.svelte';
  import ApiKeysSection from '../components/settings/ApiKeysSection.svelte';
  import ModelsSection from '../components/settings/ModelsSection.svelte';
  import PrivacySection from '../components/settings/PrivacySection.svelte';
  import AudioSection from '../components/settings/AudioSection.svelte';
  import AboutSection from '../components/settings/AboutSection.svelte';
  import DeveloperSection from '../components/settings/DeveloperSection.svelte';

  let section = $state('general');
  let animDir: 1 | -1 = $state(1);
  let appVersion = $state('');

  const sectionOrder = SETTINGS_SECTION_ORDER;

  const navSections = $derived([
    { group: 'Settings', items: [
      { id: 'general',  label: 'General',      icon: 'sliders'  as keyof typeof icons },
      { id: 'apps',     label: 'App Mappings', icon: 'apps'     as keyof typeof icons },
      { id: 'keys',     label: 'API Keys',     icon: 'key'      as keyof typeof icons },
      { id: 'models',   label: 'Models',       icon: 'command'  as keyof typeof icons },
      { id: 'privacy',  label: 'Privacy',      icon: 'lock'     as keyof typeof icons },
      { id: 'advanced', label: 'Advanced',  icon: 'mic'      as keyof typeof icons },
      ...($devModeEnabled ? [{ id: 'developer', label: 'Developer', icon: 'command' as keyof typeof icons }] : []),
    ]},
    { group: 'Account', items: [
      { id: 'about', label: 'About', icon: 'help' as keyof typeof icons },
    ]},
  ]);

  onMount(async () => {
    appVersion = await getVersion();
  });

  function close() { $settingsOpen = false; }

  function goTo(id: string) {
    if (id === section) return;
    animDir = directionFromOrder(section, id, sectionOrder);
    section = id;
  }

  $effect(() => {
    if (!$devModeEnabled && section === 'developer') {
      section = 'about';
    }
  });
</script>

{#if $settingsOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    class="settings-overlay"
    role="button"
    tabindex="0"
    transition:fade={{ duration: 200 }}
    onclick={close}
    onkeydown={(e) => (e.key === 'Enter' || e.key === 'Escape') && close()}
  >
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div
      class="settings-modal"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
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
            class="panel scroll-styled scroll-thumb-elev"
            in:fly={{ y: animDir * motionPx(MOTION_PX.page), duration: motionMs(MOTION_MS.page + 120), easing: expoOut }}
            out:fly={{ y: -animDir * motionPx(MOTION_PX.page), duration: motionMs(MOTION_MS.base + 100), easing: expoOut }}
          >
            {#if section === 'general'}
              <GeneralSection />
            {:else if section === 'apps'}
              <AppMappingsSection />
            {:else if section === 'keys'}
              <ApiKeysSection />
            {:else if section === 'models'}
              <ModelsSection />
            {:else if section === 'privacy'}
              <PrivacySection />
            {:else if section === 'advanced'}
              <AudioSection />
            {:else if section === 'about'}
              <AboutSection {appVersion} />
            {:else if section === 'developer' && $devModeEnabled}
              <DeveloperSection />
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
    background: var(--overlay);
    backdrop-filter: blur(2px);
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
    box-shadow: var(--shadow-elev);
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
  .settings-nav-item.active { color: var(--ink); font-weight: 500; background: var(--bg-elev); }
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
    scrollbar-gutter: stable;
  }

  /* Shared styles for all section components — scoped to .settings-body */
  .settings-body :global(.settings-h) {
    font-family: var(--serif);
    font-size: 19px;
    font-weight: 500;
    margin: 0 0 14px;
    letter-spacing: -0.015em;
    color: var(--ink);
  }

  .settings-body :global(.setting-row) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 13px 0;
    border-top: 1px solid var(--line);
  }

  .settings-body :global(.setting-row:last-of-type) { border-bottom: 1px solid var(--line); }

  .settings-body :global(.label) { font-size: 13px; font-weight: 500; color: var(--ink-strong); }
  .settings-body :global(.desc)  { font-size: 12px; color: var(--ink-mute); margin-top: 3px; }

  .settings-body :global(.panel-note) {
    font-size: 12px;
    color: var(--ink-mute);
    margin: 0 0 16px;
    line-height: 1.5;
  }

  .settings-body :global(.btn-ghost) {
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

  .settings-body :global(.btn-ghost:hover) { background: var(--control-hover); }
  .settings-body :global(.btn-ghost:disabled) { opacity: 0.4; cursor: default; }

  .settings-body :global(.badge) {
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

  .settings-body :global(.key-badge) {
    font-family: var(--mono);
    font-size: 11px;
    letter-spacing: 0.04em;
  }
</style>
