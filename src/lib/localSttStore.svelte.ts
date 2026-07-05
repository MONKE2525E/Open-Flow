import {
  invoke,
  listen,
  emit,
  type LocalSttDownloadProgressPayload,
  type LocalSttExtractionProgressPayload,
  type LocalSttModelEventPayload,
  type LocalSttModelInfo,
  type LocalSttVerificationProgressPayload,
  type LocalTranscriptionState,
} from './tauri';

export const localSttStore = $state({
  models: [] as LocalSttModelInfo[],
  state: {
    current_model_id: null,
    is_loaded: false,
    is_loading: false,
    is_downloading: false,
    downloading_model_id: null,
  } as LocalTranscriptionState,
  downloadProgress: {} as Record<string, LocalSttDownloadProgressPayload | undefined>,
  downloadStage: {} as Record<string, 'downloading' | 'verifying' | 'extracting' | undefined>,
});

export async function refreshLocalModels() {
  try {
    localSttStore.models = await invoke<LocalSttModelInfo[]>('list_local_stt_models');
  } catch (err) {
    console.error('load local models failed', err);
  }
}

export async function refreshLocalState() {
  try {
    localSttStore.state = await invoke<LocalTranscriptionState>('get_local_transcription_state');
  } catch (err) {
    console.error('load local transcription state failed', err);
  }
}

export async function downloadLocalModel(modelIdValue: string) {
  try {
    await invoke('download_local_stt_model', { modelId: modelIdValue });
    await refreshLocalModels();
    await refreshLocalState();
  } catch (err) {
    console.error('download local model failed', err);
    emit('verenu:error', `Failed to start model download: ${err instanceof Error ? err.message : String(err)}`);
  }
}

export async function cancelLocalModelDownload(modelIdValue: string) {
  try {
    await invoke('cancel_local_stt_model_download', { modelId: modelIdValue });
    await refreshLocalModels();
    await refreshLocalState();
    delete localSttStore.downloadProgress[modelIdValue];
    localSttStore.downloadProgress = { ...localSttStore.downloadProgress };
    delete localSttStore.downloadStage[modelIdValue];
    localSttStore.downloadStage = { ...localSttStore.downloadStage };
  } catch (err) {
    console.error('cancel local model download failed', err);
  }
}

export async function deleteLocalModel(modelIdValue: string) {
  try {
    await invoke('delete_local_stt_model', { modelId: modelIdValue });
    await refreshLocalModels();
    await refreshLocalState();
    delete localSttStore.downloadProgress[modelIdValue];
    localSttStore.downloadProgress = { ...localSttStore.downloadProgress };
    delete localSttStore.downloadStage[modelIdValue];
    localSttStore.downloadStage = { ...localSttStore.downloadStage };
  } catch (err) {
    console.error('delete local model failed', err);
  }
}

export async function openLocalModelsFolder() {
  try {
    await invoke('open_local_models_folder');
  } catch (err) {
    console.error('open local models folder failed', err);
  }
}

const unlisteners: Array<() => void> = [];
let listenersStarted = false;

/**
 * Registers local-STT Tauri event listeners for the app's lifetime, independent
 * of any Settings sub-section's mount state. ModelsSection.svelte used to own
 * these listeners directly and tore them down in onDestroy — but Settings.svelte
 * wraps sub-sections in a `{#key section}` block, so switching tabs mid-download
 * killed the listeners and silently dropped the eventual completion event,
 * leaving the UI stuck showing stale download progress.
 */
export function startLocalSttListeners(): () => void {
  if (listenersStarted) {
    return () => {};
  }
  listenersStarted = true;

  (async () => {
    unlisteners.push(
      await listen<LocalSttDownloadProgressPayload>('local-stt-model-download-progress', (event) => {
        localSttStore.downloadProgress = {
          ...localSttStore.downloadProgress,
          [event.payload.model_id]: event.payload,
        };
        localSttStore.downloadStage = {
          ...localSttStore.downloadStage,
          [event.payload.model_id]: 'downloading',
        };
      }),
    );
    for (const [eventName, stage] of [
      ['local-stt-model-verification-started', 'verifying'],
      ['local-stt-model-extraction-started', 'extracting'],
    ] as const) {
      unlisteners.push(
        await listen<LocalSttModelEventPayload>(eventName, (event) => {
          if (!event.payload?.model_id) return;
          localSttStore.downloadStage = {
            ...localSttStore.downloadStage,
            [event.payload.model_id]: stage,
          };
        }),
      );
    }
    unlisteners.push(
      await listen<LocalSttVerificationProgressPayload>('local-stt-model-verification-progress', (event) => {
        localSttStore.downloadProgress = {
          ...localSttStore.downloadProgress,
          [event.payload.model_id]: {
            model_id: event.payload.model_id,
            downloaded_bytes: 0,
            // total_bytes stays non-null so the verifying bar renders as a
            // real determinate fraction, never the indeterminate animation.
            total_bytes: 1,
            progress: event.payload.progress,
          },
        };
        localSttStore.downloadStage = {
          ...localSttStore.downloadStage,
          [event.payload.model_id]: 'verifying',
        };
      }),
    );
    unlisteners.push(
      await listen<LocalSttExtractionProgressPayload>('local-stt-model-extraction-progress', (event) => {
        localSttStore.downloadProgress = {
          ...localSttStore.downloadProgress,
          [event.payload.model_id]: {
            model_id: event.payload.model_id,
            downloaded_bytes: 0,
            total_bytes: 1,
            progress: event.payload.progress,
          },
        };
        localSttStore.downloadStage = {
          ...localSttStore.downloadStage,
          [event.payload.model_id]: 'extracting',
        };
      }),
    );
    for (const eventName of [
      'local-stt-model-download-complete',
      'local-stt-model-download-failed',
      'local-stt-model-deleted',
    ]) {
      unlisteners.push(
        await listen<LocalSttModelEventPayload>(eventName, async (event) => {
          // Refresh first so `models`/`state` already reflect the final
          // is_downloading/is_downloaded truth by the time we clear the
          // progress/stage display — otherwise there's a render frame where
          // stage is cleared but is_downloading is still stale-true, which
          // falls back to a misleading "Downloading 0%" flash.
          await Promise.all([refreshLocalModels(), refreshLocalState()]);
          if (event.payload?.model_id) {
            delete localSttStore.downloadProgress[event.payload.model_id];
            localSttStore.downloadProgress = { ...localSttStore.downloadProgress };
            delete localSttStore.downloadStage[event.payload.model_id];
            localSttStore.downloadStage = { ...localSttStore.downloadStage };
          }
        }),
      );
    }
    unlisteners.push(
      await listen<Record<string, unknown>>('local-stt-model-state', async () => {
        await refreshLocalState();
      }),
    );
  })().catch((err) => console.error('local STT listeners failed', err));

  return () => {
    listenersStarted = false;
    for (const unlisten of unlisteners) unlisten();
    unlisteners.length = 0;
  };
}
