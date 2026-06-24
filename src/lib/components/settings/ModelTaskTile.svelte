<script lang="ts">
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { cleanupPromptOverridesStore, openCleanupPromptEditor } from '../../stores.svelte';
  import { MOTION_MS, motionMs } from '../../motion';
  import type { ProviderId, ProviderModelMap } from '../../settings';
  import {
    modelId,
    providerSections,
    recommendedModels,
    splitModelId,
    taskLabel,
    type TaskType,
    type UiProviderId,
  } from './models';
  import ModelProviderGroup from './ModelProviderGroup.svelte';

  let {
    type,
    opened,
    advancedModelUi,
    apiKeyStatus,
    modelsByProvider,
    defaultModel,
    fallbackModels,
    customDrafts,
    onToggleOpen,
    onToggleModel,
    onRemoveCustomModel,
    onCustomDraftChange,
    onAddCustomModel,
  }: {
    type: TaskType;
    opened: boolean;
    advancedModelUi: boolean;
    apiKeyStatus: Record<ProviderId, boolean>;
    modelsByProvider: ProviderModelMap;
    defaultModel: string;
    fallbackModels: string[];
    customDrafts: Record<UiProviderId, string>;
    onToggleOpen: (type: TaskType) => void;
    onToggleModel: (type: TaskType, provider: ProviderId, modelName: string) => void;
    onRemoveCustomModel: (type: TaskType, provider: ProviderId, modelName: string) => void;
    onCustomDraftChange: (type: TaskType, provider: UiProviderId, value: string) => void;
    onAddCustomModel: (type: TaskType, provider: ProviderId, modelName: string) => void;
  } = $props();

  const fallbackCount = $derived(fallbackModels.length);

  function missingKeyWarning(): string {
    const missing = [defaultModel, ...fallbackModels]
      .map((id) => splitModelId(id)?.provider)
      .filter((provider): provider is ProviderId => !!provider)
      .filter((provider, index, all) => all.indexOf(provider) === index)
      .filter((provider) => !apiKeyStatus[provider]);

    return missing.length ? `Missing API keys for: ${missing.join(', ')}` : '';
  }

  function activeProviderLabel(): string {
    const parsed = splitModelId(defaultModel);
    return parsed ? parsed.provider.charAt(0).toUpperCase() + parsed.provider.slice(1) : 'None';
  }

  function activeModelLabel(): string {
    return splitModelId(defaultModel)?.model ?? 'None';
  }

  function currentCleanupModelFor(provider: ProviderId): string {
    const parsed = splitModelId(defaultModel);
    if (parsed && parsed.provider === provider) return parsed.model;
    return recommendedModels.cleanup[provider as UiProviderId].premium;
  }
</script>

