<script lang="ts">
  import { invoke } from '../../tauri';
  import Toggle from '../Toggle.svelte';
  import { cleanupPromptOverridesStore } from '../../stores.svelte';
  import { saveSetting, type ProviderId, type ProviderModelMap } from '../../settings';
  import ModelTaskTile from './ModelTaskTile.svelte';
  import {
    emptyProviderModelMap,
    mergeProviderModelMap,
    modelId,
    recommendedModels,
    splitModelId,
    type AllSettingsPayload,
    type TaskType,
    type UiProviderId,
  } from './models';

  let apiKeyStatus = $state<Record<ProviderId, boolean>>({ groq: false, openai: false, google: false });

  let transcriptionModelsByProvider = $state<ProviderModelMap>(emptyProviderModelMap());
  let cleanupModelsByProvider = $state<ProviderModelMap>(emptyProviderModelMap());

  let transcriptionDefaultModel = $state('groq/whisper-large-v3-turbo');
  let cleanupDefaultModel = $state('groq/llama-3.3-70b-versatile');
  let transcriptionFallbackModels = $state<string[]>([]);
  let cleanupFallbackModels = $state<string[]>([]);

  let customDrafts = $state<Record<TaskType, Record<UiProviderId, string>>>({
    transcription: { groq: '', openai: '', google: '' },
    cleanup: { groq: '', openai: '', google: '' },
  });

  let transcriptionOpen = $state(false);
  let cleanupOpen = $state(false);
  let advancedModelUi = $state(false);

  function isOpen(type: TaskType) {
    return type === 'transcription' ? transcriptionOpen : cleanupOpen;
  }

  function toggleTaskOpen(type: TaskType) {
    if (type === 'transcription') {
      transcriptionOpen = !transcriptionOpen;
    } else {
      cleanupOpen = !cleanupOpen;
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

  async function persistAll() {
    ensureDefaultAndFallbacks();
    const transcriptionProvider = splitModelId(transcriptionDefaultModel)?.provider ?? 'groq';
    const cleanupProvider = splitModelId(cleanupDefaultModel)?.provider ?? 'groq';

    await Promise.all([
      saveSetting('transcription_models_by_provider', transcriptionModelsByProvider),
      saveSetting('cleanup_models_by_provider', cleanupModelsByProvider),
      saveSetting('transcription_default_model', transcriptionDefaultModel),
      saveSetting('cleanup_default_model', cleanupDefaultModel),
      saveSetting('transcription_fallback_models', transcriptionFallbackModels),
      saveSetting('cleanup_fallback_models', cleanupFallbackModels),
      saveSetting('transcription_model', transcriptionDefaultModel),
      saveSetting('cleanup_model', cleanupDefaultModel),
      saveSetting('transcription_provider', transcriptionProvider),
      saveSetting('cleanup_provider', cleanupProvider),
    ]);
  }

  async function migrateAndLoad() {
    const [all, keyStatus, advancedRaw] = await Promise.all([
      invoke<AllSettingsPayload>('get_all_settings'),
      invoke<Record<ProviderId, boolean>>('get_api_key_status'),
      invoke<boolean | null>('get_setting', { key: 'advanced_model_ui' }),
    ]);

    apiKeyStatus = keyStatus;

    transcriptionModelsByProvider = mergeProviderModelMap(all.transcription_models_by_provider);
    cleanupModelsByProvider = mergeProviderModelMap(all.cleanup_models_by_provider);

    const rawLegacyTranscription = String(all.transcription_model ?? '');
    const rawLegacyCleanup = String(all.cleanup_model ?? '');
    const legacyTranscription = rawLegacyTranscription
      ? (rawLegacyTranscription.includes('/') ? rawLegacyTranscription : `groq/${rawLegacyTranscription}`)
      : 'groq/whisper-large-v3-turbo';
    const legacyCleanup = rawLegacyCleanup
      ? (rawLegacyCleanup.includes('/') ? rawLegacyCleanup : `groq/${rawLegacyCleanup}`)
      : 'groq/llama-3.3-70b-versatile';

    const transcriptionDefaultRaw = all.transcription_default_model ?? null;
    const cleanupDefaultRaw = all.cleanup_default_model ?? null;

    transcriptionDefaultModel = transcriptionDefaultRaw !== null ? transcriptionDefaultRaw : legacyTranscription;
    cleanupDefaultModel = cleanupDefaultRaw !== null ? cleanupDefaultRaw : legacyCleanup;

    if (Array.isArray(all.transcription_fallback_models)) {
      transcriptionFallbackModels = all.transcription_fallback_models.filter((id) => !!splitModelId(id));
    }
    if (Array.isArray(all.cleanup_fallback_models)) {
      cleanupFallbackModels = all.cleanup_fallback_models.filter((id) => !!splitModelId(id));
    }
    if (typeof advancedRaw === 'boolean') {
      advancedModelUi = advancedRaw;
    }

    if (all.cleanup_prompt_overrides && typeof all.cleanup_prompt_overrides === 'object') {
      const overrides: Record<string, string> = {};
      for (const [key, value] of Object.entries(all.cleanup_prompt_overrides as Record<string, unknown>)) {
        if (typeof value === 'string') {
          overrides[key] = value;
        }
      }
      cleanupPromptOverridesStore.overrides = overrides;
    }

    const preTranscriptionDefault = transcriptionDefaultModel;
    const preCleanupDefault = cleanupDefaultModel;
    const preTranscriptionFallbackCount = transcriptionFallbackModels.length;
    const preCleanupFallbackCount = cleanupFallbackModels.length;
    const needsMigration = !transcriptionDefaultRaw || !splitModelId(transcriptionDefaultRaw) || !cleanupDefaultRaw || !splitModelId(cleanupDefaultRaw);

    ensureDefaultAndFallbacks();

    const changed =
      needsMigration
      || transcriptionDefaultModel !== preTranscriptionDefault
      || cleanupDefaultModel !== preCleanupDefault
      || transcriptionFallbackModels.length !== preTranscriptionFallbackCount
      || cleanupFallbackModels.length !== preCleanupFallbackCount;

    if (changed) {
      await persistAll();
    }
  }

  function updateCustomDraft(type: TaskType, provider: UiProviderId, value: string) {
    customDrafts = {
      ...customDrafts,
      [type]: {
        ...customDrafts[type],
        [provider]: value,
      },
    };
  }

  function addCustomModel(type: TaskType, provider: ProviderId, draft: string) {
    let custom = draft.trim();
    if (!custom) return;

    let targetProvider = provider;
    const ownPrefix = `${provider}/`;
    if (custom.toLowerCase().startsWith(ownPrefix)) {
      custom = custom.slice(ownPrefix.length).trim();
    } else if (custom.includes('/')) {
      const parsed = splitModelId(custom);
      if (!parsed) return;
      targetProvider = parsed.provider;
      custom = parsed.model;
    }
    if (!custom) return;

    ensureModelsContainSelection(type, targetProvider, custom);
    const sectionId = provider as UiProviderId;
    customDrafts = {
      ...customDrafts,
      [type]: {
        ...customDrafts[type],
        [sectionId]: '',
      },
    };

    persistAll().catch((err) => console.error('persist custom model failed', err));
  }

  function removeCustomModel(type: TaskType, provider: ProviderId, modelName: string) {
    const id = modelId(provider, modelName);

    if (taskFallbacks(type).includes(id)) {
      setTaskFallbacks(type, taskFallbacks(type).filter((entry) => entry !== id));
    }

    if (taskDefault(type) === id) {
      const fallbacks = taskFallbacks(type);
      if (fallbacks.length > 0) {
        const [nextActive, ...remaining] = fallbacks;
        setTaskDefault(type, nextActive);
        setTaskFallbacks(type, remaining);
      } else {
        const providerId = provider as UiProviderId;
        setTaskDefault(type, modelId(provider, recommendedModels[type][providerId].standard));
      }
    }

    const map = taskMap(type);
    setTaskMap(type, { ...map, [provider]: map[provider].filter((model) => model !== modelName) });

    persistAll().catch((err) => console.error('persist custom model removal failed', err));
  }

  function toggleModelSelection(type: TaskType, provider: ProviderId, modelName: string) {
    const id = modelId(provider, modelName);
    ensureModelsContainSelection(type, provider, modelName);

    if (taskDefault(type) === id) {
      const fallbacks = taskFallbacks(type);
      if (fallbacks.length > 0) {
        const [nextActive, ...remaining] = fallbacks;
        setTaskDefault(type, nextActive);
        setTaskFallbacks(type, remaining);
      } else {
        const providerId = provider as UiProviderId;
        setTaskDefault(type, modelId(provider, recommendedModels[type][providerId].standard));
      }
    } else if (taskFallbacks(type).includes(id)) {
      setTaskFallbacks(type, taskFallbacks(type).filter((entry) => entry !== id));
    } else if (!splitModelId(taskDefault(type))) {
      setTaskDefault(type, id);
    } else {
      setTaskFallbacks(type, [...taskFallbacks(type), id]);
    }

    persistAll().catch((err) => console.error('persist model toggle failed', err));
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

  migrateAndLoad().catch((err) => console.error('load models failed', err));
</script>

<h2 class="settings-h">Models</h2>

{#each ['transcription', 'cleanup'] as rawType}
  {@const type = rawType as TaskType}
  <ModelTaskTile
    {type}
    opened={isOpen(type)}
    {advancedModelUi}
    {apiKeyStatus}
    modelsByProvider={taskMap(type)}
    defaultModel={taskDefault(type)}
    fallbackModels={taskFallbacks(type)}
    customDrafts={customDrafts[type]}
    onToggleOpen={toggleTaskOpen}
    onToggleModel={toggleModelSelection}
    onRemoveCustomModel={removeCustomModel}
    onCustomDraftChange={updateCustomDraft}
    onAddCustomModel={addCustomModel}
  />
{/each}

<div class="advanced-toggle-row">
  <div class="adv-text">
    <span class="adv-label">Advanced Models</span>
    <span class="adv-desc">Edit cleanup prompts and add custom models per provider</span>
  </div>
  <Toggle checked={advancedModelUi} onchange={handleAdvancedModelUi} label="Advanced Models" />
</div>

<style>
  .advanced-toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 13px 0;
    border-top: 1px solid var(--line);
    border-bottom: 1px solid var(--line);
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
</style>
