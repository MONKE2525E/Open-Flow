<script lang="ts">
  import { estimateCost } from './pricing';
  import { fmtDuration, fmtNumber, fmtUsd } from './helpers';
  import DonutChart, { type DonutSegment } from './DonutChart.svelte';
  import type { InsightsProviderUsage } from './types';

  let { providers, rangeLabel }: { providers: InsightsProviderUsage[]; rangeLabel: string } = $props();

  const summary = $derived(estimateCost(providers));

  /* Accent-derived ramp — the accent is user-swappable, so no fixed hues. */
  function segmentColor(index: number, count: number): string {
    if (count <= 1) return 'var(--accent)';
    const pct = 100 - Math.round((index / Math.max(1, count - 1)) * 62);
    return `color-mix(in srgb, var(--accent) ${pct}%, var(--paper-2))`;
  }

  const segments = $derived.by((): DonutSegment[] => {
    const priced = summary.rows.filter((row) => (row.cost ?? 0) > 0);
    return priced.map((row, i) => ({
      // Provider must be part of the id — the same model+task can exist under
      // different providers, and DonutChart keys its internal Maps by id.
      id: `${row.provider}:${row.model}:${row.task}`,
      name: row.model,
      color: segmentColor(i, priced.length),
      value: row.cost ?? 0,
      valueLabel: fmtUsd(row.cost),
    }));
  });

  function usageLabel(row: InsightsProviderUsage): string {
    if (row.audio_ms > 0) return fmtDuration(row.audio_ms);
    const tokens = (row.input_chars + row.output_chars) / 4;
    return `~${fmtNumber(tokens)} tokens`;
  }
</script>

<section class="card">
  <header class="card-head">
    <div>
      <h2 class="card-h">Estimated API cost</h2>
      <p class="card-sub">{rangeLabel} · billed by your provider, not by Verenu</p>
    </div>
  </header>

  {#if summary.rows.length === 0}
    <p class="foot">No API usage recorded in this range.</p>
  {:else}
    {#if segments.length > 0}
      <DonutChart {segments} primaryLabel={fmtUsd(summary.total)} secondaryLabel="total" />
    {/if}

    <table class="cost-table">
      <thead>
        <tr>
          <th scope="col">Model</th>
          <th scope="col" class="num">Calls</th>
          <th scope="col" class="num">Usage</th>
          <th scope="col" class="num">Est. cost</th>
          <th scope="col" class="num">Share</th>
        </tr>
      </thead>
      <tbody>
        {#each summary.rows as row}
          <tr>
            <th scope="row">
              <span class="model">{row.model}</span>
              <span class="task">{row.task}</span>
            </th>
            <td class="num">{fmtNumber(row.calls)}</td>
            <td class="num">{usageLabel(row)}</td>
            <td class="num">{fmtUsd(row.cost)}</td>
            <td class="num">{row.cost === null ? '—' : `${(row.share * 100).toFixed(1)}%`}</td>
          </tr>
        {/each}
      </tbody>
    </table>

    {#if summary.hasUnpriced}
      <p class="foot">Models shown as — have no published rate on file, so they are missing from the total.</p>
    {/if}
  {/if}
</section>

<style>
  .card {
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    padding: 15px 18px 14px;
    min-width: 0;
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
    line-height: 1.4;
  }

  .cost-table {
    width: 100%;
    border-collapse: collapse;
    margin-top: 16px;
    font-size: 11.5px;
  }
  .cost-table th,
  .cost-table td {
    text-align: left;
    padding: 7px 6px;
    border-bottom: 1px solid var(--line);
    font-weight: 400;
  }
  .cost-table thead th {
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink-mute);
  }
  .cost-table tbody tr:last-child th,
  .cost-table tbody tr:last-child td { border-bottom: 0; }

  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--ink-soft);
    white-space: nowrap;
  }

  .model {
    display: block;
    font-family: var(--mono);
    font-size: 11px;
    color: var(--ink);
  }
  .task {
    font-size: 10.5px;
    color: var(--ink-mute);
  }

  .foot {
    margin: 12px 0 0;
    font-size: 11px;
    color: var(--ink-mute);
    line-height: 1.45;
  }

</style>
