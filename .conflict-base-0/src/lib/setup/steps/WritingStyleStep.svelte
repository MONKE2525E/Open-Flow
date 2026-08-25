<script lang="ts">
  import type { CleanupIntensity, ToneId } from '../../settings';
  import { fade } from 'svelte/transition';
  import { motionMs } from '../../motion';
  import { cleanupCards, toneCards, writingStylePreview } from '../setupData';

  let {
    intensity = $bindable(),
    tone = $bindable(),
  }: { intensity: CleanupIntensity; tone: ToneId } = $props();

  // Tone is a cleanup-LLM instruction, so with cleanup off it has nothing to act on.
  // Mirrors how Style.svelte inerts the whole page when cleanup_enabled is false.
  const cleanupOff = $derived(intensity === 'none');
  const preview = $derived(writingStylePreview(intensity, tone));
</script>

<div class="step writing-style-step">
  <div class="style-group">
    <p class="group-label">Cleanup intensity</p>
    <div class="style-grid">
      {#each cleanupCards as c}
        <button
          class="pick-card style-card"
          class:selected={intensity === c.id}
          aria-pressed={intensity === c.id}
          onclick={() => { intensity = c.id; }}
        >
          <div class="card-top">
            <span class="card-name">{c.name}</span>
            <div class="pick-radio" class:checked={intensity === c.id}></div>
          </div>
          <p class="card-desc">{c.desc}</p>
        </button>
      {/each}
    </div>
  </div>

  <div class="style-group" class:disabled={cleanupOff}>
    <div class="group-head">
      <p class="group-label">Tone</p>
      {#if cleanupOff}
        <span class="group-note">Only applies when cleanup runs</span>
      {/if}
    </div>
    <div class="style-grid tone-grid" inert={cleanupOff}>
      {#each toneCards as t}
        <button
          class="pick-card style-card"
          class:selected={tone === t.id && !cleanupOff}
          aria-pressed={tone === t.id && !cleanupOff}
          onclick={() => { tone = t.id; }}
        >
          <div class="card-top">
            <span class="card-name">{t.name}</span>
            <div class="pick-radio" class:checked={tone === t.id && !cleanupOff}></div>
          </div>
          <p class="card-desc">{t.desc}</p>
        </button>
      {/each}
    </div>
  </div>

  <div class="preview-box">
    <!-- Stacked in one grid cell so the outgoing and incoming previews
         cross-fade in place instead of the box snapping to a new height. -->
    <div class="preview-stack">
      {#key cleanupOff ? 'off' : `${intensity}-${tone}`}
        <div class="preview-slot" in:fade={{ duration: motionMs(180), delay: motionMs(70) }} out:fade={{ duration: motionMs(70) }}>
          {#if cleanupOff}
            <p class="preview-off">Cleanup is off — whatever you say is injected exactly as transcribed.</p>
          {:else}
            <div class="preview-row">
              <span class="preview-sample"><span class="preview-label">Before</span><span class="preview-before">"{preview.before}"</span></span>
              <span class="preview-arrow" aria-hidden="true">→</span>
              <span class="preview-sample"><span class="preview-label">After</span><span class="preview-after">"{preview.after}"</span></span>
            </div>
          {/if}
        </div>
      {/key}
    </div>
  </div>
</div>

<style>
  .writing-style-step { gap: 16px; }

  .style-group { display: flex; flex-direction: column; gap: 8px; }

  /* Fading rather than snapping when cleanup is switched off. */
  .style-group { transition: opacity 0.22s ease; }
  .style-group.disabled { opacity: 0.4; }

  .group-head { display: flex; align-items: baseline; justify-content: space-between; gap: 10px; }

  .group-note { font-size: 11px; color: var(--ink-faint); font-style: italic; }

  /* Both groups share one 12-column track so a 4-up row and a 3-up row line up
     on the same left and right edges — previously they were separate grids at
     different widths, which is what made the buttons look mismatched. */
  .style-grid {
    display: grid;
    grid-template-columns: repeat(12, minmax(0, 1fr));
    gap: 8px;
  }

  .style-grid > :global(*) { grid-column: span 3; }
  .tone-grid > :global(*) { grid-column: span 4; }

  .style-card { min-height: 72px; justify-content: flex-start; }

  .card-top { display: flex; justify-content: space-between; align-items: center; gap: 6px; }
  .card-name { font-size: 12.5px; font-weight: 500; color: var(--ink-strong); }
  .card-desc { font-size: 11px; color: var(--ink-mute); margin: 0; line-height: 1.35; }

  .preview-box {
    background: var(--paper-2);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    padding: 11px 14px;
    display: flex;
    align-items: stretch;
    /* Tallest state (two wrapped lines) reserved up front, so switching
       intensity cross-fades the text without resizing the box. */
    min-height: 62px;
    box-sizing: border-box;
  }

  .preview-stack { flex: 1; min-width: 0; display: grid; }
  .preview-stack > :global(*) { grid-column: 1; grid-row: 1; align-self: center; }

  .preview-row { display: grid; grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr); align-items: center; gap: 12px; min-width: 0; }

  .preview-sample { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .preview-label { font-size: 9.5px; font-weight: 650; text-transform: uppercase; letter-spacing: 0.08em; color: var(--ink-faint); }

  .preview-before { font-size: 12px; font-style: italic; color: var(--ink-mute); line-height: 1.4; }

  .preview-arrow { font-size: 12px; color: var(--ink-faint); flex-shrink: 0; }

  .preview-after { font-size: 12.5px; font-weight: 500; color: var(--accent-ink); line-height: 1.4; }

  .preview-off { margin: 0; font-size: 12.5px; color: var(--ink-mute); line-height: 1.4; }

  @media (max-width: 720px) {
    .style-grid > :global(*) { grid-column: span 6; }
    .tone-grid > :global(*) { grid-column: span 6; }
    .preview-row { grid-template-columns: 1fr; gap: 5px; }
    .preview-arrow { display: none; }
  }
</style>
