<script lang="ts">
  import type { Stats } from './helpers';

  export let stats: Stats;
</script>

<div class="stat-card">
  <div class="stat-line">
    <span class="stat-num">
      {#if stats.total_words >= 1000}
        {(stats.total_words / 1000).toFixed(1)}<small>k</small>
      {:else}
        {stats.total_words}
      {/if}
    </span>
    <span class="stat-label">total words</span>
  </div>
  <div class="stat-line">
    <span class="stat-num">{Math.round(stats.avg_wpm) || '—'}</span>
    <span class="stat-label">wpm</span>
  </div>
  <div class="stat-line">
    <span class="stat-num">{stats.day_streak}</span>
    <span class="stat-label">day streak</span>
  </div>
</div>

<style>
  .stat-card { display: flex; flex-direction: column; gap: 10px; }

  .stat-line {
    display: flex;
    align-items: baseline;
    gap: 8px;
    border-bottom: 1px solid var(--line);
    padding-bottom: 9px;
  }
  .stat-line:last-child { border-bottom: 0; padding-bottom: 0; }

  .stat-num {
    font-family: var(--sans);
    font-size: 22px;
    font-weight: 600;
    letter-spacing: -0.02em;
    line-height: 1;
    color: var(--ink);
  }
  .stat-num :global(small) {
    font-family: var(--serif);
    font-size: 14px;
    color: var(--ink-mute);
    margin-left: 1px;
    font-weight: 400;
  }

  .stat-label { font-size: 11.5px; color: var(--ink-mute); margin-left: auto; }

  @media (max-width: 1060px) {
    .stat-card {
      display: grid;
      grid-template-columns: repeat(3, minmax(0, 1fr));
      gap: 12px;
    }

    .stat-line {
      border-bottom: 0;
      border-top: 1px solid var(--line);
      padding: 10px 0 0;
      min-width: 0;
    }
  }

  @media (max-width: 720px) {
    .stat-card {
      grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
    }
  }
</style>
