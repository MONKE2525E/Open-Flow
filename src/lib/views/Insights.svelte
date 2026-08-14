<script lang="ts">
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import { invoke, listen } from '../tauri';
  import { formatIpcError } from '../stores';
  import { MOTION_MS, motionMs } from '../motion';
  import Dropdown from '../components/Dropdown.svelte';
  import HeroStats from './insights/HeroStats.svelte';
  import DailyChart from './insights/DailyChart.svelte';
  import StreakHeatmap from './insights/StreakHeatmap.svelte';
  import CostBreakdown from './insights/CostBreakdown.svelte';
  import HourStrip from './insights/HourStrip.svelte';
  import WordStats from './insights/WordStats.svelte';
  import { EMPTY_INSIGHTS, RANGE_OPTIONS, type InsightsPayload, type InsightsRange } from './insights/types';

  let range = $state<InsightsRange>(30);
  let data = $state<InsightsPayload | null>(null);
  let status = $state<'loading' | 'loaded' | 'error'>('loading');
  let error = $state('');
  let rangeOpen = $state(false);

  let fetchToken = 0;
  let mounted = false;

  const rangeLabel = $derived(RANGE_OPTIONS.find((o) => o.value === range)?.label ?? 'Last 30 days');
  const dataRangeLabel = $derived(
    RANGE_OPTIONS.find((o) => o.value === data?.range_days)?.label ?? rangeLabel,
  );
  const isEmpty = $derived(!data || data.totals.total_transcriptions === 0);

  async function load(opts?: { silent?: boolean; days?: InsightsRange }) {
    const token = ++fetchToken;
    const requestedRange = opts?.days ?? range;
    if (!opts?.silent) {
      status = 'loading';
      error = '';
    }
    try {
      const payload = await invoke<InsightsPayload>('get_insights', { days: requestedRange });
      if (!mounted || token !== fetchToken) return;
      if (payload && payload.range_days !== requestedRange) {
        throw new Error(`Insights response used ${payload.range_days} days instead of ${requestedRange}.`);
      }
      data = payload ?? { ...EMPTY_INSIGHTS, range_days: requestedRange };
      status = 'loaded';
      error = '';
    } catch (err) {
      if (!mounted || token !== fetchToken) return;
      console.error('IPC get_insights failed:', err);
      if (!opts?.silent) {
        error = formatIpcError(err);
        status = 'error';
      } else if (!data) {
        // No data to fall back on — surface the error so the skeleton can't
        // spin forever (e.g. initial load failed, or a silent refresh landed
        // while the first load was still in flight).
        error = formatIpcError(err);
        status = 'error';
      } else {
        // Silent refresh failed but we have good data: keep it up. Also clear
        // any 'loading' a superseded non-silent load may have left behind, or
        // the skeleton would stick even though data is present.
        status = 'loaded';
      }
    }
  }

  function pickRange(next: InsightsRange) {
    rangeOpen = false;
    if (next === range) return;
    range = next;
    load({ days: next });
  }

  onMount(() => {
    mounted = true;
    load();
    let unlisten: (() => void) | undefined;
    // Refresh live as new dictations land, so the page never shows stale
    // numbers while it's open. Silent so a background refresh never flashes
    // the loading state.
    listen('verenu:transcribed', () => load({ silent: true }))
      .then((cleanup) => {
        // If the component already unmounted while `listen` was still
        // resolving, tear the listener down immediately rather than leaking
        // it onto a dead component.
        if (!mounted) {
          cleanup();
        } else {
          unlisten = cleanup;
        }
      })
      .catch(() => {});
    // Belt and suspenders: poll on a fixed cadence too, so the page catches
    // up even when a transcription event was missed (e.g. it landed before
    // the page opened). Silent ticks never flash the loading state.
    const timer = setInterval(() => load({ silent: true }), 10_000);
    return () => {
      mounted = false;
      unlisten?.();
      clearInterval(timer);
    };
  });
</script>

