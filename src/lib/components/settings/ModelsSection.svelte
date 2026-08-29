<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { fade, fly, slide } from 'svelte/transition';
  import { cubicOut, expoOut } from 'svelte/easing';
  import { invoke } from '../../tauri';
  import {
    localSttStore,
    refreshLocalModels,
    refreshLocalState,
    downloadLocalModel,
    cancelLocalModelDownload,
    deleteLocalModel,
    openLocalModelsFolder,
  } from '../../localSttStore.svelte';
  import {
    localLlmStore,
    refreshLocalLlmModels,
    refreshLocalLlmState,
    refreshLocalLlmRuntimeInfo,
    downloadLocalLlmModel,
    downloadLocalLlmRuntime,
    cancelLocalLlmRuntimeDownload,
    deleteLocalLlmRuntime,
    cancelLocalLlmModelDownload,
    deleteLocalLlmModel,
  } from '../../localLlmStore.svelte';
  import { animateWidth, MOTION_MS, MOTION_PX, motionMs, motionPx } from '../../motion';
  import Toggle from '../Toggle.svelte';
  import Dropdown from '../Dropdown.svelte';
  import { appStore, cleanupPromptTemplateStore, openCleanupPromptEditor } from '../../stores.svelte';
  import {
    saveSetting,
    type LocalModelMemoryPolicy,
    type ProviderId,
    type ProviderModelMap,
  } from '../../settings';
  import ModelTaskTile from './ModelTaskTile.svelte';
  import ModelPresetPicker from './ModelPresetPicker.svelte';
  import {
    getHardware,
    type ActiveConfig,
    type Hardware,
    type Preset,
    type PresetTarget,
    type RequiredLocalModel,
  } from './modelPresets';
  import { transcriptionModelStore } from '../../transcriptionModelStore.svelte';
  import {
    hydrateCatalogCache,
    modelCatalogStore,
    refreshCatalog,
    refreshStaleCatalogs,
    trackedIds,
  } from '../../modelCatalogStore.svelte';
  import { type LocalControls, type PickerContext } from './modelStates';
  import {
    CATALOG,
    emptyProviderModelMap,
    mergeProviderModelMap,
    modelId,
    recommendedModels,
    migrateDeprecatedGroqCleanupModel,
    migrateDeprecatedGoogleModel,
    splitModelId,
    type AllSettingsPayload,
    type TaskType,
    type UiProviderId,
  } from './models';

  let apiKeyStatus = $state<Record<ProviderId, boolean>>({
    groq: false,
    openai: false,
    google: false,
    assemblyai: false,
    local: false,
  });

  let transcriptionModelsByProvider = $state<ProviderModelMap>(emptyProviderModelMap());
  let cleanupModelsByProvider = $state<ProviderModelMap>(emptyProviderModelMap());

  let transcriptionDefaultModel = $state('groq/whisper-large-v3-turbo');
  let cleanupDefaultModel = $state('groq/qwen/qwen3.6-27b');
  let transcriptionFallbackModels = $state<string[]>([]);
  let dualTranscriptionEnabled = $state(false);
  let cleanupFallbackModels = $state<string[]>([]);
  let cleanupEnabled = $state(true);

  // Hardware drives which local presets are offered. Starts as the "assume
  // capable" default so the picker never flashes a degraded set before the
  // real read lands (or if it fails outright — see getHardware).
  let hardware = $state<Hardware>({ totalRamMb: 16384, freeRamMb: 12288, gpus: [], unknown: true });

  // A preset whose local models are still downloading. Its settings are applied
  // (activated) only once every required model is on disk, so the active
  // selection never points at a model that isn't there yet.
  let pendingPreset = $state<Preset | null>(null);
  let pendingPresetDownloads = $state<RequiredLocalModel[]>([]);
  let pendingPresetRequestsSettled = $state(0);

  const activeConfig = $derived<ActiveConfig>({
    transcriptionDefaultModel,
    cleanupEnabled,
    cleanupDefaultModel,
    dualTranscription: dualTranscriptionEnabled,
    transcriptionFallbacks: transcriptionFallbackModels,
    cleanupFallbacks: cleanupFallbackModels,
  });

  const installedLocal = $derived({
    transcription: localSttStore.models.filter((model) => model.is_downloaded).map((model) => model.id),
    cleanup: localLlmStore.models.filter((model) => model.is_downloaded).map((model) => model.id),
  });

  /**
   * Local models are downloaded, deleted and prompt-edited from inside the
   * picker now. The old "Local models" section listed the very same models a
   * second time under a different UI, which is what made the page feel like
   * two products stapled together.
   */
  function localControls(type: TaskType): LocalControls {
    if (type === 'transcription') {
      return {
        supported: localModelsSupported,
        models: localSttStore.models,
        downloadProgress: localSttStore.downloadProgress,
        downloadStage: localSttStore.downloadStage,
        onDownload: (id) => downloadLocalModel(id).catch((err) => console.error('download stt model failed', err)),
        onCancel: (id) => cancelLocalModelDownload(id).catch((err) => console.error('cancel stt download failed', err)),
        onDelete: (id) => handleDeleteTranscriptionModel(id).catch((err) => console.error('delete stt model failed', err)),
      };
    }
    return {
      supported: localModelsSupported,
      models: localLlmStore.models,
      downloadProgress: localLlmStore.downloadProgress,
      downloadStage: localLlmStore.downloadStage,
      onDownload: (id) => downloadLocalLlmModel(id).catch((err) => console.error('download llm model failed', err)),
      onCancel: (id) => cancelLocalLlmModelDownload(id).catch((err) => console.error('cancel llm download failed', err)),
      onDelete: (id) => handleDeleteCleanupModel(id).catch((err) => console.error('delete llm model failed', err)),
      runtime: {
        info: localLlmStore.runtime,
        progress: localLlmStore.runtimeDownloadProgress,
        onDownload: () => downloadLocalLlmRuntime().catch((err) => console.error('download runtime failed', err)),
        onCancel: () => cancelLocalLlmRuntimeDownload().catch((err) => console.error('cancel runtime download failed', err)),
        onDelete: () => deleteLocalLlmRuntime().catch((err) => console.error('delete runtime failed', err)),
      },
    };
  }

  /** Everything the picker needs to classify a model, per task. */
  function pickerContext(type: TaskType): PickerContext {
    return {
      task: type,
      apiKeyStatus,
      cache: modelCatalogStore.cache,
      localModels: type === 'transcription' ? localSttStore.models : localLlmStore.models,
      hardware,
    };
  }

  /** Selections plus the curated catalog — the ids worth counting misses for. */
  function trackedModelIds(): string[] {
    const selected = [
      transcriptionDefaultModel,
      ...transcriptionFallbackModels,
      cleanupDefaultModel,
      ...cleanupFallbackModels,
    ].filter(Boolean);
    return trackedIds(selected, CATALOG);
  }

  const downloadingLocal = $derived({
    transcription: localSttStore.state.downloading_model_id ?? null,
    cleanup: localLlmStore.state.downloading_model_id ?? null,
  });

  function requiredModelsInstalled(target: PresetTarget): boolean {
    return target.requiredLocalModels.every((model) => installedLocal[model.task]?.includes(model.id) ?? false);
  }

  function clearPendingPreset() {
    pendingPreset = null;
    pendingPresetDownloads = [];
    pendingPresetRequestsSettled = 0;
  }

  async function setCleanupEnabled(value: boolean) {
    const previousValue = cleanupEnabled;
    const previousStoreValue = appStore.cleanupEnabled;
    cleanupEnabled = value;
    appStore.cleanupEnabled = value;
    try {
      await saveSetting('cleanup_enabled', value);
    } catch (err) {
      cleanupEnabled = previousValue;
      appStore.cleanupEnabled = previousStoreValue;
      console.error('save cleanup_enabled from preset failed', err);
      throw err;
    }
  }

  function activatePreset(target: PresetTarget) {
    transcriptionDefaultModel = target.transcriptionDefaultModel;
    transcriptionFallbackModels = [...target.transcriptionFallbacks];
    dualTranscriptionEnabled = target.dualTranscription;
    if (target.cleanupEnabled && target.cleanupDefaultModel) {
      cleanupDefaultModel = target.cleanupDefaultModel;
    }
    cleanupFallbackModels = [...target.cleanupFallbacks];
    setCleanupEnabled(target.cleanupEnabled).catch((err) => console.error('preset cleanup flag failed', err));
    persistAll().catch((err) => console.error('persist preset failed', err));
  }

  function applyPreset(preset: Preset) {
    const target = preset.target;
    if (!target) return;

    const missing = target.requiredLocalModels.filter(
      (model) => !installedLocal[model.task]?.includes(model.id),
    );
    if (missing.length > 0) {
      // Kick off the downloads and defer activation until they land (see the
      // $effect below). Reuses the same download plumbing as the Advanced panel.
      pendingPreset = preset;
      pendingPresetDownloads = missing;
      pendingPresetRequestsSettled = 0;
      for (const model of missing) {
        if (model.task === 'transcription') {
          downloadLocalModel(model.id)
            .then((started) => {
              if (pendingPreset?.id !== preset.id) return;
              if (!started) {
                pendingPresetRequestsSettled += 1;
              } else {
                pendingPresetRequestsSettled += 1;
              }
            })
            .catch((err) => {
              if (pendingPreset?.id === preset.id) {
                pendingPresetRequestsSettled += 1;
              }
              console.error('preset stt download failed', err);
            });
        } else {
          downloadLocalLlmModel(model.id)
            .then((started) => {
              if (pendingPreset?.id !== preset.id) return;
              if (!started) {
                pendingPresetRequestsSettled += 1;
              } else {
                pendingPresetRequestsSettled += 1;
              }
            })
            .catch((err) => {
              if (pendingPreset?.id === preset.id) {
                pendingPresetRequestsSettled += 1;
              }
              console.error('preset llm download failed', err);
            });
        }
      }
      return;
    }

    activatePreset(target);
  }

  function isExpectedModelDownloading(model: RequiredLocalModel): boolean {
    if (downloadingLocal[model.task] === model.id) return true;
    if (model.task === 'transcription') {
      return localSttStore.models.some((entry) => entry.id === model.id && entry.is_downloading);
    }
    return localLlmStore.models.some((entry) => entry.id === model.id && entry.is_downloading);
  }

  function openApiKeysSection() {
    appStore.settingsSection = 'keys';
  }

  function cancelPresetDownload(preset: Preset) {
    const target = preset.target;
    if (!target) return;
    if (pendingPreset?.id === preset.id) clearPendingPreset();
    for (const model of target.requiredLocalModels) {
      if (!isExpectedModelDownloading(model)) continue;
      if (model.task === 'transcription') {
        cancelLocalModelDownload(model.id).catch((err) => console.error('cancel preset stt download failed', err));
      } else {
        cancelLocalLlmModelDownload(model.id).catch((err) => console.error('cancel preset llm download failed', err));
      }
    }
  }

  function deletePresetModels(preset: Preset) {
    const target = preset.target;
    if (!target) return;
    for (const model of target.requiredLocalModels) {
      if (!installedLocal[model.task]?.includes(model.id)) continue;
      if (model.task === 'transcription') {
        handleDeleteTranscriptionModel(model.id).catch((err) => console.error('delete preset stt model failed', err));
      } else {
        handleDeleteCleanupModel(model.id).catch((err) => console.error('delete preset llm model failed', err));
      }
    }
  }

  // Once a pending preset's downloads finish, activate it.
  $effect(() => {
    const preset = pendingPreset;
    if (!preset?.target) return;
    if (requiredModelsInstalled(preset.target)) {
      activatePreset(preset.target);
      clearPendingPreset();
      return;
    }

    const isDownloadingExpectedModel = pendingPresetDownloads.some(
      isExpectedModelDownloading,
    );
    if (!isDownloadingExpectedModel && pendingPresetRequestsSettled >= pendingPresetDownloads.length) {
      // A failure or cancellation can leave the preset incomplete without
      // passing through the explicit cancel button. Do not keep a stale
      // pending preset that could activate after an unrelated later download.
      clearPendingPreset();
    }
  });

  // One draft per task now that the custom-id field lives in the picker
  // rather than once per provider group.
  let customDrafts = $state<Record<TaskType, string>>({ transcription: '', cleanup: '' });

  let advancedModelUi = $state(false);
  let localModelMemoryPolicy = $state<LocalModelMemoryPolicy>('unload_after_5m');
  let localModelMemoryDropdownOpen = $state(false);
  // Local on-device STT/LLM inference is gated off entirely on Intel Mac
  // builds — see system::platform::is_macos_intel on the backend for why.
  // Defaults to true (never assume unsupported) until the one-time check
  // resolves, since almost every user is on a supported platform.
  let localModelsSupported = $state(true);
  const LOCAL_MEMORY_POLICY_MENU_ID = 'models-local-memory-policy-menu';
  const localMemoryPolicyOptions: { value: LocalModelMemoryPolicy; label: string }[] = [
    { value: 'unload_after_5m', label: 'Unload after 5 minutes' },
    { value: 'unload_after_15m', label: 'Unload after 15 minutes' },
    { value: 'keep_loaded', label: 'Keep loaded' },
    { value: 'unload_immediately', label: 'Unload immediately' },
  ];

  function localMemoryPolicyLabel(policy: LocalModelMemoryPolicy): string {
    switch (policy) {
      case 'keep_loaded':
        return 'Keep loaded';
      case 'unload_after_15m':
        return 'Unload after 15 minutes';
      case 'unload_immediately':
        return 'Unload immediately';
      default:
        return 'Unload after 5 minutes';
    }
  }

  function taskMap(type: TaskType): ProviderModelMap {
    return type === 'transcription' ? transcriptionModelsByProvider : cleanupModelsByProvider;
  }

  function setTaskMap(type: TaskType, next: ProviderModelMap) {
    if (type === 'transcription') {
      transcriptionModelsByProvider = next;
    } else {
      cleanupModelsByProvider = next;
    }
  }

  function taskDefault(type: TaskType): string {
    return type === 'transcription' ? transcriptionDefaultModel : cleanupDefaultModel;
  }

  function setTaskDefault(type: TaskType, value: string) {
    if (type === 'transcription') {
      transcriptionDefaultModel = value;
    } else {
      cleanupDefaultModel = value;
    }
  }

  function taskFallbacks(type: TaskType): string[] {
    return type === 'transcription' ? transcriptionFallbackModels : cleanupFallbackModels;
  }

  function setTaskFallbacks(type: TaskType, next: string[]) {
    if (type === 'transcription') {
      transcriptionFallbackModels = next;
    } else {
      cleanupFallbackModels = next;
    }
  }

  function ensureModelsContainSelection(type: TaskType, provider: ProviderId, modelName: string) {
    const map = taskMap(type);
    if (!map[provider].includes(modelName)) {
      setTaskMap(type, { ...map, [provider]: [...map[provider], modelName] });
    }
  }

  // A deleted local model drops out of the picker's Ready list entirely (only
  // downloaded models appear there), so if it was selected there is no row left
  // to click to deselect it. Reassign to another installed local model for this
  // task, or fall back to Groq's recommended model, so the selection never points
  // at something that no longer exists.
  function pickLocalReplacementDefault(type: TaskType, excludeLocalId: string): string {
    const localModels = type === 'transcription' ? localSttStore.models : localLlmStore.models;
    const otherLocal = localModels.find((model) => model.is_downloaded && model.id !== excludeLocalId);
    if (otherLocal) {
      return modelId('local', otherLocal.id);
    }
    // Safe: Groq always has a recommendedModels entry for both tasks.
    return modelId('groq', recommendedModels[type].groq!.standard);
  }

  function reassignAfterLocalModelDeleted(type: TaskType, deletedLocalId: string) {
    const deletedId = modelId('local', deletedLocalId);
    const remainingFallbacks = taskFallbacks(type).filter((id) => id !== deletedId);

    if (taskDefault(type) === deletedId) {
      if (remainingFallbacks.length > 0) {
        const [nextActive, ...rest] = remainingFallbacks;
        setTaskDefault(type, nextActive);
        setTaskFallbacks(type, rest);
      } else {
        setTaskDefault(type, pickLocalReplacementDefault(type, deletedLocalId));
        setTaskFallbacks(type, remainingFallbacks);
      }
    } else if (remainingFallbacks.length !== taskFallbacks(type).length) {
      setTaskFallbacks(type, remainingFallbacks);
    }

    const map = taskMap(type);
    if (map.local?.includes(deletedLocalId)) {
      setTaskMap(type, { ...map, local: map.local.filter((id) => id !== deletedLocalId) });
    }

    persistAll().catch((err) => console.error('persist after local model delete failed', err));
  }

  async function handleDeleteTranscriptionModel(localModelId: string) {
    await deleteLocalModel(localModelId);
    const stillDownloaded = localSttStore.models.some(
      (model) => model.id === localModelId && model.is_downloaded,
    );
    if (!stillDownloaded) {
      reassignAfterLocalModelDeleted('transcription', localModelId);
    }
  }

  async function handleDeleteCleanupModel(localModelId: string) {
    await deleteLocalLlmModel(localModelId);
    const stillDownloaded = localLlmStore.models.some(
      (model) => model.id === localModelId && model.is_downloaded,
    );
    if (!stillDownloaded) {
      reassignAfterLocalModelDeleted('cleanup', localModelId);
    }
  }

  function ensureDefaultAndFallbacks() {
    for (const type of ['transcription', 'cleanup'] as TaskType[]) {
      const defaultParsed = splitModelId(taskDefault(type));
      if (!defaultParsed) continue;

      ensureModelsContainSelection(type, defaultParsed.provider, defaultParsed.model);

      const normalizedFallbacks = taskFallbacks(type)
        .map(splitModelId)
        .filter((parsed): parsed is { provider: ProviderId; model: string } => !!parsed)
        .map((parsed) => {
          ensureModelsContainSelection(type, parsed.provider, parsed.model);
          return modelId(parsed.provider, parsed.model);
        })
        .filter((id, index, all) => all.indexOf(id) === index)
        .filter((id) => id !== taskDefault(type));

      setTaskFallbacks(type, normalizedFallbacks);
    }
  }

  // Concurrent persistAll() calls (e.g. rapid row clicks) each fire several
  // parallel save_setting IPC calls. Without serialization, an older call's
  // writes can land on disk after a newer call's writes, silently reverting
  // the user's last action. Chaining through persistChain guarantees writes
  // apply in invocation order, while the values themselves are still read
  // synchronously at call time so each batch reflects the click that caused it.
  let persistChain: Promise<void> = Promise.resolve();

  function persistAll(): Promise<void> {
    ensureDefaultAndFallbacks();
    const transcriptionProvider = splitModelId(transcriptionDefaultModel)?.provider ?? 'groq';
    const cleanupProvider = splitModelId(cleanupDefaultModel)?.provider ?? 'groq';
    const snapshot = {
      transcriptionModelsByProvider,
      cleanupModelsByProvider,
      transcriptionDefaultModel,
      cleanupDefaultModel,
      transcriptionFallbackModels,
      dualTranscriptionEnabled,
      cleanupFallbackModels,
      transcriptionProvider,
      cleanupProvider,
      localModelMemoryPolicy,
    };
    transcriptionModelStore.defaultModel = transcriptionDefaultModel;

    const writeSnapshot = () =>
      Promise.all([
        saveSetting('transcription_models_by_provider', snapshot.transcriptionModelsByProvider),
        saveSetting('cleanup_models_by_provider', snapshot.cleanupModelsByProvider),
        saveSetting('transcription_default_model', snapshot.transcriptionDefaultModel),
        saveSetting('cleanup_default_model', snapshot.cleanupDefaultModel),
        saveSetting('transcription_fallback_models', snapshot.transcriptionFallbackModels),
        saveSetting('dual_transcription_enabled', snapshot.dualTranscriptionEnabled),
        saveSetting('cleanup_fallback_models', snapshot.cleanupFallbackModels),
        saveSetting('transcription_model', snapshot.transcriptionDefaultModel),
        saveSetting('cleanup_model', snapshot.cleanupDefaultModel),
        saveSetting('transcription_provider', snapshot.transcriptionProvider),
        saveSetting('cleanup_provider', snapshot.cleanupProvider),
        saveSetting('local_model_memory_policy', snapshot.localModelMemoryPolicy),
      ]).then(() => {});

    persistChain = persistChain.then(writeSnapshot, writeSnapshot);
    return persistChain;
  }

  async function migrateAndLoad() {
    const [all, keyStatus, advancedRaw, cleanupRaw] = await Promise.all([
      invoke<AllSettingsPayload>('get_all_settings'),
      invoke<Record<ProviderId, boolean>>('get_api_key_status'),
      invoke<boolean | null>('get_setting', { key: 'advanced_model_ui' }),
      invoke<boolean | null>('get_setting', { key: 'cleanup_enabled' }),
    ]);

    apiKeyStatus = { ...apiKeyStatus, ...keyStatus, local: true };
    hydrateCatalogCache(all.provider_model_cache);
    if (typeof cleanupRaw === 'boolean') {
      cleanupEnabled = cleanupRaw;
      appStore.cleanupEnabled = cleanupRaw;
    }
    transcriptionModelsByProvider = mergeProviderModelMap(all.transcription_models_by_provider);
    const rawCleanupModelsByProvider = mergeProviderModelMap(all.cleanup_models_by_provider);
    cleanupModelsByProvider = {
      ...rawCleanupModelsByProvider,
      groq: rawCleanupModelsByProvider.groq
        .map(migrateDeprecatedGroqCleanupModel)
        .filter((model, index, models) => models.indexOf(model) === index),
    };
    const cleanupModelMapChanged = JSON.stringify(cleanupModelsByProvider) !== JSON.stringify(rawCleanupModelsByProvider);

    const rawLegacyTranscription = String(all.transcription_model ?? '');
    const rawLegacyCleanup = String(all.cleanup_model ?? '');
    const legacyTranscription = rawLegacyTranscription
      ? (rawLegacyTranscription.includes('/') ? rawLegacyTranscription : `groq/${rawLegacyTranscription}`)
      : 'groq/whisper-large-v3-turbo';
    const legacyCleanup = rawLegacyCleanup
      ? (rawLegacyCleanup.includes('/') ? rawLegacyCleanup : `groq/${rawLegacyCleanup}`)
      : 'groq/qwen/qwen3.6-27b';

    transcriptionDefaultModel = migrateDeprecatedGoogleModel(all.transcription_default_model ?? legacyTranscription);
    cleanupDefaultModel = migrateDeprecatedGoogleModel(all.cleanup_default_model ?? legacyCleanup);
    const parsedTranscriptionDefault = splitModelId(transcriptionDefaultModel);
    if (parsedTranscriptionDefault?.provider === 'google') {
      transcriptionDefaultModel = modelId('google', migrateDeprecatedGoogleModel(parsedTranscriptionDefault.model));
    }
    const parsedCleanupDefault = splitModelId(cleanupDefaultModel);
    if (parsedCleanupDefault?.provider === 'groq') {
      cleanupDefaultModel = modelId('groq', migrateDeprecatedGroqCleanupModel(parsedCleanupDefault.model));
    } else if (parsedCleanupDefault?.provider === 'google') {
      cleanupDefaultModel = modelId('google', migrateDeprecatedGoogleModel(parsedCleanupDefault.model));
    }

    if (Array.isArray(all.transcription_fallback_models)) {
      transcriptionFallbackModels = all.transcription_fallback_models
        .filter((id) => !!splitModelId(id))
        .map((id) => {
          const parsed = splitModelId(id);
          return parsed?.provider === 'google'
            ? modelId('google', migrateDeprecatedGoogleModel(parsed.model))
            : id;
        });
    }
    dualTranscriptionEnabled = all.dual_transcription_enabled === true;
    if (Array.isArray(all.cleanup_fallback_models)) {
      cleanupFallbackModels = all.cleanup_fallback_models
        .filter((id) => !!splitModelId(id))
        .map((id) => {
          const parsed = splitModelId(id);
          if (parsed?.provider === 'groq') {
            return modelId('groq', migrateDeprecatedGroqCleanupModel(parsed.model));
          }
          return parsed?.provider === 'google'
            ? modelId('google', migrateDeprecatedGoogleModel(parsed.model))
            : id;
        });
    }
    if (typeof advancedRaw === 'boolean') {
      advancedModelUi = advancedRaw;
    }
    if (
      all.local_model_memory_policy === 'keep_loaded'
      || all.local_model_memory_policy === 'unload_after_5m'
      || all.local_model_memory_policy === 'unload_after_15m'
      || all.local_model_memory_policy === 'unload_immediately'
    ) {
      localModelMemoryPolicy = all.local_model_memory_policy;
    }
    cleanupPromptTemplateStore.template =
      typeof all.cleanup_prompt_template === 'string' && all.cleanup_prompt_template.trim()
        ? all.cleanup_prompt_template
        : null;

    const preTranscriptionDefault = transcriptionDefaultModel;
    const preCleanupDefault = cleanupDefaultModel;
    const preTranscriptionFallbackCount = transcriptionFallbackModels.length;
    const preCleanupFallbackCount = cleanupFallbackModels.length;
    const preCleanupFallbackModels = [...cleanupFallbackModels];
    const needsMigration =
      !all.transcription_default_model
      || !splitModelId(all.transcription_default_model)
      || !all.cleanup_default_model
      || !splitModelId(all.cleanup_default_model);

    ensureDefaultAndFallbacks();

    const changed =
      needsMigration
      || transcriptionDefaultModel !== preTranscriptionDefault
      || cleanupDefaultModel !== preCleanupDefault
      || transcriptionFallbackModels.length !== preTranscriptionFallbackCount
      || cleanupFallbackModels.length !== preCleanupFallbackCount
      || JSON.stringify(cleanupFallbackModels) !== JSON.stringify(preCleanupFallbackModels)
      || cleanupModelMapChanged
      || false;

    if (changed) {
      await persistAll();
    }

    await Promise.all([
      refreshLocalModels(),
      refreshLocalState(),
      refreshLocalLlmModels(),
      refreshLocalLlmState(),
    ]);

    // Local model existence can only be checked once the stores above have
    // loaded (ensureDefaultAndFallbacks ran earlier, before that data was
    // available). Settings saved before a model was deleted — or written by
    // an older build — can still point at a local model that's no longer on
    // disk; self-heal that here instead of leaving a dead selection stuck
    // in the UI forever.
    validateLocalSelectionsAfterLoad();
  }

  function validateLocalSelectionsAfterLoad() {
    for (const type of ['transcription', 'cleanup'] as TaskType[]) {
      const localModels = type === 'transcription' ? localSttStore.models : localLlmStore.models;
      const downloadedIds = new Set(localModels.filter((model) => model.is_downloaded).map((model) => model.id));

      const missingLocalIds = [...new Set(
        [taskDefault(type), ...taskFallbacks(type)]
          .map((id) => splitModelId(id))
          .filter((parsed): parsed is { provider: ProviderId; model: string } => !!parsed && parsed.provider === 'local')
          .map((parsed) => parsed.model)
          .filter((modelName) => !downloadedIds.has(modelName)),
      )];

      for (const missingId of missingLocalIds) {
        reassignAfterLocalModelDeleted(type, missingId);
      }
    }
  }

  /** Makes a model the active one, pushing the previous default aside. */
  function selectModel(type: TaskType, id: string) {
    const parsed = splitModelId(id);
    if (!parsed) return;
    ensureModelsContainSelection(type, parsed.provider, parsed.model);
    // Picking a model that was a fallback promotes it rather than leaving it
    // in the chain twice.
    setTaskFallbacks(type, taskFallbacks(type).filter((entry) => entry !== id));
    setTaskDefault(type, id);
    persistAll().catch((err) => console.error('persist model selection failed', err));
  }

  function addFallbackModel(type: TaskType, id: string) {
    const parsed = splitModelId(id);
    if (!parsed) return;
    if (taskDefault(type) === id || taskFallbacks(type).includes(id)) return;
    ensureModelsContainSelection(type, parsed.provider, parsed.model);
    setTaskFallbacks(type, [...taskFallbacks(type), id]);
    persistAll().catch((err) => console.error('persist fallback add failed', err));
  }

  function removeFallbackModel(type: TaskType, id: string) {
    setTaskFallbacks(type, taskFallbacks(type).filter((entry) => entry !== id));
    persistAll().catch((err) => console.error('persist fallback removal failed', err));
  }

  function moveFallbackModel(type: TaskType, id: string, delta: -1 | 1) {
    const chain = [...taskFallbacks(type)];
    const index = chain.indexOf(id);
    const next = index + delta;
    if (index === -1 || next < 0 || next >= chain.length) return;
    [chain[index], chain[next]] = [chain[next], chain[index]];
    setTaskFallbacks(type, chain);
    persistAll().catch((err) => console.error('persist fallback reorder failed', err));
  }

  function setCustomDraft(type: TaskType, value: string) {
    customDrafts = { ...customDrafts, [type]: value };
  }

  /** Adds a hand-typed id and makes it active — you typed it to use it. */
  function addCustomModelForTask(type: TaskType, draft: string) {
    const raw = draft.trim();
    if (!raw) return;
    // A bare id with no provider prefix belongs to whoever is active now.
    const parsed = splitModelId(raw) ?? {
      provider: splitModelId(taskDefault(type))?.provider ?? 'groq',
      model: raw,
    };
    if (!parsed.model) return;
    ensureModelsContainSelection(type, parsed.provider, parsed.model);
    setCustomDraft(type, '');
    selectModel(type, modelId(parsed.provider, parsed.model));
  }

  async function handleAdvancedModelUi(value: boolean) {
    advancedModelUi = value;
    try {
      await saveSetting('advanced_model_ui', value);
    } catch (err) {
      advancedModelUi = !value;
      console.error('save advanced_model_ui failed:', err);
    }
  }

  let transcriptionModeDropdownOpen = $state(false);

  async function handleDualTranscription(value: boolean) {
    dualTranscriptionEnabled = value;
    transcriptionModeDropdownOpen = false;
    try {
      await saveSetting('dual_transcription_enabled', value);
    } catch (err) {
      dualTranscriptionEnabled = !value;
      console.error('save dual transcription setting failed:', err);
    }
  }

  async function updateLocalMemoryPolicy(policy: LocalModelMemoryPolicy) {
    localModelMemoryPolicy = policy;
    localModelMemoryDropdownOpen = false;
    try {
      await saveSetting('local_model_memory_policy', policy);
    } catch (err) {
      console.error('save local model memory policy failed', err);
    }
  }

  // Reached from "Manage local models" inside a task tile — revealing the
  // downloads necessarily closes the tile it was launched from, which is the
  // point: the user asked to go there.
  onMount(() => {
    refreshLocalModels().catch((err) => console.error('refresh local models failed', err));
    refreshLocalState().catch((err) => console.error('refresh local state failed', err));
    refreshLocalLlmModels().catch((err) => console.error('refresh local cleanup models failed', err));
    refreshLocalLlmState().catch((err) => console.error('refresh local cleanup state failed', err));
    refreshLocalLlmRuntimeInfo().catch((err) => console.error('refresh local cleanup runtime info failed', err));
    // Only ever act on an explicit `false` — an unrecognized/older backend
    // command, a transient error, or any other non-boolean response must
    // never hide the download UI for the overwhelming majority of users on
    // a supported platform. "Assume supported" is the only safe default.
    invoke<boolean>('local_models_supported_on_this_platform')
      .then((supported) => {
        if (supported === false) localModelsSupported = false;
      })
      .catch((err) => console.error('check local models platform support failed', err));
    getHardware()
      .then((hw) => (hardware = hw))
      .catch((err) => console.error('read hardware capabilities failed', err));

    const onKeySaved = async (event: Event) => {
      const provider = (event as CustomEvent<{ provider: ProviderId }>).detail?.provider;
      if (!provider) return;
      apiKeyStatus = { ...apiKeyStatus, [provider]: true };
      // Refresh on save, not on validation: validation checks an unsaved key
      // held in JS, and list_provider_models only ever reads the stored one.
      await refreshCatalog(provider, trackedModelIds());
    };
    window.addEventListener('verenu:api-key-saved', onKeySaved);
    return () => window.removeEventListener('verenu:api-key-saved', onKeySaved);
  });

  // Settings just opened: top up any provider whose list has gone stale, or
  // that failed long enough ago to be worth another try.
  $effect(() => {
    refreshStaleCatalogs(apiKeyStatus, trackedModelIds()).catch((err) =>
      console.error('refresh model catalogs failed', err),
    );
  });

  migrateAndLoad().catch((err) => console.error('load models failed', err));
