<script lang="ts">
  import { MOTION_MS, reducedMotionEnabled } from '../../motion';

  // 0 = maximum accuracy (left) … 1 = maximum efficiency (right).
  let { position = 0.5 }: { position?: number } = $props();

  const clamped = $derived(Math.min(1, Math.max(0, position)));
  // Reduced motion: snap instead of gliding the marker between cards.
  const durationMs = $derived(reducedMotionEnabled() ? 0 : MOTION_MS.panel);
</script>

<div class="eff-bar" role="img" aria-label={`Accuracy versus efficiency: ${Math.round(clamped * 100)}% toward efficiency`}>
  <div class="eff-labels">
    <span>Accuracy</span>
    <span>Efficiency</span>
  </div>
  <div class="eff-track">
    <div
      class="eff-marker"
      style={`left: ${clamped * 100}%; transition-duration: ${durationMs}ms;`}
    ></div>
  </div>
</div>

<style>
  .eff-bar {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .eff-labels {
    display: flex;
    justify-content: space-between;
    font-family: var(--sans);
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.02em;
    color: var(--ink-mute);
  }

  .eff-track {
    position: relative;
    height: 6px;
    border-radius: 999px;
    /* Fixed orange scale (not --accent) so the legend stays orange regardless
       of the user's chosen accent theme. Deep terracotta = accuracy (left),
       light = efficiency (right). */
    background: linear-gradient(
      to right,
      var(--jap-600) 0%,
      var(--jap-400) 50%,
      var(--jap-200) 100%
    );
  }

  .eff-marker {
    position: absolute;
    top: 50%;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--bg-elev);
    border: 2px solid var(--jap-700);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
    transform: translate(-50%, -50%);
    transition-property: left;
    transition-timing-function: cubic-bezier(0.22, 1, 0.36, 1);
  }
</style>
