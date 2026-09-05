<script lang="ts">
  import { onMount } from 'svelte';
  import { fade, fly } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { invoke, listen } from '../tauri';
  import { startPolling } from '../polling';
  import { formatIpcError } from '../stores';
  import { MOTION_MS, motionMs } from '../motion';
  import Dropdown from '../components/Dropdown.svelte';
  import HeroStats from './insights/HeroStats.svelte';
  import DailyChart from './insights/DailyChart.svelte';
  import StreakHeatmap from './insights/StreakHeatmap.svelte';
  import CostBreakdown from './insights/CostBreakdown.svelte';
  import HourStrip from './insights/HourStrip.svelte';
  import WordStats from './insights/WordStats.svelte';
  import { icons } from '../icons';
  import { contextsStore, loadContexts, orderedContexts } from '../contextsStore.svelte';
  import { EMPTY_INSIGHTS, RANGE_OPTIONS, type InsightsPayload, type InsightsRange } from './insights/types';

  let range = $state<InsightsRange>(30);
  // null = every context. Kept local to the page rather than sharing the
  // sidebar's selection: filtering a chart is not the same act as opening a
  // context to edit it, and tying them together would make each surprise the
  // other.
  let contextId = $state<number | null>(null);
  let contextOpen = $state(false);
  let data = $state<InsightsPayload | null>(null);
  let status = $state<'loading' | 'loaded' | 'error'>('loading');
  let error = $state('');
  let rangeOpen = $state(false);
  let displayVersion = $state(0);

  let fetchToken = 0;
  let mounted = false;

  const rangeLabel = $derived(RANGE_OPTIONS.find((o) => o.value === range)?.label ?? 'Last 30 days');
  const contextOptions = $derived(orderedContexts(contextsStore.contexts));
  const activeContext = $derived(contextOptions.find((c) => c.id === contextId) ?? null);
  const contextLabel = $derived(activeContext?.name ?? 'All contexts');
  const isEmpty = $derived(!data || data.totals.total_transcriptions === 0);

  // A context deleted elsewhere (sidebar, contexts page) must not keep
  // filtering the page behind an "All contexts" label: drop the stale
  // selection and reload so the numbers match what the filter shows.
  $effect(() => {
    if (contextId === null || contextOptions.length === 0) return;
    if (contextOptions.some((c) => c.id === contextId)) return;
    contextId = null;
    void load();
  });

  async function load(opts?: { silent?: boolean }) {
    const token = ++fetchToken;
    if (!opts?.silent) {
      status = 'loading';
      error = '';
    }
    try {
      const payload = await invoke<InsightsPayload>('get_insights', { days: range, contextId });
      if (!mounted || token !== fetchToken) return;
      data = payload ?? EMPTY_INSIGHTS;
      status = 'loaded';
      error = '';
      // Keep the previous range visible while its replacement loads, then
      // give explicit range/manual refreshes one deliberate entry. Silent
      // polling must update values in place so chart hover/focus state survives.
      if (!opts?.silent) displayVersion += 1;
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
    load();
  }

  function pickContext(next: number | null) {
    contextOpen = false;
    if (next === contextId) return;
    contextId = next;
    load();
  }

  onMount(() => {
    mounted = true;
    void loadContexts();
    load();
    // Events keep dictations current; reconcile missed events and date rollover
    // once a minute while visible, and immediately when the window returns.
    const poll = startPolling(() => load({ silent: true }), 60_000, { immediate: false });
    let unlisten: (() => void) | undefined;
    // Refresh live as new dictations land, so the page never shows stale
    // numbers while it's open. Silent so a background refresh never flashes
    // the loading state.
    listen('verenu:transcribed', () => { if (!document.hidden) poll.request(); })
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
    return () => {
      mounted = false;
      unlisten?.();
      poll.stop();
    };
  });
</script>

<div class="content-inner">
  <div class="head">
    <div>
      <h1 class="page-h">Insights</h1>
      <p class="page-sub">How much you dictate, how fast, and what it costs. Everything here is computed locally from your own history — nothing leaves your machine.</p>
    </div>

    <div class="head-filters">
    <!-- The shared Dropdown owns Escape, outside-click, and arrow navigation
         for each menu once it is open. -->
    <Dropdown bind:open={contextOpen} closeSelector=".context-picker">
      <div class="ui-dropdown range-picker context-picker">
        <button
          type="button"
          class="ui-dropdown-trigger ui-dropdown-trigger--compact"
          aria-expanded={contextOpen}
          aria-haspopup="listbox"
          onclick={() => (contextOpen = !contextOpen)}
        >
          {#if activeContext}
            <span class="ctx-swatch" style={activeContext.color ? `color: ${activeContext.color}` : ''} aria-hidden="true">
              {#if activeContext.is_everywhere}
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"><circle cx="12" cy="12" r="8"/><path d="M4 12h16M12 4c2 2.3 3 5 3 8s-1 5.7-3 8c-2-2.3-3-5-3-8s1-5.7 3-8Z"/></svg>
              {:else if activeContext.icon && icons[activeContext.icon as keyof typeof icons]}
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">{@html icons[activeContext.icon as keyof typeof icons]}</svg>
              {/if}
            </span>
          {/if}
          {contextLabel}
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>
        </button>
        {#if contextOpen}
          <div class="ui-dropdown-menu ui-dropdown-menu--padded ctx-menu-scroll" role="listbox" aria-label="Context group" in:fly={{ y: 5, duration: motionMs(MOTION_MS.fast), easing: expoOut }} out:fly={{ y: 3, duration: motionMs(110), easing: expoOut }}>
            <button
              type="button"
              class="ui-dropdown-option"
              class:active={contextId === null}
              role="option"
              aria-selected={contextId === null}
              onclick={() => pickContext(null)}
            >All contexts</button>
            {#each contextOptions as option (option.id)}
              <button
                type="button"
                class="ui-dropdown-option"
                class:active={option.id === contextId}
                role="option"
                aria-selected={option.id === contextId}
                onclick={() => pickContext(option.id)}
              >{option.name}</button>
            {/each}
          </div>
        {/if}
      </div>
    </Dropdown>

    <Dropdown bind:open={rangeOpen} closeSelector=".range-picker">
      <div class="ui-dropdown range-picker">
        <button
          type="button"
          class="ui-dropdown-trigger ui-dropdown-trigger--compact"
          aria-expanded={rangeOpen}
          aria-haspopup="listbox"
          onclick={() => (rangeOpen = !rangeOpen)}
        >
          {rangeLabel}
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>
        </button>
        {#if rangeOpen}
          <div class="ui-dropdown-menu ui-dropdown-menu--padded" role="listbox" aria-label="Date range" in:fly={{ y: 5, duration: motionMs(MOTION_MS.fast), easing: expoOut }} out:fly={{ y: 3, duration: motionMs(110), easing: expoOut }}>
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
        {/if}
      </div>
    </Dropdown>
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
      {#if activeContext}
        <p class="empty-h">Nothing dictated in {activeContext.name} yet</p>
        <p class="empty-sub">This context group hasn't been active for a dictation in this range. Try a wider range, or switch back to all contexts.</p>
        <button type="button" class="btn-ghost" onclick={() => pickContext(null)}>Show all contexts</button>
      {:else}
        <p class="empty-h">No dictations yet</p>
        <p class="empty-sub">Hold your hotkey and say something. Once you've dictated a few times, your streaks, speed, and cost estimates will show up here.</p>
      {/if}
    </div>
  {:else if data}
    {#if status === 'error'}
      <p class="fetch-status fetch-status-error" role="alert">Refresh failed: {error}</p>
    {:else if status === 'loading'}
      <p class="fetch-status" role="status" aria-live="polite">Refreshing insights…</p>
    {/if}

    {#key displayVersion}
      <div class="insights-results" in:fade={{ duration: motionMs(MOTION_MS.base) }}>
        <HeroStats {data} {rangeLabel} />

        <DailyChart daily={data.daily} {rangeLabel} />
        <StreakHeatmap daily={data.streak_daily} streak={data.streak} historyStartedOn={data.history_started_on} />
        <HourStrip hourly={data.hourly} />
        <WordStats words={data.words} cleanup={data.cleanup} totals={data.totals} />
        <CostBreakdown providers={data.providers} {rangeLabel} />
      </div>
    {/key}
  {/if}
</div>

<style>
  .content-inner {
    width: min(100%, var(--page-max));
    margin-inline: auto;
    padding: var(--page-pad-y) var(--page-pad-x) 36px;
    min-width: 0;
    /* The sidebar eats ~220px of viewport width, so viewport media queries
       fire far too late for this column. Query the column itself instead —
       same pattern as the settings-panel container in Settings.svelte. */
    container-type: inline-size;
    container-name: insights;
  }

  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px 16px;
    flex-wrap: wrap;
    min-width: 0;
  }

  .head > div:first-child {
    flex: 1 1 240px;
    min-width: 0;
  }

  .page-h {
    font-family: var(--sans);
    font-size: 23px;
    font-weight: 600;
    letter-spacing: -0.025em;
    margin: 0 0 4px;
    line-height: 1.1;
    color: var(--ink);
  }

  .page-sub { color: var(--ink-mute); font-size: 12.5px; margin: 0 0 22px; max-width: 560px; line-height: 1.5; }

  .head-filters {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: flex-end;
    min-width: 0;
    flex: 0 1 auto;
  }

  .head-filters > :global(*) {
    min-width: 0;
    max-width: 100%;
  }

  /* Match the app-mappings / tone dropdowns rather than the default pill radius. */
  .range-picker {
    margin-top: 2px;
    --ui-dropdown-trigger-height: 28px;
  }

  .context-picker { max-width: 200px; }
  .context-picker .ui-dropdown-trigger { gap: 6px; }

  /* A long context list scrolls inside the menu instead of running off-screen. */
  .ctx-menu-scroll { max-height: 280px; overflow-y: auto; }

  .ctx-swatch {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    color: var(--ink-mute);
  }

  .fetch-status { margin: 0 0 10px; font-size: 12px; color: var(--ink-mute); }
  .fetch-status-error { color: var(--danger); }

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
    gap: 8px 12px;
    flex-wrap: wrap;
    padding-bottom: 8px;
    margin-bottom: 14px;
    border-bottom: 1px solid var(--line);
  }

  /* Matches .settings-subhead — 17px was a card title, this is a section head. */
  .content-inner :global(.card-h) {
    font-family: var(--sans);
    font-size: 13px;
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
    font-family: var(--sans);
    font-style: normal;
    font-size: 15px;
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

  /* Container query, not viewport: the rail + sidebar decide how wide this
     column actually is. Below ~560px the title and both dropdowns stack and
     left-align instead of squeezing into one row. */
  @container insights (max-width: 560px) {
    .head { flex-direction: column; align-items: stretch; }
    .head-filters { justify-content: flex-start; }
    .page-sub { max-width: none; }
    .context-picker { max-width: 100%; }
    .range-picker { max-width: 100%; }
  }

  @media (prefers-reduced-motion: reduce) {
    .skeleton { animation-duration: 2.6s; }
  }
</style>
