<script lang="ts">
  import { fmtHour, fmtNumber } from './helpers';
  import AnimatedNumber from './AnimatedNumber.svelte';
  import ChartTooltip from './ChartTooltip.svelte';

  let { hourly = Array.from({ length: 24 }, () => 0) }: { hourly?: number[] } = $props();

  // Default destructure above normalizes a missing/null prop; guard the
  // derivations against a non-array payload too.
  const series = $derived(
    Array.isArray(hourly)
      ? [...hourly.slice(0, 24), ...Array.from({ length: Math.max(0, 24 - hourly.length) }, () => 0)]
      : Array.from({ length: 24 }, () => 0),
  );
  const max = $derived(Math.max(...series, 0));
  const total = $derived(series.reduce((sum, n) => sum + n, 0));
  const peak = $derived(max > 0 ? series.indexOf(max) : -1);
  const peakSharePct = $derived(Math.round((max / Math.max(1, total)) * 100));

  let hover = $state<number | null>(null);
  let hoverX = $state(0);
  let stripHeight = $state(96);

  function onEnter(hour: number, event: PointerEvent) {
    hover = hour;
    const strip = (event.currentTarget as HTMLElement).parentElement;
    const stripRect = strip?.getBoundingClientRect();
    const colRect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    hoverX = stripRect ? colRect.left - stripRect.left + colRect.width / 2 : 0;
    stripHeight = stripRect?.height ?? 96;
  }
</script>

<section class="card">
  <header class="card-head">
    <div>
      <h2 class="card-h">When you dictate</h2>
      <p class="card-sub">Words by hour of day</p>
    </div>
  </header>

  <div
    class="strip"
    role="img"
    aria-label={peak >= 0
      ? `Words by hour of day. Peak activity at ${fmtHour(peak)} with ${fmtNumber(max)} words.`
      : 'No hourly activity recorded yet.'}
  >
    {#each series as words, hour}
      <div
        class="col"
        role="presentation"
        onpointerenter={(e) => onEnter(hour, e)}
        onpointerleave={() => (hover = null)}
      >
        <span
          class="bar"
          class:peak={hour === peak}
          class:hovered={hover === hour}
          style:height={`${max > 0 ? Math.max(2, (words / max) * 100) : 2}%`}
        ></span>
      </div>
    {/each}
    {#if hover !== null && series[hover] !== undefined}
      <ChartTooltip
        x={hoverX}
        y={stripHeight - (Math.max(2, max > 0 ? (series[hover] / max) * 100 : 2) / 100) * stripHeight}
        visible={true}
      >
        <strong>{fmtNumber(series[hover])}</strong> words
        <div class="tooltip-dim">{fmtHour(hover)}</div>
      </ChartTooltip>
    {/if}
  </div>

  <div class="axis">
    <span>12 AM</span><span>6 AM</span><span>12 PM</span><span>6 PM</span><span>11 PM</span>
  </div>

  <p class="foot">
    {#if peak >= 0}
      You dictate most around {fmtHour(peak)} — <AnimatedNumber value={peakSharePct} format={(n) => Math.round(n).toString()} />% of your words land in that hour.
    {:else}
      Hourly patterns appear once you've dictated a few times.
    {/if}
  </p>
</section>

<style>
  /* .card / .card-head / .card-h / .card-sub are owned by Insights.svelte. */

  .strip {
    position: relative;
    display: grid;
    grid-template-columns: repeat(24, minmax(0, 1fr));
    gap: 3px;
    flex: 1;
    min-height: 96px;
    align-items: end;
  }

  .col {
    display: flex;
    align-items: flex-end;
    height: 100%;
    min-width: 0;
  }

  .bar {
    display: block;
    width: 100%;
    border-radius: 2px;
    background: color-mix(in srgb, var(--accent) 45%, transparent);
    transition: height var(--ui-duration-base) var(--ui-ease-out), background-color var(--ui-duration-fast) var(--ui-ease-out);
  }
  .bar.peak { background: var(--accent); }
  .bar.hovered { background: var(--accent); }

  .axis {
    display: flex;
    justify-content: space-between;
    gap: 6px;
    font-size: 10.5px;
    color: var(--ink-mute);
    margin-top: 6px;
    min-width: 0;
  }

  .axis span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  @container insights (max-width: 420px) {
    .strip { gap: 2px; }
    .axis span:nth-child(2),
    .axis span:nth-child(4) { display: none; }
  }

  .foot {
    margin: 0;
    padding-top: 10px;
    font-size: 11.5px;
    color: var(--ink-soft);
    line-height: 1.45;
  }
</style>
