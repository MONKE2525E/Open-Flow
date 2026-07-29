import {
  invoke,
  listen,
  emit,
  type LocalLlmDownloadProgressPayload,
  type LocalLlmModelEventPayload,
  type LocalLlmModelInfo,
  type LocalLlmRuntimeDownloadProgressPayload,
  type LocalLlmRuntimeEventPayload,
  type LocalLlmRuntimeInfo,
  type LocalLlmState,
  type LocalLlmVerificationProgressPayload,
} from './tauri';

export const localLlmStore = $state({
  models: [] as LocalLlmModelInfo[],
  state: {
    current_model_id: null,
    is_loaded: false,
    is_loading: false,
    is_downloading: false,
    downloading_model_id: null,
    endpoint: null,
  } as LocalLlmState,
  downloadProgress: {} as Record<string, LocalLlmDownloadProgressPayload | undefined>,
  downloadStage: {} as Record<string, 'downloading' | 'verifying' | undefined>,
  runtime: {
    installed: false,
    is_downloading: false,
    backend: 'vulkan',
    approx_download_mb: 0,
  } as LocalLlmRuntimeInfo,
  runtimeDownloadProgress: undefined as LocalLlmRuntimeDownloadProgressPayload | undefined,
});

export async function refreshLocalLlmModels() {
  try {
    localLlmStore.models = await invoke<LocalLlmModelInfo[]>('list_local_llm_models');
  } catch (err) {
    console.error('load local cleanup models failed', err);
  }
}

export async function refreshLocalLlmState() {
  try {
    localLlmStore.state = await invoke<LocalLlmState>('get_local_llm_state');
  } catch (err) {
    console.error('load local cleanup state failed', err);
  }
}

export async function refreshLocalLlmRuntimeInfo() {
  try {
    localLlmStore.runtime = await invoke<LocalLlmRuntimeInfo>('get_local_llm_runtime_info');
  } catch (err) {
    console.error('load local cleanup runtime info failed', err);
  }
}

export async function downloadLocalLlmRuntime() {
  try {
    await invoke('download_local_llm_runtime');
    await refreshLocalLlmRuntimeInfo();
  } catch (err) {
    console.error('download local cleanup runtime failed', err);
    emit('verenu:error', `Failed to start runtime download: ${err instanceof Error ? err.message : String(err)}`);
  }
}

export async function cancelLocalLlmRuntimeDownload() {
  try {
    await invoke('cancel_local_llm_runtime_download');
    await refreshLocalLlmRuntimeInfo();
    localLlmStore.runtimeDownloadProgress = undefined;
  } catch (err) {
    console.error('cancel local cleanup runtime download failed', err);
  }
}

export async function deleteLocalLlmRuntime() {
  try {
    await invoke('delete_local_llm_runtime');
    await refreshLocalLlmRuntimeInfo();
    localLlmStore.runtimeDownloadProgress = undefined;
  } catch (err) {
    console.error('delete local cleanup runtime failed', err);
  }
}

export async function downloadLocalLlmModel(modelIdValue: string) {
  try {
    await invoke('download_local_llm_model', { modelId: modelIdValue });
    await Promise.all([refreshLocalLlmModels(), refreshLocalLlmState()]);
  } catch (err) {
    console.error('download local cleanup model failed', err);
    emit('verenu:error', `Failed to start model download: ${err instanceof Error ? err.message : String(err)}`);
  }
}

export async function cancelLocalLlmModelDownload(modelIdValue: string) {
  try {
    await invoke('cancel_local_llm_model_download', { modelId: modelIdValue });
    await Promise.all([refreshLocalLlmModels(), refreshLocalLlmState()]);
    delete localLlmStore.downloadProgress[modelIdValue];
    localLlmStore.downloadProgress = { ...localLlmStore.downloadProgress };
    delete localLlmStore.downloadStage[modelIdValue];
    localLlmStore.downloadStage = { ...localLlmStore.downloadStage };
  } catch (err) {
    console.error('cancel local cleanup model download failed', err);
  }
}

export async function deleteLocalLlmModel(modelIdValue: string) {
  try {
    await invoke('delete_local_llm_model', { modelId: modelIdValue });
    await Promise.all([refreshLocalLlmModels(), refreshLocalLlmState()]);
    delete localLlmStore.downloadProgress[modelIdValue];
    localLlmStore.downloadProgress = { ...localLlmStore.downloadProgress };
    delete localLlmStore.downloadStage[modelIdValue];
    localLlmStore.downloadStage = { ...localLlmStore.downloadStage };
  } catch (err) {
    console.error('delete local cleanup model failed', err);
  }
}

