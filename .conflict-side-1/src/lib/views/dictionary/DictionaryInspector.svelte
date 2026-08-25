<script lang="ts">
  import { fade } from 'svelte/transition';
  import type { DictionaryEntry } from '../../stores';
  import { MOTION_MS, MOTION_PX, motionMs, motionPx, pageSwap } from '../../motion';
  import { confidenceLabel, fmtDate } from './helpers';

  let {
    selected,
    inspectorDir,
    deleteTarget,
    onEdit,
    onDelete,
  }: {
    selected: DictionaryEntry | null;
    inspectorDir: 1 | -1;
    deleteTarget: number | null;
    onEdit: (e: DictionaryEntry) => void;
    onDelete: (id: number) => void;
  } = $props();
</script>

<div class="inspector-col">
  {#if selected}
    {#key selected.id}
      <div
        class="inspector"
        in:pageSwap={{ axis: 'x', distance: inspectorDir * motionPx(MOTION_PX.panel), duration: motionMs(MOTION_MS.panel) }}
      >
        <div class="insp-trigger">{selected.term}</div>

        {#if selected.mistake}
          <div class="insp-often">
            <span class="insp-often-label">often:</span>
            <span class="insp-often-text">"{selected.mistake}"</span>
          </div>
        {/if}

        {#if selected.auto_learned}
          <div class="insp-auto-badge">
            <svg width="10" height="10" viewBox="0 0 24 24" fill="currentColor">
              <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z"/>
            </svg>
            Auto-learned
          </div>
          <div class="insp-often">
            <span class="insp-often-label">Confidence:</span>
            <span class="insp-often-text">{confidenceLabel(selected.confidence_tier)}</span>
          </div>
        {/if}

        <div class="insp-divider"></div>

        <div class="insp-stats">
          {#if selected.correction_count > 0}
            <div class="insp-stat-row">
              <span class="insp-stat-num">{selected.correction_count}</span>
              <span class="insp-stat-label">{selected.correction_count === 1 ? 'correction' : 'corrections'}</span>
            </div>
          {/if}
          <div class="insp-stat-row">
            <span class="insp-stat-label">Added</span>
            <span class="insp-stat-date">{fmtDate(selected.created_at)}</span>
          </div>
          {#if selected.last_seen_at}
            <div class="insp-stat-row">
              <span class="insp-stat-label">Last seen</span>
              <span class="insp-stat-date">{fmtDate(selected.last_seen_at)}</span>
            </div>
          {/if}
        </div>

        <div class="insp-actions">
          <button class="btn-insp-edit" onclick={() => onEdit(selected!)}>
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M12 20h9M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4z"/></svg>
            Edit
          </button>
          <button
            class="btn-insp-delete"
            class:armed={deleteTarget === selected.id}
            onclick={() => onDelete(selected!.id)}
          >
            {#if deleteTarget === selected.id}
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M20 6 9 17l-5-5"/></svg>
              Confirm
            {:else}
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M3 6h18"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><path d="m19 6-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/></svg>
              Delete
            {/if}
          </button>
        </div>
      </div>
    {/key}
  {:else}
    <div class="inspector-empty" in:fade={{ duration: motionMs(MOTION_MS.base) }}>
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" style="color:var(--arm-300)">
        <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"/>
      </svg>
      <p>Select a term<br>to inspect it</p>
    </div>
  {/if}
</div>

<style>
  .inspector-col {
    position: sticky;
    top: 0;
  }

  .inspector {
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    padding: 20px 22px;
    display: flex;
    flex-direction: column;
  }

  .insp-trigger {
    font-family: var(--serif);
    font-size: 19px;
    font-weight: 500;
    letter-spacing: -0.015em;
    color: var(--ink);
    line-height: 1.2;
  }

  .insp-often {
    display: flex;
    align-items: baseline;
    gap: 6px;
    margin-top: 8px;
  }

  .insp-often-label {
    font-family: var(--mono);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--ink-faint);
    flex-shrink: 0;
  }

  .insp-often-text {
    font-size: 13px;
    color: var(--ink-soft);
    font-style: italic;
    line-height: 1.5;
    word-break: break-word;
  }

  .insp-auto-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin-top: 12px;
    padding: 4px 9px;
    background: var(--accent-soft);
    color: var(--accent-ink);
    border-radius: 99px;
    font-size: 11px;
    font-weight: 500;
    align-self: flex-start;
  }

  .insp-divider {
    height: 1px;
    background: var(--line-soft);
    margin: 18px 0 14px;
  }

  .insp-stats {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 18px;
  }

  .insp-stat-row {
    display: flex;
    align-items: baseline;
    gap: 7px;
  }

  .insp-stat-num {
    font-family: var(--serif);
    font-size: 22px;
    font-weight: 500;
    letter-spacing: -0.02em;
    color: var(--accent-ink);
    line-height: 1;
  }

  .insp-stat-label { font-size: 11.5px; color: var(--ink-mute); }

  .insp-stat-date {
    font-size: 12.5px;
    color: var(--ink-soft);
    font-weight: 500;
    margin-left: auto;
  }

  .insp-actions { display: flex; gap: 8px; }

  .btn-insp-edit {
    flex: 1;
    background: var(--ink);
    color: var(--amber-50);
    border: 0;
    border-radius: 8px;
    padding: 7px 14px;
    font-size: 12.5px;
    font-weight: 500;
    font-family: var(--sans);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    cursor: pointer;
    transition: opacity 0.15s;
  }
  .btn-insp-edit:hover { opacity: 0.82; }

  .btn-insp-delete {
    background: transparent;
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 7px 14px;
    font-size: 12.5px;
    font-family: var(--sans);
    color: var(--ink-soft);
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    transition: background 0.12s, color 0.12s, border-color 0.12s;
  }
  .btn-insp-delete:hover { background: var(--control-hover); color: var(--ink-strong); }
  .btn-insp-delete.armed { background: var(--danger-bg); color: var(--danger); border-color: var(--danger-line); }
  .btn-insp-delete.armed:hover { background: var(--danger-bg); }

  .inspector-empty {
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    padding: 40px 22px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    text-align: center;
  }

  .inspector-empty p {
    font-family: var(--serif);
    font-style: italic;
    font-size: 14px;
    color: var(--ink-mute);
    margin: 0;
    line-height: 1.6;
  }

  @media (max-width: 1060px) {
    .inspector-col { position: static; }
  }
</style>
