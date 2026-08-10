<script lang="ts">
  import { accentStep, fmtDay, fmtDayLong, fmtNumber, intensityLevel, parseLocalDay } from './helpers';
  import AnimatedNumber from './AnimatedNumber.svelte';
  import ChartTooltip from './ChartTooltip.svelte';
  import type { InsightsDay, InsightsStreak } from './types';

  let { daily, streak }: { daily: InsightsDay[]; streak: InsightsStreak } = $props();

  const CELL = 11;
  const GAP = 3;
  const STEP = CELL + GAP;
  const WEEKDAY_LABELS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
  const WEEKDAY_NAMES = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];

  /* Minimum grid width, in weeks — short ranges (a 7-day view is only one
     column) would otherwise leave the card looking mostly empty. */
  const MIN_COLUMNS = 10;

  // Reduce rather than Math.max(...spread) — an all-time range can exceed the
  // call-stack limit for spread arguments on a very long daily series.
  const max = $derived(daily.reduce((m, d) => Math.max(m, d.words), 0));

  interface GridCell { day: string; words: number; noData: boolean }

  /*
   * Every cell is a real calendar date, generated as one unbroken sequence —
   * no separate "alignment padding" vs "filler" concept. The grid always
   * ends on the Saturday containing the last real day (or today, if there's
   * no data at all) and always starts on a Sunday, so weekday rows are
   * correct by construction and there's nothing to hover that isn't an
   * actual date.
   */
  const cells = $derived.by((): GridCell[] => {
    const pad = (n: number) => String(n).padStart(2, '0');
    const toKey = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const lastDate = daily.length > 0 ? parseLocalDay(daily[daily.length - 1].day) : today;

    const endDate = new Date(lastDate);
    endDate.setDate(endDate.getDate() + (6 - endDate.getDay())); // extend to that week's Saturday

    const firstRealDate = daily.length > 0 ? parseLocalDay(daily[0].day) : lastDate;
    const daysSpanningReal = Math.round((endDate.getTime() - firstRealDate.getTime()) / 86_400_000) + 1;
    const totalWeeks = Math.max(MIN_COLUMNS, Math.ceil(daysSpanningReal / 7));

    const startDate = new Date(endDate);
    startDate.setDate(startDate.getDate() - (totalWeeks * 7 - 1));

    const byDay = new Map(daily.map((d) => [d.day, d]));
    const list: GridCell[] = [];
    const cursor = new Date(startDate);
    for (let i = 0; i < totalWeeks * 7; i++) {
      const key = toKey(cursor);
      const real = byDay.get(key);
      list.push(real ? { day: key, words: real.words, noData: false } : { day: key, words: 0, noData: true });
      cursor.setDate(cursor.getDate() + 1);
    }
    return list;
  });

  const columnCount = $derived(Math.ceil(cells.length / 7));
  const gridWidth = $derived(columnCount * STEP - GAP);

  /* One label per month the grid spans, positioned above that month's first
     column. A month change landing mid-week would otherwise put two labels on
     the same column (or in immediately adjacent 14px columns, whose text
     overlaps), so a label is only placed when its column is clear of the
     previous one. lastMonth advances on every boundary so each month is
     considered exactly once — a skipped label is omitted, never misplaced
     above a mid-month column. */
  const monthLabels = $derived.by(() => {
    const labels: Array<{ col: number; label: string }> = [];
    let lastMonth = -1;
    let lastCol = -Infinity;
    cells.forEach((cell, i) => {
      const month = parseLocalDay(cell.day).getMonth();
      if (month !== lastMonth) {
        lastMonth = month;
        const col = Math.floor(i / 7);
        if (col >= lastCol + 2) {
          labels.push({ col, label: parseLocalDay(cell.day).toLocaleDateString([], { month: 'short' }) });
          lastCol = col;
        }
      }
    });
    return labels;
  });

  /* Which day of the week tends to be busiest — a second read on the same
     data the grid already has, so the card earns its space beyond the grid. */
  const bestWeekday = $derived.by(() => {
    const sums = new Array(7).fill(0);
    const counts = new Array(7).fill(0);
    for (const d of daily) {
      // Only days with actual dictation count toward the average and the
      // minimum-activity gate — zero-word days (no-data padding or quiet
      // days) would otherwise inflate the weekday coverage.
      if (d.words <= 0) continue;
      const dow = parseLocalDay(d.day).getDay();
      sums[dow] += d.words;
      counts[dow] += 1;
    }
    const averages = sums.map((s, i) => (counts[i] > 0 ? s / counts[i] : 0));
    const peak = Math.max(...averages);
    if (peak <= 0) return null;
    const activeDays = counts.filter((c) => c > 0).length;
    if (activeDays < 3) return null;
    return { name: WEEKDAY_NAMES[averages.indexOf(peak)], avg: peak };
  });

  let hover = $state<GridCell | null>(null);
  let hoverPos = $state<{ x: number; y: number } | null>(null);

  function onEnter(cell: GridCell, event: PointerEvent) {
    hover = cell;
    // Positioned relative to the card, not the scrollable grid — the grid's
    // horizontal auto-scroll also clips vertical overflow per the CSS spec,
    // which would cut the tooltip off above short streaks.
    const card = (event.currentTarget as HTMLElement).closest('.card');
    const cardRect = card?.getBoundingClientRect();
    const cellRect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    hoverPos = cardRect
      ? { x: cellRect.left - cardRect.left + CELL / 2, y: cellRect.top - cardRect.top }
      : null;
  }

  function clearHover() {
    hover = null;
    hoverPos = null;
  }

  const longestRange = $derived(
    streak.longest_started_on && streak.longest_ended_on
      ? `${fmtDay(streak.longest_started_on)} – ${fmtDay(streak.longest_ended_on)}`
      : null
  );
