<script lang="ts">
  import { fly, slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { MOTION_MS, MOTION_PX, motionMs, motionPx } from '../../motion';
  import { getProviderLogo } from '../../setup/ProviderLogos';
  import type { ProviderId } from '../../settings';
  import {
    modelId,
    recommendedModels,
    type ProviderSection,
    type TaskType,
    type UiProviderId,
  } from './models';

  function pillScale(
    node: Element,
    { duration = motionMs(MOTION_MS.fast) }: { duration?: number } = {},
  ) {
    const width = (node as HTMLElement).getBoundingClientRect().width;
    return {
      duration,
      easing: cubicOut,
      css: (t: number) => `max-width: ${width * t}px; opacity: ${t}; overflow: hidden;`,
    };
  }

  let {
    type,
    section,
    advancedModelUi,
    hasKey,
    models,
    defaultModel,
    fallbackModels,
    customDraft,
    onToggleModel,
    onRemoveCustomModel,
    onCustomDraftChange,
    onAddCustomModel,
  }: {
    type: TaskType;
    section: ProviderSection;
    advancedModelUi: boolean;
    hasKey: boolean;
    models: string[];
    defaultModel: string;
    fallbackModels: string[];
    customDraft: string;
    onToggleModel: (type: TaskType, provider: ProviderId, modelName: string) => void;
    onRemoveCustomModel: (type: TaskType, provider: ProviderId, modelName: string) => void;
    onCustomDraftChange: (type: TaskType, provider: UiProviderId, value: string) => void;
    onAddCustomModel: (type: TaskType, provider: ProviderId, modelName: string) => void;
  } = $props();

  // Safe: a section is only ever rendered for a task it declares support for
  // (filtered by `tasks` in ModelTaskTile), so its recommendedModels entry always exists.
  const tiers = $derived(recommendedModels[type][section.id]!);

  const customModels = $derived(
    models.filter((model) => model !== tiers.premium && model !== tiers.standard),
  );

  function selectionState(model: string): 'active' | 'fallback' | 'none' {
    const id = modelId(section.storeProvider, model);
    if (defaultModel === id) return 'active';
    if (fallbackModels.includes(id)) return 'fallback';
    return 'none';
  }
</script>

<div class="simple-group" class:no-key={!hasKey}>
  <span class="simple-provider">
    <span class="simple-provider-logo">{@html getProviderLogo(section.storeProvider)}</span>
    {section.label}
  </span>

  {#each ['premium', 'standard'] as rawTier}
    {@const tier = rawTier as 'premium' | 'standard'}
    {@const modelName = tiers[tier]}
    {@const state = selectionState(modelName)}
    {@const isActive = state === 'active'}
    {@const isFallback = state === 'fallback'}
    <div
      class="simple-row model-row"
      class:simple-active={isActive}
      class:simple-fallback={isFallback}
      class:chain-row={isFallback}
      role="button"
      tabindex="0"
      onclick={() => onToggleModel(type, section.storeProvider, modelName)}
      onkeydown={(event) => {
        if (event.target === event.currentTarget && (event.key === 'Enter' || event.key === ' ')) {
          if (event.key === ' ') event.preventDefault();
          onToggleModel(type, section.storeProvider, modelName);
        }
      }}
    >
      <span class="simple-dot" class:dot-active={isActive} class:dot-fallback={isFallback}></span>
      <span class="simple-name model-name">{modelName}</span>
      {#if isActive}
        <span transition:pillScale class="state-pill active">Active</span>
      {:else if isFallback}
        <span transition:pillScale class="state-pill fallback">F{fallbackModels.indexOf(modelId(section.storeProvider, modelName)) + 1}</span>
      {:else}
        <span transition:pillScale class="state-pill tier-pill">
          <span class="tier-label">{tier === 'premium' ? 'Accurate' : 'Efficient'}</span>
          <span class="fallback-label" aria-hidden="true">Add fallback</span>
        </span>
      {/if}
    </div>
  {/each}

  {#each customModels.filter((model) => advancedModelUi || selectionState(model) !== 'none') as custom (custom)}
    {@const state = selectionState(custom)}
    {@const isActive = state === 'active'}
    {@const isFallback = state === 'fallback'}
    <div
      in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.base), easing: cubicOut }}
      out:fly={{ y: -motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.fast), easing: cubicOut }}
      class="simple-row model-row"
      class:simple-active={isActive}
      class:simple-fallback={isFallback}
      class:chain-row={isFallback}
    >
      <button
        class="remove-dot"
        class:dot-active={isActive}
        class:dot-fallback={isFallback}
        type="button"
        aria-label="Remove {custom}"
        onclick={(event) => {
          event.stopPropagation();
          onRemoveCustomModel(type, section.storeProvider, custom);
        }}
      ></button>
      <button
        type="button"
        class="simple-name model-name custom-toggle-btn"
        onclick={(event) => {
          event.stopPropagation();
          onToggleModel(type, section.storeProvider, custom);
        }}
        aria-label="Toggle selection for {custom}"
      >
        {custom}
      </button>
      {#if isActive}
        <span transition:pillScale class="state-pill active">Active</span>
      {:else if isFallback}
        <span transition:pillScale class="state-pill fallback">F{fallbackModels.indexOf(modelId(section.storeProvider, custom)) + 1}</span>
      {:else}
        <button
          type="button"
          class="state-pill muted add-fallback-btn"
          onclick={(event) => {
            event.stopPropagation();
            onToggleModel(type, section.storeProvider, custom);
          }}
        >
          Add fallback
        </button>
      {/if}
    </div>
  {/each}

  {#if advancedModelUi}
    <div class="custom-row" transition:slide={{ duration: motionMs(240), easing: cubicOut }}>
      <input
        class="model-input"
        placeholder="custom model..."
        value={customDraft}
        oninput={(event) => onCustomDraftChange(type, section.id, (event.currentTarget as HTMLInputElement).value)}
        onkeydown={(event) => {
          if (event.key === 'Enter') {
            event.stopPropagation();
            onAddCustomModel(type, section.storeProvider, customDraft);
          }
        }}
      />
      <button
        class="custom-add-btn"
        onclick={() => onAddCustomModel(type, section.storeProvider, customDraft)}
        disabled={!customDraft.trim()}
      >Add</button>
    </div>
  {/if}
</div>

<style>
  /* ── Provider groups ─────────────────────── */
  .simple-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px 6px;
    transition: opacity 200ms ease;
    position: relative;
  }

  .simple-group.no-key { opacity: 0.45; }

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

  /* ── Remove dot (custom models only) ────── */
  .remove-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    border: 1.5px solid var(--ink-faint);
    flex-shrink: 0;
    cursor: pointer;
    padding: 0;
    background: transparent;
    position: relative;
    transition: border-color 280ms ease, background 300ms ease, box-shadow 420ms ease;
  }

  .remove-dot.dot-active {
    background: var(--accent);
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent-soft) 80%, transparent);
  }

  .remove-dot.dot-fallback {
    background: color-mix(in srgb, var(--accent) 45%, transparent);
    border-color: color-mix(in srgb, var(--accent) 60%, var(--line-strong));
  }

  .remove-dot::before,
  .remove-dot::after {
    content: '';
    position: absolute;
    width: 5px;
    height: 1.5px;
    background: transparent;
    top: 50%;
    left: 50%;
    border-radius: 1px;
    transition: background 140ms ease;
  }

  .remove-dot::before { transform: translate(-50%, -50%) rotate(45deg); }
  .remove-dot::after  { transform: translate(-50%, -50%) rotate(-45deg); }

  .remove-dot:hover {
    background: color-mix(in srgb, var(--danger) 14%, var(--bg-elev));
    border-color: var(--danger);
    box-shadow: none;
  }

  .remove-dot:hover::before,
  .remove-dot:hover::after {
    background: var(--danger);
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

  .custom-toggle-btn {
    appearance: none;
    background: transparent;
    border: none;
    padding: 0;
    cursor: pointer;
    outline: none;
    text-align: left;
  }

  .custom-toggle-btn:focus-visible {
    outline: 2px solid var(--accent, #d97757);
    outline-offset: 2px;
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
    transform-origin: center;
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

  .simple-row:hover .state-pill.muted {
    color: var(--ink-soft);
    border-color: var(--line-strong);
    background: color-mix(in srgb, var(--paper) 50%, var(--bg-elev));
  }

  .add-fallback-btn {
    appearance: none;
    background: transparent;
    border: 1px solid var(--line);
    padding: 2px 7px;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }

  .add-fallback-btn:focus-visible {
    outline: 2px solid var(--accent, #d97757);
    outline-offset: 2px;
  }

  .tier-pill {
    display: grid;
    place-items: center;
    cursor: pointer;
    color: var(--ink-faint);
    background: transparent;
    border-color: var(--line);
    overflow: hidden;
    transition: color 160ms ease, border-color 160ms ease, background 160ms ease;
  }

  .simple-row:hover .tier-pill {
    color: var(--ink-soft);
    border-color: var(--line-strong);
    background: color-mix(in srgb, var(--paper) 50%, var(--bg-elev));
  }

  .tier-label,
  .fallback-label {
    grid-area: 1 / 1;
    white-space: nowrap;
    transition: opacity 180ms ease, transform 180ms ease;
  }

  .fallback-label {
    opacity: 0;
    transform: translateY(4px);
  }

  .simple-row:hover .tier-label {
    opacity: 0;
    transform: translateY(-4px);
  }

  .simple-row:hover .fallback-label {
    opacity: 1;
    transform: translateY(0);
  }

  .custom-row {
    display: flex;
    gap: 5px;
    padding: 6px 8px 2px;
    margin-top: 4px;
    border-top: 1px solid var(--line);
  }

  .model-input {
    flex: 1;
    min-width: 0;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    padding: 5px 8px;
    background: var(--bg-elev);
    color: var(--ink);
    font-size: 11px;
    font-family: var(--mono);
    transition: border-color 150ms ease, box-shadow 150ms ease;
  }

  .model-input:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent-soft) 60%, transparent);
  }

  .model-input::placeholder {
    color: var(--ink-mute);
    font-family: var(--sans);
  }

  .custom-add-btn {
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    background: var(--bg-elev);
    color: var(--ink-strong);
    font-size: 11px;
    font-family: var(--sans);
    font-weight: 500;
    padding: 4px 9px;
    cursor: pointer;
    flex-shrink: 0;
    transition: background 140ms ease, border-color 140ms ease;
  }

  .custom-add-btn:hover:not(:disabled) {
    background: color-mix(in srgb, var(--paper) 40%, var(--bg-elev));
    border-color: var(--ink-mute);
  }

  .custom-add-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
