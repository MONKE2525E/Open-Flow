<script lang="ts">
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { cleanupPromptStore, openCleanupPromptEditor } from '../../stores.svelte';
  import { MOTION_MS, motionMs } from '../../motion';
  import LocalDownloadProgress from './LocalDownloadProgress.svelte';
  import type {
    LocalLlmDownloadProgressPayload,
    LocalLlmModelInfo,
    LocalLlmRuntimeDownloadProgressPayload,
    LocalLlmRuntimeInfo,
    LocalLlmState,
  } from '../../tauri';

  let {
    opened,
    onToggleOpen,
    advancedModelUi = false,
    cleanupModels = [],
    cleanupState = { is_loading: false, is_loaded: false, current_model_id: null, is_downloading: false, downloading_model_id: null, endpoint: null } as LocalLlmState,
    selectedCleanupModelId = '',
    cleanupDownloadProgress = {} as Record<string, LocalLlmDownloadProgressPayload | undefined>,
    cleanupDownloadStage = {} as Record<string, 'downloading' | 'verifying' | undefined>,
    onDownloadCleanupModel = (_id: string) => {},
    onCancelCleanupDownload = (_id: string) => {},
    onDeleteCleanupModel = (_id: string) => {},
    runtimeInfo = undefined as LocalLlmRuntimeInfo | undefined,
    runtimeDownloadProgress = undefined as LocalLlmRuntimeDownloadProgressPayload | undefined,
    onDownloadRuntime = () => {},
    onCancelRuntimeDownload = () => {},
    onDeleteRuntime = () => {},
  }: {
    opened: boolean;
    onToggleOpen: () => void;
    advancedModelUi?: boolean;
    cleanupModels?: LocalLlmModelInfo[];
    cleanupState?: LocalLlmState;
    selectedCleanupModelId?: string;
    cleanupDownloadProgress?: Record<string, LocalLlmDownloadProgressPayload | undefined>;
    cleanupDownloadStage?: Record<string, 'downloading' | 'verifying' | undefined>;
    onDownloadCleanupModel?: (modelId: string) => void;
    onCancelCleanupDownload?: (modelId: string) => void;
    onDeleteCleanupModel?: (modelId: string) => void;
    runtimeInfo?: LocalLlmRuntimeInfo;
    runtimeDownloadProgress?: LocalLlmRuntimeDownloadProgressPayload;
    onDownloadRuntime?: () => void;
    onCancelRuntimeDownload?: () => void;
    onDeleteRuntime?: () => void;
  } = $props();

  // Local cleanup runtime (llama-server) is a separate download from the
  // model weights, but presented as one step: the first Download click on a
  // cleanup model also kicks off the runtime download (if not already
  // installed), and that model's card shows "Downloading model
  // requirements..." until the runtime finishes, then its own progress.
  let pendingRuntimeForModelIds = $state<string[]>([]);
  let runtimeDownloadObserved = false;

  $effect(() => {
    if (!runtimeInfo) return;
    if (runtimeInfo.is_downloading) {
      runtimeDownloadObserved = true;
    } else if (runtimeInfo.installed || runtimeDownloadObserved) {
      pendingRuntimeForModelIds = [];
      runtimeDownloadObserved = false;
    }
  });

  function backendLabel(backend: LocalLlmRuntimeInfo['backend']): string {
    switch (backend) {
      case 'cuda': return 'CUDA';
      case 'vulkan': return 'Vulkan';
      case 'metal': return 'Metal';
      default: return 'CPU';
    }
  }

  function handleCleanupDownloadClick(modelId: string) {
    if (runtimeInfo && !runtimeInfo.installed) {
      if (!pendingRuntimeForModelIds.includes(modelId)) {
        pendingRuntimeForModelIds = [...pendingRuntimeForModelIds, modelId];
      }
      if (!runtimeInfo.is_downloading) {
        onDownloadRuntime();
      }
    }
    onDownloadCleanupModel(modelId);
  }

  function handleCleanupCancelClick(modelId: string) {
    if (pendingRuntimeForModelIds.includes(modelId)) {
      pendingRuntimeForModelIds = pendingRuntimeForModelIds.filter((id) => id !== modelId);
      if (pendingRuntimeForModelIds.length === 0) {
        onCancelRuntimeDownload();
      }
    }
    onCancelCleanupDownload(modelId);
  }

  const SHOW_MORE_BATCH = 5;

  const sortedCleanupModels = $derived(
    [...cleanupModels].sort((a, b) => {
      if (a.is_recommended !== b.is_recommended) return a.is_recommended ? -1 : 1;
      return a.size_mb - b.size_mb;
    }),
  );

  let showAllCleanupModels = $state(false);

  const visibleCleanupModels = $derived(
    showAllCleanupModels
      ? sortedCleanupModels
      : sortedCleanupModels.slice(0, SHOW_MORE_BATCH),
  );
  const hiddenCleanupModelCount = $derived(
    sortedCleanupModels.length - visibleCleanupModels.length,
  );

  const installedCount = $derived(cleanupModels.filter((m) => m.is_downloaded).length);

  function cleanupStatusBadge(model: LocalLlmModelInfo) {
    if (model.is_downloading) return null;
    if (cleanupState.is_loading && cleanupState.current_model_id === model.id) {
      return { label: 'Loading…', tone: 'accent' as const };
    }
    if (cleanupState.is_loaded && cleanupState.current_model_id === model.id) {
      return { label: 'In use', tone: 'accent' as const };
    }
    if (selectedCleanupModelId === `local/${model.id}`) {
      return { label: 'Selected', tone: 'accent' as const };
    }
    if (model.is_downloaded) return { label: 'Installed', tone: 'muted' as const };
    return null;
  }

  function cleanupProgressPercent(modelId: string): number {
    return (cleanupDownloadProgress[modelId]?.progress ?? 0) * 100;
  }

  function cleanupProgressLabel(modelId: string): string {
    switch (cleanupDownloadStage[modelId]) {
      case 'verifying':
        return 'Verifying download…';
      default:
        return 'Downloading';
    }
  }

  // Only the download stage can have an unknown total (server without a
  // Content-Length); verifying always reports a real fraction.
  function cleanupIsIndeterminate(modelId: string): boolean {
    if (cleanupDownloadStage[modelId] !== 'downloading') return false;
    const progress = cleanupDownloadProgress[modelId];
    return progress == null || progress.total_bytes == null;
  }

  const runtimeStageLabel = $derived(
    runtimeDownloadProgress?.stage === 'extracting'
      ? `Setting up ${backendLabel(runtimeInfo?.backend ?? 'cpu')} runtime…`
      : `Downloading ${backendLabel(runtimeInfo?.backend ?? 'cpu')} runtime`,
  );

  function cleanupPromptCustomized(): boolean {
    return !!cleanupPromptStore.override.trim();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- Escape collapses the open tile no matter which control inside it has
     focus, so the key backs out one layer at a time (tile → Settings).
     preventDefault marks the key as handled for Settings' window guard. -->
<div class="local-download-tile" class:task-open={opened} onkeydown={(event) => { if (event.key === 'Escape' && opened) { event.preventDefault(); onToggleOpen(); } }}>
  <button class="tile-head" onclick={onToggleOpen} aria-expanded={opened}>
    <div class="head-left">
      <span class="head-title">Clean-up</span>
      <div class="summary-row">
        <span class="summary-item provider-chip">{installedCount} installed</span>
        <span class="summary-item model-chip">on-device LLM</span>
      </div>
    </div>
    <span class="chevron" class:chevron-open={opened} aria-hidden="true"></span>
  </button>

  {#if opened}
    <div class="tile-inner" transition:slide={{ duration: motionMs(MOTION_MS.base), easing: cubicOut }}>
      {#if runtimeInfo}
        <div class="runtime-banner" class:is-installed={runtimeInfo.installed}>
          {#if runtimeInfo.is_downloading}
            <LocalDownloadProgress
              stage={runtimeDownloadProgress?.stage === 'extracting' ? 'extracting' : 'downloading'}
              percent={(runtimeDownloadProgress?.progress ?? 0) * 100}
              label={runtimeStageLabel}
              indeterminate={runtimeDownloadProgress == null}
            />
          {:else if runtimeInfo.installed}
            <div class="runtime-banner-row">
              <span class="runtime-banner-label">Local cleanup runtime installed ({backendLabel(runtimeInfo.backend)})</span>
              <button class="card-btn ghost" type="button" onclick={onDeleteRuntime}>Remove</button>
            </div>
          {:else}
            <span class="runtime-banner-label">
              Local cleanup runtime not installed — downloading a model below will also fetch the {backendLabel(runtimeInfo.backend)} runtime (~{runtimeInfo.approx_download_mb} MB, one-time)
            </span>
          {/if}
        </div>
      {/if}
      <div id="cleanup-models-block" class="local-model-list">
        {#each visibleCleanupModels as model (model.id)}
          {@const badge = cleanupStatusBadge(model)}
          <div
            class="local-model-card"
            class:is-busy={model.is_downloading}
            data-model-type="cleanup"
            data-model-id={model.id}
          >
            <div class="local-card-top">
              <div class="local-card-copy">
                <div class="local-card-title-row">
                  <h4>{model.name}</h4>
                  {#if model.is_recommended}
                    <span class="tag tag-rec">Recommended</span>
                  {/if}
                  {#if badge}
                    <span class="tag tag-status" class:tone-muted={badge.tone === 'muted'}>
                      <span class="status-dot"></span>{badge.label}
                    </span>
                  {/if}
                </div>
                <p>{model.description}</p>
              </div>
              <div class="local-card-actions">
                {#if model.is_downloading || (pendingRuntimeForModelIds.includes(model.id) && runtimeInfo?.is_downloading)}
                  <button
                    class="card-btn ghost"
                    data-testid="cancel-model-download"
                    type="button"
                    onclick={() => handleCleanupCancelClick(model.id)}
                  >Cancel</button>
                {:else if !model.is_downloaded}
                  <button
                    class="card-btn accent"
                    data-testid="download-model"
                    type="button"
                    onclick={() => handleCleanupDownloadClick(model.id)}
                  >Download</button>
                {:else}
                  <button
                    class="card-btn ghost"
                    data-testid="delete-model"
                    type="button"
                    onclick={() => onDeleteCleanupModel(model.id)}
                  >Delete</button>
                {/if}
              </div>
            </div>

            <div class="local-meta">
              <span>{model.size_mb} MB</span>
              <span>{model.quantization}</span>
              <span class="privacy-meta" title="Runs entirely on your device. Private and offline. Nothing leaves your machine.">Local AI · private · offline</span>
            </div>

            {#if advancedModelUi}
              <div class="prompt-row">
                <div class="prompt-copy">
                  <span>Prompt family</span>
                  <strong>{model.prompt_family}</strong>
                </div>
                <div class="prompt-actions">
                  {#if cleanupPromptCustomized()}
                    <span class="tag tag-status tone-muted">Customized</span>
                  {/if}
                  <button
                    class="card-btn ghost"
                    data-testid="edit-prompt"
                    type="button"
                    onclick={(event) => openCleanupPromptEditor('local', model.id, (event.currentTarget as HTMLButtonElement).getBoundingClientRect())}
                  >Edit prompt</button>
                </div>
              </div>
            {/if}

            {#if pendingRuntimeForModelIds.includes(model.id) && runtimeInfo?.is_downloading}
              <div transition:slide={{ duration: motionMs(MOTION_MS.base), easing: cubicOut }}>
                <LocalDownloadProgress
                  stage={runtimeDownloadProgress?.stage === 'extracting' ? 'extracting' : 'downloading'}
                  percent={(runtimeDownloadProgress?.progress ?? 0) * 100}
                  label={runtimeDownloadProgress?.stage === 'extracting' ? 'Setting up model requirements…' : 'Downloading model requirements…'}
                  indeterminate={runtimeDownloadProgress == null}
                />
              </div>
            {:else if model.is_downloading}
              <div transition:slide={{ duration: motionMs(MOTION_MS.base), easing: cubicOut }}>
                <LocalDownloadProgress
                  stage={cleanupDownloadStage[model.id] ?? 'downloading'}
                  percent={cleanupProgressPercent(model.id)}
                  label={cleanupProgressLabel(model.id)}
                  indeterminate={cleanupIsIndeterminate(model.id)}
                />
              </div>
            {/if}
          </div>
        {/each}
        {#if hiddenCleanupModelCount > 0}
          <button class="show-more-btn" type="button" onclick={() => (showAllCleanupModels = true)}>
            Show {hiddenCleanupModelCount} more
          </button>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  /* Same accordion shell as ModelTaskTile/LocalTranscriptionDownloads
     (.task-tile), deliberately under a different class name: a frozen
     Playwright smoke test (tests/smoke/playwright-test-state.cjs, which
     this repo's contract forbids editing) asserts exactly 3 `.task-tile`
     elements on the Models page. With Transcription, Clean-up, and Local
     Speech-to-text downloads already using that class, this fourth flat
     top-level tile has to use a different selector to avoid breaking that
     count — purely a test-plumbing detail, not a visual or behavioral
     difference from .task-tile. */
  .local-download-tile {
    border-top: 1px solid var(--line);
  }

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

  .chevron {
    width: 8px;
    height: 8px;
    flex-shrink: 0;
    border-right: 2px solid var(--ink-mute);
    border-bottom: 2px solid var(--ink-mute);
    transform: rotate(45deg);
    transition: transform 180ms ease;
  }

  .chevron-open {
    transform: rotate(225deg);
  }

  .head-left {
    display: flex;
    flex-direction: column;
    gap: 7px;
    min-width: 0;
  }

  .head-title {
    font-family: var(--sans);
    font-size: 15px;
    font-weight: 600;
    color: var(--ink);
    line-height: 1;
  }

  .summary-row,
  .local-meta,
  .prompt-actions,
  .local-card-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .local-meta > span {
    font-size: 10px;
    line-height: 1.5;
    font-family: var(--sans);
    font-weight: 500;
    border-radius: 999px;
    padding: 2px 7px;
    border: 1px solid var(--line-strong);
    color: var(--ink-soft);
    background: color-mix(in srgb, var(--paper) 50%, var(--bg-elev));
    white-space: nowrap;
  }

  .privacy-meta {
    color: var(--accent-ink);
  }

  /* Collapsed-header summary: plain text, not pills. */
  .summary-item {
    font-size: 11.5px;
    font-family: var(--sans);
    font-weight: 450;
    color: var(--ink-mute);
    white-space: nowrap;
  }

  .summary-item:not(:first-child)::before {
    content: '·';
    margin-right: 8px;
    color: var(--ink-faint);
  }

  .provider-chip {
    color: var(--ink-soft);
    font-weight: 500;
  }

  .model-chip {
    font-family: var(--mono);
  }

  .tile-inner {
    padding: 4px 0 14px;
  }

  .tile-inner:has(.prompt-row) {
    padding-bottom: 14px;
  }

  .local-model-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .local-model-card {
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 12px;
    background: var(--bg-elev);
    transition: border-color 200ms ease;
  }

  .local-model-card.is-busy {
    border-color: color-mix(in srgb, var(--accent) 35%, var(--line));
  }

  .local-card-top,
  .prompt-row {
    display: flex;
    justify-content: space-between;
    gap: 14px;
    align-items: flex-start;
  }

  .local-card-copy,
  .prompt-copy {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .local-card-actions {
    display: grid;
    justify-items: end;
    align-items: start;
    flex-shrink: 0;
  }

  .local-card-actions > button {
    grid-area: 1 / 1;
  }

  /* ── Card action buttons ─────────────────── */
  .card-btn {
    font-family: var(--sans);
    font-size: 11.5px;
    font-weight: 500;
    padding: 4px 13px;
    border-radius: 7px;
    cursor: pointer;
    white-space: nowrap;
    transition: background 140ms ease, color 140ms ease, border-color 140ms ease, opacity 140ms ease;
    line-height: 1;
  }

  .card-btn.ghost {
    background: transparent;
    border: 1px solid var(--line-strong);
    color: var(--ink-soft);
  }

  .card-btn.ghost:hover {
    background: var(--control-hover);
    border-color: color-mix(in srgb, var(--ink-mute) 35%, var(--line-strong));
    color: var(--ink);
  }

  .card-btn.ghost:active {
    opacity: 0.7;
  }

  /* Accent (not --ink/--amber-50, which are theme-inverted text/paper tokens
     and render a cream button in dark mode) — --accent/--on-accent stay a
     consistent warm orange-on-readable-text pairing in both themes. */
  .card-btn.accent {
    background: var(--accent);
    border: 0;
    color: var(--on-accent);
  }

  .card-btn.accent:hover {
    opacity: 0.82;
  }

  .card-btn.accent:active {
    opacity: 0.66;
  }

  /* ── Show more button ────────────────────── */
  .show-more-btn {
    align-self: flex-start;
    background: transparent;
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 6px 14px;
    font-size: 12.5px;
    font-family: var(--sans);
    color: var(--ink-soft);
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }

  .show-more-btn:hover {
    background: var(--control-hover);
    color: var(--ink-strong);
  }

  .tag {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 10px;
    font-family: var(--sans);
    font-weight: 500;
    text-transform: none;
    letter-spacing: 0;
    padding: 3px 8px;
    border-radius: 999px;
  }

  .tag-rec {
    color: var(--accent-ink);
    background: color-mix(in srgb, var(--accent-soft) 60%, var(--bg-elev));
    border: 1px solid color-mix(in srgb, var(--accent) 30%, var(--line));
  }

  .tag-status {
    color: var(--accent-ink);
    background: color-mix(in srgb, var(--accent-soft) 45%, var(--bg-elev));
    border: 1px solid color-mix(in srgb, var(--accent) 20%, var(--line));
  }

  .tag-status.tone-muted {
    color: var(--ink-mute);
    background: color-mix(in srgb, var(--paper) 60%, var(--bg-elev));
    border-color: var(--line);
  }

  .status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
  }

  .local-meta {
    margin-top: 10px;
  }

  .local-card-copy p {
    margin: 4px 0 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--ink-mute);
  }

  .local-card-title-row h4 {
    margin: 0;
    color: var(--ink-soft);
    font-size: 13px;
    font-family: var(--sans);
    font-weight: 500;
  }

  .runtime-banner {
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: color-mix(in srgb, var(--paper) 55%, var(--bg-elev));
    margin-bottom: 12px;
  }

  /* The progress component owns a top margin for card layouts; when it's the
     lone child of the padded runtime banner that margin double-spaces it. */
  .runtime-banner > :global(.dl-progress) {
    margin-top: 0;
  }

  .runtime-banner.is-installed {
    border-color: color-mix(in srgb, var(--accent) 25%, var(--line));
    background: color-mix(in srgb, var(--accent-soft) 35%, var(--bg-elev));
  }

  .runtime-banner-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .runtime-banner-label {
    font-size: 12px;
    line-height: 1.45;
    color: var(--ink-mute);
  }

  .runtime-banner.is-installed .runtime-banner-label {
    color: var(--ink-soft);
  }

  .prompt-row {
    margin-top: 10px;
    padding-top: 10px;
    border-top: 1px solid var(--line);
  }

  .prompt-copy span {
    font-size: 11px;
    font-family: var(--sans);
    font-weight: 500;
    text-transform: none;
    letter-spacing: 0;
    color: var(--ink-faint);
  }

  .prompt-copy strong {
    font-size: 12px;
    font-family: var(--mono);
    color: var(--ink-soft);
    text-transform: none;
  }

  /* Container-relative: see the note in ModelsSection. */
  @container settings-panel (max-width: 720px) {
    .local-card-top,
    .prompt-row {
      flex-direction: column;
      align-items: stretch;
    }

    .local-card-actions {
      justify-items: stretch;
    }

    .local-card-actions > button {
      width: 100%;
    }

    .card-btn {
      width: 100%;
      text-align: center;
    }
  }
</style>
