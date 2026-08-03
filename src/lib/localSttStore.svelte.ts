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
import { ensureNotificationPermission } from './notifications';

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

export async function downloadLocalModel(modelIdValue: string): Promise<boolean> {
  try {
    // Ask for notification permission now (contextual) so the backend can raise
    // a "model ready" notification when this finishes, even if the window is hidden.
    ensureNotificationPermission().catch(() => {});
    await invoke('download_local_stt_model', { modelId: modelIdValue });
    await refreshLocalModels();
    await refreshLocalState();
    return true;
  } catch (err) {
    console.error('download local model failed', err);
    emit('verenu:error', `Failed to start model download: ${err instanceof Error ? err.message : String(err)}`);
    return false;
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

let listenersStarted = false;
// Bumped on every `startLocalSttListeners` call. Captured per-invocation
// below so `registerUnlisten` can tell whether it belongs to the *current*
// session — if `startLocalSttListeners` is torn down and restarted while a
// previous invocation's `listen()` promises are still in-flight, those
// stale promises must unlisten immediately instead of registering into the
// new session's `unlisteners` array (or leaking forever).
let currentSession = 0;

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
  const session = ++currentSession;
  const unlisteners: Array<() => void> = [];

  function registerUnlisten(unlisten: () => void) {
    if (session !== currentSession) {
      unlisten();
    } else {
      unlisteners.push(unlisten);
    }
  }

  (async () => {
    registerUnlisten(
      await listen<LocalSttDownloadProgressPayload>('local-stt-model-download-progress', (event) => {
        if (!event.payload?.model_id) return;
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
      registerUnlisten(
        await listen<LocalSttModelEventPayload>(eventName, (event) => {
          if (!event.payload?.model_id) return;
          localSttStore.downloadStage = {
            ...localSttStore.downloadStage,
            [event.payload.model_id]: stage,
          };
        }),
      );
    }
    registerUnlisten(
      await listen<LocalSttVerificationProgressPayload>('local-stt-model-verification-progress', (event) => {
        if (!event.payload?.model_id) return;
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
    registerUnlisten(
      await listen<LocalSttExtractionProgressPayload>('local-stt-model-extraction-progress', (event) => {
        if (!event.payload?.model_id) return;
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
      registerUnlisten(
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
    registerUnlisten(
      await listen<Record<string, unknown>>('local-stt-model-state', async () => {
        await refreshLocalState();
      }),
    );
  })().catch((err) => {
    console.error('local STT listeners failed', err);
    // Registration didn't fully succeed — tear down whatever did register
    // and reset so a future call can retry instead of permanently returning
    // a no-op cleanup function forever.
    if (session === currentSession) {
      listenersStarted = false;
    }
    for (const unlisten of unlisteners) unlisten();
    unlisteners.length = 0;
  });

  return () => {
    if (session === currentSession) {
      listenersStarted = false;
    }
    currentSession += 1;
    for (const unlisten of unlisteners) unlisten();
    unlisteners.length = 0;
  };
}
