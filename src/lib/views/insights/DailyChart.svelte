<script lang="ts">
  import { fmtDayLong, fmtNumber, niceCeiling } from './helpers';
  import AnimatedNumber from './AnimatedNumber.svelte';
  import ChartTooltip from './ChartTooltip.svelte';
  import type { InsightsDay } from './types';

  let { daily, rangeLabel }: { daily: InsightsDay[]; rangeLabel: string } = $props();

  // Unique per instance so multiple charts on the page never share a <mask> id.
  const gradientId = `daily-edge-fade-${Math.random().toString(36).slice(2)}`;

  /* Fixed viewBox + preserveAspectRatio="none" makes the chart fully fluid with
     no resize observer; vector-effect keeps strokes an even width regardless. */
  const W = 600;
  const H = 170;
  const PAD_TOP = 12;
  const PAD_BOTTOM = 22;

  /* Below this many points a line reads as noise — discrete bars are clearer. */
  const BAR_THRESHOLD = 14;

  // Reduce rather than Math.max(...spread) — an all-time range can exceed the
  // call-stack limit for spread arguments on a very long daily series.
  const max = $derived(niceCeiling(daily.reduce((m, d) => Math.max(m, d.words), 0)));
  const asBars = $derived(daily.length <= BAR_THRESHOLD);
  const plotH = H - PAD_TOP - PAD_BOTTOM;

  function x(i: number): number {
    if (daily.length <= 1) return W / 2;
    // Bars sit in the middle of their slot so the first and last aren't clipped
    // by the viewBox edge; the line spans edge to edge.
    if (asBars) return (i + 0.5) * (W / daily.length);
    return (i / (daily.length - 1)) * W;
  }

  function y(words: number): number {
    return PAD_TOP + plotH * (1 - words / max);
  }

  /* Catmull-Rom control points → cubic bezier, so the area reads as a curve
     without pulling above the data the way a naive spline does. */
  const linePath = $derived.by(() => {
    if (daily.length === 0) return '';
    if (daily.length === 1) return `M 0 ${y(daily[0].words)} L ${W} ${y(daily[0].words)}`;
    const pts = daily.map((d, i) => [x(i), y(d.words)] as const);
    let path = `M ${pts[0][0]} ${pts[0][1]}`;
    for (let i = 0; i < pts.length - 1; i++) {
      const p0 = pts[i - 1] ?? pts[i];
      const p1 = pts[i];
      const p2 = pts[i + 1];
      const p3 = pts[i + 2] ?? p2;
      const c1x = p1[0] + (p2[0] - p0[0]) / 6;
      const c1y = p1[1] + (p2[1] - p0[1]) / 6;
      const c2x = p2[0] - (p3[0] - p1[0]) / 6;
      const c2y = p2[1] - (p3[1] - p1[1]) / 6;
      path += ` C ${c1x} ${c1y}, ${c2x} ${c2y}, ${p2[0]} ${p2[1]}`;
    }
    return path;
  });

  const areaPath = $derived(
    linePath ? `${linePath} L ${W} ${H - PAD_BOTTOM} L 0 ${H - PAD_BOTTOM} Z` : ''
  );

  const barWidth = $derived(daily.length > 0 ? Math.min(28, (W / daily.length) * 0.55) : 0);

  let hover = $state<number | null>(null);
  let hoverPos = $state<{ x: number; y: number } | null>(null);
  const active = $derived(hover !== null ? daily[hover] : null);

  function onMove(event: PointerEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    if (rect.width === 0 || daily.length === 0) return;
    const ratio = (event.clientX - rect.left) / rect.width;
    // Bars fill equal-width slots spanning [i/n, (i+1)/n), so floor maps the
    // pointer into its slot; line charts align points at i/(n-1), so round
    // to the nearest point.
    const idx = asBars
      ? Math.min(daily.length - 1, Math.max(0, Math.floor(ratio * daily.length)))
      : Math.min(daily.length - 1, Math.max(0, Math.round(ratio * (daily.length - 1))));
    hover = idx;
    // The svg's CSS height matches the viewBox height 1:1, only width is
    // fluid, so x scales by the rendered width and y needs no conversion.
    const px = (x(idx) / W) * rect.width;
    const py = daily[idx].words > 0 ? y(daily[idx].words) : H - PAD_BOTTOM;
    hoverPos = { x: px, y: py };
  }

  const total = $derived(daily.reduce((sum, d) => sum + d.words, 0));
  const best = $derived(daily.reduce<InsightsDay | null>((b, d) => (!b || d.words > b.words ? d : b), null));
  const summary = $derived(
    daily.length === 0
      ? 'No daily activity in this range.'
      : `Words dictated per day, ${rangeLabel.toLowerCase()}. ${fmtNumber(total)} words across ${daily.length} days, peaking at ${fmtNumber(best?.words ?? 0)} on ${best ? fmtDayLong(best.day) : '—'}.`
  );
</script>

