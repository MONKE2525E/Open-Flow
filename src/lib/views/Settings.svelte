<script lang="ts">
  import { appStore } from '../stores';
  import { onMount } from 'svelte';
  import { listen } from '../tauri';
  import { fade } from 'svelte/transition';
  import { MOTION_MS, MOTION_PX, SETTINGS_SECTION_ORDER, directionFromOrder, modalBackdrop, motionMs, motionPx, pageSwap } from '../motion';
  import { isSettingsSectionId } from '../settingsSections';
  import { scrollEdges, type ScrollEdgeCallback } from '../scrollFade';

  import GeneralSection from '../components/settings/GeneralSection.svelte';
  import AppMappingsSection from '../components/settings/AppMappingsSection.svelte';
  import ApiKeysSection from '../components/settings/ApiKeysSection.svelte';
  import ModelsSection from '../components/settings/ModelsSection.svelte';
  import PrivacySection from '../components/settings/PrivacySection.svelte';
  import AudioSection from '../components/settings/AudioSection.svelte';
  import PermissionsSection from '../components/settings/PermissionsSection.svelte';
  import AboutSection from '../components/settings/AboutSection.svelte';
  import DeveloperSection from '../components/settings/DeveloperSection.svelte';
  import { isMac } from '../platform';

  let settingsPageEl = $state<HTMLDivElement | null>(null);
  let settingsPanelEl = $state<HTMLDivElement | null>(null);
  let previousFocusEl: HTMLElement | null = null;

  const section = $derived(appStore.settingsSection);
  const animDir = $derived(appStore.settingsAnimDir);
  const appVersion = $derived(appStore.appVersion);

  /*
   * The settings page enters and leaves on the vertical axis while the sidebar
   * rail moves horizontally, so the two halves of the transition stay legible
   * as separate motions. Mirrors --content-swap-y / --content-swap-ms on
   * .content in App.svelte, which animates the outgoing page — keep in sync.
   */
  const SETTINGS_SWAP_PX = 26;
  const SETTINGS_SWAP_MS = 320;

  onMount(() => {
    const unlistenPromise = listen<string>('open-flow:open-settings-section', (event) => {
      const target = event.payload;
      const nextSection = target && isSettingsSectionId(target) ? target : 'general';
      if (nextSection !== appStore.settingsSection) {
        appStore.settingsAnimDir = directionFromOrder(
          appStore.settingsSection,
          nextSection,
          SETTINGS_SECTION_ORDER
        );
      }
      appStore.settingsSection = nextSection;
      appStore.settingsOpen = true;
    });
    return () => {
      void unlistenPromise.then((unlisten) => unlisten());
    };
  });

  function close() { appStore.settingsOpen = false; }

  // Soft fades at the top and bottom of the scroll area, shown only when there
  // is actually more content in that direction — so a scrolled-to-top page keeps
  // its heading crisp and the fade never obscures anything at rest.
  let fadeTop = $state(false);
  let fadeBottom = $state(false);
  const setFades: ScrollEdgeCallback = (top, bottom, node) => {
    const apply = () => {
      if (node !== settingsPanelEl) return;
      fadeTop = top;
      fadeBottom = bottom;
    };
    // Svelte actions run before bind:this is assigned on initial mount. Defer
    // only that first callback so the active panel can claim the state, while
    // callbacks from an outgoing keyed panel remain ignored.
    if (node !== settingsPanelEl) queueMicrotask(apply);
    else apply();
  };

  $effect(() => {
    if (!appStore.devModeEnabled && appStore.settingsSection === 'developer') {
      appStore.settingsSection = 'about';
    }
  });

  $effect(() => {
    if (typeof document === 'undefined') return;

    if (!appStore.settingsOpen) {
      const target = previousFocusEl;
      previousFocusEl = null;
      if (target?.isConnected) {
        requestAnimationFrame(() => target.focus());
      }
      return;
    }

    if (!previousFocusEl && document.activeElement instanceof HTMLElement) {
      previousFocusEl = document.activeElement;
    }

    requestAnimationFrame(() => {
      (firstFocusableInShell() ?? settingsPageEl)?.focus();
    });
  });

  /**
   * Settings is a page, not a dialog, so there is no focus trap — Tab is free to
   * reach the rail in the sidebar. Focus still moves into the shell on open so
   * keyboard users land on the content they just asked for.
   */
  function firstFocusableInShell(): HTMLElement | null {
    if (!settingsPageEl) return null;
    const selector = [
      'button:not([disabled])',
      'input:not([disabled])',
      'select:not([disabled])',
      'textarea:not([disabled])',
      'a[href]',
      '[tabindex]:not([tabindex="-1"])',
    ].join(',');

    return Array.from(settingsPageEl.querySelectorAll<HTMLElement>(selector))
      .find((el) => !el.hasAttribute('inert') && el.offsetParent !== null) ?? null;
  }

  function onWindowKeydown(e: KeyboardEvent) {
    if (appStore.settingsOpen && e.key === 'Escape') {
      const target = e.target as HTMLElement | null;
      if (target?.closest('input, textarea, select, [contenteditable="true"]')) {
        return;
      }
      if (typeof document !== 'undefined' && document.querySelector('[aria-expanded="true"]')) {
        return;
      }
      close();
    }
  }
