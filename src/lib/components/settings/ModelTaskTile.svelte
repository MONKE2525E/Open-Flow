<script lang="ts">
  import { openCleanupPromptEditor, cleanupPromptStore } from '../../stores.svelte';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { MOTION_MS, motionMs } from '../../motion';
  import { getProviderLogo, getProviderPlate } from '../../setup/ProviderLogos';
  import type { ProviderId } from '../../settings';
  import {
    providerDisplayLabel,
    qualifiedModelLabel,
    splitModelId,
    taskLabel,
    type TaskType,
  } from './models';
  import {
    rowForSelection,
    unavailableMessages,
    type LocalControls,
    type PickerContext,
  } from './modelStates';
  import ModelPickerModal from './ModelPickerModal.svelte';

  let {
    type,
    advancedModelUi,
    apiKeyStatus,
    context,
    defaultModel,
    fallbackModels,
    customDraft,
    onSelectModel,
    onAddFallback,
    onRemoveFallback,
    onMoveFallback,
    onCustomDraftChange,
    onAddCustomModel,
    onOpenApiKeys,
    local,
  }: {
    type: TaskType;
    advancedModelUi: boolean;
    apiKeyStatus: Record<ProviderId, boolean>;
    context: PickerContext;
    defaultModel: string;
    fallbackModels: string[];
    customDraft: string;
    onSelectModel: (type: TaskType, id: string) => void;
    onAddFallback: (type: TaskType, id: string) => void;
    onRemoveFallback: (type: TaskType, id: string) => void;
    onMoveFallback: (type: TaskType, id: string, delta: -1 | 1) => void;
    onCustomDraftChange: (type: TaskType, value: string) => void;
    onAddCustomModel: (type: TaskType, id: string) => void;
    onOpenApiKeys: () => void;
    local: LocalControls;
  } = $props();

  let picker = $state<'select' | 'fallback' | null>(null);
  /** Centre of whichever button opened the dialog, so it grows out of it. */
  let pickerOrigin = $state<{ x: number; y: number } | null>(null);

  function openPicker(mode: 'select' | 'fallback', event: MouseEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    pickerOrigin = { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
    picker = mode;
  }

  const parsedDefault = $derived(splitModelId(defaultModel));
  const activeRow = $derived(rowForSelection(defaultModel, context));

  const missingKeys = $derived(
    [defaultModel, ...fallbackModels]
      .map((id) => splitModelId(id)?.provider)
      .filter((provider): provider is ProviderId => !!provider)
      .filter((provider) => provider !== 'local')
      .filter((provider, index, all) => all.indexOf(provider) === index)
      .filter((provider) => !apiKeyStatus[provider]),
  );

  const warnings = $derived([
    ...(missingKeys.length
      ? [`Missing API keys for: ${missingKeys.map(providerDisplayLabel).join(', ')}`]
      : []),
    ...unavailableMessages(type, defaultModel, fallbackModels, context),
  ]);

  const promptCustomized = $derived(!!cleanupPromptStore.override.trim());

  function chipLabel(id: string): string {
    const parsed = splitModelId(id);
    return parsed ? qualifiedModelLabel(parsed.provider, parsed.model) : id;
  }
</script>

<section class="task-tile">
  <header class="tile-head">
    <span class="head-title">{taskLabel(type)}</span>
    <div class="head-actions">
      {#if type === 'cleanup' && advancedModelUi && parsedDefault}
        <button
          class="tile-btn"
          type="button"
          onclick={(event) =>
            openCleanupPromptEditor(
              parsedDefault.provider,
              parsedDefault.model,
              (event.currentTarget as HTMLButtonElement).getBoundingClientRect(),
            )}
        >
          Edit prompt{promptCustomized ? ' •' : ''}
        </button>
      {/if}
      <button
        class="tile-btn tile-btn-primary"
        type="button"
        onclick={(event) => openPicker('select', event)}
      >
        Change model
      </button>
    </div>
  </header>

  <div class="active-row">
    {#if parsedDefault && activeRow}
      <span
        class="active-logo"
        class:plate-bleed={getProviderPlate(parsedDefault.provider, parsedDefault.model) === 'bleed'}
      >
        {@html getProviderLogo(parsedDefault.provider, parsedDefault.model)}
      </span>
      <span class="active-text">
        <span class="active-name">{activeRow.label}</span>
        <span class="summary-row">
          <span class="summary-item provider-chip">{providerDisplayLabel(parsedDefault.provider)}</span>
          <span class="summary-item model-chip">{parsedDefault.model}</span>
          {#if fallbackModels.length > 0}
            <span class="summary-item fallback-chip">
              {fallbackModels.length} fallback{fallbackModels.length !== 1 ? 's' : ''}
            </span>
          {/if}
        </span>
      </span>
    {:else}
      <span class="active-text">
        <span class="active-name active-none">No model selected</span>
        <span class="summary-row">
          <span class="summary-item provider-chip">None</span>
          <span class="summary-item model-chip">None</span>
        </span>
      </span>
    {/if}
  </div>

  {#each warnings as warning (warning)}
    <p class="warn-banner">{warning}</p>
  {/each}

  <div class="fallback-block">
    <p class="fallback-label">
      Fallbacks
      <span>tried in order if the model above can’t run</span>
    </p>

    <ul class="fallback-list">
      {#each fallbackModels as id, index (id)}
        {@const parsed = splitModelId(id)}
        <li
          class="fallback-chip-item"
          in:fly={{ y: -6, duration: motionMs(MOTION_MS.base), easing: cubicOut }}
          out:fly={{ y: -6, duration: motionMs(MOTION_MS.fast), easing: cubicOut }}
        >
          <span class="chip-index">{index + 1}</span>
          {#if parsed}
            <span
              class="chip-logo"
              class:plate-bleed={getProviderPlate(parsed.provider, parsed.model) === 'bleed'}
            >
              {@html getProviderLogo(parsed.provider, parsed.model)}
            </span>
          {/if}
          <span class="chip-name">{chipLabel(id)}</span>
          <span class="chip-controls">
            <button
              class="chip-btn"
              type="button"
              disabled={index === 0}
              aria-label="Move {chipLabel(id)} earlier"
              onclick={() => onMoveFallback(type, id, -1)}>↑</button
            >
            <button
              class="chip-btn"
              type="button"
              disabled={index === fallbackModels.length - 1}
              aria-label="Move {chipLabel(id)} later"
              onclick={() => onMoveFallback(type, id, 1)}>↓</button
            >
            <button
              class="chip-btn chip-remove"
              type="button"
              aria-label="Remove {chipLabel(id)} from the fallback chain"
              onclick={() => onRemoveFallback(type, id)}>×</button
            >
          </span>
        </li>
      {:else}
        <li class="fallback-empty">None — this task fails if the model above can’t run.</li>
      {/each}
    </ul>

    <button class="add-fallback" type="button" onclick={(event) => openPicker('fallback', event)}>
      + Add fallback
    </button>
  </div>
</section>

{#if picker}
  <ModelPickerModal
    mode={picker}
    origin={pickerOrigin}
    task={type}
    {context}
    {defaultModel}
    {fallbackModels}
    {advancedModelUi}
    {customDraft}
    onSelect={(id) => onSelectModel(type, id)}
    onAddFallback={(id) => onAddFallback(type, id)}
    onCustomDraftChange={(value) => onCustomDraftChange(type, value)}
    onAddCustomModel={(id) => onAddCustomModel(type, id)}
    {local}
    onOpenApiKeys={() => {
      picker = null;
      onOpenApiKeys();
    }}
    onClose={() => (picker = null)}
  />
{/if}

<style>
  /* Card, so the advanced area reads as part of the same system as the preset
     cards above it rather than loose text on the page background. */
  .task-tile {
    border: 1px solid var(--line);
    border-radius: var(--r-lg, 12px);
    background: var(--bg-elev);
    padding: 16px 18px 14px;
    margin-bottom: 12px;
  }

  .tile-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 12px;
  }

  .head-title {
    font-family: var(--serif);
    font-size: 16px;
    font-weight: 500;
    color: var(--ink);
    line-height: 1;
  }

  .head-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .tile-btn {
    font-family: var(--sans);
    font-size: 11.5px;
    font-weight: 500;
    padding: 5px 12px;
    border: 1px solid var(--line-strong);
    border-radius: 7px;
    background: transparent;
    color: var(--ink-soft);
    cursor: pointer;
    transition: background 0.14s, color 0.14s;
    white-space: nowrap;
  }

  .tile-btn:hover {
    background: var(--control-hover);
    color: var(--ink);
  }

  .tile-btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .tile-btn-primary {
    border-color: color-mix(in srgb, var(--accent) 45%, var(--line));
    color: var(--accent-ink);
  }

  /* ── Active model ───────────────────────── */
  .active-row {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--paper);
  }

  /* Marks are shown as each brand draws them — a logo carrying its own
     background fills the tile, the rest sit bare at true colour. */
  .active-logo,
  .chip-logo {
    flex-shrink: 0;
    display: grid;
    place-items: center;
    border-radius: 8px;
    overflow: hidden;
    color: var(--ink-soft);
  }

  .active-logo {
    width: 30px;
    height: 30px;
  }

  .active-logo :global(svg) {
    width: 21px;
    height: 21px;
  }

  .chip-logo {
    width: 18px;
    height: 18px;
    border-radius: 5px;
  }

  .chip-logo :global(svg) {
    width: 13px;
    height: 13px;
  }

  .plate-bleed :global(svg) {
    width: 100%;
    height: 100%;
  }


  .active-text {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .active-name {
    font-family: var(--sans);
    font-size: 14px;
    font-weight: 500;
    color: var(--ink);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .active-none {
    color: var(--ink-mute);
  }

  /* Plain inline text, not pills — the selection reads as a quiet
     "Provider · model · N fallbacks" line. A frozen smoke test reads the
     fallback count off .summary-item, so the order stays fixed. */
  .summary-row {
    display: flex;
    align-items: baseline;
    gap: 6px;
    flex-wrap: wrap;
  }

  .summary-item {
    font-size: 11px;
    font-family: var(--sans);
    color: var(--ink-mute);
    white-space: nowrap;
  }

  .summary-item:not(:first-child)::before {
    content: '·';
    margin-right: 6px;
    color: var(--ink-faint);
  }

  .provider-chip {
    color: var(--ink-soft);
  }

  .model-chip {
    font-family: var(--mono);
    font-size: 10.5px;
  }

  /* ── Warnings ───────────────────────────── */
  .warn-banner {
    margin: 10px 0 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--danger);
    background: var(--danger-bg);
    border: 1px solid var(--danger-line);
    border-radius: 8px;
    padding: 8px 10px;
  }

  /* ── Fallback chain ─────────────────────── */
  .fallback-block {
    margin-top: 12px;
  }

  .fallback-label {
    margin: 0 0 6px;
    font-family: var(--sans);
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.09em;
    color: var(--ink-faint);
  }

  .fallback-label span {
    margin-left: 8px;
    font-weight: 450;
    text-transform: none;
    letter-spacing: 0;
    font-size: 11px;
  }

  .fallback-list {
    list-style: none;
    margin: 0 0 6px;
    padding: 0;
    display: flex;
    flex-direction: column;
  }

  .fallback-chip-item {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 5px 8px;
    border-radius: 7px;
    font-family: var(--sans);
    font-size: 12px;
    color: var(--ink-soft);
  }

  .fallback-chip-item:hover {
    background: var(--control-hover);
  }

  .chip-index {
    width: 12px;
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
    font-size: 11px;
    color: var(--ink-faint);
  }

  .chip-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chip-controls {
    display: flex;
    gap: 1px;
    flex-shrink: 0;
  }

  .chip-btn {
    width: 20px;
    height: 20px;
    border: none;
    background: transparent;
    color: var(--ink-faint);
    font-size: 11px;
    line-height: 1;
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.14s, color 0.14s;
  }

  /* Keyboard users never hover, so focus has to reveal them too. */
  .fallback-chip-item:hover .chip-btn,
  .chip-btn:focus-visible {
    opacity: 1;
  }

  .chip-btn:hover:not(:disabled) {
    color: var(--ink);
  }

  .chip-btn:disabled {
    opacity: 0;
    cursor: default;
  }

  .fallback-chip-item:hover .chip-btn:disabled {
    opacity: 0.25;
  }

  .chip-remove:hover {
    color: var(--danger);
  }

  .fallback-empty {
    padding: 5px 8px;
    font-family: var(--sans);
    font-size: 12px;
    color: var(--ink-faint);
  }

  .add-fallback {
    font-family: var(--sans);
    font-size: 11.5px;
    font-weight: 500;
    padding: 4px 8px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--ink-mute);
    cursor: pointer;
    transition: background 0.14s, color 0.14s;
  }

  .add-fallback:hover {
    background: var(--control-hover);
    color: var(--ink);
  }

  .add-fallback:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
</style>