<section class="card">
  <header class="card-head">
    <div>
      <h2 class="card-h">Words per day</h2>
      <p class="card-sub">{rangeLabel}</p>
    </div>
    <div class="readout" aria-live="polite">
      {#if active}
        <span class="readout-num">{fmtNumber(active.words)}</span>
        <span class="readout-day">{fmtDayLong(active.day)}</span>
      {:else}
        <span class="readout-num"><AnimatedNumber value={total} /></span>
        <span class="readout-day">total</span>
      {/if}
    </div>
  </header>

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="plot"
    role="img"
    aria-label={summary}
    onpointermove={onMove}
    onpointerleave={() => { hover = null; hoverPos = null; }}
  >
    <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none">
      <defs>
        <!-- Fades the area fill's hard vertical edges instead of cutting it off flush. -->
        <linearGradient id={gradientId} x1="0" x2="1" y1="0" y2="0">
          <stop offset="0%" stop-color="white" stop-opacity="0" />
          <stop offset="4%" stop-color="white" stop-opacity="1" />
          <stop offset="96%" stop-color="white" stop-opacity="1" />
          <stop offset="100%" stop-color="white" stop-opacity="0" />
        </linearGradient>
        <mask id="{gradientId}-mask" maskUnits="userSpaceOnUse" x="0" y="0" width={W} height={H}>
          <rect x="0" y="0" width={W} height={H} fill="url(#{gradientId})" />
        </mask>
      </defs>
      <line
        x1="0" y1={H - PAD_BOTTOM} x2={W} y2={H - PAD_BOTTOM}
        stroke="var(--line)" stroke-width="1" vector-effect="non-scaling-stroke"
      />
      {#if asBars}
        {#each daily as d, i}
          <rect
            x={x(i) - barWidth / 2}
            y={d.words > 0 ? y(d.words) : H - PAD_BOTTOM - 1}
            width={barWidth}
            height={d.words > 0 ? Math.max(1, H - PAD_BOTTOM - y(d.words)) : 1}
            fill={hover === i ? 'var(--accent)' : 'color-mix(in srgb, var(--accent) 55%, transparent)'}
          ><title>{fmtDayLong(d.day)} — {fmtNumber(d.words)} words</title></rect>
        {/each}
      {:else}
        <path d={areaPath} fill="color-mix(in srgb, var(--accent) 18%, transparent)" mask="url(#{gradientId}-mask)" class="area-path" />
        <path
          d={linePath}
          fill="none"
          stroke="var(--accent)"
          stroke-width="1.5"
          stroke-linejoin="round"
          vector-effect="non-scaling-stroke"
          class="line-path"
        />
      {/if}
      {#if hover !== null && daily[hover]}
        <line
          x1={x(hover)} y1={PAD_TOP - 6} x2={x(hover)} y2={H - PAD_BOTTOM}
          stroke="var(--accent)" stroke-width="1" stroke-dasharray="3 3" opacity="0.55"
          vector-effect="non-scaling-stroke"
        />
        {#if !asBars}
          <circle
            cx={x(hover)} cy={y(daily[hover].words)} r="4.5"
            fill="var(--accent)" stroke="var(--bg-elev)" stroke-width="2.5"
            vector-effect="non-scaling-stroke"
            class="hover-dot"
          />
        {/if}
      {/if}
    </svg>
    <div class="axis">
      <span>{daily.length ? fmtDayLong(daily[0].day) : ''}</span>
      <span>{daily.length > 1 ? fmtDayLong(daily[daily.length - 1].day) : ''}</span>
    </div>
    {#if hoverPos && active}
      <ChartTooltip x={hoverPos.x} y={hoverPos.y} visible={true}>
        <strong>{fmtNumber(active.words)}</strong> words
        <div class="tooltip-dim">{fmtDayLong(active.day)}</div>
      </ChartTooltip>
    {/if}
  </div>
</section>

<style>
  .card {
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    padding: 15px 18px 13px;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .card-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 10px;
  }

  .card-h {
    font-family: var(--serif);
    font-size: 17px;
    font-weight: 500;
    margin: 0;
    color: var(--ink);
    letter-spacing: -0.01em;
  }

  .card-sub {
    margin: 3px 0 0;
    font-size: 11.5px;
    color: var(--ink-mute);
  }

  .readout {
    text-align: right;
    white-space: nowrap;
  }
  .readout-num {
    display: block;
    font-family: var(--serif);
    font-size: 20px;
    font-weight: 500;
    color: var(--ink);
    line-height: 1.1;
    font-variant-numeric: tabular-nums;
  }
  .readout-day {
    font-size: 11px;
    color: var(--ink-mute);
  }

  .plot { flex: 1; position: relative; }

  svg {
    display: block;
    width: 100%;
    height: 170px;
    overflow: visible;
  }

  .axis {
    display: flex;
    justify-content: space-between;
    font-size: 10.5px;
    color: var(--ink-mute);
    margin-top: 2px;
  }

  rect { transition: fill var(--ui-duration-fast) var(--ui-ease-out), y var(--ui-duration-base) var(--ui-ease-out), height var(--ui-duration-base) var(--ui-ease-out); }

  /* Progressive enhancement: browsers that support animating `d` glide to a
     new shape when the range or a fresh dictation changes the data. */
  .area-path,
  .line-path {
    transition: d var(--ui-duration-base) var(--ui-ease-out);
  }

  .hover-dot {
    filter: drop-shadow(0 1px 3px color-mix(in srgb, var(--accent) 55%, transparent));
  }
</style>
