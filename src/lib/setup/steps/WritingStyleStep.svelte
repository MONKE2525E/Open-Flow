<script lang="ts">
  import type { CleanupIntensity, ToneId } from '../../settings';
  import { cleanupCards, toneCards, writingStylePreview } from '../setupData';

  let {
    intensity = $bindable(),
    tone = $bindable(),
  }: { intensity: CleanupIntensity; tone: ToneId } = $props();

  let preview = $derived(writingStylePreview(intensity, tone));
</script>

<div class="step writing-style-step">
  <div class="style-group">
    <p class="style-group-label">Cleanup intensity</p>
    <div class="option-cards">
      {#each cleanupCards as c}
        <button
          class="option-card"
          class:selected={intensity === c.id}
          onclick={() => { intensity = c.id; }}
        >
          <div class="option-card-top">
            <span class="option-name">{c.name}</span>
            <div class="option-radio" class:checked={intensity === c.id}></div>
          </div>
          <p class="option-desc">{c.desc}</p>
        </button>
      {/each}
    </div>
  </div>

  <div class="style-group">
    <p class="style-group-label">Tone</p>
    <div class="tone-grid">
      {#each toneCards as t}
        <button
          class="tone-card"
          class:selected={tone === t.id}
          onclick={() => { tone = t.id; }}
        >
          <div class="tone-name">{t.name}</div>
          <p class="tone-desc">{t.desc}</p>
          <div class="tone-check" class:visible={tone === t.id}>
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
          </div>
        </button>
      {/each}
    </div>
  </div>

  <div class="preview-box">
    <span class="preview-label">Preview</span>
    <div class="preview-row">
      <span class="preview-before">"{preview.before}"</span>
      <span class="preview-arrow" aria-hidden="true">→</span>
      <span class="preview-after">"{preview.after}"</span>
    </div>
  </div>
</div>

<style>
  .writing-style-step { gap: 16px; }

  .style-group { display: flex; flex-direction: column; gap: 8px; }

  .style-group-label {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--ink-faint);
    margin: 0;
  }

  .option-cards { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 8px; }

  .option-card {
    background: var(--bg-elev);
    border: 1.5px solid var(--line);
    border-radius: var(--r-sm);
    padding: 10px 11px;
    text-align: left;
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .option-card:hover { border-color: var(--line-strong); }
  .option-card.selected { border-color: var(--accent); background: var(--accent-soft); }

  .option-card-top { display: flex; justify-content: space-between; align-items: center; gap: 4px; }
  .option-name { font-size: 12.5px; font-weight: 500; color: var(--ink-strong); }
  .option-desc { font-size: 11px; color: var(--ink-mute); margin: 0; line-height: 1.3; }

  .option-radio {
    width: 13px;
    height: 13px;
    border-radius: 50%;
    border: 2px solid var(--line-strong);
    flex-shrink: 0;
    position: relative;
    transition: border-color 0.15s;
  }

  .option-radio.checked { border-color: var(--accent); }
  .option-radio.checked::after {
    content: '';
    position: absolute;
    inset: 2px;
    border-radius: 50%;
    background: var(--accent);
  }

  .tone-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; }

  .tone-card {
    background: var(--bg-elev);
    border: 1.5px solid var(--line);
    border-radius: var(--r-sm);
    padding: 10px 11px 9px;
    text-align: left;
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .tone-card:hover { border-color: var(--line-strong); }
  .tone-card.selected { border-color: var(--accent); background: var(--accent-soft); }

  .tone-name { font-size: 12.5px; font-weight: 500; color: var(--ink-strong); }
  .tone-desc { font-size: 11px; color: var(--ink-mute); margin: 0; line-height: 1.3; }

  .tone-check {
    position: absolute;
    top: 7px;
    right: 7px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--on-accent);
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    transform: scale(0.6);
    transition: opacity 0.15s, transform 0.15s;
  }

  .tone-check.visible { opacity: 1; transform: scale(1); }

  .preview-box {
    background: var(--paper-2);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    padding: 10px 14px;
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .preview-label {
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--ink-faint);
    flex-shrink: 0;
  }

  .preview-row { display: flex; align-items: baseline; gap: 8px; flex-wrap: wrap; min-width: 0; }

  .preview-before {
    font-size: 12px;
    font-style: italic;
    color: var(--ink-mute);
    line-height: 1.4;
  }

  .preview-arrow { font-size: 12px; color: var(--ink-faint); flex-shrink: 0; }

  .preview-after {
    font-size: 12.5px;
    font-weight: 500;
    color: var(--accent-ink);
    line-height: 1.4;
  }
</style>
