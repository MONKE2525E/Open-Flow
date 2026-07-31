<script lang="ts">
  import EfficiencyBar from './EfficiencyBar.svelte';
  import type { Preset } from './modelPresets';

  let {
    preset,
    active = false,
    downloadMb = 0,
    downloading = false,
    installedCount = 0,
    onSelect,
    onAddKey,
    onCancelDownload,
    onDeleteModels,
  }: {
    preset: Preset;
    active?: boolean;
    /** Total MB of required local models not yet on disk (0 = ready to use). */
    downloadMb?: number;
    downloading?: boolean;
    /** How many of this preset's local models are already downloaded. */
    installedCount?: number;
    onSelect: () => void;
    onAddKey?: () => void;
    onCancelDownload?: () => void;
    onDeleteModels?: () => void;
  } = $props();

  const isAddKey = $derived(preset.kind === 'add-key');
  const needsDownload = $derived(downloadMb > 0);

  function formatSize(mb: number): string {
    return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${mb} MB`;
  }

  // The action pill can roll to a destructive action on hover: cancel an
  // in-flight download, or delete a selected offline preset's downloaded models.
  // You can't deselect a preset, so the "selected" pill has no idle purpose.
  const cancelable = $derived(downloading);
  const manageable = $derived(preset.offline && active && installedCount > 0);
  const hoverMode = $derived(cancelable || manageable);

  const defaultLabel = $derived(
    downloading ? 'Downloading…' : needsDownload ? `Download ${formatSize(downloadMb)}` : active ? 'Selected' : 'Use',
  );
  const hoverLabel = $derived(cancelable ? 'Cancel download' : manageable ? 'Delete models' : '');

  function handleAction(event: MouseEvent) {
    event.stopPropagation();
    if (cancelable) onCancelDownload?.();
    else if (manageable) {
      if (globalThis.confirm(`Delete downloaded models for ${preset.name}?`)) {
        onDeleteModels?.();
      }
    }
    else onSelect();
  }
</script>

{#if isAddKey}
  <div class="preset-card preset-info">
    <div class="preset-main">
      <div class="preset-head">
        <span class="preset-name">{preset.name}</span>
      </div>
      <p class="preset-tagline">{preset.tagline}</p>
    </div>
    <div class="preset-side">
      <button class="preset-action-btn" type="button" onclick={() => onAddKey?.()}>Open API keys</button>
    </div>
  </div>
{:else}
  <div class="preset-card" class:preset-active={active}>
    <!-- Full-card select target sitting behind the content; the content is
         click-through except the action pill, so a click anywhere selects. -->
    <button
      class="preset-select"
      type="button"
      aria-label={`Use ${preset.name}`}
      aria-pressed={active}
      disabled={downloading}
      onclick={() => onSelect()}
    ></button>
    <div class="preset-content">
      <div class="preset-main">
        <div class="preset-head">
          <span class="preset-name">{preset.name}</span>
          {#if preset.offline}
            <span class="preset-offline">Local AI</span>
          {/if}
        </div>
        <p class="preset-tagline">{preset.tagline}</p>
      </div>
      <div class="preset-side">
        <EfficiencyBar position={preset.position} />
        <button
          class="preset-action-btn"
          class:is-active={active}
          class:is-download={needsDownload}
          class:hover-mode={hoverMode}
          type="button"
          tabindex={hoverMode || needsDownload ? 0 : -1}
          aria-label={hoverMode ? `${defaultLabel} - ${hoverLabel}` : defaultLabel}
          onclick={handleAction}
        >
          {#if hoverMode}
            <span class="pa-roll">
              <span class="pa-face pa-default" aria-hidden="true">{defaultLabel}</span>
              <span class="pa-face pa-hover" aria-hidden="true">{hoverLabel}</span>
            </span>
          {:else}
            {defaultLabel}
          {/if}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .preset-card {
    position: relative;
    display: flex;
    flex-direction: row;
    align-items: center;
    width: 100%;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--bg-elev);
    color: var(--ink);
    transition: border-color 180ms ease, box-shadow 180ms ease, background 180ms ease;
  }

  .preset-info {
    gap: 20px;
    padding: 14px 16px;
  }

  .preset-card:hover:not(.preset-active) {
    border-color: var(--line-strong);
    box-shadow: var(--shadow-card, 0 2px 8px rgba(0, 0, 0, 0.06));
  }

  .preset-card.preset-active {
    border-color: var(--jap-400);
    background: color-mix(in srgb, var(--jap-100) 40%, var(--bg-elev));
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--jap-400) 35%, transparent);
  }

  /* Select target: covers the whole card, sits behind the content. */
  .preset-select {
    position: absolute;
    inset: 0;
    z-index: 1;
    border: none;
    background: transparent;
    border-radius: 12px;
    cursor: pointer;
  }
  .preset-select:disabled { cursor: default; }
  .preset-select:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  /* Content layer: click-through so the select target underneath gets the
     click; only the action pill re-enables pointer events. */
  .preset-content {
    position: relative;
    z-index: 2;
    pointer-events: none;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 20px;
    width: 100%;
    padding: 14px 16px;
  }

  .preset-main {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }

  .preset-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .preset-name {
    font-family: var(--serif);
    font-size: 16px;
    font-weight: 500;
    color: var(--ink);
    line-height: 1;
  }

  /* Plain text, not a pill. */
  .preset-offline {
    font-family: var(--sans);
    font-size: 11.5px;
    font-weight: 450;
    color: var(--ink-mute);
  }

  .preset-offline::before {
    content: '·';
    margin-right: 8px;
    color: var(--ink-faint);
  }

  .preset-tagline {
    margin: 0;
    font-family: var(--sans);
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--ink-mute);
  }

  .preset-side {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 10px;
    width: 200px;
    flex-shrink: 0;
  }

  /* ── Action pill ─────────────────────────── */
  .preset-action-btn {
    pointer-events: auto;
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    min-height: 30px;
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 600;
    padding: 6px 12px;
    border-radius: 7px;
    border: 1px solid var(--line-strong);
    background: transparent;
    color: var(--ink-soft);
    cursor: pointer;
    transition:
      border-color 280ms cubic-bezier(0.16, 1, 0.3, 1),
      color 280ms cubic-bezier(0.16, 1, 0.3, 1),
      background 280ms cubic-bezier(0.16, 1, 0.3, 1),
      box-shadow 280ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  .preset-action-btn:hover:not(.hover-mode) {
    background: var(--jap-100);
    color: var(--jap-700);
    border-color: var(--jap-400);
  }

  .preset-action-btn.is-active {
    border-color: var(--jap-400);
    color: var(--jap-700);
    background: var(--jap-100);
  }

  .preset-action-btn.is-download {
    border-color: var(--jap-400);
    color: var(--jap-700);
  }

  /* Hover/focus on a manageable pill: turn red for the destructive action. */
  .preset-action-btn.hover-mode:hover,
  .preset-action-btn.hover-mode:focus-visible {
    border-color: var(--danger);
    color: var(--danger);
    background: var(--danger-bg, color-mix(in srgb, var(--danger) 12%, transparent));
    box-shadow: 0 2px 8px color-mix(in srgb, var(--danger) 14%, transparent);
  }

  .preset-action-btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .preset-action-btn.hover-mode:focus-visible {
    outline-color: var(--danger);
  }

  /* Full width + centered text keeps the longer destructive label from being
     clipped while the two faces trade places. */
  .pa-roll {
    position: relative;
    display: block;
    width: 100%;
    overflow: hidden;
    height: 1.35em;
    line-height: 1.35em;
    perspective: 240px;
  }

  .pa-face {
    display: block;
    width: 100%;
    text-align: center;
    height: 1.35em;
    line-height: 1.35em;
    white-space: nowrap;
    backface-visibility: hidden;
    transition:
      transform 360ms cubic-bezier(0.16, 1, 0.3, 1),
      opacity 220ms ease;
  }

  .pa-hover {
    position: absolute;
    inset: 0;
    transform: translateY(100%);
    opacity: 0;
  }

  .hover-mode:hover .pa-default,
  .hover-mode:focus-visible .pa-default {
    transform: translateY(-112%) rotateX(-8deg) scale(0.96);
    opacity: 0;
  }
  .hover-mode:hover .pa-hover,
  .hover-mode:focus-visible .pa-hover {
    transform: translateY(0) rotateX(0) scale(1);
    opacity: 1;
  }

  @media (prefers-reduced-motion: reduce) {
    .pa-face {
      transition: opacity 100ms ease;
      transform: none;
    }

    .pa-hover { opacity: 0; }

    .hover-mode:hover .pa-default,
    .hover-mode:focus-visible .pa-default {
      transform: none;
    }

    .hover-mode:hover .pa-hover,
    .hover-mode:focus-visible .pa-hover {
      transform: none;
    }
  }

  /* Narrow settings column: fold the right rail under the text. */
  @container settings-panel (max-width: 560px) {
    .preset-content {
      flex-direction: column;
      align-items: stretch;
      gap: 12px;
    }

    .preset-info {
      flex-direction: column;
      align-items: stretch;
      gap: 12px;
    }

    .preset-side {
      width: 100%;
    }
  }
</style>