<div class="content-inner">
  <div class="head">
    <div>
      <h1 class="page-h">Insights</h1>
      <p class="page-sub">How much you dictate, how fast, and what it costs. Everything here is computed locally from your own history — nothing leaves your machine.</p>
    </div>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="ui-dropdown range-picker"
      onclick={(event) => event.stopPropagation()}
      onkeydown={(event) => {
        if (event.key === 'Escape' && rangeOpen) {
          rangeOpen = false;
          event.stopPropagation();
        }
      }}
    >
      <button
        type="button"
        class="ui-dropdown-trigger"
        aria-expanded={rangeOpen}
        aria-haspopup="listbox"
        onclick={() => (rangeOpen = !rangeOpen)}
      >
        {rangeLabel}
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>
      </button>
      {#if rangeOpen}
        <Dropdown bind:open={rangeOpen} closeSelector=".range-picker">
          <div class="ui-dropdown-menu ui-dropdown-menu--padded" role="listbox" aria-label="Date range">
            {#each RANGE_OPTIONS as option}
              <button
                type="button"
                class="ui-dropdown-option"
                class:active={option.value === range}
                role="option"
                aria-selected={option.value === range}
                onclick={() => pickRange(option.value)}
              >{option.label}</button>
            {/each}
          </div>
        </Dropdown>
      {/if}
    </div>
  </div>

  {#if status === 'error' && !data}
    <div class="empty-state empty-state-error" role="alert" in:fade={{ duration: motionMs(MOTION_MS.base) }}>
      <p class="empty-h">Could not load insights</p>
      <p class="empty-sub">The backend is unavailable right now. {error}</p>
      <button type="button" class="btn-ghost" onclick={() => load()}>Try again</button>
    </div>
  {:else if status === 'loading' && !data}
    <div class="skeletons" aria-busy="true" aria-label="Loading insights">
      <div class="skeleton skeleton-band"></div>
      <div class="skeleton tall"></div>
      <div class="skeleton"></div>
      <div class="skeleton"></div>
    </div>
  {:else if data && isEmpty}
    <div class="empty-state" in:fade={{ duration: motionMs(MOTION_MS.base) }}>
      <p class="empty-h">No dictations yet</p>
      <p class="empty-sub">Hold your hotkey and say something. Once you've dictated a few times, your streaks, speed, and cost estimates will show up here.</p>
    </div>
  {:else if data}
    <div class="insights-results-shell" class:refreshing={status === 'loading'} aria-busy={status === 'loading'}>
      {#if status === 'loading'}
        <p class="fetch-status refresh-status" role="status" aria-live="polite">Refreshing insights…</p>
      {:else if status === 'error'}
        <p class="fetch-status fetch-status-error" role="alert">Refresh failed: {error}</p>
      {/if}

      <div class="insights-results">
        <HeroStats {data} rangeLabel={dataRangeLabel} />

        <DailyChart daily={data.daily} rangeLabel={dataRangeLabel} />
        <StreakHeatmap daily={data.streak_daily} streak={data.streak} historyStartedOn={data.history_started_on} />
        <HourStrip hourly={data.hourly} />
        <WordStats words={data.words} cleanup={data.cleanup} totals={data.totals} />
        <CostBreakdown providers={data.providers} rangeLabel={dataRangeLabel} />
      </div>
    </div>
  {/if}
</div>

<style>
  .content-inner {
    width: min(100%, var(--page-max));
    margin-inline: auto;
    padding: var(--page-pad-y) var(--page-pad-x) 36px;
    min-width: 0;
  }

  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }

  .page-h {
    font-family: var(--serif);
    font-size: 26px;
    font-weight: 500;
    letter-spacing: -0.02em;
    margin: 0 0 4px;
    line-height: 1.1;
    color: var(--ink);
  }

  .page-sub { color: var(--ink-mute); font-size: 12.5px; margin: 0 0 22px; max-width: 560px; line-height: 1.5; }

  /* Match the app-mappings / tone dropdowns rather than the default pill radius. */
  .range-picker {
    margin-top: 2px;
    --ui-dropdown-trigger-height: 28px;
  }

  .fetch-status { margin: 0 0 10px; font-size: 12px; color: var(--ink-mute); }
  .fetch-status-error {
    position: absolute;
    top: -22px;
    inset-inline: 0;
    margin: 0;
    color: var(--danger);
  }

  .insights-results-shell {
    position: relative;
    min-width: 0;
  }

  .insights-results-shell.refreshing .insights-results {
    visibility: hidden;
  }

  .refresh-status {
    position: absolute;
    inset: 0;
    z-index: 1;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 24px;
    margin: 0;
    background: color-mix(in srgb, var(--paper) 88%, transparent);
    pointer-events: none;
  }

  /* Section shell for every child component. Owned here rather than repeated
     in each of them, so the page's visual weight has a single lever. These are
     sections in an editorial page — a serif sub-heading over a hairline rule,
     sitting on bare paper — not cards. Elevation stays reserved for modals and
     popovers, per DESIGN.md. */
  .content-inner :global(.card) {
    display: flex;
    flex-direction: column;
    min-width: 0;
    margin-bottom: 30px;
  }

  .content-inner :global(.card:last-child) { margin-bottom: 0; }

  .insights-results { min-width: 0; }

  .content-inner :global(.card-head) {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 12px;
    padding-bottom: 8px;
    margin-bottom: 14px;
    border-bottom: 1px solid var(--line);
  }

  /* Matches .settings-subhead — 17px was a card title, this is a section head. */
  .content-inner :global(.card-h) {
    font-family: var(--serif);
    font-size: 14px;
    font-weight: 500;
    margin: 0;
    color: var(--ink-soft);
    letter-spacing: -0.01em;
  }

  .content-inner :global(.card-sub) {
    margin: 3px 0 0;
    font-size: 11.5px;
    line-height: 1.4;
    color: var(--ink-mute);
  }

  .skeletons {
    display: flex;
    flex-direction: column;
    gap: 30px;
  }

  .skeleton {
    height: 96px;
    border-radius: var(--r-sm);
    background: linear-gradient(
      100deg,
      var(--bg-elev) 30%,
      var(--control-hover) 50%,
      var(--bg-elev) 70%
    );
    background-size: 300% 100%;
    animation: shimmer 1.4s linear infinite;
  }
  .skeleton.tall { height: 200px; }
  .skeleton-band { height: 116px; }

  @keyframes shimmer {
    from { background-position: 150% 0; }
    to   { background-position: -150% 0; }
  }

  .empty-state {
    padding: 52px 4px;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 6px;
  }

  .empty-h {
    font-family: var(--serif);
    font-style: italic;
    font-size: 17px;
    font-weight: 500;
    color: var(--ink-soft);
    margin: 0;
  }

  .empty-sub {
    font-size: 12.5px;
    color: var(--ink-mute);
    line-height: 1.5;
    margin: 0 0 10px;
    max-width: 380px;
  }

  @media (max-width: 900px) {
    .head { flex-direction: column; }
  }

  @media (prefers-reduced-motion: reduce) {
    .skeleton { animation-duration: 2.6s; }
  }
</style>
