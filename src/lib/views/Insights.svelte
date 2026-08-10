<script lang="ts">
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import { invoke, listen } from '../tauri';
  import { formatIpcError } from '../stores';
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
  const isEmpty = $derived(!data || data.totals.total_transcriptions === 0);

  async function load(opts?: { silent?: boolean }) {
    const token = ++fetchToken;
    if (!opts?.silent) {
      status = 'loading';
      error = '';
    }
    try {
      const payload = await invoke<InsightsPayload>('get_insights', { days: range });
      if (!mounted || token !== fetchToken) return;
      data = payload ?? EMPTY_INSIGHTS;
      status = 'loaded';
      error = '';
    } catch (err) {
      if (!mounted || token !== fetchToken) return;
      console.error('IPC get_insights failed:', err);
      // A background refresh failure must not replace good data with an
      // error banner — the last known numbers stay up, next tick retries.
      if (!opts?.silent) {
        error = formatIpcError(err);
        status = 'error';
      }
    }
  }

  function pickRange(next: InsightsRange) {
    rangeOpen = false;
    if (next === range) return;
    range = next;
    load();
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

    <div class="ui-dropdown range-picker">
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
    <div class="empty-state empty-state-error" role="alert" in:fade={{ duration: 220 }}>
      <p class="empty-h">Could not load insights</p>
      <p class="empty-sub">The backend is unavailable right now. {error}</p>
      <button type="button" class="btn-ghost" onclick={() => load()}>Try again</button>
    </div>
  {:else if status === 'loading' && !data}
    <div class="grid" aria-busy="true" aria-label="Loading insights">
      <div class="skeleton span-4"></div>
      <div class="skeleton span-4"></div>
      <div class="skeleton span-4"></div>
      <div class="skeleton span-8 tall"></div>
      <div class="skeleton span-4 tall"></div>
      <div class="skeleton span-12 tall"></div>
    </div>
  {:else if data && isEmpty}
    <div class="empty-state" in:fade={{ duration: 220 }}>
      <p class="empty-h">No dictations yet</p>
      <p class="empty-sub">Hold your hotkey and say something. Once you've dictated a few times, your streaks, speed, and cost estimates will show up here.</p>
    </div>
  {:else if data}
    {#if status === 'error'}
      <p class="fetch-status fetch-status-error" role="alert">Refresh failed: {error}</p>
    {:else if status === 'loading'}
      <p class="fetch-status" role="status" aria-live="polite">Refreshing insights…</p>
    {/if}

    <HeroStats {data} {rangeLabel} />

    <div class="grid">
      <div class="span-8"><DailyChart daily={data.daily} {rangeLabel} /></div>
      <div class="span-4"><StreakHeatmap daily={data.daily} streak={data.streak} /></div>
      <div class="span-6"><CostBreakdown providers={data.providers} {rangeLabel} /></div>
      <div class="span-6"><HourStrip hourly={data.hourly} /></div>
      <div class="span-12">
        <WordStats words={data.words} cleanup={data.cleanup} totals={data.totals} />
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

  .range-picker .ui-dropdown-trigger {
    --ui-dropdown-trigger-bg: transparent;
    border-color: var(--line-strong);
    border-radius: 6px;
    font-size: 12px;
  }

  .fetch-status { margin: 0 0 10px; font-size: 12px; color: var(--ink-mute); }
  .fetch-status-error { color: var(--danger); }

  .grid {
    display: grid;
    grid-template-columns: repeat(12, minmax(0, 1fr));
    gap: 14px;
  }

  /* display:flex + a flex:1 child makes each card fill the tallest sibling in
     its row (e.g. cost breakdown's table vs. the shorter hour strip), instead
     of every card sizing to its own content. */
  .span-4,
  .span-6,
  .span-8,
  .span-12 {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .span-4  { grid-column: span 4; }
  .span-6  { grid-column: span 6; }
  .span-8  { grid-column: span 8; }
  .span-12 { grid-column: span 12; }

  .span-4 :global(.card),
  .span-6 :global(.card),
  .span-8 :global(.card),
  .span-12 :global(.card) {
    flex: 1;
  }

  .skeleton {
    height: 118px;
    border-radius: var(--r-md);
    border: 1px solid var(--line);
    background: linear-gradient(
      100deg,
      var(--bg-elev) 30%,
      var(--control-hover) 50%,
      var(--bg-elev) 70%
    );
    background-size: 300% 100%;
    animation: shimmer 1.4s linear infinite;
  }
  .skeleton.tall { height: 232px; }

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
    .grid { grid-template-columns: 1fr; }
    .span-4, .span-6, .span-8, .span-12 { grid-column: span 1; }
    .head { flex-direction: column; }
  }

  @media (prefers-reduced-motion: reduce) {
    .skeleton { animation-duration: 2.6s; }
  }
</style>
