<script lang="ts">
  import { Tween } from 'svelte/motion';
  import { fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { MOTION_MS, motionMs } from '../../motion';

  type Stage = 'downloading' | 'verifying' | 'extracting';

  let {
    stage = 'downloading' as Stage,
    percent = 0,
    label = 'Downloading',
    indeterminate = false,
  }: {
    stage?: Stage;
    percent?: number;
    label?: string;
    indeterminate?: boolean;
  } = $props();

  const clampedPercent = $derived(Math.max(0, Math.min(100, percent)));

  // One tween drives both the bar width and the counter so they stay in
  // lockstep and animate smoothly between the coarse checkpoints the backend
  // emits. Each stage restarts its own 0→100 fill, so on a stage change we
  // snap to 0 (duration 0) rather than letting the bar sweep backwards from
  // the previous stage's 100%.
  const value = new Tween(0, { duration: motionMs(MOTION_MS.base), easing: cubicOut });
  let prevStage: Stage | undefined = undefined;

  $effect(() => {
    const nextPercent = clampedPercent;
    if (prevStage !== undefined && stage !== prevStage) {
      value.set(0, { duration: 0 });
    }
    prevStage = stage;
    value.target = nextPercent;
  });

  const displayPercent = $derived(Math.round(value.current));
</script>

<div class="dl-progress" data-stage={stage}>
  <div class="dl-progress-row">
    <span class="dl-stage-slot">
      {#key label}
        <span
          class="dl-stage"
          in:fade={{ duration: motionMs(MOTION_MS.fast) }}
          out:fade={{ duration: motionMs(MOTION_MS.fast) }}
        >
          <span class="dl-pulse" aria-hidden="true"></span>
          {label}
        </span>
      {/key}
    </span>
    {#if !indeterminate}
      <span class="dl-pct">{displayPercent}%</span>
    {/if}
  </div>
  <div
    class="dl-track"
    class:is-indeterminate={indeterminate}
    role="progressbar"
    aria-label={label}
    aria-valuenow={indeterminate ? undefined : displayPercent}
    aria-valuemin={indeterminate ? undefined : 0}
    aria-valuemax={indeterminate ? undefined : 100}
  >
    {#if indeterminate}
      <div class="dl-indeterminate" aria-hidden="true"></div>
    {:else}
      <div class="dl-fill" style={`width:${value.current}%`}></div>
    {/if}
  </div>
</div>

<style>
  .dl-progress {
    margin-top: 10px;
  }

  .dl-progress-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }

  .dl-stage-slot {
    position: relative;
    display: inline-flex;
    min-width: 0;
  }

  .dl-stage {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-size: 11px;
    color: var(--ink-mute);
    white-space: nowrap;
  }

  /* Crossfading stage labels stack in the same slot so the row height and the
     percentage on the right never jump as the text swaps. */
  .dl-stage-slot :global(.dl-stage:not(:first-child)) {
    position: absolute;
    inset: 0;
  }

  .dl-pulse {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
    flex-shrink: 0;
    animation: dl-pulse 1.1s ease-in-out infinite;
  }

  .dl-pct {
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    color: var(--ink-mute);
    flex-shrink: 0;
  }

  .dl-track {
    position: relative;
    margin-top: 6px;
    height: 6px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--paper) 65%, var(--bg-elev));
    overflow: hidden;
  }

  .dl-fill {
    position: relative;
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
    overflow: hidden;
  }

  /* A soft highlight sweeps across the filled portion so the bar reads as
     actively working even while a coarse percentage sits still (e.g. during a
     multi-second checksum pass between emitted checkpoints). */
  .dl-fill::after {
    content: '';
    position: absolute;
    inset: 0;
    background: linear-gradient(
      90deg,
      transparent 0%,
      color-mix(in srgb, var(--on-accent) 45%, transparent) 50%,
      transparent 100%
    );
    background-size: 220% 100%;
    animation: dl-sheen 1.4s linear infinite;
  }

  .dl-indeterminate {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 38%;
    border-radius: inherit;
    background: var(--accent);
    animation: dl-indeterminate 1.15s ease-in-out infinite;
  }

  @keyframes dl-pulse {
    0%,
    100% {
      opacity: 1;
      transform: scale(1);
    }
    50% {
      opacity: 0.3;
      transform: scale(0.68);
    }
  }

  @keyframes dl-sheen {
    from {
      background-position: 220% 0;
    }
    to {
      background-position: -220% 0;
    }
  }

  @keyframes dl-indeterminate {
    0% {
      left: -40%;
    }
    100% {
      left: 100%;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .dl-pulse,
    .dl-fill::after {
      animation: none;
    }
    .dl-pulse {
      opacity: 0.7;
    }
    .dl-indeterminate {
      animation-duration: 1.8s;
    }
  }
</style>
