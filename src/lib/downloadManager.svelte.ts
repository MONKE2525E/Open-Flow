// Aggregates every in-flight local-model download (STT, cleanup LLM, and the
// LLM runtime) into one list the sidebar download panel can render, plus a
// short-lived "recently completed" list that lingers until the user next opens
// and closes Settings. It reads the existing local stores rather than owning
// any download state — the only new state here is the completed list.

import { listen } from './tauri';
import { localSttStore, cancelLocalModelDownload } from './localSttStore.svelte';
import {
  localLlmStore,
  cancelLocalLlmModelDownload,
  cancelLocalLlmRuntimeDownload,
} from './localLlmStore.svelte';

export type DownloadKind = 'stt' | 'llm' | 'runtime';
export type DownloadStage = 'downloading' | 'verifying' | 'extracting';

export type ActiveDownload = {
  /** Stable unique key across all three sources: `${kind}:${id}`. */
  key: string;
  kind: DownloadKind;
  id: string;
  name: string;
  stage: DownloadStage;
  label: string;
  percent: number;
  indeterminate: boolean;
};

export type CompletedDownload = { key: string; name: string };

export const downloadUi = $state({
  completed: [] as CompletedDownload[],
});

const STAGE_LABEL: Record<DownloadStage, string> = {
  downloading: 'Downloading',
  verifying: 'Verifying',
  extracting: 'Extracting',
};

function sttName(id: string): string {
  return localSttStore.models.find((model) => model.id === id)?.name ?? id;
}

function llmName(id: string): string {
  return localLlmStore.models.find((model) => model.id === id)?.name ?? id;
}

// Reads the live stores, so calling this inside a $derived/template tracks the
// underlying download state reactively.
export function getActiveDownloads(): ActiveDownload[] {
  const out: ActiveDownload[] = [];

  for (const [id, stage] of Object.entries(localSttStore.downloadStage)) {
    if (!stage) continue;
    const progress = localSttStore.downloadProgress[id];
    out.push({
      key: `stt:${id}`,
      kind: 'stt',
      id,
      name: sttName(id),
      stage,
      label: STAGE_LABEL[stage],
      percent: (progress?.progress ?? 0) * 100,
      indeterminate: progress == null || progress.total_bytes == null,
    });
  }

  for (const [id, stage] of Object.entries(localLlmStore.downloadStage)) {
    if (!stage) continue;
    const progress = localLlmStore.downloadProgress[id];
    out.push({
      key: `llm:${id}`,
      kind: 'llm',
      id,
      name: llmName(id),
      stage,
      label: STAGE_LABEL[stage],
      percent: (progress?.progress ?? 0) * 100,
      indeterminate: progress == null || progress.total_bytes == null,
    });
  }

  const runtime = localLlmStore.runtimeDownloadProgress;
  if (runtime) {
    out.push({
      key: 'runtime:llm',
      kind: 'runtime',
      id: 'runtime',
      name: 'Local cleanup runtime',
      stage: 'downloading',
      label: 'Downloading runtime',
      percent: (runtime.progress ?? 0) * 100,
      indeterminate: runtime.total_bytes == null,
    });
  }

  return out;
}

export function cancelDownload(item: ActiveDownload) {
  if (item.kind === 'stt') {
    cancelLocalModelDownload(item.id).catch((err) => console.error('cancel stt download failed', err));
  } else if (item.kind === 'llm') {
    cancelLocalLlmModelDownload(item.id).catch((err) => console.error('cancel llm download failed', err));
  } else {
    cancelLocalLlmRuntimeDownload().catch((err) => console.error('cancel runtime download failed', err));
  }
}

/** Clears the "ready" list — called when Settings is closed. */
export function acknowledgeDownloads() {
  if (downloadUi.completed.length) downloadUi.completed = [];
}

function noteCompleted(key: string, name: string) {
  if (downloadUi.completed.some((entry) => entry.key === key)) return;
  downloadUi.completed = [...downloadUi.completed, { key, name }];
}

let started = false;
let listenerSession = 0;

export function startDownloadManagerListeners(): () => void {
  if (started) return () => {};
  started = true;
  const session = ++listenerSession;
  let cancelled = false;
  const unlisteners: Array<() => void> = [];

  async function register(promise: Promise<() => void>) {
    const unlisten = await promise;
    if (cancelled || session !== listenerSession) {
      unlisten();
      return;
    }
    unlisteners.push(unlisten);
  }

  (async () => {
    await Promise.all([
      register(listen<{ model_id?: string }>('local-stt-model-download-complete', (event) => {
        if (event.payload?.model_id) noteCompleted(`stt:${event.payload.model_id}`, sttName(event.payload.model_id));
      })),
      register(listen<{ model_id?: string }>('local-llm-model-download-complete', (event) => {
        if (event.payload?.model_id) noteCompleted(`llm:${event.payload.model_id}`, llmName(event.payload.model_id));
      })),
      register(listen('local-llm-runtime-download-complete', () => {
        noteCompleted('runtime:llm', 'Local cleanup runtime');
      })),
    ]);
  })().catch((err) => {
    if (cancelled || session !== listenerSession) return;
    console.error('download manager listeners failed', err);
    for (const unlisten of unlisteners) unlisten();
    unlisteners.length = 0;
    started = false;
  });

  return () => {
    cancelled = true;
    listenerSession += 1;
    for (const unlisten of unlisteners) unlisten();
    unlisteners.length = 0;
    started = false;
  };
}
