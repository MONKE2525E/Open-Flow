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

  function clampY(value: number): number {
    return Math.max(PAD_TOP, Math.min(H - PAD_BOTTOM, value));
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
      // Catmull-Rom handles gentle curves well, but its control points can
      // overshoot between a large spike and a zero-value day. Keeping them in
      // the plot bounds preserves the curve without inventing negative data.
      const c1y = clampY(p1[1] + (p2[1] - p0[1]) / 6);
      const c2x = p2[0] - (p3[0] - p1[0]) / 6;
      const c2y = clampY(p2[1] - (p3[1] - p1[1]) / 6);
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
      {/if}
    </svg>
    {#if hoverPos && !asBars}
      <span class="hover-dot" style:left="{hoverPos.x}px" style:top="{hoverPos.y}px" aria-hidden="true"></span>
    {/if}
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
  /* .card / .card-head / .card-h / .card-sub are owned by Insights.svelte. */

  .readout {
    text-align: right;
    white-space: nowrap;
    min-width: 0;
    flex-shrink: 1;
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
    gap: 8px;
    font-size: 10.5px;
    color: var(--ink-mute);
    margin-top: 2px;
    min-width: 0;
  }

  .axis span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .axis span:last-child { text-align: right; }

  rect { transition: fill var(--ui-duration-fast) var(--ui-ease-out), y var(--ui-duration-base) var(--ui-ease-out), height var(--ui-duration-base) var(--ui-ease-out); }

  /* Progressive enhancement: browsers that support animating `d` glide to a
     new shape when the range or a fresh dictation changes the data. */
  .area-path,
  .line-path {
    transition: d var(--ui-duration-base) var(--ui-ease-out);
  }

  .line-path {
    animation: daily-line-in 260ms var(--ui-ease-out) both;
  }

  @keyframes daily-line-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  @media (prefers-reduced-motion: reduce) {
    .line-path { animation: none; }
  }

  .hover-dot {
    position: absolute;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--accent);
    border: 2.5px solid var(--bg-elev);
    box-sizing: border-box;
    transform: translate(-50%, -50%);
    pointer-events: none;
    filter: drop-shadow(0 1px 3px color-mix(in srgb, var(--accent) 55%, transparent));
  }
</style>
