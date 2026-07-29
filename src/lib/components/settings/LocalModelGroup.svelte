<script lang="ts">
  import { getProviderLogo } from '../../setup/ProviderLogos';
  import type { ProviderId } from '../../settings';
  import { modelDisplayLabel, modelId, type TaskType } from './models';

  let {
    type,
    models,
    defaultModel,
    fallbackModels,
    emptyLabel,
    onToggleModel,
    onManageLocalModels,
  }: {
    type: TaskType;
    models: Array<{ id: string; is_downloaded?: boolean }>;
    defaultModel: string;
    fallbackModels: string[];
    emptyLabel: string;
    onToggleModel: (type: TaskType, provider: ProviderId, modelName: string) => void;
    onManageLocalModels: () => void;
  } = $props();

  const downloadedModels = $derived(models.filter((model) => model.is_downloaded !== false));

  function selectionState(modelIdValue: string): 'active' | 'fallback' | 'none' {
    const id = modelId('local', modelIdValue);
    if (defaultModel === id) return 'active';
    if (fallbackModels.includes(id)) return 'fallback';
    return 'none';
  }
</script>

<div class="simple-group">
  <span class="simple-provider">
    <span class="simple-provider-logo">{@html getProviderLogo('local')}</span>
    Local
  </span>

  {#if downloadedModels.length === 0}
    <div class="simple-row empty-row">
      <span class="simple-name empty-name">{emptyLabel}</span>
      <button type="button" class="state-pill muted" onclick={onManageLocalModels}>
        Manage
      </button>
    </div>
  {:else}
    {#each downloadedModels as model (model.id)}
      {@const state = selectionState(model.id)}
      {@const isActive = state === 'active'}
      {@const isFallback = state === 'fallback'}
      <div
        class="simple-row model-row"
        class:simple-active={isActive}
        class:simple-fallback={isFallback}
        class:chain-row={isFallback}
        role="button"
        tabindex="0"
        onclick={() => onToggleModel(type, 'local', model.id)}
        onkeydown={(event) => {
          if (event.target === event.currentTarget && (event.key === 'Enter' || event.key === ' ')) {
            if (event.key === ' ') event.preventDefault();
            onToggleModel(type, 'local', model.id);
          }
        }}
      >
        <span class="simple-dot" class:dot-active={isActive} class:dot-fallback={isFallback}></span>
        <span class="simple-name model-name">{modelDisplayLabel('local', model.id)}</span>
        {#if isActive}
          <span class="state-pill active">Active</span>
        {:else if isFallback}
          <span class="state-pill fallback pill-action" title="Remove fallback">
            F{fallbackModels.indexOf(modelId('local', model.id)) + 1}
          </span>
        {:else}
          <span class="state-pill muted pill-action">Add fallback</span>
        {/if}
      </div>
    {/each}
  {/if}
</div>

<style>
  /* ── Provider group (copied from ModelProviderGroup.svelte for visual
     consistency — Svelte scoped styles aren't shared across components in
     this codebase's existing convention) ───────────────────────────────── */
  .simple-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px 6px;
    position: relative;
  }

  .simple-provider {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.09em;
    font-family: var(--sans);
    font-weight: 700;
    color: var(--ink-faint);
    padding: 0 10px;
    margin-bottom: 3px;
  }

  .simple-provider-logo { display: inline-flex; width: 12px; height: 12px; flex-shrink: 0; }
  .simple-provider-logo :global(svg) { width: 100%; height: 100%; }

  .simple-row {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 8px 10px;
    border-radius: 7px;
    border: 1px solid transparent;
    background: transparent;
    cursor: pointer;
    text-align: left;
    width: 100%;
    transition: background 380ms ease, border-color 300ms ease;
  }

  .simple-row:hover:not(.simple-active) {
    background: color-mix(in srgb, var(--paper) 70%, var(--bg-elev));
  }

  .simple-row.simple-active {
    background: color-mix(in srgb, var(--accent-soft) 60%, var(--bg-elev));
    border-color: color-mix(in srgb, var(--accent) 22%, var(--line));
  }

  .simple-row.simple-fallback {
    background: color-mix(in srgb, var(--accent-soft) 28%, var(--bg-elev));
    border-color: color-mix(in srgb, var(--accent) 14%, var(--line));
  }

  .simple-row.empty-row {
    cursor: default;
  }

  .simple-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    border: 1.5px solid var(--ink-faint);
    flex-shrink: 0;
    transition: background 300ms ease, border-color 280ms ease, box-shadow 420ms ease;
  }

  .simple-dot.dot-active {
    background: var(--accent);
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent-soft) 80%, transparent);
  }

  .simple-dot.dot-fallback {
    background: color-mix(in srgb, var(--accent) 45%, transparent);
    border-color: color-mix(in srgb, var(--accent) 60%, var(--line-strong));
  }

  .simple-name {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--ink-soft);
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .simple-active .simple-name {
    color: var(--accent-ink);
    font-weight: 500;
  }

  .empty-name {
    font-family: var(--sans);
    color: var(--ink-mute);
  }

  .state-pill {
    font-size: 9px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    border-radius: 999px;
    padding: 2px 7px;
    border: 1px solid var(--line-strong);
    flex-shrink: 0;
    white-space: nowrap;
    font-family: var(--sans);
    font-weight: 600;
    transition: color 160ms ease, background 160ms ease, border-color 160ms ease;
  }

  .state-pill.active {
    color: var(--accent-ink);
    border-color: color-mix(in srgb, var(--accent) 45%, var(--line-strong));
    background: color-mix(in srgb, var(--accent-soft) 75%, var(--bg-elev));
  }

  .state-pill.fallback {
    color: color-mix(in srgb, var(--accent-ink) 65%, var(--ink-mute));
    border-color: color-mix(in srgb, var(--accent) 28%, var(--line-strong));
    background: color-mix(in srgb, var(--accent-soft) 38%, var(--bg-elev));
  }

  .state-pill.muted {
    color: var(--ink-mute);
    background: transparent;
    border-color: var(--line);
    cursor: pointer;
  }

  .state-pill.muted:hover {
    color: var(--ink-soft);
    border-color: var(--line-strong);
    background: color-mix(in srgb, var(--paper) 50%, var(--bg-elev));
  }

  .pill-action {
    appearance: none;
    cursor: pointer;
  }

  .simple-row:hover .state-pill.muted {
    color: var(--ink-soft);
    border-color: var(--line-strong);
    background: color-mix(in srgb, var(--paper) 50%, var(--bg-elev));
  }
</style>
