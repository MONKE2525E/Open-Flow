<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '../../tauri';
  import type { ProviderId } from '../../settings';
  import ModelPresetPicker from '../../components/settings/ModelPresetPicker.svelte';
  import {
    buildPresets,
    getHardware,
    type ActiveConfig,
    type Hardware,
    type Preset,
  } from '../../components/settings/modelPresets';
  import {
    localSttStore,
    refreshLocalModels,
    refreshLocalState,
    downloadLocalModel,
    cancelLocalModelDownload,
    deleteLocalModel,
  } from '../../localSttStore.svelte';
  import {
    localLlmStore,
    refreshLocalLlmModels,
    refreshLocalLlmState,
    downloadLocalLlmModel,
    cancelLocalLlmModelDownload,
    deleteLocalLlmModel,
  } from '../../localLlmStore.svelte';

  let {
    apiKeyStatus,
    preset = $bindable(),
    onOpenApiKeys,
  }: {
    apiKeyStatus: Record<ProviderId, boolean>;
    /** The chosen preset. Written to settings by Setup's finish(), not here. */
    preset: Preset | null;
    onOpenApiKeys: () => void;
  } = $props();

  // Same "assume capable" default as the Models tab — never flash a degraded
  // preset list while the real hardware read is in flight.
  let hardware = $state<Hardware>({ totalRamMb: 16384, freeRamMb: 12288, gpus: [], unknown: true });
  let localSupported = $state(true);

  const presets = $derived(buildPresets(apiKeyStatus, hardware, localSupported));

  const installedLocal = $derived({
    transcription: localSttStore.models.filter((m) => m.is_downloaded).map((m) => m.id),
    cleanup: localLlmStore.models.filter((m) => m.is_downloaded).map((m) => m.id),
  });

  const downloadingLocal = $derived({
    transcription: localSttStore.state.downloading_model_id ?? null,
    cleanup: localLlmStore.state.downloading_model_id ?? null,
  });

  // The picker highlights whichever card matches this config, so reflecting the
  // selection back through it is what makes the card read as "Selected".
  const activeConfig = $derived<ActiveConfig>({
    transcriptionDefaultModel: preset?.target?.transcriptionDefaultModel ?? '',
    cleanupEnabled: preset?.target?.cleanupEnabled ?? false,
    cleanupDefaultModel: preset?.target?.cleanupDefaultModel ?? '',
    dualTranscription: preset?.target?.dualTranscription ?? false,
    transcriptionFallbacks: preset?.target?.transcriptionFallbacks ?? [],
    cleanupFallbacks: preset?.target?.cleanupFallbacks ?? [],
  });

  function needsDownload(candidate: Preset): boolean {
    return (candidate.target?.requiredLocalModels ?? []).some(
      (m) => !installedLocal[m.task]?.includes(m.id),
    );
  }

  // Pre-select a sensible middle option so the step has a working answer even if
  // the user just hits Next — but never one that would commit them to a
  // multi-gigabyte download they didn't ask for. If everything needs a download,
  // nothing is pre-selected and the provider defaults stand.
  let userPicked = $state(false);
  $effect(() => {
    if (userPicked) return;
    const list = presets.filter((p) => p.kind === 'preset' && !needsDownload(p));
    if (list.length === 0) return;
    if (preset) return;
    preset = list.find((p) => p.id.endsWith('-balanced')) ?? list[0];
  });

  const downloading = $derived(
    (preset?.target?.requiredLocalModels ?? []).some((m) => downloadingLocal[m.task] === m.id),
  );

  function choose(next: Preset) {
    if (!next.target) return;
    userPicked = true;
    preset = next;
    // Start any missing downloads now so they run while the user finishes the
    // wizard. Unlike the Models tab we don't defer activation — finish() writes
    // the settings minutes later, and the card shows download progress meanwhile.
    for (const model of next.target.requiredLocalModels) {
      if (installedLocal[model.task]?.includes(model.id)) continue;
      const start = model.task === 'transcription' ? downloadLocalModel : downloadLocalLlmModel;
      start(model.id).catch((err) => console.error('setup preset download failed', err));
    }
  }

  function cancel(target: Preset) {
    for (const model of target.target?.requiredLocalModels ?? []) {
      const stop = model.task === 'transcription' ? cancelLocalModelDownload : cancelLocalLlmModelDownload;
      stop(model.id).catch((err) => console.error('setup preset cancel failed', err));
    }
  }

  function remove(target: Preset) {
    for (const model of target.target?.requiredLocalModels ?? []) {
      if (!installedLocal[model.task]?.includes(model.id)) continue;
      const drop = model.task === 'transcription' ? deleteLocalModel : deleteLocalLlmModel;
      drop(model.id).catch((err) => console.error('setup preset delete failed', err));
    }
  }

  onMount(() => {
    refreshLocalModels().catch(() => {});
    refreshLocalState().catch(() => {});
    refreshLocalLlmModels().catch(() => {});
    refreshLocalLlmState().catch(() => {});
    // Only an explicit false hides local presets — an older backend or a
    // transient error must not strip the offline option for everyone else.
    invoke<boolean>('local_models_supported_on_this_platform')
      .then((supported) => { if (supported === false) localSupported = false; })
      .catch(() => {});
    getHardware().then((hw) => { hardware = hw; }).catch(() => {});
  });
</script>

<div class="step models-step">
  <div class="models-picker">
    <ModelPresetPicker
      {apiKeyStatus}
      {hardware}
      {localSupported}
      {activeConfig}
      {installedLocal}
      {downloadingLocal}
      onApplyPreset={choose}
      onOpenApiKeys={onOpenApiKeys}
      onCancelPreset={cancel}
      onDeletePreset={remove}
      showCustomNote={false}
    />
  </div>

  <p class="models-note">
    {#if downloading}
      Downloading in the background — keep going. Dictation starts working once it finishes.
    {:else}
      Change this anytime in Settings → Models, where you can also pick individual models.
    {/if}
  </p>
</div>

<style>
  .models-step { gap: 12px; }

  /* PresetCard's narrow layout is keyed to the settings panel container, which
     doesn't exist here — name the container so the cards still fold on small
     windows instead of overflowing the wizard column. */
  .models-picker {
    container-type: inline-size;
    container-name: settings-panel;
  }

  /* The Settings cards are sized for a scrolling panel. The wizard has a fixed
     height budget and up to four cards, so tighten the vertical rhythm here
     rather than letting the step overflow the action bar. */
  .models-picker :global(.preset-grid) { gap: 7px; margin-bottom: 0; }
  .models-picker :global(.preset-content) { padding: 9px 14px; gap: 14px; }
  .models-picker :global(.preset-info) { padding: 9px 14px; gap: 14px; }
  .models-picker :global(.preset-side) { width: 156px; gap: 7px; }
  .models-picker :global(.preset-name) { font-size: 15px; }
  .models-picker :global(.preset-tagline) { font-size: 12px; }
  .models-picker :global(.preset-action-btn) { min-height: 26px; padding: 4px 10px; }

  /* Short windows: four cards plus a footnote don't fit. The footnote is the
     least load-bearing thing on the step, so it goes first. */
  @media (max-height: 660px) {
    .models-note { display: none; }
    .models-picker :global(.preset-grid) { gap: 6px; }
    .models-picker :global(.preset-content) { padding: 7px 12px; }
  }

  .models-note {
    margin: 0;
    font-size: 11.5px;
    color: var(--ink-faint);
    line-height: 1.5;
    text-align: center;
  }
</style>
