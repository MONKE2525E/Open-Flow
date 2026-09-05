<script lang="ts">
  import PresetCard from './PresetCard.svelte';
  import { buildPresets, matchActivePreset, type ActiveConfig, type Hardware, type Preset } from './modelPresets';
  import type { ProviderId } from '../../settings';

  let {
    apiKeyStatus,
    hardware,
    localSupported,
    activeConfig,
    installedLocal,
    downloadingLocal,
    onApplyPreset,
    onOpenApiKeys,
    onCancelPreset,
    onDeletePreset,
    /** The note points at the Advanced panel, which only exists in Settings. */
    showCustomNote = true,
  }: {
    showCustomNote?: boolean;
    apiKeyStatus: Record<ProviderId, boolean>;
    hardware: Hardware;
    localSupported: boolean;
    activeConfig: ActiveConfig;
    /** Downloaded local model ids, per task. */
    installedLocal: { transcription: string[]; cleanup: string[] };
    /** The local model id currently downloading per task, if any. */
    downloadingLocal: { transcription: string | null; cleanup: string | null };
    onApplyPreset: (preset: Preset) => void;
    onOpenApiKeys: () => void;
    onCancelPreset: (preset: Preset) => void;
    onDeletePreset: (preset: Preset) => void;
  } = $props();

  const presets = $derived(buildPresets(apiKeyStatus, hardware, localSupported));
  const activeId = $derived(matchActivePreset(presets, activeConfig));
  // Only surface "Custom" when there's a real, actionable preset list that the
  // current selection simply doesn't match (not the add-key placeholder state).
  const showCustom = $derived(showCustomNote && activeId === null && presets.some((preset) => preset.kind === 'preset'));

  function downloadMbFor(preset: Preset): number {
    if (!preset.target) return 0;
    let total = 0;
    for (const model of preset.target.requiredLocalModels) {
      if (!installedLocal[model.task]?.includes(model.id)) total += model.sizeMb;
    }
    return total;
  }

  function isDownloading(preset: Preset): boolean {
    if (!preset.target) return false;
    return preset.target.requiredLocalModels.some(
      (model) => downloadingLocal[model.task] === model.id,
    );
  }

  function installedCountFor(preset: Preset): number {
    if (!preset.target) return 0;
    return preset.target.requiredLocalModels.filter(
      (model) => installedLocal[model.task]?.includes(model.id) ?? false,
    ).length;
  }
</script>

<div class="preset-grid">
  {#each presets as preset (preset.id)}
    <PresetCard
      {preset}
      active={preset.id === activeId}
      downloadMb={downloadMbFor(preset)}
      downloading={isDownloading(preset)}
      installedCount={installedCountFor(preset)}
      onSelect={() => onApplyPreset(preset)}
      onAddKey={onOpenApiKeys}
      onCancelDownload={() => onCancelPreset(preset)}
      onDeleteModels={() => onDeletePreset(preset)}
    />
  {/each}
</div>

{#if showCustom}
  <p class="preset-custom-note">
    <strong>Custom setup.</strong> Your current models don't match a recommended preset — manage them in
    Advanced below.
  </p>
{/if}

<style>
  .preset-grid {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 6px;
  }

  .preset-custom-note {
    margin: 4px 0 0;
    font-family: var(--sans);
    font-size: 12px;
    line-height: 1.45;
    color: var(--ink-mute);
  }

  .preset-custom-note strong {
    color: var(--ink-soft);
    font-weight: 600;
  }
</style>
