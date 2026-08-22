<script lang="ts">
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { appStore } from '../../stores';
  import { motionMs } from '../../motion';
  import { isMac } from '../../platform';
  import LogoMark from './LogoMark.svelte';
</script>

<!--
  data-tauri-drag-region makes this row the window's drag handle under the
  native overlay titlebar; children opt out of pointer events so clicks
  anywhere in the row drag instead of dead-ending on the logo or wordmark.
-->
<div class="brand" class:brand-mac={isMac} data-tauri-drag-region>
  <div class="brand-mark"><LogoMark /></div>
  <div class="brand-name">
    <span>Verenu</span>
    {#if appStore.betaUpdatesEnabled}
      <span class="beta-marker" aria-label="Beta updates enabled" in:fly|global={{ y: -4, duration: motionMs(180), easing: cubicOut }} out:fly|global={{ y: -5, duration: motionMs(220), easing: cubicOut }}>BETA</span>
    {/if}
  </div>
</div>

<style>
  .brand { min-height: var(--native-titlebar-height, 32px); padding: 16px 18px 0 16px; display: flex; align-items: center; gap: 6px; }
  /* On macOS the traffic lights overlay the content, so the brand row starts
     below that strip and doubles as the drag region. */
  .brand.brand-mac { padding-top: calc(var(--mac-titlebar-h, 28px) + 8px); }
  .brand > * { pointer-events: none; }
  .brand-mark { width: 24px; height: 20px; color: var(--accent); }
  .brand-mark :global(svg) { display: block; }
  .brand-name { font-family: var(--serif); font-size: 17px; letter-spacing: -0.015em; font-weight: 500; color: var(--ink); white-space: nowrap; display: flex; align-items: flex-end; gap: 2px; }
  .beta-marker { font-family: var(--sans); font-size: 8.5px; font-weight: 750; letter-spacing: 0.08em; line-height: 1; color: var(--accent); position: relative; top: -5px; }
</style>
