<script lang="ts">
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';
  import { flip } from 'svelte/animate';
  import { expoOut } from 'svelte/easing';
  import { icons } from '../icons';

  interface Entry { id: number; clean_text: string; words: number; created_at: string; }
  interface Stats { total_words: number; avg_wpm: number; day_streak: number; }

  let copiedId: number | null = null;

  async function copyText(entry: Entry) {
    try {
      await navigator.clipboard.writeText(entry.clean_text);
      copiedId = entry.id;
      setTimeout(() => { copiedId = null; }, 1500);
    } catch { /* clipboard not available in dev */ }
  }

  let recents: Entry[] = [];
  let stats: Stats = { total_words: 0, avg_wpm: 0, day_streak: 0 };
  let loading = true;

  function fmtTime(iso: string) {
    try {
      return new Date(iso + 'Z').toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
    } catch { return iso; }
  }

  function fmtDate(iso: string) {
    try {
      const d = new Date(iso + 'Z');
      const today = new Date();
      const yesterday = new Date(today); yesterday.setDate(today.getDate() - 1);
      if (d.toDateString() === today.toDateString()) return 'Today';
      if (d.toDateString() === yesterday.toDateString()) return 'Yesterday';
      return d.toLocaleDateString([], { weekday: 'short', month: 'short', day: 'numeric' });
    } catch { return iso.slice(0, 10); }
  }

  // Group entries by day label
  $: grouped = recents.reduce<{ label: string; rows: Entry[] }[]>((acc, entry) => {
    const label = fmtDate(entry.created_at);
    const group = acc.find(g => g.label === label);
    if (group) group.rows.push(entry);
    else acc.push({ label, rows: [entry] });
    return acc;
  }, []);

  async function load() {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const [r, s] = await Promise.all([invoke<Entry[]>('get_recent'), invoke<Stats>('get_stats')]);
      recents = r;
      stats = s;
    } catch {
      // dev mode — show placeholder
      recents = [];
      stats = { total_words: 0, avg_wpm: 0, day_streak: 0 };
    }
    loading = false;
  }

  onMount(() => {
    load();
    let unlisten: (() => void) | undefined;
    
    // Refresh when a new transcription comes in
    import('@tauri-apps/api/event').then(({ listen }) => {
      listen('open-flow:transcribed', load).then(u => unlisten = u).catch(() => {});
    }).catch(() => {});

    return () => {
      if (unlisten) unlisten();
    };
  });
</script>