const unlisteners: Array<() => void> = [];
let listenersStarted = false;
// True once the cleanup function below has run. `listen()` calls still
// in-flight at that point would otherwise push their unlisten functions into
// `unlisteners` *after* it's already been cleared, leaking those listeners
// forever instead of tearing them down — `registerUnlisten` catches that by
// unlistening immediately instead of queuing when teardown already happened.
let tornDown = false;

function registerUnlisten(unlisten: () => void) {
  if (tornDown) {
    unlisten();
  } else {
    unlisteners.push(unlisten);
  }
}

export function startLocalLlmListeners(): () => void {
  if (listenersStarted) {
    return () => {};
  }
  listenersStarted = true;
  tornDown = false;

  (async () => {
    registerUnlisten(
      await listen<LocalLlmDownloadProgressPayload>('local-llm-model-download-progress', (event) => {
        localLlmStore.downloadProgress = {
          ...localLlmStore.downloadProgress,
          [event.payload.model_id]: event.payload,
        };
        localLlmStore.downloadStage = {
          ...localLlmStore.downloadStage,
          [event.payload.model_id]: 'downloading',
        };
      }),
    );
    registerUnlisten(
      await listen<LocalLlmModelEventPayload>('local-llm-model-verification-started', (event) => {
        if (!event.payload?.model_id) return;
        localLlmStore.downloadStage = {
          ...localLlmStore.downloadStage,
          [event.payload.model_id]: 'verifying',
        };
      }),
    );
    registerUnlisten(
      await listen<LocalLlmVerificationProgressPayload>('local-llm-model-verification-progress', (event) => {
        if (!event.payload?.model_id) return;
        localLlmStore.downloadProgress = {
          ...localLlmStore.downloadProgress,
          [event.payload.model_id]: {
            model_id: event.payload.model_id,
            downloaded_bytes: 0,
            // Non-null total keeps the verifying bar determinate.
            total_bytes: 1,
            progress: event.payload.progress,
          },
        };
        localLlmStore.downloadStage = {
          ...localLlmStore.downloadStage,
          [event.payload.model_id]: 'verifying',
        };
      }),
    );
    for (const eventName of [
      'local-llm-model-download-complete',
      'local-llm-model-download-failed',
      'local-llm-model-deleted',
    ]) {
      registerUnlisten(
        await listen<LocalLlmModelEventPayload>(eventName, async (event) => {
          await Promise.all([refreshLocalLlmModels(), refreshLocalLlmState()]);
          if (event.payload?.model_id) {
            delete localLlmStore.downloadProgress[event.payload.model_id];
            localLlmStore.downloadProgress = { ...localLlmStore.downloadProgress };
            delete localLlmStore.downloadStage[event.payload.model_id];
            localLlmStore.downloadStage = { ...localLlmStore.downloadStage };
          }
        }),
      );
    }
    registerUnlisten(
      await listen<Record<string, unknown>>('local-llm-model-state', async () => {
        await refreshLocalLlmState();
      }),
    );
    registerUnlisten(
      await listen<LocalLlmRuntimeDownloadProgressPayload>('local-llm-runtime-download-progress', (event) => {
        localLlmStore.runtimeDownloadProgress = event.payload;
      }),
    );
    for (const eventName of [
      'local-llm-runtime-download-complete',
      'local-llm-runtime-download-failed',
      'local-llm-runtime-deleted',
    ]) {
      registerUnlisten(
        await listen<LocalLlmRuntimeEventPayload>(eventName, async () => {
          await refreshLocalLlmRuntimeInfo();
          localLlmStore.runtimeDownloadProgress = undefined;
        }),
      );
    }
  })().catch((err) => {
    console.error('local cleanup listeners failed', err);
    // Registration didn't fully succeed — tear down whatever did register
    // and reset so a future call can retry instead of permanently returning
    // a no-op cleanup function forever.
    listenersStarted = false;
    for (const unlisten of unlisteners) unlisten();
    unlisteners.length = 0;
  });

  return () => {
    listenersStarted = false;
    tornDown = true;
    for (const unlisten of unlisteners) unlisten();
    unlisteners.length = 0;
  };
}