</script>

<svelte:window onkeydown={onWindowKeydown} />

{#if appStore.settingsOpen}
  <div class="settings-overlay-wrap">
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <!-- Fades on roughly the same clock as .content's exit, so the outgoing
         page's rise stays visible through the wash instead of being snapped away. -->
    <div class="settings-overlay" aria-hidden="true" onclick={close} in:modalBackdrop={{ duration: motionMs(MOTION_MS.panel) }} out:modalBackdrop={{ duration: motionMs(MOTION_MS.base) }}></div>
    <div
      bind:this={settingsPageEl}
      class="settings-page"
      role="region"
      aria-label="Settings"
      tabindex="-1"
      in:pageSwap={{ axis: 'y', distance: motionPx(SETTINGS_SWAP_PX), duration: motionMs(SETTINGS_SWAP_MS) }}
      out:pageSwap={{ axis: 'y', distance: motionPx(SETTINGS_SWAP_PX), duration: motionMs(SETTINGS_SWAP_MS) }}
    >
      <!-- The section rail lives in Sidebar.svelte, which morphs into it.
           Closing is handled by the sidebar's "Back to app" button and Esc —
           the old corner ✕ sat right under the window controls and was
           redundant once settings became a page rather than a modal. -->
      <div class="settings-body">
        <div class="fade-edge fade-edge-top" class:visible={fadeTop} aria-hidden="true"></div>
        <div class="fade-edge fade-edge-bottom" class:visible={fadeBottom} aria-hidden="true"></div>
        {#key section}
          <div
            bind:this={settingsPanelEl}
            class="panel scroll-styled"
            use:scrollEdges={setFades}
            in:pageSwap={{ axis: 'y', distance: animDir * motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.panel) }}
            out:pageSwap={{ axis: 'y', distance: -animDir * motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.base) }}
          >
            <div class="panel-inner">
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
              {:else if section === 'permissions' && isMac}
                <PermissionsSection />
              {:else if section === 'about'}
                <AboutSection {appVersion} />
              {:else if section === 'developer' && appStore.devModeEnabled}
                <DeveloperSection />
              {/if}
            </div>
          </div>
        {/key}
      </div>

      <!--
        The page's sign-off, on About only. The bar keeps its height on every
        section so moving on and off About doesn't resize the panel underneath.
      -->
      <div class="settings-footbar">
        {#if section === 'about'}
          <div class="settings-foot" transition:fade={{ duration: motionMs(MOTION_MS.base) }}>
            Verenu v{appVersion} · MIT
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .settings-overlay-wrap {
    position: absolute;
    inset: 0;
    z-index: 60;
  }

  /*
   * Full-bleed opaque wash rather than a dim scrim: settings reads as the page
   * clearing, not as a card floating over it. It still fades (the smoke tests
   * probe it mid-opacity on open and close) and it stays exposed in the
   * --app-gutter strip that .body leaves on the left and bottom, which is what
   * keeps click-outside-to-dismiss working now that the shell fills the window.
   */
  .settings-overlay {
    position: absolute;
    inset: 0;
    background: var(--paper);
    border: 0;
    padding: 0;
  }

  /*
   * Geometry deliberately mirrors .content in App.svelte, and there is no card
   * treatment — no elevated background, border or radius. Settings is a page in
   * the same content region as Home/Dictionary/Style, not a panel floating on
   * top of one, so it should be indistinguishable from them as a surface.
   */
  .settings-page {
    position: absolute;
    top: 0;
    bottom: var(--app-gutter);
    left: calc(var(--sidebar-w) + var(--app-gutter) * 2);
    right: 0;
    z-index: 1;
    background: transparent;
    border: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  /*
   * Reserves the same gutter .panel does via `scrollbar-gutter: stable`, so the
   * centred footer lines up with the centred content column instead of sitting
   * half a scrollbar to its right.
   */
  .settings-footbar {
    flex-shrink: 0;
    padding: 0 calc(var(--page-pad-x) + var(--scrollbar-w)) 16px var(--page-pad-x);
  }

  /* Centred across the settings page, not aligned to the content column's left
     edge — as a page sign-off it should read as centred, and column-aligned it
     just looked adrift somewhere right of middle. */
  .settings-foot {
    text-align: center;
    font-family: var(--sans);
    font-size: 11px;
    letter-spacing: 0.01em;
    color: var(--ink-faint);
  }

  /* Panel area */
  .settings-body {
    flex: 1;
    position: relative;
    overflow: hidden;
    display: grid;
  }

  /*
   * Soft top/bottom scroll fades. Overlays (not a mask on the scroller) so the
   * scrollbar and the per-row entrance animations are untouched. They fade from
   * the page background to transparent and only appear when there's more to
   * scroll in that direction. Right edge stops short of the scrollbar gutter.
   */
  .fade-edge {
    position: absolute;
    left: 0;
    right: var(--scrollbar-w, 0);
    height: 30px;
    pointer-events: none;
    z-index: 3;
    opacity: 0;
    transition: opacity 180ms ease;
  }

  .fade-edge.visible { opacity: 1; }

  .fade-edge-top {
    top: 0;
    background: linear-gradient(to bottom, var(--paper), transparent);
  }

  .fade-edge-bottom {
    bottom: 0;
    background: linear-gradient(to top, var(--paper), transparent);
  }

  @media (prefers-reduced-motion: reduce) {
    .fade-edge { transition: none; }
  }

  .panel {
    grid-area: 1 / 1;
    width: 100%;
    height: 100%;
    padding: var(--page-pad-y) var(--page-pad-x) 36px;
    overflow-y: auto;
    scrollbar-gutter: stable;
  }

  /*
   * .setting-row is a space-between flex row with no intrinsic max width, so
   * without a measure every label ends up marooned from its control once the
   * shell fills the window. container-type lets the sections swap their old
   * viewport media queries for container queries against this column.
   */
  .panel-inner {
    width: min(100%, var(--settings-measure));
    margin-inline: auto;
    container-type: inline-size;
    container-name: settings-panel;
  }

  /*
   * Per-row entrance. .panel remounts on every section change ({#key section}),
   * so a plain CSS animation on the children runs exactly once per swap — no
   * per-section markup changes needed. The panel's own pageSwap distance is
   * deliberately small (MOTION_PX.nudge) so the two layers don't fight.
   *
   * fill-mode is `backwards`, never `forwards`/`both`: an animated transform or
   * opacity makes each row its own stacking context, which would trap the
   * absolutely-positioned dropdown menus (language, mic, models) inside their
   * row so the following row paints over them. `backwards` covers the delay
   * window and then releases the row back to its natural styles.
   */
  .panel-inner > :global(*) {
    animation: settings-row-in var(--settings-row-ms) cubic-bezier(0.22, 1, 0.36, 1) backwards;
  }

  .panel-inner {
    --settings-row-ms: 260ms;
    --settings-row-step: 26ms;
  }

  .panel-inner > :global(:nth-child(1)) { animation-delay: 0ms; }
  .panel-inner > :global(:nth-child(2)) { animation-delay: calc(var(--settings-row-step) * 1); }
  .panel-inner > :global(:nth-child(3)) { animation-delay: calc(var(--settings-row-step) * 2); }
  .panel-inner > :global(:nth-child(4)) { animation-delay: calc(var(--settings-row-step) * 3); }
  .panel-inner > :global(:nth-child(5)) { animation-delay: calc(var(--settings-row-step) * 4); }
  .panel-inner > :global(:nth-child(6)) { animation-delay: calc(var(--settings-row-step) * 5); }
  .panel-inner > :global(:nth-child(7)) { animation-delay: calc(var(--settings-row-step) * 6); }
  /* Everything past the 8th row shares the last step so long sections don't crawl. */
  .panel-inner > :global(:nth-child(n + 8)) { animation-delay: calc(var(--settings-row-step) * 7); }

  @keyframes settings-row-in {
    from { opacity: 0; transform: translate3d(0, 6px, 0); }
    to   { opacity: 1; transform: none; }
  }

  @media (prefers-reduced-motion: reduce) {
    .panel-inner {
      --settings-row-ms: 140ms;
      --settings-row-step: 10ms;
    }
    @keyframes settings-row-in {
      from { opacity: 0; transform: translate3d(0, 2px, 0); }
      to   { opacity: 1; transform: none; }
    }
  }

  /* Shared styles for all section components — scoped to .settings-body */
  /* Matches .page-h on the app views so a section heading reads at the same
     level as "Home" or "Style" rather than as a panel title. */
  .settings-body :global(.settings-h) {
    font-family: var(--serif);
    font-size: 26px;
    font-weight: 500;
    margin: 0 0 var(--settings-h-mb, 18px);
    letter-spacing: -0.02em;
    line-height: 1.1;
    color: var(--ink);
  }

  .settings-body :global(.settings-subhead) {
    font-family: var(--serif);
    font-size: 14px;
    font-weight: 500;
    color: var(--ink-soft);
    letter-spacing: -0.01em;
    margin: 30px 0 4px;
  }

  .settings-body :global(.settings-subhead.first) { margin-top: 4px; }

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
