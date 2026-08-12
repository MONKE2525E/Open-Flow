<script lang="ts">
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { appStore } from '../../stores';
  import { motionMs } from '../../motion';
  import LogoMark from './LogoMark.svelte';
</script>

<div class="brand">
  <div class="brand-mark">
    <LogoMark />
  </div>
  <div class="brand-name">
    <span>Verenu</span>
    {#if appStore.betaUpdatesEnabled}
      <span
        class="beta-marker"
        aria-label="Beta updates enabled"
        in:fly|global={{ y: -4, duration: motionMs(180), easing: cubicOut }}
        out:fly|global={{ y: -5, duration: motionMs(220), easing: cubicOut }}
      >BETA</span>
    {/if}
  </div>
</div>

<style>
  .brand {
    min-height: var(--native-titlebar-height, 32px);
    /* Brand header: 16px breathing room from the top, and the mark's left edge
       (16px) aligns with the nav icons below it, so the logo establishes the
       starting alignment for the navigation. The nav-section's 12px top
       padding supplies the gap to the first nav target. */
    padding: 16px 18px 0 16px;
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }

  .brand-mark {
    width: 24px;
    height: 20px;
    color: var(--accent);
  }

  .brand-mark :global(svg) { display: block; }

  .brand-name {
    font-family: var(--serif);
    font-size: 17px;
    letter-spacing: -0.015em;
    font-weight: 500;
    color: var(--ink);
    white-space: nowrap;
    display: flex;
    align-items: flex-end;
    gap: 2px;
  }

  .beta-marker {
    font-family: var(--sans);
    font-size: 8.5px;
    font-weight: 750;
    letter-spacing: 0.08em;
    line-height: 1;
    color: var(--accent);
    position: relative;
    top: -5px;
  }
</style>