</script>

<h2 class="settings-h">Models</h2>

<ModelPresetPicker
  {apiKeyStatus}
  {hardware}
  localSupported={localModelsSupported}
  {activeConfig}
  {installedLocal}
  {downloadingLocal}
  onApplyPreset={applyPreset}
  onOpenApiKeys={openApiKeysSection}
  onCancelPreset={cancelPresetDownload}
  onDeletePreset={deletePresetModels}
/>

<div class="advanced-toggle-row">
  <div class="adv-text">
    <span class="adv-label">Advanced Models</span>
    <span class="adv-desc">Choose specific models, edit cleanup prompts, and manage downloads</span>
  </div>
  <Toggle checked={advancedModelUi} onchange={handleAdvancedModelUi} label="Advanced Models" />
</div>

{#if advancedModelUi}
<div class="advanced-block" transition:slide={{ duration: motionMs(MOTION_MS.base), easing: cubicOut }}>
<h3 class="settings-subhead">Model selection</h3>
<ModelTaskTile
  type="transcription"
  {advancedModelUi}
  {apiKeyStatus}
  context={pickerContext('transcription')}
  defaultModel={transcriptionDefaultModel}
  fallbackModels={transcriptionFallbackModels}
  customDraft={customDrafts.transcription}
  onSelectModel={selectModel}
  onAddFallback={addFallbackModel}
  onRemoveFallback={removeFallbackModel}
  onMoveFallback={moveFallbackModel}
  onCustomDraftChange={setCustomDraft}
  onAddCustomModel={addCustomModelForTask}
  onOpenApiKeys={openApiKeysSection}
  local={localControls('transcription')}
/>

<ModelTaskTile
  type="cleanup"
  {advancedModelUi}
  {apiKeyStatus}
  context={pickerContext('cleanup')}
  defaultModel={cleanupDefaultModel}
  fallbackModels={cleanupFallbackModels}
  customDraft={customDrafts.cleanup}
  onSelectModel={selectModel}
  onAddFallback={addFallbackModel}
  onRemoveFallback={removeFallbackModel}
  onMoveFallback={moveFallbackModel}
  onCustomDraftChange={setCustomDraft}
  onAddCustomModel={addCustomModelForTask}
  onOpenApiKeys={openApiKeysSection}
  local={localControls('cleanup')}
/>

<h3 class="settings-subhead">Model settings</h3>
<div class="setting-row transcription-mode-row">
  <div>
    <div class="label">Transcription strategy</div>
    <div class="desc">Use one model, or compare two working models from the existing transcription fallback chain before cleanup.</div>
  </div>
  <Dropdown bind:open={transcriptionModeDropdownOpen} closeSelector=".models-dropdown">
    <div class="ui-dropdown models-dropdown">
      <button
        class="btn-ghost ui-dropdown-trigger models-dropdown-btn"
        type="button"
        onclick={() => (transcriptionModeDropdownOpen = !transcriptionModeDropdownOpen)}
        aria-haspopup="listbox"
        aria-expanded={transcriptionModeDropdownOpen}
        aria-controls="transcription-mode-menu"
        aria-label="Transcription strategy"
      >
        <span>{dualTranscriptionEnabled ? 'Dual model' : 'Single model'}</span>
        <svg class:open={transcriptionModeDropdownOpen} width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="m6 9 6 6 6-6"/>
        </svg>
      </button>
      {#if transcriptionModeDropdownOpen}
        <div
          id="transcription-mode-menu"
          class="ui-dropdown-menu models-dropdown-menu scroll-styled scroll-thumb-elev"
          role="listbox"
          tabindex="-1"
          aria-label="Transcription strategy"
          in:fly={{ y: -motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.panel), easing: expoOut }}
          out:fade={{ duration: motionMs(MOTION_MS.fast) }}
        >
          <button
            class="ui-dropdown-option models-dropdown-item"
            class:is-active={!dualTranscriptionEnabled}
            type="button"
            onclick={() => handleDualTranscription(false)}
            role="option"
            aria-selected={!dualTranscriptionEnabled}
          >
            <span>Single model</span>
            <small>Fastest, uses the primary model and fallbacks only on failure.</small>
          </button>
          <button
            class="ui-dropdown-option models-dropdown-item"
            class:is-active={dualTranscriptionEnabled}
            type="button"
            onclick={() => handleDualTranscription(true)}
            role="option"
            aria-selected={dualTranscriptionEnabled}
          >
            <span>Dual model</span>
            <small>Compares two successful fallback-chain models before cleanup.</small>
          </button>
        </div>
      {/if}
    </div>
  </Dropdown>
</div>
</div>
{/if}

<div class="setting-row">
  <div>
    <div class="label">Memory policy</div>
    <div class="desc">Controls when idle local models are unloaded.</div>
  </div>
  <Dropdown bind:open={localModelMemoryDropdownOpen} closeSelector=".models-dropdown">
    <div class="ui-dropdown models-dropdown">
      <button
        class="btn-ghost ui-dropdown-trigger models-dropdown-btn"
        type="button"
        use:animateWidth={{ text: localMemoryPolicyLabel(localModelMemoryPolicy), max: 220 }}
        onclick={() => (localModelMemoryDropdownOpen = !localModelMemoryDropdownOpen)}
        aria-haspopup="listbox"
        aria-expanded={localModelMemoryDropdownOpen}
        aria-controls={LOCAL_MEMORY_POLICY_MENU_ID}
        aria-label="Local model memory policy"
      >
        <span>{localMemoryPolicyLabel(localModelMemoryPolicy)}</span>
        <svg class:open={localModelMemoryDropdownOpen} width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="m6 9 6 6 6-6"/>
        </svg>
      </button>
      {#if localModelMemoryDropdownOpen}
        <div
          id={LOCAL_MEMORY_POLICY_MENU_ID}
          class="ui-dropdown-menu models-dropdown-menu scroll-styled scroll-thumb-elev"
          role="listbox"
          tabindex="-1"
          aria-label="Local model memory policy options"
          in:fly={{ y: -motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.panel), easing: expoOut }}
          out:fade={{ duration: motionMs(MOTION_MS.fast) }}
        >
          {#each localMemoryPolicyOptions as option}
            <button
              class="ui-dropdown-option models-dropdown-item"
              class:is-active={localModelMemoryPolicy === option.value}
              type="button"
              onclick={() => updateLocalMemoryPolicy(option.value)}
              role="option"
              aria-selected={localModelMemoryPolicy === option.value}
            >
              {option.label}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </Dropdown>
</div>

<div class="setting-row">
  <div>
    <div class="label">Models folder</div>
    <div class="desc">Open the shared folder where local transcription and cleanup models are stored.</div>
  </div>
  <button class="btn-ghost" type="button" onclick={openLocalModelsFolder}>Open models folder</button>
</div>

<style>




  /* Only a top border — the row below it (the advanced block's first tile when
     expanded, or the Memory policy row when collapsed) supplies its own top
     border, so a bottom border here would double up into one thick line. */
  .advanced-toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 13px 0;
    border-top: 1px solid var(--line);
  }

  .transcription-mode-row {
    position: relative;
    padding: 14px 0;
  }

  .models-dropdown-item {
    display: grid;
    gap: 3px;
    text-align: left;
  }

  .models-dropdown-item small {
    color: var(--ink-mute);
    font-size: 11px;
    line-height: 1.35;
  }

  .adv-text {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .adv-label {
    font-size: 13px;
    font-family: var(--sans);
    font-weight: 500;
    color: var(--ink-soft);
  }

  .adv-desc {
    font-size: 11px;
    font-family: var(--sans);
    color: var(--ink-mute);
  }

  .models-dropdown-btn {
    max-width: 220px;
  }

  .models-dropdown-btn span:first-child {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .models-dropdown-menu {
    min-width: 220px;
    max-height: 220px;
  }

  /* Keyed to the settings content column, not the window — this stacks when the
     panel itself is narrow, which is what the rule was always standing in for. */
  @container settings-panel (max-width: 700px) {
    .models-dropdown {
      width: 100%;
    }

    .models-dropdown-btn {
      width: 100%;
      max-width: none;
      justify-content: space-between;
    }

    .models-dropdown-menu {
      min-width: 100%;
    }
  }
</style>
