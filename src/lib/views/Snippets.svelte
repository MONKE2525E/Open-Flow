<script lang="ts">
  const snips = [
    { trigger: '/sig',     text: "— Noah Bergeron, Maintainer, Open Flow", uses: 142, when: '2h ago'   },
    { trigger: '/addr',    text: "1428 Pearl St, Boulder CO 80302",          uses: 27,  when: 'Yesterday'},
    { trigger: '/wfh',     text: "Working from home today — reply may be slower.", uses: 9, when: '3d ago' },
    { trigger: '/standup', text: "Yesterday I — Today I'm — Blockers: none.", uses: 64, when: '1d ago'  },
    { trigger: '/repo',    text: "github.com/noah/open-flow",                uses: 51,  when: '4h ago'  },
    { trigger: '/ty',      text: "Thanks — appreciate it. I'll follow up later.", uses: 88, when: '11h ago' },
  ];
</script>

<div class="content-inner">
  <h1 class="page-h">Snippets</h1>
  <p class="page-sub">Speak a trigger and Open Flow expands it.</p>

  <div class="toolbar">
    <div class="search">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="11" cy="11" r="7"/><path d="m21 21-4.35-4.35"/>
      </svg>
      <span>Search snippets…</span>
    </div>
    <button class="btn-primary" disabled>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M12 5v14M5 12h14"/></svg>
      <span>New snippet</span>
    </button>
  </div>

  <div class="list-wrap">
    <div class="list-row snip-row header">
      <div class="head">Trigger</div>
      <div class="head">Expansion</div>
      <div class="head"></div>
    </div>
    {#each snips as s}
      <div class="list-row snip-row">
        <div>
          <div class="snip-trigger">{s.trigger}</div>
          <div class="snip-meta">{s.uses} uses · {s.when}</div>
        </div>
        <div class="snip-text">{s.text}</div>
        <div class="row-actions">
          <button class="icon-btn" disabled>
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M12 20h9M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4z"/></svg>
          </button>
          <button class="icon-btn" disabled>
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M3 6h18"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><path d="m19 6-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/></svg>
          </button>
        </div>
      </div>
    {/each}
  </div>

  <div class="lock-overlay">
    <div class="lock-card">
      <div class="lock-eyebrow">Coming soon</div>
      <h3 class="lock-h">Snippets is on the way</h3>
      <p class="lock-sub">Speak a trigger, watch it expand. Coming once the core dictation flow is locked in.</p>
    </div>
  </div>
</div>

<style>
  .content-inner {
    padding: 18px 28px 36px;
    max-width: 920px;
    position: relative;
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

  .toolbar { display: flex; gap: 8px; margin-bottom: 14px; }

  .search {
    flex: 1;
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 6px 12px;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--ink-mute);
  }

  .btn-primary {
    background: var(--ink);
    color: var(--amber-50);
    border: 0;
    border-radius: 8px;
    padding: 6px 14px;
    font-size: 12.5px;
    font-weight: 500;
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }

  .list-wrap {
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    overflow: hidden;
    background: var(--bg-elev);
  }

  .list-row {
    display: grid;
    gap: 16px;
    padding: 11px 16px;
    border-bottom: 1px solid var(--line);
    align-items: center;
    font-size: 13px;
    color: var(--ink-strong);
  }

  .list-row:last-child { border-bottom: 0; }

  .list-row .head {
    font-family: var(--mono);
    font-size: 9.5px;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--ink-mute);
  }

  .list-row.header { padding: 8px 16px; background: var(--paper); }

  .snip-row { grid-template-columns: 1fr 1fr 60px; }

  .snip-trigger {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--accent-ink);
    font-weight: 500;
  }

  .snip-meta {
    font-size: 11px;
    color: var(--ink-mute);
    margin-top: 2px;
    font-family: var(--mono);
  }

  .snip-text {
    font-size: 13px;
    line-height: 1.45;
    color: var(--ink-strong);
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
  }

  .row-actions {
    display: flex;
    gap: 2px;
    justify-content: flex-end;
    opacity: 0;
    transition: opacity 0.15s;
  }

  .list-row:hover .row-actions { opacity: 1; }

  .icon-btn {
    width: 24px; height: 24px;
    background: transparent;
    border: 0;
    border-radius: 6px;
    display: grid;
    place-items: center;
    color: var(--ink-mute);
    cursor: pointer;
  }

  .icon-btn:hover { background: var(--paper); color: var(--ink-strong); }

  .lock-overlay {
    position: absolute;
    inset: 0;
    background: linear-gradient(to bottom, transparent 25%, rgba(249,247,243,0.85) 55%, var(--paper) 100%);
    backdrop-filter: blur(2px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 4;
  }

  .lock-card {
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    padding: 22px 26px;
    text-align: center;
    max-width: 340px;
    box-shadow: 0 12px 30px -10px rgba(13,10,8,0.08);
  }

  .lock-eyebrow {
    font-family: var(--mono);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.14em;
    color: var(--accent-ink);
    margin-bottom: 8px;
  }

  .lock-h {
    font-family: var(--serif);
    font-size: 20px;
    font-weight: 500;
    letter-spacing: -0.015em;
    margin: 0 0 6px;
    line-height: 1.2;
    color: var(--ink);
  }

  .lock-sub { font-size: 12.5px; color: var(--ink-mute); line-height: 1.5; margin: 0; }
</style>
