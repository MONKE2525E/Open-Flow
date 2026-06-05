<script lang="ts">
  import { invoke } from '../../tauri';
  import { fly, slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { MOTION_MS, MOTION_PX, motionMs, motionPx } from '../../motion';

  function pillScale(
    node: Element,
    { duration = motionMs(MOTION_MS.fast) }: { duration?: number } = {},
  ) {
    const w = (node as HTMLElement).getBoundingClientRect().width;
    return {
      duration,
      easing: cubicOut,
      css: (t: number) => `max-width: ${w * t}px; opacity: ${t}; overflow: hidden;`,
    };
  }
  import Toggle from '../Toggle.svelte';
  import { saveSetting, type ProviderId, type ProviderModelMap } from '../../settings';

  type TaskType = 'transcription' | 'cleanup';
  type UiProviderId = 'groq' | 'openai' | 'google';

  type ProviderSection = {
    id: UiProviderId;
    label: string;
    storeProvider: ProviderId;
  };
  type AllSettingsPayload = {
    transcription_model?: string | null;
    cleanup_model?: string | null;
    transcription_models_by_provider?: unknown;
    cleanup_models_by_provider?: unknown;
    transcription_default_model?: string | null;
    cleanup_default_model?: string | null;
    transcription_fallback_models?: string[] | null;
    cleanup_fallback_models?: string[] | null;
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
    const [all, keyStatus, advancedRaw] = await Promise.all([
      invoke<AllSettingsPayload>('get_all_settings'),
      invoke<typeof apiKeyStatus>('get_api_key_status'),
      invoke<boolean | null>('get_setting', { key: 'advanced_model_ui' }),
    ]);

    apiKeyStatus = keyStatus;
    const tMapRaw = all.transcription_models_by_provider;
    const cMapRaw = all.cleanup_models_by_provider;
    const tDefaultRaw = all.transcription_default_model ?? null;
    const cDefaultRaw = all.cleanup_default_model ?? null;
    const tFallbackRaw = all.transcription_fallback_models ?? null;
    const cFallbackRaw = all.cleanup_fallback_models ?? null;

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

    const rawLegacyT = String(all.transcription_model ?? '');
    const rawLegacyC = String(all.cleanup_model ?? '');
    const legacyT = rawLegacyT
      ? (rawLegacyT.includes('/') ? rawLegacyT : `groq/${rawLegacyT}`)
      : 'groq/whisper-large-v3-turbo';
    const legacyC = rawLegacyC
      ? (rawLegacyC.includes('/') ? rawLegacyC : `groq/${rawLegacyC}`)
      : 'groq/llama-3.3-70b-versatile';

    transcriptionDefaultModel = tDefaultRaw !== null ? tDefaultRaw : legacyT;
    cleanupDefaultModel = cDefaultRaw !== null ? cDefaultRaw : legacyC;

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

  function removeCustomModel(type: TaskType, provider: ProviderId, modelName: string) {
    const id = modelId(provider, modelName);

    // Remove from fallbacks if present
    if (taskFallbacks(type).includes(id)) {
      setTaskFallbacks(type, taskFallbacks(type).filter((m) => m !== id));
    }

    // If it's the active model, promote first fallback or default to a recommended model
    if (taskDefault(type) === id) {
      const fallbacks = taskFallbacks(type);
      if (fallbacks.length > 0) {
        const [nextActive, ...remaining] = fallbacks;
        setTaskDefault(type, nextActive);
        setTaskFallbacks(type, remaining);
      } else {
        const provId = provider as UiProviderId;
        setTaskDefault(type, modelId(provider, recommendedModels[type][provId].standard));
      }
    }

    // Remove from models_by_provider
    const map = taskMap(type);
    setTaskMap(type, { ...map, [provider]: map[provider].filter((m) => m !== modelName) });

    persistAll().catch((err) => console.error('persist remove custom failed', err));
  }

  function toggleModelSelection(type: TaskType, provider: ProviderId, modelName: string) {
    const id = modelId(provider, modelName);
    ensureModelsContainSelection(type, provider, modelName);
    const currentState = selectionState(type, provider, modelName);

    if (currentState === 'active') {
      const fallbacks = taskFallbacks(type);
      if (fallbacks.length > 0) {
        const [nextActive, ...remaining] = fallbacks;
        setTaskDefault(type, nextActive);
        setTaskFallbacks(type, remaining);
      } else {
        const provId = provider as UiProviderId;
        setTaskDefault(type, modelId(provider, recommendedModels[type][provId].standard));
      }
    } else if (currentState === 'fallback') {
      setTaskFallbacks(type, taskFallbacks(type).filter((m) => m !== id));
    } else {
      if (!splitModelId(taskDefault(type))) {
        setTaskDefault(type, id);
      } else {
        setTaskFallbacks(type, [...taskFallbacks(type), id]);
      }
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
          {#if fallbackCount > 0}
            <span class="summary-item fallback-chip">{fallbackCount} fallback{fallbackCount !== 1 ? 's' : ''}</span>
          {/if}
        </div>
      </div>
      <span class="chevron" class:chevron-open={opened} aria-hidden="true"></span>
    </button>

    {#if opened}
      <div class="tile-inner" transition:slide={{ duration: motionMs(MOTION_MS.base), easing: cubicOut }}>
        {#if missingKeyWarning(type)}
          <div class="warn-banner">{missingKeyWarning(type)}</div>
        {/if}

        <div class="model-container">
        {#each providerSections as section (section.id)}
          {@const hasKey = apiKeyStatus[section.storeProvider]}
          {@const customModels = (taskMap(type)[section.storeProvider] ?? []).filter(
            (m) => m !== recommendedModels[type][section.id].premium && m !== recommendedModels[type][section.id].standard
          )}
          <div class="simple-group" class:no-key={!hasKey}>
            <span class="simple-provider">{section.label}</span>

            {#each ['premium', 'standard'] as rawTier}
              {@const tier = rawTier as 'premium' | 'standard'}
              {@const mName = recommendedModels[type][section.id][tier]}
              {@const state = selectionState(type, section.storeProvider, mName)}
              {@const isActive = state === 'active'}
              {@const isFallback = state === 'fallback'}
              <div
                class="simple-row model-row"
                class:simple-active={isActive}
                class:simple-fallback={isFallback}
                class:chain-row={isFallback}
                role="button"
                tabindex="0"
                onclick={() => toggleModelSelection(type, section.storeProvider, mName)}
                onkeydown={(e) => { if (e.target === e.currentTarget && (e.key === 'Enter' || e.key === ' ')) { if (e.key === ' ') e.preventDefault(); toggleModelSelection(type, section.storeProvider, mName); } }}
              >
                <span class="simple-dot" class:dot-active={isActive} class:dot-fallback={isFallback}></span>
                <span class="simple-name model-name">{mName}</span>
                {#if isActive}
                  <span transition:pillScale class="state-pill active">Active</span>
                {:else if isFallback}
                  <span transition:pillScale class="state-pill fallback">F{taskFallbacks(type).indexOf(modelId(section.storeProvider, mName)) + 1}</span>
                {:else}
                  <span transition:pillScale class="state-pill tier-pill">
                    <span class="tier-label">{tier === 'premium' ? 'Accurate' : 'Efficient'}</span>
                    <span class="fallback-label" aria-hidden="true">Add fallback</span>
                  </span>
                {/if}
              </div>
            {/each}

            {#each customModels.filter((m) => advancedModelUi || selectionState(type, section.storeProvider, m) !== 'none') as custom (custom)}
              {@const state = selectionState(type, section.storeProvider, custom)}
              {@const isActive = state === 'active'}
              {@const isFallback = state === 'fallback'}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.base), easing: cubicOut }}
                out:fly={{ y: -motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.fast), easing: cubicOut }}
                class="simple-row model-row"
                class:simple-active={isActive}
                class:simple-fallback={isFallback}
                class:chain-row={isFallback}
                onclick={() => toggleModelSelection(type, section.storeProvider, custom)}
              >
                <button
                  class="remove-dot"
                  class:dot-active={isActive}
                  class:dot-fallback={isFallback}
                  type="button"
                  aria-label="Remove {custom}"
                  onclick={(e) => { e.stopPropagation(); removeCustomModel(type, section.storeProvider, custom); }}
                ></button>
                <button
                  type="button"
                  class="simple-name model-name custom-toggle-btn"
                  onclick={(e) => {
                    e.stopPropagation();
                    toggleModelSelection(type, section.storeProvider, custom);
                  }}
                  aria-label="Toggle selection for {custom}"
                >
                  {custom}
                </button>
                {#if isActive}
                  <span transition:pillScale class="state-pill active">Active</span>
                {:else if isFallback}
                  <span transition:pillScale class="state-pill fallback">F{taskFallbacks(type).indexOf(modelId(section.storeProvider, custom)) + 1}</span>
                {:else}
                  <button
                    type="button"
                    class="state-pill muted add-fallback-btn"
                    onclick={(e) => {
                      e.stopPropagation();
                      toggleModelSelection(type, section.storeProvider, custom);
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
            {/if}
          </div>
        {/each}
        </div>
      </div>
    {/if}
  </div>
{/each}

<div class="advanced-toggle-row">
  <div class="adv-text">
    <span class="adv-label">Custom models</span>
    <span class="adv-desc">Add custom model names per provider</span>
  </div>
  <Toggle checked={advancedModelUi} onchange={handleAdvancedModelUi} label="Custom models" />
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

  .tile-inner .warn-banner {
    margin: 10px 0 8px;
  }

  /* ── Model container ─────────────────────── */
  .model-container {
    border: 1px solid var(--line);
    border-radius: 10px;
    overflow: hidden;
  }

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

  .simple-group + .simple-group::before {
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


  /* ── State pills ─────────────────────────── */
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
    font: inherit;
    color: inherit;
  }
  .add-fallback-btn:focus-visible {
    outline: 2px solid var(--accent, #d97757);
    outline-offset: 2px;
  }

  .custom-toggle-btn {
    appearance: none;
    background: transparent;
    border: none;
    padding: 0;
    cursor: pointer;
    outline: none;
  }
  .custom-toggle-btn:focus-visible {
    outline: 2px solid var(--accent, #d97757);
    outline-offset: 2px;
  }

  /* ── Tier pill (Accurate/Efficient → Add fallback on hover) ── */
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

  /* ── Warning ─────────────────────────────── */
  .warn-banner {
    font-size: 12px;
    color: var(--danger);
    background: var(--danger-bg);
    border: 1px solid var(--danger-line);
    border-radius: 8px;
    padding: 8px 10px;
  }

  /* ── Custom input row ────────────────────── */
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

  /* ── Advanced toggle row ─────────────────── */
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
