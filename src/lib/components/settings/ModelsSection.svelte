<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { saveSetting, type ProviderId, type ProviderModelMap } from '../../settings';

  type TaskType = 'transcription' | 'cleanup';
  type UiProviderId = 'groq' | 'openai' | 'google';

  type ProviderSection = {
    id: UiProviderId;
    label: string;
    storeProvider: ProviderId;
  };

  const providerSections: ProviderSection[] = [
    { id: 'groq', label: 'Groq', storeProvider: 'groq' },
    { id: 'openai', label: 'OpenAI', storeProvider: 'openai' },
    { id: 'google', label: 'Google', storeProvider: 'google' },
  ];

  const recommendedModels: Record<TaskType, Record<UiProviderId, { premium: string; standard: string }>> = {
    transcription: {
      groq: { premium: 'whisper-large-v3', standard: 'whisper-large-v3-turbo' },
      openai: { premium: 'gpt-4o-transcribe', standard: 'gpt-4o-mini-transcribe' },
      google: { premium: 'gemini-3.5-flash', standard: 'gemini-2.5-flash' },
    },
    cleanup: {
      groq: { premium: 'llama-3.3-70b-versatile', standard: 'llama-3.1-8b-instant' },
      openai: { premium: 'gpt-4o', standard: 'gpt-4o-mini' },
      google: { premium: 'gemini-3.5-flash', standard: 'gemini-2.5-flash' },
    },
  };

  const emptyMap = (): ProviderModelMap => ({ groq: [], openai: [], google: [] });

  let apiKeyStatus = $state({ groq: false, openai: false, google: false });

  let transcriptionModelsByProvider = $state<ProviderModelMap>(emptyMap());
  let cleanupModelsByProvider = $state<ProviderModelMap>(emptyMap());

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

  function isOpen(type: TaskType) { return type === 'transcription' ? transcriptionOpen : cleanupOpen; }
  function openTile(type: TaskType) { if (type === 'transcription') transcriptionOpen = true; else cleanupOpen = true; }
  function closeTile(type: TaskType) { if (type === 'transcription') transcriptionOpen = false; else cleanupOpen = false; }

  function modelId(provider: ProviderId, modelName: string): string {
    return `${provider}/${modelName.trim()}`;
  }

  function splitModelId(id: string): { provider: ProviderId; model: string } | null {
    const idx = id.indexOf('/');
    if (idx <= 0) return null;
    const provider = id.slice(0, idx) as ProviderId;
    const model = id.slice(idx + 1).trim();
    if (!['groq', 'openai', 'google'].includes(provider) || !model) return null;
    return { provider, model };
  }

  function taskMap(type: TaskType): ProviderModelMap {
    return type === 'transcription' ? transcriptionModelsByProvider : cleanupModelsByProvider;
  }

  function setTaskMap(type: TaskType, next: ProviderModelMap) {
    if (type === 'transcription') transcriptionModelsByProvider = next;
    else cleanupModelsByProvider = next;
  }

  function taskDefault(type: TaskType): string {
    return type === 'transcription' ? transcriptionDefaultModel : cleanupDefaultModel;
  }

  function setTaskDefault(type: TaskType, value: string) {
    if (type === 'transcription') transcriptionDefaultModel = value;
    else cleanupDefaultModel = value;
  }

  function taskFallbacks(type: TaskType): string[] {
    return type === 'transcription' ? transcriptionFallbackModels : cleanupFallbackModels;
  }

  function setTaskFallbacks(type: TaskType, next: string[]) {
    if (type === 'transcription') transcriptionFallbackModels = next;
    else cleanupFallbackModels = next;
  }

  function selectionState(type: TaskType, provider: ProviderId, model: string): 'active' | 'fallback' | 'none' {
    const id = modelId(provider, model);
    if (taskDefault(type) === id) return 'active';
    if (taskFallbacks(type).includes(id)) return 'fallback';
    return 'none';
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
        .filter((id, index, arr) => arr.indexOf(id) === index)
        .filter((id) => id !== taskDefault(type));

      setTaskFallbacks(type, normalizedFallbacks);
    }
  }

  async function persistAll() {
    ensureDefaultAndFallbacks();
    const tProvider = splitModelId(transcriptionDefaultModel)?.provider ?? 'groq';
    const cProvider = splitModelId(cleanupDefaultModel)?.provider ?? 'groq';

    await Promise.all([
      saveSetting('transcription_models_by_provider', transcriptionModelsByProvider),
      saveSetting('cleanup_models_by_provider', cleanupModelsByProvider),
      saveSetting('transcription_default_model', transcriptionDefaultModel),
      saveSetting('cleanup_default_model', cleanupDefaultModel),
      saveSetting('transcription_fallback_models', transcriptionFallbackModels),
      saveSetting('cleanup_fallback_models', cleanupFallbackModels),
      saveSetting('transcription_model', transcriptionDefaultModel),
      saveSetting('cleanup_model', cleanupDefaultModel),
      saveSetting('transcription_provider', tProvider),
      saveSetting('cleanup_provider', cProvider),
    ]);
  }

  async function migrateAndLoad() {
    const [all, keyStatus, tMapRaw, cMapRaw, tDefaultRaw, cDefaultRaw, tFallbackRaw, cFallbackRaw, advancedRaw] =
      await Promise.all([
        invoke<Record<string, unknown>>('get_all_settings'),
        invoke<typeof apiKeyStatus>('get_api_key_status'),
        invoke<unknown>('get_setting', { key: 'transcription_models_by_provider' }),
        invoke<unknown>('get_setting', { key: 'cleanup_models_by_provider' }),
        invoke<string | null>('get_setting', { key: 'transcription_default_model' }),
        invoke<string | null>('get_setting', { key: 'cleanup_default_model' }),
        invoke<string[] | null>('get_setting', { key: 'transcription_fallback_models' }),
        invoke<string[] | null>('get_setting', { key: 'cleanup_fallback_models' }),
        invoke<boolean | null>('get_setting', { key: 'advanced_model_ui' }),
      ]);

    apiKeyStatus = keyStatus;

    const mergeMap = (raw: unknown): ProviderModelMap => {
      const base = emptyMap();
      if (raw && typeof raw === 'object') {
        for (const provider of ['groq', 'openai', 'google'] as ProviderId[]) {
          const values = (raw as Record<string, unknown>)[provider];
          if (Array.isArray(values)) {
            base[provider] = values.map((v) => String(v).trim()).filter(Boolean);
          }
        }
      }
      return base;
    };

    transcriptionModelsByProvider = mergeMap(tMapRaw);
    cleanupModelsByProvider = mergeMap(cMapRaw);

    const rawLegacyT = String((all as Record<string, unknown>).transcription_model ?? '');
    const rawLegacyC = String((all as Record<string, unknown>).cleanup_model ?? '');
    const legacyT = rawLegacyT
      ? (rawLegacyT.includes('/') ? rawLegacyT : `groq/${rawLegacyT}`)
      : 'groq/whisper-large-v3-turbo';
    const legacyC = rawLegacyC
      ? (rawLegacyC.includes('/') ? rawLegacyC : `groq/${rawLegacyC}`)
      : 'groq/llama-3.3-70b-versatile';

    transcriptionDefaultModel = tDefaultRaw && splitModelId(tDefaultRaw) ? tDefaultRaw : legacyT;
    cleanupDefaultModel = cDefaultRaw && splitModelId(cDefaultRaw) ? cDefaultRaw : legacyC;

    if (Array.isArray(tFallbackRaw)) transcriptionFallbackModels = tFallbackRaw.filter((m) => !!splitModelId(m));
    if (Array.isArray(cFallbackRaw)) cleanupFallbackModels = cFallbackRaw.filter((m) => !!splitModelId(m));
    if (typeof advancedRaw === 'boolean') advancedModelUi = advancedRaw;

    const preT = transcriptionDefaultModel;
    const preC = cleanupDefaultModel;
    const preTFb = transcriptionFallbackModels.length;
    const preCFb = cleanupFallbackModels.length;
    const needsMigration = !tDefaultRaw || !splitModelId(tDefaultRaw) || !cDefaultRaw || !splitModelId(cDefaultRaw);

    ensureDefaultAndFallbacks();

    const changed =
      needsMigration ||
      transcriptionDefaultModel !== preT ||
      cleanupDefaultModel !== preC ||
      transcriptionFallbackModels.length !== preTFb ||
      cleanupFallbackModels.length !== preCFb;

    if (changed) await persistAll();
  }

  function activateModel(type: TaskType, provider: ProviderId, modelName: string, addAsFallback: boolean) {
    const id = modelId(provider, modelName);
    ensureModelsContainSelection(type, provider, modelName);

    if (addAsFallback) {
      if (id !== taskDefault(type) && !taskFallbacks(type).includes(id)) {
        setTaskFallbacks(type, [...taskFallbacks(type), id]);
      }
    } else {
      setTaskDefault(type, id);
      setTaskFallbacks(type, taskFallbacks(type).filter((m) => m !== id));
    }

    persistAll().catch((err) => console.error('persist model failed', err));
  }

  function addCustomToList(type: TaskType, section: ProviderSection) {
    let custom = customDrafts[type][section.id].trim();
    if (!custom) return;
    let targetProvider: ProviderId = section.storeProvider;
    const ownPrefix = `${section.storeProvider}/`;
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
    customDrafts = { ...customDrafts, [type]: { ...customDrafts[type], [section.id]: '' } };
    persistAll().catch((err) => console.error('persist custom failed', err));
  }

  function removeFallback(type: TaskType, id: string) {
    setTaskFallbacks(type, taskFallbacks(type).filter((m) => m !== id));
    persistAll().catch((err) => console.error('persist fallback remove failed', err));
  }

  function toggleModelSelection(type: TaskType, provider: ProviderId, modelName: string) {
    const id = modelId(provider, modelName);
    const currentState = selectionState(type, provider, modelName);

    ensureModelsContainSelection(type, provider, modelName);

    if (currentState === 'active') {
      const fallbacks = taskFallbacks(type);
      if (fallbacks.length > 0) {
        const [nextActive, ...remaining] = fallbacks;
        setTaskDefault(type, nextActive);
        setTaskFallbacks(type, remaining);
      } else {
        setTaskDefault(type, '');
      }
      persistAll().catch((err) => console.error('persist model toggle failed', err));
      return;
    }

    if (currentState === 'fallback') {
      setTaskFallbacks(type, taskFallbacks(type).filter((m) => m !== id));
      persistAll().catch((err) => console.error('persist model toggle failed', err));
      return;
    }

    if (!splitModelId(taskDefault(type))) {
      setTaskDefault(type, id);
    } else {
      setTaskFallbacks(type, [...taskFallbacks(type), id]);
    }

    persistAll().catch((err) => console.error('persist model toggle failed', err));
  }

  function missingKeyWarning(type: TaskType): string {
    const ids = [taskDefault(type), ...taskFallbacks(type)];
    const missing = ids
      .map((id) => splitModelId(id)?.provider)
      .filter((p): p is ProviderId => !!p)
      .filter((p, idx, arr) => arr.indexOf(p) === idx)
      .filter((p) => !apiKeyStatus[p]);
    return missing.length ? `Missing API keys for: ${missing.join(', ')}` : '';
  }

  function activeProviderLabel(type: TaskType): string {
    const parsed = splitModelId(taskDefault(type));
    return parsed ? parsed.provider.charAt(0).toUpperCase() + parsed.provider.slice(1) : 'None';
  }

  function activeModelLabel(type: TaskType): string {
    return splitModelId(taskDefault(type))?.model ?? 'None';
  }

  migrateAndLoad().catch((err) => console.error('load models failed', err));
