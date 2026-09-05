import { invoke } from './tauri';

/**
 * App-lifetime store for the currently selected transcription model
 * (`"provider/model"`, e.g. `"groq/whisper-large-v3-turbo"` or
 * `"local/parakeet-v3"`). Mirrors the `localSttStore.svelte.ts` pattern —
 * `transcription_default_model` was previously only local `$state` inside
 * `ModelsSection.svelte`, unreachable from other Settings sections (e.g.
 * GeneralSection's language filtering needs to read it live).
 */
export const transcriptionModelStore = $state({ defaultModel: 'groq/whisper-large-v3-turbo' });

export async function refreshTranscriptionModel() {
  try {
    const value = await invoke<string | null>('get_setting', { key: 'transcription_default_model' });
    if (value) transcriptionModelStore.defaultModel = value;
  } catch (err) {
    console.error('refresh transcription model failed', err);
  }
}
