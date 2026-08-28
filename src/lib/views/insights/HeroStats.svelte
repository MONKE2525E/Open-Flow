<script lang="ts">
  import { untrack } from 'svelte';
  import { tweened } from 'svelte/motion';
  import { expoOut } from 'svelte/easing';
  import { motionMs } from '../../motion';
  import { fmtCompact, fmtNumber, bookEquivalent, pctDelta } from './helpers';
  import AnimatedNumber from './AnimatedNumber.svelte';
  import type { InsightsPayload } from './types';

  let { data, rangeLabel }: { data: InsightsPayload; rangeLabel: string } = $props();

  // Dictionary fixes and auto-learned terms are lifetime counters with no
  // context dimension in the schema, so they stay global even when the rest of
  // the page is filtered. Say so rather than letting them read as scoped.
  const scoped = $derived(data.context_id !== null);

  /* Gauge spans 0–200 wpm. The average speaking pace (~100 wpm) sits dead
     centre, so most readings land mid-arc with room on both sides for
     unusually slow or fast talkers. */
  const GAUGE_MAX = 200;
  const GAUGE_MID = 100;
  const ARC_LENGTH = Math.PI * 52; // r=52 semicircle

  // Tweened so the arc — and everything derived from it — glides to a new
  // reading instead of snapping when a fresh dictation lands.
  const wpmT = tweened(untrack(() => data.totals.avg_wpm), { duration: motionMs(650), easing: expoOut });
  $effect(() => { wpmT.set(data.totals.avg_wpm); });

  const totalWordsT = tweened(untrack(() => data.totals.total_words), { duration: motionMs(700), easing: expoOut });
  $effect(() => { totalWordsT.set(data.totals.total_words); });

  const wpm = $derived(Math.round($wpmT));
  const gaugeFill = $derived(Math.min(1, Math.max(0, wpm / GAUGE_MAX)));

  const words = $derived(fmtCompact($totalWordsT));
  // Derived from the tweened count so the book equivalent stays in sync with
  // the animating word number instead of snapping to the target immediately.
  const books = $derived(bookEquivalent($totalWordsT));
  // The exact string shown in the note; pluralization keys off its parsed
  // value (so 80,080 words → "1.0" → singular) rather than a fragile strict
  // equality against the raw float.
  const booksLabel = $derived(books < 10 ? books.toFixed(1) : String(Math.round(books)));
  const isOneBook = $derived(Number(booksLabel) === 1);
  const delta = $derived(pctDelta(data.totals.words_in_range, data.totals.words_prev_range));
</script>

