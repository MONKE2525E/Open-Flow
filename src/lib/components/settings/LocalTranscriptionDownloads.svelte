<script lang="ts">
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { MOTION_MS, motionMs } from '../../motion';
  import LocalDownloadProgress from './LocalDownloadProgress.svelte';
  import type {
    LocalSttDownloadProgressPayload,
    LocalSttModelInfo,
    LocalTranscriptionState,
  } from '../../tauri';

  type DownloadStage = 'downloading' | 'verifying' | 'extracting' | undefined;

  let {
    opened,
    onToggleOpen,
    transcriptionModels = [],
    transcriptionState = { is_loading: false, is_loaded: false, current_model_id: null } as LocalTranscriptionState,
    selectedTranscriptionModelId = '',
    transcriptionDownloadProgress = {} as Record<string, LocalSttDownloadProgressPayload | undefined>,
    transcriptionDownloadStage = {} as Record<string, DownloadStage>,
    onDownloadTranscriptionModel = (_id: string) => {},
    onCancelTranscriptionDownload = (_id: string) => {},
    onDeleteTranscriptionModel = (_id: string) => {},
  }: {
    opened: boolean;
    onToggleOpen: () => void;
    transcriptionModels?: LocalSttModelInfo[];
    transcriptionState?: LocalTranscriptionState;
    selectedTranscriptionModelId?: string;
    transcriptionDownloadProgress?: Record<string, LocalSttDownloadProgressPayload | undefined>;
    transcriptionDownloadStage?: Record<string, DownloadStage>;
    onDownloadTranscriptionModel?: (modelId: string) => void;
    onCancelTranscriptionDownload?: (modelId: string) => void;
    onDeleteTranscriptionModel?: (modelId: string) => void;
  } = $props();

  const SHOW_MORE_BATCH = 5;
  const COLLAPSIBLE_LANGUAGE_COUNT = 6;

  const sortedTranscriptionModels = $derived(
    [...transcriptionModels].sort((a, b) => {
      if (a.is_recommended !== b.is_recommended) return a.is_recommended ? -1 : 1;
      return a.size_mb - b.size_mb;
    }),
  );

  let showAllTranscriptionModels = $state(false);
  let expandedLanguageModelIds = $state<string[]>([]);

  const visibleTranscriptionModels = $derived(
    showAllTranscriptionModels
      ? sortedTranscriptionModels
      : sortedTranscriptionModels.slice(0, SHOW_MORE_BATCH),
  );
  const hiddenTranscriptionModelCount = $derived(
    sortedTranscriptionModels.length - visibleTranscriptionModels.length,
  );

  const installedCount = $derived(transcriptionModels.filter((m) => m.is_downloaded).length);

  function transcriptionStatusBadge(model: LocalSttModelInfo) {
    if (model.is_downloading) return null;
    if (transcriptionState.is_loading && transcriptionState.current_model_id === model.id) {
      return { label: 'Loading…', tone: 'accent' as const };
    }
    if (transcriptionState.is_loaded && transcriptionState.current_model_id === model.id) {
      return { label: 'In use', tone: 'accent' as const };
    }
    if (selectedTranscriptionModelId === `local/${model.id}`) {
      return { label: 'Selected', tone: 'accent' as const };
    }
    if (model.is_downloaded) return { label: 'Installed', tone: 'muted' as const };
    return null;
  }

  function transcriptionProgressPercent(modelId: string): number {
    return (transcriptionDownloadProgress[modelId]?.progress ?? 0) * 100;
  }

  function transcriptionProgressLabel(modelId: string): string {
    switch (transcriptionDownloadStage[modelId]) {
      case 'verifying':
        return 'Verifying download…';
      case 'extracting':
        return 'Extracting…';
      default:
        return 'Downloading';
    }
  }

  // The download stage is the only one whose total can be unknown (a server
  // that omits Content-Length); verifying and extracting always report a real
  // fraction, so they never fall back to the indeterminate animation.
  function transcriptionIsIndeterminate(modelId: string): boolean {
    if ((transcriptionDownloadStage[modelId] ?? 'downloading') !== 'downloading') return false;
    const progress = transcriptionDownloadProgress[modelId];
    return progress == null || progress.total_bytes == null;
  }

  function shouldCollapseLanguages(model: LocalSttModelInfo): boolean {
    return (model.supported_languages?.length ?? 0) > COLLAPSIBLE_LANGUAGE_COUNT;
  }

  function areLanguagesExpanded(modelId: string): boolean {
    return expandedLanguageModelIds.includes(modelId);
  }

  function toggleLanguages(modelId: string) {
    expandedLanguageModelIds = areLanguagesExpanded(modelId)
      ? expandedLanguageModelIds.filter((id) => id !== modelId)
      : [...expandedLanguageModelIds, modelId];
  }

  function languageSummaryLabel(model: LocalSttModelInfo): string {
    const languages = model.supported_languages ?? [];
    const count = languages.length;
    return count === 0 ? '0 languages' : count === 1 ? languages[0] : `${count} languages`;
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- Escape collapses the open tile no matter which control inside it has
     focus, so the key backs out one layer at a time (tile → Settings). The
     per-model language disclosure handles Escape first (see .local-meta-toggle)
     so the innermost layer always closes before the tile. preventDefault
     marks the key as handled for Settings' window guard. -->
<div class="task-tile" class:task-open={opened} onkeydown={(event) => { if (event.key === 'Escape' && opened) { event.preventDefault(); onToggleOpen(); } }}>
  <button class="tile-head" onclick={onToggleOpen} aria-expanded={opened}>
    <div class="head-left">
      <span class="head-title">Speech-to-text</span>
      <div class="summary-row">
        <span class="summary-item provider-chip">{installedCount} installed</span>
        <span class="summary-item model-chip">on-device STT</span>
      </div>
    </div>
    <span class="chevron" class:chevron-open={opened} aria-hidden="true"></span>
  </button>

  {#if opened}
    <div class="tile-inner" transition:slide={{ duration: motionMs(MOTION_MS.base), easing: cubicOut }}>
      <div id="transcription-models-block" class="local-model-list">
        {#each visibleTranscriptionModels as model (model.id)}
          {@const badge = transcriptionStatusBadge(model)}
          <div
            class="local-model-card"
            class:is-busy={model.is_downloading}
            data-model-type="transcription"
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
                {#if model.is_downloading}
                  <button
                    class="card-btn ghost"
                    data-testid="cancel-model-download"
                    type="button"
                    onclick={() => onCancelTranscriptionDownload(model.id)}
                  >Cancel</button>
                {:else if !model.is_downloaded}
                  <button
                    class="card-btn accent"
                    data-testid="download-model"
                    type="button"
                    onclick={() => onDownloadTranscriptionModel(model.id)}
                  >Download</button>
                {:else}
                  <button
                    class="card-btn ghost"
                    data-testid="delete-model"
                    type="button"
                    onclick={() => onDeleteTranscriptionModel(model.id)}
                  >Delete</button>
                {/if}
              </div>
            </div>

            <div class="local-meta">
              <span>{model.size_mb} MB</span>
              <span class="privacy-meta" title="Runs entirely on your device. Private and offline. Nothing leaves your machine.">Local AI · private · offline</span>
              {#if shouldCollapseLanguages(model)}
                <button
                  class="local-meta-toggle"
                  class:is-open={areLanguagesExpanded(model.id)}
                  type="button"
                  aria-expanded={areLanguagesExpanded(model.id)}
                  aria-controls={areLanguagesExpanded(model.id) ? `local-languages-${model.id}` : undefined}
                  onclick={() => toggleLanguages(model.id)}
                >
                  <span>{languageSummaryLabel(model)}</span>
                  <svg
                    class:open={areLanguagesExpanded(model.id)}
                    width="9"
                    height="9"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2.5"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    aria-hidden="true"
                  >
                    <path d="m6 9 6 6 6-6" />
                  </svg>
                </button>
              {:else}
                <span class="local-meta-langs">{(model.supported_languages ?? []).join(', ')}</span>
              {/if}
            </div>

            {#if shouldCollapseLanguages(model) && areLanguagesExpanded(model.id)}
              <div
                id={`local-languages-${model.id}`}
                class="local-language-panel"
                transition:slide={{ duration: motionMs(MOTION_MS.base), easing: cubicOut }}
              >
                <div class="local-language-label">Supported languages</div>
                <p>{(model.supported_languages ?? []).join(', ')}</p>
              </div>
            {/if}

            {#if model.is_downloading}
              <div transition:slide={{ duration: motionMs(MOTION_MS.base), easing: cubicOut }}>
                <LocalDownloadProgress
                  stage={transcriptionDownloadStage[model.id] ?? 'downloading'}
                  percent={transcriptionProgressPercent(model.id)}
                  label={transcriptionProgressLabel(model.id)}
                  indeterminate={transcriptionIsIndeterminate(model.id)}
                />
              </div>
            {/if}
          </div>
        {/each}
        {#if hiddenTranscriptionModelCount > 0}
          <button class="show-more-btn" type="button" onclick={() => (showAllTranscriptionModels = true)}>
            Show {hiddenTranscriptionModelCount} more
          </button>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .task-tile {
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

  .local-meta-toggle svg {
    flex-shrink: 0;
    transition: transform 150ms;
  }

  .local-meta-toggle svg.open {
    transform: rotate(180deg);
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
  .local-card-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .local-meta > span,
  .local-meta > button {
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

  .local-language-panel {
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: color-mix(in srgb, var(--paper) 55%, var(--bg-elev));
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

  .local-card-top {
    display: flex;
    justify-content: space-between;
    gap: 14px;
    align-items: flex-start;
  }

  .local-card-copy {
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

  .local-meta-toggle {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    cursor: pointer;
    transition: border-color 160ms ease, color 160ms ease, background 160ms ease;
  }

  .local-meta-toggle:hover {
    border-color: color-mix(in srgb, var(--accent) 20%, var(--line));
    color: var(--ink-soft);
  }

  .local-meta-toggle:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .local-meta-toggle.is-open {
    border-color: color-mix(in srgb, var(--accent) 32%, var(--line));
    background: color-mix(in srgb, var(--accent-soft) 40%, var(--bg-elev));
    color: var(--accent-ink);
  }

  .local-language-label {
    font-size: 11px;
    font-family: var(--sans);
    font-weight: 500;
    text-transform: none;
    letter-spacing: 0;
    color: var(--ink-faint);
  }

  .local-card-copy p,
  .local-language-panel p {
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

  /* Container-relative: see the note in ModelsSection. */
  @container settings-panel (max-width: 720px) {
    .local-card-top {
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