<div class="content-inner">
  <div class="home-grid">
    <!-- Left column -->
    <div>
      <h1 class="page-h">Welcome back</h1>
      <p class="page-sub">Local-first dictation. Bring your own keys.</p>

      <!-- Dark hero card -->
      <div class="hero-photo">
        <div class="hero-photo-content">
          <h2 class="hero-photo-title">
            Hold <kbd>Alt</kbd> <kbd>Space</kbd> to dictate
          </h2>
          <p class="hero-photo-sub">
            Open Flow works in any app. Try it in
            <em class="hero-em">email, messages, docs</em> — or anywhere else.
          </p>
        </div>
      </div>

      {#if loading}
        <div class="empty-state">Loading history…</div>
      {:else if recents.length === 0}
        <div class="empty-state">
          No dictations yet. Hold <kbd>Alt</kbd> <kbd>Space</kbd> to get started.
        </div>
      {:else}
        {#each grouped as group, gi (group.label)}
          <div class="day-head" class:muted={gi > 0}>{group.label}</div>
          <div class="day-table">
            {#each group.rows as r (r.id)}
              <div class="day-row" in:fly={{ y: -10, duration: 400, easing: expoOut }} animate:flip={{ duration: 400, easing: expoOut }}>
                <div class="day-time">{fmtTime(r.created_at)}</div>
                <div class="day-text">{r.clean_text}</div>
                <button
                  class="copy-btn"
                  class:copied={copiedId === r.id}
                  onclick={() => copyText(r)}
                  title="Copy to clipboard"
                  aria-label="Copy"
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    {#if copiedId === r.id}
                      {@html icons.check}
                    {:else}
                      {@html icons.copy}
                    {/if}
                  </svg>
                </button>
              </div>
            {/each}
          </div>
        {/each}
      {/if}
    </div>

    <!-- Right column — flat stats -->
    <div class="stat-stack">
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
    </div>
  </div>
</div>

<style>
  .content-inner {
    padding: 18px 28px 36px;
    max-width: 920px;
  }

  .home-grid {
    display: grid;
    grid-template-columns: 1fr 240px;
    gap: 18px;
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

  .page-sub { color: var(--ink-mute); font-size: 12.5px; margin: 0 0 22px; }

  .hero-photo {
    position: relative;
    border-radius: var(--r-lg);
    overflow: hidden;
    margin-bottom: 22px;
    height: 120px;
    background: var(--arm-950);
  }

  .hero-photo::before {
    content: '';
    position: absolute;
    right: -80px; top: -80px;
    width: 360px; height: 360px;
    background: radial-gradient(circle, rgba(217,119,87,0.22) 0%, transparent 60%);
    pointer-events: none;
  }

  .hero-photo-content {
    position: relative;
    padding: 22px 28px;
    max-width: 80%;
  }

  .hero-photo-title {
    font-family: var(--serif);
    font-size: 22px;
    font-weight: 500;
    letter-spacing: -0.02em;
    margin: 0 0 8px;
    line-height: 1.15;
    color: white;
  }

  .hero-photo-title :global(kbd) {
    background: rgba(255,255,255,0.10);
    border: 1px solid rgba(255,255,255,0.18);
    border-radius: 5px;
    font-family: var(--mono);
    font-size: 13px;
    padding: 1px 6px;
    color: var(--jap-300);
    font-weight: 500;
  }

  .hero-photo-sub {
    font-size: 12.5px;
    color: var(--arm-400);
    margin: 0;
    line-height: 1.5;
  }

  .hero-em {
    font-family: var(--serif);
    font-style: italic;
    color: var(--amber-50);
  }

  .day-head {
    font-family: var(--serif);
    font-style: italic;
    font-size: 14px;
    color: var(--ink-soft);
    margin: 4px 4px 10px;
  }
  .day-head.muted { margin-top: 22px; color: var(--ink-mute); }

  .day-table { border-top: 1px solid var(--line); }

  .day-row {
    display: grid;
    grid-template-columns: 84px 1fr auto;
    align-items: start;
    padding: 11px 4px;
    border-bottom: 1px solid var(--line);
    gap: 14px;
    cursor: default;
  }
  .day-row:hover { background: var(--amber-100); }
  .day-row:not(:hover) .copy-btn { opacity: 0; pointer-events: none; }

  .copy-btn {
    all: unset;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: 4px;
    color: var(--ink-mute);
    opacity: 0.45;
    transition: color 0.12s, opacity 0.12s;
    flex-shrink: 0;
    margin-top: 2px;
  }
  .copy-btn:hover { opacity: 0.9; }
  .copy-btn.copied { color: var(--jap-500, #d97757); opacity: 1; }
  .copy-btn svg { width: 10px; height: 10px; }

  .day-time {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--ink-mute);
    font-variant-numeric: tabular-nums;
    padding-top: 2px;
    font-weight: 500;
  }

  .day-text { font-size: 13px; line-height: 1.55; color: var(--ink-strong); }

  .empty-state {
    padding: 32px 4px;
    font-size: 13px;
    color: var(--ink-mute);
    font-style: italic;
  }

  .empty-state :global(kbd) {
    font-style: normal;
    background: var(--paper-2);
    border: 1px solid var(--line-strong);
    border-radius: 4px;
    font-family: var(--mono);
    font-size: 11px;
    padding: 1px 5px;
    color: var(--ink);
  }

  /* Flat stats */
  .stat-stack { display: flex; flex-direction: column; gap: 22px; }

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
    font-family: var(--serif);
    font-size: 24px;
    font-weight: 500;
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
</style>