<div class="hero">
  <section class="tile tile-gauge" aria-label="Average words per minute">
    <svg class="gauge" viewBox="-10 -12 148 98" aria-hidden="true">
      <path
        d="M 12 70 A 52 52 0 0 1 116 70"
        fill="none"
        stroke="var(--control-hover)"
        stroke-width="11"
        stroke-linecap="round"
      />
      <path
        d="M 12 70 A 52 52 0 0 1 116 70"
        fill="none"
        stroke="var(--accent)"
        stroke-width="11"
        stroke-linecap="round"
        stroke-dasharray={`${ARC_LENGTH * gaugeFill} ${ARC_LENGTH}`}
        class="gauge-fill"
      />
      <!-- Scale labels — 0 / 100 / 200 wpm — so the arc reads as a fixed
           reference, not a bare unlabelled sweep. Each centred on its own
           mark so "0" and "200" sit at identical, mirrored distances from
           the arc's two ends. -->
      <text x="4" y="86" class="gauge-tick" text-anchor="middle">0</text>
      <text x="64" y="0" class="gauge-tick gauge-tick-mid" text-anchor="middle">{GAUGE_MID}</text>
      <text x="124" y="86" class="gauge-tick" text-anchor="middle">{GAUGE_MAX}</text>
      <!-- Sits low in the arc's hollow, where the semicircle is widest, so
           a 3-digit reading never reaches the curve above it. No "wpm"
           suffix — the label right below already says it. -->
      <text x="64" y="63" class="gauge-value" text-anchor="middle">{wpm > 0 ? wpm : '—'}</text>
    </svg>
    <p class="tile-label">words per minute</p>
    <p class="tile-note">
      {#if data.totals.best_wpm > 0}
        Your best is {Math.round(data.totals.best_wpm)} wpm
      {:else}
        Speak a little longer to measure this
      {/if}
    </p>
  </section>

  <section class="tile tile-relative" aria-label="Total words dictated">
    {#if delta !== null}
      <span class="delta" class:down={delta < 0}>
        <svg class="delta-arrow" class:flip={delta < 0} width="9" height="9" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M12 19V5M5 12l7-7 7 7"/>
        </svg>
        {Math.abs(delta).toFixed(1)}%
      </span>
    {/if}
    <div class="tile-head">
      <span class="big">{words.value}{#if words.suffix}<small>{words.suffix}</small>{/if}</span>
    </div>
    <p class="tile-label">total words dictated</p>
    <p class="tile-note">
      {#if books >= 1}
        <!-- Pluralize off the displayed amount: "1" is singular, but "1.5"
             isn't. -->
        {#if isOneBook}
          That's 1 full-length book of writing.
        {:else}
          That's {booksLabel} full-length books of writing.
        {/if}
      {:else if data.totals.total_words > 0}
        {Math.round(books * 100)}% of a full-length book so far.
      {:else}
        Your first dictation starts the count.
      {/if}
    </p>
    <p class="tile-note tile-note-dim">
      {fmtNumber(data.totals.words_in_range)} words · {rangeLabel.toLowerCase()}
    </p>
  </section>

  <section class="tile" aria-label="Fixes made by Verenu">
    <div class="tile-head">
      <span class="big"><AnimatedNumber value={data.cleanup.edits_applied} /></span>
    </div>
    <p class="tile-label">fixes made by Verenu</p>
    {#if scoped}
      <p class="tile-note tile-note-dim">across all contexts</p>
    {/if}
    <div class="sub-rows">
      <div class="stat-line">
        <span class="stat-num"><AnimatedNumber value={data.cleanup.dictionary_fixes} /></span>
        <span class="stat-label">dictionary fixes</span>
      </div>
      <div class="stat-line">
        <span class="stat-num"><AnimatedNumber value={data.cleanup.auto_learned_terms} /></span>
        <span class="stat-label">terms auto-learned</span>
      </div>
    </div>
  </section>
</div>

<style>
  /* A summary band on bare paper, not three cards: columns are separated by
     the same 1px hairline the rest of the app uses between rows, and the band
     itself is closed off by a rule before the first section below it. */
  .hero {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: clamp(18px, 3vw, 32px);
    padding-bottom: 20px;
    margin-bottom: 26px;
    border-bottom: 1px solid var(--line);
  }

  .tile {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .tile + .tile {
    border-left: 1px solid var(--line);
    padding-left: clamp(18px, 3vw, 32px);
  }

  .tile-head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    flex-wrap: wrap;
    min-width: 0;
  }

  .tile-relative .tile-head {
    /* Reserve room for the absolutely-positioned delta pill so a large
       "303.0k" reading never slides underneath it on narrow columns. */
    padding-right: 92px;
  }

  .big {
    font-family: var(--sans);
    font-size: 28px;
    font-weight: 600;
    letter-spacing: -0.03em;
    line-height: 1;
    color: var(--ink);
    font-variant-numeric: tabular-nums;
  }
  .big small {
    font-size: 18px;
    color: var(--ink-mute);
    font-weight: 400;
  }

  .tile-relative { position: relative; }

  .delta {
    position: absolute;
    top: 0;
    right: 0;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 11px;
    font-weight: 500;
    padding: 3px 8px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent-ink);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .delta.down {
    background: var(--danger-bg);
    color: var(--danger);
  }

  .delta-arrow.flip { transform: rotate(180deg); }

  .tile-label {
    margin: 9px 0 0;
    font-size: 10.5px;
    letter-spacing: 0;
    text-transform: none;
    color: var(--ink-mute);
  }

  .tile-note {
    margin: 8px 0 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--ink-soft);
  }
  .tile-note-dim {
    margin-top: auto;
    padding-top: 8px;
    color: var(--ink-mute);
    font-size: 11.5px;
  }

  .tile-gauge {
    align-items: center;
    text-align: center;
  }

  .tile-gauge .tile-label {
    margin-top: 13px;
  }

  .gauge {
    width: 100%;
    max-width: min(150px, 100%);
    height: auto;
    display: block;
  }
  .gauge-tick {
    font-family: var(--serif);
    font-size: 10px;
    font-weight: 600;
    fill: var(--ink-mute);
    font-variant-numeric: tabular-nums;
  }
  .gauge-tick-mid {
    font-size: 11px;
    fill: var(--ink-soft);
  }

  .gauge-value {
    font-family: var(--sans);
    font-size: 24px;
    font-weight: 600;
    fill: var(--ink);
    font-variant-numeric: tabular-nums;
  }

  .sub-rows {
    margin-top: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .stat-line {
    display: flex;
    align-items: baseline;
    gap: 8px;
    border-top: 1px solid var(--line);
    padding-top: 8px;
  }

  .stat-num {
    font-family: var(--sans);
    font-size: 16px;
    font-weight: 600;
    line-height: 1;
    color: var(--ink);
    font-variant-numeric: tabular-nums;
  }

  .stat-label {
    font-size: 11.5px;
    color: var(--ink-mute);
    margin-left: auto;
  }

  /* Container queries against the insights column (owned by Insights.svelte):
     viewport queries fired too late because the sidebar consumes ~220px.
     The band stays 3-up as long as possible — stacking early is what left a
     150px gauge stranded in a full-width row with empty space beside it. */
  @container insights (max-width: 680px) {
    /* Compact 3-up: tighter gutters and a smaller gauge/number so all three
       tiles still fit side by side instead of stacking. */
    .hero { gap: 14px; }
    .tile + .tile { padding-left: 14px; }
    .tile-relative .tile-head { padding-right: 84px; }
    .delta { font-size: 10px; padding: 2px 7px; }
    .big { font-size: 24px; }
    .big small { font-size: 15px; }
    .gauge { max-width: min(120px, 100%); }
    .gauge-value { font-size: 20px; }
    .tile-note { font-size: 11.5px; }
    .stat-num { font-size: 14px; }
    .stat-label { font-size: 10.5px; }
  }

  @container insights (max-width: 500px) {
    .hero { grid-template-columns: 1fr; gap: 0; }
    /* Stacked, the vertical rules become horizontal ones — same trick
       StatsCard uses when its row collapses. */
    .tile + .tile {
      border-left: 0;
      padding-left: 0;
      border-top: 1px solid var(--line);
      padding-top: 18px;
      margin-top: 18px;
    }
    /* Stacked, the gauge goes horizontal — dial on the left, labels on the
       right — so the row reads as one full-width fact instead of a small
       centred dial with empty space beside it. */
    .tile-gauge {
      display: grid;
      grid-template-columns: 110px minmax(0, 1fr);
      column-gap: 14px;
      align-items: center;
      text-align: left;
    }
    .tile-gauge .gauge {
      grid-row: span 2;
      max-width: 110px;
    }
    .tile-gauge .tile-label { margin-top: 0; }
    .tile-gauge .tile-note { margin-top: 4px; }
  }

  /* Very narrow: the delta pill joins the flow instead of floating over the
     hero number it would otherwise collide with. */
  @container insights (max-width: 380px) {
    .tile-relative .tile-head { padding-right: 0; }
    .delta { position: static; align-self: flex-start; margin-bottom: 8px; }
  }
</style>