<div class="task-tile" class:task-open={opened}>
  <button class="tile-head" onclick={() => onToggleOpen(type)} aria-expanded={opened}>
    <div class="head-left">
      <span class="head-title">{taskLabel(type)}</span>
      <div class="summary-row">
        <span class="summary-item provider-chip">{activeProviderLabel()}</span>
        <span class="summary-item model-chip">{activeModelLabel()}</span>
        {#if fallbackCount > 0}
          <span class="summary-item fallback-chip">{fallbackCount} fallback{fallbackCount !== 1 ? 's' : ''}</span>
        {/if}
      </div>
    </div>
    <span class="chevron" class:chevron-open={opened} aria-hidden="true"></span>
  </button>

  {#if opened}
    <div class="tile-inner" transition:slide={{ duration: motionMs(MOTION_MS.base), easing: cubicOut }}>
      {#if missingKeyWarning()}
        <div class="warn-banner">{missingKeyWarning()}</div>
      {/if}

      <div class="model-container">
        {#each providerSections as section (section.id)}
          <ModelProviderGroup
            {type}
            {section}
            {advancedModelUi}
            hasKey={apiKeyStatus[section.storeProvider]}
            models={modelsByProvider[section.storeProvider] ?? []}
            {defaultModel}
            {fallbackModels}
            customDraft={customDrafts[section.id]}
            {onToggleModel}
            {onRemoveCustomModel}
            {onCustomDraftChange}
            {onAddCustomModel}
          />
        {/each}
      </div>

      {#if type === 'cleanup' && advancedModelUi}
        <div class="prompt-editor-section" transition:slide={{ duration: motionMs(MOTION_MS.base), easing: cubicOut }}>
          {#each providerSections as section (section.id)}
            {@const model = currentCleanupModelFor(section.storeProvider)}
            {@const key = modelId(section.storeProvider, model)}
            {@const isCustomized = !!cleanupPromptOverridesStore.overrides[key]?.trim()}
            <div class="prompt-edit-row">
              <div class="prompt-edit-meta">
                <span class="prompt-editor-provider">{section.label}</span>
                <span class="prompt-editor-model">{model}</span>
              </div>
              <div class="prompt-edit-right">
                {#if isCustomized}
                  <span class="prompt-customized-badge">Customized</span>
                {/if}
                <button
                  class="prompt-edit-btn"
                  type="button"
                  onclick={(event) => openCleanupPromptEditor(section.storeProvider, model, (event.currentTarget as HTMLButtonElement).getBoundingClientRect())}
                >Edit prompt</button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  /* ── Task tile ───────────────────────────── */
  .task-tile {
    border-top: 1px solid var(--line);
  }

  /* ── Tile header ─────────────────────────── */
  .tile-head {
    width: 100%;
    border: none;
    outline: none;
    background: transparent;
    padding: 13px 10px 13px 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    cursor: pointer;
    text-align: left;
    transition: background 160ms ease;
    user-select: none;
  }

  .tile-head:hover {
    background: color-mix(in srgb, var(--paper) 30%, transparent);
  }

  .head-left {
    display: flex;
    flex-direction: column;
    gap: 7px;
    min-width: 0;
  }

  .head-title {
    font-family: var(--serif);
    font-size: 16px;
    font-weight: 500;
    color: var(--ink);
    line-height: 1;
  }

  .summary-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .summary-item {
    font-size: 10px;
    font-family: var(--sans);
    font-weight: 500;
    letter-spacing: 0.03em;
    border-radius: 999px;
    padding: 2px 7px;
    border: 1px solid var(--line-strong);
    color: var(--ink-soft);
    background: color-mix(in srgb, var(--paper) 50%, var(--bg-elev));
    white-space: nowrap;
  }

  .provider-chip {
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--accent-ink);
    border-color: color-mix(in srgb, var(--accent) 40%, var(--line-strong));
    background: color-mix(in srgb, var(--accent-soft) 65%, var(--bg-elev));
  }

  .model-chip {
    font-family: var(--mono);
    font-size: 10px;
  }

  .chevron {
    width: 8px;
    height: 8px;
    flex-shrink: 0;
    border-right: 2px solid var(--ink-mute);
    border-bottom: 2px solid var(--ink-mute);
    transform: rotate(45deg);
    transition: transform 220ms ease;
  }

  .chevron-open {
    transform: rotate(225deg);
  }

  /* ── Tile body ───────────────────────────── */
  .tile-inner {
    padding: 0 0 14px;
    display: flex;
    flex-direction: column;
  }

  .tile-inner:has(.prompt-editor-section) {
    padding-bottom: 0;
  }

  .tile-inner .warn-banner {
    margin: 10px 0 8px;
  }

  /* ── Model container ─────────────────────── */
  .model-container {
    border: 1px solid var(--line);
    border-radius: 10px;
    overflow: hidden;
  }

  /* Gradient divider between provider groups. Each .simple-group is a
     separate <ModelProviderGroup> instance, so the adjacent-sibling rule
     must live here on the scoped container with the inner part global. */
  .model-container :global(.simple-group + .simple-group::before) {
    content: '';
    position: absolute;
    top: 0;
    left: 5%;
    right: 5%;
    height: 1px;
    background: linear-gradient(
      to right,
      transparent,
      var(--line-strong) 30%,
      var(--line-strong) 70%,
      transparent
    );
  }

  /* ── Warning ─────────────────────────────── */
  .warn-banner {
    font-size: 12px;
    color: var(--danger);
    background: var(--danger-bg);
    border: 1px solid var(--danger-line);
    border-radius: 8px;
    padding: 8px 10px;
  }

  /* ── Prompt editor (compact rows) ───────────────────────── */
  .prompt-editor-section {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px 0;
  }

  .prompt-edit-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 7px 10px;
    border-radius: 8px;
    transition: background 0.14s;
  }

  .prompt-edit-row:hover {
    background: var(--control-hover);
  }

  .prompt-edit-meta {
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
  }

  .prompt-editor-provider {
    font-size: 12px;
    font-family: var(--sans);
    font-weight: 600;
    color: var(--ink);
  }

  .prompt-editor-model {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--ink-mute);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .prompt-edit-right {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .prompt-customized-badge {
    font-family: var(--sans);
    font-size: 10px;
    font-weight: 500;
    padding: 2px 7px;
    border-radius: 20px;
    background: var(--accent-soft);
    color: var(--accent-ink);
    white-space: nowrap;
  }

  .prompt-edit-btn {
    font-family: var(--sans);
    font-size: 11.5px;
    font-weight: 500;
    padding: 4px 11px;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    background: transparent;
    color: var(--ink-soft);
    cursor: pointer;
    transition: background 0.14s, color 0.14s;
    white-space: nowrap;
  }

  .prompt-edit-btn:hover {
    background: var(--bg-elev);
    color: var(--ink);
  }

  .prompt-edit-btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
</style>
