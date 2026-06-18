<script lang="ts">
  import type { AppearanceMode } from '../../settings';
  import { appearanceModes } from '../setupData';
  import { appStore } from '../../stores';

  let { appearance = $bindable() }: { appearance: AppearanceMode } = $props();

  function pick(mode: AppearanceMode) {
    appearance = mode;
    appStore.appearanceMode = mode;
  }
</script>

<div class="step appearance-step">
  <div class="appearance-mode-grid">
    {#each appearanceModes as mode}
      <button
        class="appearance-mode-card"
        class:selected={appearance === mode.id}
        onclick={() => pick(mode.id)}
      >
        <div class="appearance-mode-title-row">
          <span class="appearance-mode-name">{mode.name}</span>
          <span class="appearance-mode-radio" class:checked={appearance === mode.id}></span>
        </div>
        <p class="appearance-mode-desc">{mode.desc}</p>
      </button>
    {/each}
  </div>
</div>

<style>
  .appearance-step { max-width: 640px; gap: 16px; }

  .appearance-mode-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 8px; }

  .appearance-mode-card {
    background: var(--bg-elev);
    border: 1.5px solid var(--line);
    border-radius: var(--r-md);
    padding: 12px;
    text-align: left;
    cursor: pointer;
    display: grid;
    gap: 6px;
    transition: border-color 0.15s, background 0.15s;
  }

  .appearance-mode-card:hover { border-color: var(--line-strong); }
  .appearance-mode-card.selected { border-color: var(--accent); background: var(--accent-soft); }

  .appearance-mode-title-row { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .appearance-mode-name { margin: 0; font-size: 13px; font-weight: 500; color: var(--ink-strong); }
  .appearance-mode-desc { margin: 0; font-size: 11.5px; color: var(--ink-mute); line-height: 1.35; }

  .appearance-mode-radio {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid var(--line-strong);
    position: relative;
    flex-shrink: 0;
  }

  .appearance-mode-radio.checked { border-color: var(--accent); }
  .appearance-mode-radio.checked::after {
    content: '';
    position: absolute;
    inset: 2px;
    border-radius: 50%;
    background: var(--accent);
  }

  @media (max-width: 960px) {
    .appearance-mode-grid { grid-template-columns: 1fr; }
  }
</style>
