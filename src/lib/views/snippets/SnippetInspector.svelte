<script lang="ts">
  import { fly, fade } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import type { Snippet } from '../../stores';
  import { MOTION_MS, MOTION_PX, motionMs, motionPx, pageSwap } from '../../motion';
  import { fmtDate } from './helpers';

  let {
    selected,
    inspectorDir,
    deleteTarget,
    onEdit,
    onDelete,
  }: {
    selected: Snippet | null;
    inspectorDir: 1 | -1;
    deleteTarget: number | null;
    onEdit: (s: Snippet) => void;
    onDelete: (id: number) => void;
  } = $props();
</script>

<div class="inspector-col">
  {#if selected}
    {#key selected.id}
      <div
        class="inspector"
        in:pageSwap={{ axis: 'x', distance: inspectorDir * motionPx(MOTION_PX.panel), duration: motionMs(MOTION_MS.panel) }}
        out:pageSwap={{ axis: 'x', distance: -inspectorDir * motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.fast + 40) }}
      >
        <div class="insp-trigger">{selected.trigger}</div>
        <div class="insp-arrow" aria-hidden="true">
        <svg width="11" height="16" viewBox="0 0 11 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <line x1="5.5" y1="0" x2="5.5" y2="12"/>
          <polyline points="2,9 5.5,13.5 9,9"/>
        </svg>
        </div>
        <div class="insp-expansion scroll-styled">{selected.expansion}</div>

        {#if selected.instructions}
          <div class="insp-instructions" in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.base), easing: expoOut }}>
            <div class="insp-instr-label">
            <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="12" cy="12" r="10"/><path d="M12 16v-4M12 8h.01"/>
            </svg>
            Cleanup instructions
            </div>
            <p class="insp-instr-text">{selected.instructions}</p>
          </div>
        {/if}

        <div class="insp-divider"></div>

        <div class="insp-stats">
          <div class="insp-stat-row">
          <span class="insp-stat-num">{selected.use_count}</span>
          <span class="insp-stat-label">{selected.use_count === 1 ? 'use' : 'uses'}</span>
          </div>
          <div class="insp-stat-row">
          <span class="insp-stat-label">Added</span>
          <span class="insp-stat-date">{fmtDate(selected.created_at)}</span>
          </div>
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
        <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/>
        <polyline points="14 2 14 8 20 8"/>
        <line x1="8" y1="13" x2="16" y2="13"/>
        <line x1="8" y1="17" x2="16" y2="17"/>
      </svg>
      <p>Select a snippet<br>to inspect it</p>
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
    font-family: var(--sans);
    font-size: 16px;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--ink);
    line-height: 1.2;
  }

  .insp-arrow {
    color: var(--arm-300);
    margin: 6px 0 5px 1px;
    line-height: 0;
    display: block;
  }

  .insp-expansion {
    font-size: 13px;
    color: var(--ink-strong);
    line-height: 1.6;
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 120px;
    overflow-y: auto;
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
    font-family: var(--sans);
    font-size: 20px;
    font-weight: 600;
    letter-spacing: -0.015em;
    color: var(--ink);
    line-height: 1;
  }

  .insp-stat-label {
    font-size: 11.5px;
    color: var(--ink-mute);
  }

  .insp-stat-date {
    font-size: 12.5px;
    color: var(--ink-soft);
    font-weight: 500;
    margin-left: auto;
  }

  .insp-actions {
    display: flex;
    gap: 8px;
  }

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
    font-family: var(--sans);
    font-style: normal;
    font-size: 13px;
    color: var(--ink-mute);
    margin: 0;
    line-height: 1.6;
  }

  .insp-instructions {
    margin-top: 14px;
    background: var(--paper);
    border: 1px dashed var(--line);
    border-radius: var(--r-sm);
    padding: 10px 12px;
  }

  .insp-instr-label {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 10.5px;
    font-weight: 500;
    color: var(--ink-mute);
    text-transform: none;
    letter-spacing: 0;
    margin-bottom: 6px;
  }

  .insp-instr-text {
    font-size: 12.5px;
    color: var(--ink-soft);
    line-height: 1.55;
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }

  @media (max-width: 1060px) {
    .inspector-col {
      position: static;
    }
  }
</style>