</script>

<section class="card">
  <header class="card-head">
    <div>
      <h2 class="card-h"><AnimatedNumber value={streak.current_days} /> day streak</h2>
      <p class="card-sub">{fmtNumber(streak.active_days)} active days all time</p>
    </div>
    <div class="best">
      <span class="best-num"><AnimatedNumber value={streak.longest_days} /></span>
      <span class="best-label">longest</span>
    </div>
  </header>

  <div class="grid-row">
    <div class="weekday-col" aria-hidden="true">
      {#each WEEKDAY_LABELS as label}
        <span>{label}</span>
      {/each}
    </div>

    <div class="grid-scroll scroll-styled" onscroll={clearHover}>
      <div class="month-row" style:width="{gridWidth}px" aria-hidden="true">
        {#each monthLabels as m}
          <span style:left="{m.col * STEP}px">{m.label}</span>
        {/each}
      </div>
      <div
        class="grid"
        style:width="{gridWidth}px"
        role="img"
        aria-label={`Daily activity calendar. Current streak ${streak.current_days} days, longest ${streak.longest_days} days.`}
      >
        {#each cells as cell}
          {#if cell.noData}
            <span
              class="cell cell-nodata"
              role="presentation"
              onpointerenter={(e) => onEnter(cell, e)}
              onpointerleave={clearHover}
            ></span>
          {:else}
            <span
              class="cell"
              role="presentation"
              style:background={accentStep(intensityLevel(cell.words, max))}
              onpointerenter={(e) => onEnter(cell, e)}
              onpointerleave={clearHover}
            ></span>
          {/if}
        {/each}
      </div>
    </div>
  </div>
  {#if hoverPos && hover}
    <ChartTooltip x={hoverPos.x} y={hoverPos.y} visible={true}>
      {#if hover.noData}
        <strong>No data</strong>
      {:else}
        <strong>{fmtNumber(hover.words)}</strong> words
      {/if}
      <div class="tooltip-dim">{fmtDayLong(hover.day)}</div>
    </ChartTooltip>
  {/if}

  <div class="legend">
    <span class="legend-label">Less</span>
    {#each [0, 1, 2, 3, 4] as level}
      <span class="cell cell-key" style:background={accentStep(level as 0 | 1 | 2 | 3 | 4)} aria-hidden="true"></span>
    {/each}
    <span class="legend-label legend-label-more">More</span>
    <span class="legend-gap"></span>
    <span class="cell cell-nodata" aria-hidden="true"></span>
    <span class="legend-label">No data</span>
  </div>

  {#if daily.length === 0}
    <p class="foot">No activity to chart yet.</p>
  {:else if streak.longest_days === 0}
    <p class="foot">Dictate on two days in a row to start a streak.</p>
  {:else}
    <div class="streak-stats">
      <div class="stat">
        <span class="stat-label">Longest streak</span>
        <span class="stat-value">{streak.longest_days} {streak.longest_days === 1 ? 'day' : 'days'}</span>
        <span class="stat-sub">{fmtNumber(streak.longest_words)} words{#if longestRange} · {longestRange}{/if}</span>
      </div>
      {#if bestWeekday}
        <div class="stat">
          <span class="stat-label">Best day</span>
          <span class="stat-value">{bestWeekday.name}</span>
          <span class="stat-sub">{fmtNumber(bestWeekday.avg)} words on average</span>
        </div>
      {/if}
    </div>
  {/if}

  <table class="sr-only">
    <caption>Words dictated per day</caption>
    <thead><tr><th scope="col">Day</th><th scope="col">Words</th></tr></thead>
    <tbody>
      {#each daily as d}
        <tr><th scope="row">{fmtDayLong(d.day)}</th><td>{fmtNumber(d.words)}</td></tr>
      {/each}
    </tbody>
  </table>
</section>

<style>
  .card {
    position: relative;
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
    margin-bottom: 14px;
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

  .best { text-align: right; }
  .best-num {
    display: block;
    font-family: var(--serif);
    font-size: 20px;
    font-weight: 500;
    color: var(--ink);
    line-height: 1.1;
    font-variant-numeric: tabular-nums;
  }
  .best-label {
    font-size: 10.5px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink-mute);
  }

  .grid-row {
    display: flex;
    gap: 6px;
    justify-content: center;
  }

  .weekday-col {
    display: grid;
    grid-template-rows: repeat(7, 11px);
    gap: 3px;
    flex: 0 0 auto;
    padding-top: 16px; /* clears the month-row above the grid */
  }
  .weekday-col span {
    font-size: 9px;
    line-height: 11px;
    color: var(--ink-mute);
    white-space: nowrap;
  }

  .grid-scroll {
    overflow-x: auto;
    padding-bottom: 2px;
    min-width: 0;
  }

  .month-row {
    position: relative;
    height: 13px;
    margin-bottom: 3px;
  }
  .month-row span {
    position: absolute;
    top: 0;
    font-size: 9.5px;
    color: var(--ink-mute);
    white-space: nowrap;
  }

  .grid {
    position: relative;
    display: grid;
    grid-template-rows: repeat(7, 11px);
    grid-auto-flow: column;
    grid-auto-columns: 11px;
    gap: 3px;
  }

  .cell {
    width: 11px;
    height: 11px;
    border-radius: 3px;
    display: block;
    /* A flat fill at 0-intensity nearly disappears against the card
       background — an outline keeps every day visible in the grid. */
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--ink) 12%, transparent);
    transition: background-color var(--ui-duration-base) var(--ui-ease-out);
  }
  /* A true neutral grey (mixed from --ink, not the warm accent ramp) so it
     reads as "no data" at a glance instead of blending in as a low-activity
     day — --paper-2 was too close to the accent-tinted cells' own dark end. */
  .cell-nodata {
    background: color-mix(in srgb, var(--ink) 14%, transparent);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .legend {
    display: flex;
    align-items: center;
    gap: 3px;
    margin-top: 14px;
    justify-content: center;
  }
  .legend-label {
    font-size: 10.5px;
    color: var(--ink-mute);
  }
  .legend-label:first-child { margin-right: 3px; }
  .legend-label-more { margin-left: 3px; }
  .legend-gap { width: 10px; }

  .foot {
    margin: 10px 0 0;
    font-size: 11.5px;
    color: var(--ink-soft);
    line-height: 1.45;
  }

  /* Two labelled stat blocks instead of a run-on sentence — each fact gets
     its own value and sub-line so it can be read at a glance. */
  .streak-stats {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px 16px;
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid var(--line);
  }
  .stat {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .stat-label {
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink-mute);
  }
  .stat-value {
    font-family: var(--serif);
    font-size: 15px;
    font-weight: 500;
    color: var(--ink);
    line-height: 1.2;
  }
  .stat-sub {
    font-size: 11px;
    color: var(--ink-soft);
    line-height: 1.35;
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