</script>

<h2 class="settings-h">Models</h2>

{#each ['transcription', 'cleanup'] as rawType}
  {@const type = rawType as TaskType}
  {@const opened = isOpen(type)}
  {@const fallbackCount = taskFallbacks(type).length}

  <div class="task-tile" class:task-open={opened}>
    <button class="tile-head" onclick={() => { if (opened) closeTile(type); else openTile(type); }}
      aria-expanded={opened}>
      <div class="head-left">
        <span class="head-title">{type === 'transcription' ? 'Transcription' : 'Clean-up'}</span>
        <div class="summary-row">
          <span class="summary-item provider-chip">{activeProviderLabel(type)}</span>
          <span class="summary-item model-chip">{activeModelLabel(type)}</span>
          {#if advancedModelUi}
            <span class="summary-item fallback-chip">{fallbackCount} fallback{fallbackCount !== 1 ? 's' : ''}</span>
          {/if}
        </div>
      </div>
      <span class="chevron" class:chevron-open={opened} aria-hidden="true"></span>
    </button>

    {#if opened}
      <div class="tile-inner" in:fly={{ y: 8, duration: 220, easing: cubicOut }}>
        {#if missingKeyWarning(type)}
          <div class="warn-banner">{missingKeyWarning(type)}</div>
        {/if}

        {#if advancedModelUi}
          <!-- ── Advanced picker ── -->
          <div class="chain-bar">
            {#each providerSections as section (section.id)}
              {@const hasKey = apiKeyStatus[section.storeProvider]}
              <div class="simple-group" class:no-key={!hasKey}>
                <span class="simple-provider">{section.label}</span>
                {#each ['premium', 'standard'] as rawTier}
                  {@const tier = rawTier as 'premium' | 'standard'}
                  {@const mName = recommendedModels[type][section.id][tier]}
                  {@const state = selectionState(type, section.storeProvider, mName)}
                  {@const isActive = state === 'active'}
                  {@const isFallback = state === 'fallback'}
                  <button
                    class="simple-row"
                    class:simple-active={isActive}
                    class:simple-fallback={isFallback}
                    onclick={() => toggleModelSelection(type, section.storeProvider, mName)}
                  >
                    <span class="simple-dot" class:dot-active={isActive} class:dot-fallback={isFallback}></span>
                    <span class="simple-name">{mName}</span>
                    <span class="simple-badge {isFallback ? 'badge-fallback' : (tier === 'premium' ? 'badge-accuracy' : 'badge-efficiency')}">
                      {isActive ? 'Active' : (isFallback ? `F${taskFallbacks(type).indexOf(modelId(section.storeProvider, mName)) + 1}` : (tier === 'premium' ? 'Accuracy' : 'Efficiency'))}
                    </span>
                  </button>
                {/each}
              </div>
            {/each}
          </div>
        {:else}
          <!-- ── Simple picker ── -->
          <div class="simple-picker">
            <div class="chain-row-item">
              <span class="chain-label">Active</span>
              <span class="chain-chip active-chip">{taskDefault(type)}</span>
            </div>
            {#each taskFallbacks(type) as id, i (id)}
              <div class="chain-row-item">
                <span class="chain-label">F{i + 1}</span>
                <span class="chain-row">
                  <span class="chain-id">{id}</span>
                  <button
                    class="chain-remove"
                    onclick={() => removeFallback(type, id)}
                    aria-label="Remove fallback"
                  >×</button>
                </span>
              </div>
            {/each}
            {#if taskFallbacks(type).length === 0}
              <div class="chain-row-item">
                <span class="chain-label">F1</span>
                <span class="chain-empty">No fallbacks configured</span>
              </div>
            {/if}
          </div>

          <div class="provider-grid">
            {#each providerSections as section (section.id)}
              {@const hasKey = apiKeyStatus[section.storeProvider]}
              <div class="provider-card" class:no-key={!hasKey}>
                <div class="prov-head">
                  <span class="prov-name">{section.label}</span>
                  {#if !hasKey}
                    <span class="prov-badge">No key</span>
                  {/if}
                </div>

                <div class="model-list">
                  {#each ['premium', 'standard'] as rawTier}
                    {@const tier = rawTier as 'premium' | 'standard'}
                    {@const mName = recommendedModels[type][section.id][tier]}
                    {@const state = selectionState(type, section.storeProvider, mName)}
                    <div
                      class="model-row"
                      class:row-active={state === 'active'}
                      class:row-fallback={state === 'fallback'}
                      onclick={() => toggleModelSelection(type, section.storeProvider, mName)}
                      role="button"
                      tabindex="0"
                      onkeydown={(e) => e.key === 'Enter' && toggleModelSelection(type, section.storeProvider, mName)}
                    >
                      <div class="row-info">
                        <span class="row-tier">{tier === 'premium' ? 'Premium' : 'Standard'}</span>
                        <span class="model-name">{mName}</span>
                      </div>
                      {#if state === 'active'}
                        <span class="state-pill active">Active</span>
                      {:else if state === 'fallback'}
                        <span class="state-pill fallback">F{taskFallbacks(type).indexOf(modelId(section.storeProvider, mName)) + 1}</span>
                      {:else}
                        <span class="state-pill muted">Off</span>
                      {/if}
                    </div>
                  {/each}

                  {#each (taskMap(type)[section.storeProvider] ?? []).filter(m => m !== recommendedModels[type][section.id].premium && m !== recommendedModels[type][section.id].standard) as custom (custom)}
                    {@const state = selectionState(type, section.storeProvider, custom)}
                    <div
                      class="model-row custom-model-row"
                      class:row-active={state === 'active'}
                      class:row-fallback={state === 'fallback'}
                      onclick={() => toggleModelSelection(type, section.storeProvider, custom)}
                      role="button"
                      tabindex="0"
                      onkeydown={(e) => e.key === 'Enter' && toggleModelSelection(type, section.storeProvider, custom)}
                    >
                      <div class="row-info">
                        <span class="row-tier">Custom</span>
                        <span class="model-name">{custom}</span>
                      </div>
                      {#if state === 'active'}
                        <span class="state-pill active">Active</span>
                      {:else if state === 'fallback'}
                        <span class="state-pill fallback">F{taskFallbacks(type).indexOf(modelId(section.storeProvider, custom)) + 1}</span>
                      {:else}
                        <span class="state-pill muted">Off</span>
                      {/if}
                    </div>
                  {/each}
                </div>

                <div class="custom-row">
                  <input
                    class="model-input"
                    placeholder="custom model…"
                    value={customDrafts[type][section.id]}
                    oninput={(e) => {
                      const v = (e.currentTarget as HTMLInputElement).value;
                      customDrafts = { ...customDrafts, [type]: { ...customDrafts[type], [section.id]: v } };
                    }}
                    onkeydown={(e) => { if (e.key === 'Enter') { e.stopPropagation(); addCustomToList(type, section); } }}
                  />
                  <button
                    class="custom-add-btn"
                    onclick={() => addCustomToList(type, section)}
                    disabled={!customDrafts[type][section.id].trim()}
                  >Add</button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </div>
{/each}

<style>
  /* ── Task tile ───────────────────────────── */
  .task-tile {
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--bg-elev);
    margin-bottom: 10px;
    overflow: hidden;
    transition: border-color 200ms ease;
  }

  .task-tile.task-open {
    border-color: var(--line-strong);
  }

  /* ── Tile header ─────────────────────────── */
  .tile-head {
    width: 100%;
    border: none;
    outline: none;
    background: transparent;
    padding: 13px 16px;
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
    background: color-mix(in srgb, var(--paper) 30%, var(--bg-elev));
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
    border-top: 1px solid var(--line);
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    background: color-mix(in srgb, var(--paper) 20%, var(--bg-elev));
  }

  /* ── Simple picker ──────────────────────── */
  .simple-picker {
    display: flex;
    flex-direction: column;
  }

  .simple-group {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 12px 0;
    transition: opacity 200ms ease;
    position: relative;
  }

  .simple-group.no-key { opacity: 0.45; }

  .simple-group + .simple-group::before {
    content: '';
    position: absolute;
    top: 0;
    left: 6%;
    right: 6%;
    height: 1px;
    background: linear-gradient(
      to right,
      transparent,
      var(--line-strong) 30%,
      var(--line-strong) 70%,
      transparent
    );
  }

  .simple-provider {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.09em;
    font-family: var(--sans);
    font-weight: 700;
    color: var(--ink-faint);
    padding: 0 10px;
    margin-bottom: 3px;
  }

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
    transition: background 140ms ease, border-color 140ms ease;
  }

  .simple-row:hover:not(.simple-active) {
    background: color-mix(in srgb, var(--paper) 70%, var(--bg-elev));
  }

  .simple-row.simple-active {
    background: color-mix(in srgb, var(--accent-soft) 60%, var(--bg-elev));
    border-color: color-mix(in srgb, var(--accent) 22%, var(--line));
  }

  .simple-row.simple-fallback {
    background: color-mix(in srgb, var(--accent-soft) 32%, var(--bg-elev));
    border-color: color-mix(in srgb, var(--accent) 16%, var(--line));
  }

  .simple-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    border: 1.5px solid var(--ink-faint);
    flex-shrink: 0;
    transition: background 140ms ease, border-color 140ms ease, box-shadow 140ms ease;
  }

  .simple-dot.dot-active {
    background: var(--accent);
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent-soft) 80%, transparent);
  }

  .simple-dot.dot-fallback {
    background: color-mix(in srgb, var(--accent) 55%, var(--bg-elev));
    border-color: color-mix(in srgb, var(--accent) 70%, var(--line-strong));
  }

  .simple-name {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--ink-soft);
  }

  .simple-active .simple-name {
    color: var(--accent-ink);
    font-weight: 500;
  }

  .simple-badge {
    font-size: 9px;
    font-family: var(--sans);
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    padding: 2px 6px;
    border-radius: 4px;
    margin-left: auto;
    flex-shrink: 0;
  }

  .badge-accuracy {
    color: var(--ink-mute);
    background: color-mix(in srgb, var(--paper) 55%, var(--bg-elev));
    border: 1px solid var(--line-strong);
  }

  .badge-efficiency {
    color: var(--ink-faint);
    background: transparent;
    border: 1px solid var(--line);
  }

  .badge-fallback {
    color: color-mix(in srgb, var(--accent-ink) 75%, var(--ink));
    background: color-mix(in srgb, var(--accent-soft) 45%, var(--bg-elev));
    border: 1px solid color-mix(in srgb, var(--accent) 35%, var(--line-strong));
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

  /* ── Fallback chain ──────────────────────── */
  .chain-bar {
    display: flex;
    flex-direction: column;
    gap: 1px;
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: 8px;
    overflow: hidden;
  }

  .chain-row-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 12px;
    border-bottom: 1px solid var(--line);
  }

  .chain-row-item:last-child {
    border-bottom: none;
  }

  .chain-label {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--ink-mute);
    font-family: var(--sans);
    font-weight: 500;
    flex-shrink: 0;
    width: 28px;
  }

  .chain-chip {
    font-size: 11px;
    font-family: var(--mono);
    padding: 2px 8px;
    border-radius: 999px;
    white-space: nowrap;
  }

  .active-chip {
    background: color-mix(in srgb, var(--accent-soft) 85%, var(--bg-elev));
    border: 1px solid color-mix(in srgb, var(--accent) 30%, var(--line));
    color: var(--accent-ink);
  }

  .chain-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }

  .chain-id {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--ink-soft);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }

  .chain-remove {
    border: none;
    background: none;
    color: var(--ink-mute);
    cursor: pointer;
    font-size: 14px;
    line-height: 1;
    padding: 2px 4px;
    border-radius: 4px;
    flex-shrink: 0;
    transition: color 130ms ease, background 130ms ease;
  }

  .chain-remove:hover {
    color: var(--danger);
    background: color-mix(in srgb, var(--danger-bg) 60%, transparent);
  }

  .chain-empty {
    font-size: 11px;
    color: var(--ink-mute);
    font-style: italic;
    font-family: var(--sans);
  }

  /* ── Provider grid ───────────────────────── */
  .provider-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  /* ── Provider card ───────────────────────── */
  .provider-card {
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--bg-elev);
    overflow: hidden;
    transition: opacity 200ms ease;
  }

  .provider-card.no-key {
    opacity: 0.6;
  }

  .prov-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 9px 11px 8px;
    border-bottom: 1px solid var(--line);
    background: color-mix(in srgb, var(--paper) 30%, var(--bg-elev));
  }

  .prov-name {
    font-family: var(--serif);
    font-size: 13px;
    font-weight: 500;
    color: var(--ink-strong);
  }

  .prov-badge {
    font-size: 9px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--ink-mute);
    border: 1px solid var(--line-strong);
    border-radius: 999px;
    padding: 1px 5px;
  }

  /* ── Model rows ──────────────────────────── */
  .model-list {
    display: flex;
    flex-direction: column;
  }

  .model-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 10px;
    gap: 8px;
    cursor: pointer;
    border-bottom: 1px solid var(--line);
    transition: background 150ms ease;
    min-width: 0;
  }

  .model-row:last-child {
    border-bottom: none;
  }

  .model-row:hover:not(.row-active) {
    background: color-mix(in srgb, var(--paper) 40%, var(--bg-elev));
  }

  .model-row.row-active {
    background: color-mix(in srgb, var(--accent-soft) 55%, var(--bg-elev));
  }

  .model-row.row-fallback {
    background: color-mix(in srgb, var(--accent-soft) 30%, var(--bg-elev));
  }

  .row-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .row-tier {
    font-size: 9px;
    color: var(--ink-mute);
    font-family: var(--sans);
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    line-height: 1;
  }

  .model-name {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--ink-soft);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .row-active .model-name {
    color: var(--accent-ink);
  }

  .state-pill {
    font-size: 9px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    border-radius: 999px;
    padding: 2px 6px;
    border: 1px solid var(--line-strong);
    flex-shrink: 0;
    white-space: nowrap;
    transition: all 160ms ease;
  }

  .state-pill.active {
    color: var(--accent-ink);
    border-color: color-mix(in srgb, var(--accent) 45%, var(--line-strong));
    background: color-mix(in srgb, var(--accent-soft) 75%, var(--bg-elev));
  }

  .state-pill.fallback {
    color: color-mix(in srgb, var(--accent-ink) 75%, var(--ink));
    border-color: color-mix(in srgb, var(--accent) 35%, var(--line-strong));
    background: color-mix(in srgb, var(--accent-soft) 45%, var(--bg-elev));
  }

  .state-pill.muted {
    color: var(--ink-mute);
    background: color-mix(in srgb, var(--paper) 55%, var(--bg-elev));
  }

  /* ── Custom input row ────────────────────── */
  .custom-row {
    display: flex;
    gap: 5px;
    padding: 7px 8px;
    border-top: 1px solid var(--line);
    background: color-mix(in srgb, var(--paper) 15%, var(--bg-elev));
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

  @media (max-width: 860px) {
    .provider-grid {
      grid-template-columns: 1fr;
    }
  }
</style>

