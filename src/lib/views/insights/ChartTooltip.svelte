<script lang="ts">
  /*
   * Lightweight floating tooltip shared by the hour strip and streak heatmap.
   * Replaces native `title` attributes, which have an OS hover delay and can't
   * be themed — this appears immediately and matches the app's popover style.
   */
  let {
    x,
    y,
    visible,
    children,
  }: { x: number; y: number; visible: boolean; children?: import('svelte').Snippet } = $props();
</script>

<div class="chart-tooltip" class:visible style:left="{x}px" style:top="{y}px" aria-hidden="true">
  {@render children?.()}
</div>

<style>
  .chart-tooltip {
    position: absolute;
    transform: translate(-50%, calc(-100% - 9px)) scale(0.94);
    background: var(--bg-elev);
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    box-shadow: var(--shadow-popover);
    padding: 5px 9px;
    font-size: 11px;
    line-height: 1.35;
    color: var(--ink);
    white-space: nowrap;
    pointer-events: none;
    opacity: 0;
    z-index: 5;
    transition: opacity 80ms var(--ui-ease-out), transform 80ms var(--ui-ease-out);
  }

  .chart-tooltip.visible {
    opacity: 1;
    transform: translate(-50%, calc(-100% - 9px)) scale(1);
  }

  .chart-tooltip :global(strong) {
    color: var(--ink);
    font-weight: 500;
  }

  .chart-tooltip :global(.tooltip-dim) {
    color: var(--ink-mute);
  }

  @media (prefers-reduced-motion: reduce) {
    .chart-tooltip { transition-duration: 1ms; }
  }
</style>
