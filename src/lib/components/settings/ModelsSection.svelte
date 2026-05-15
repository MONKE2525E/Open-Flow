<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { saveSetting, type ProviderId } from '../../settings';

  const transcriptionModels = [
    { id: 'groq/whisper-large-v3-turbo', provider: 'Groq',   name: 'whisper-large-v3-turbo', note: '~0.5s · free tier · recommended', recommended: true },
    { id: 'openai/gpt-4o-transcribe',    provider: 'OpenAI', name: 'gpt-4o-transcribe',       note: '~1s · best accuracy for accents & noise', recommended: false },
    { id: 'google/gemini-2.5-flash',     provider: 'Google', name: 'gemini-2.5-flash',        note: '~3s · slow for transcription, not recommended', recommended: false },
  ];

  const cleanupModels = [
    { id: 'groq/llama-3.3-70b-versatile', provider: 'Groq',   name: 'llama-3.3-70b-versatile', note: '~0.3s · free tier · recommended', recommended: true },
    { id: 'openai/gpt-4o-mini',           provider: 'OpenAI', name: 'gpt-4o-mini',              note: '~0.5s · best cost/quality balance', recommended: false },
    { id: 'google/gemini-2.5-flash',      provider: 'Google', name: 'gemini-2.5-flash',          note: '~1s · fused with transcription when both Google', recommended: false },
  ];

  let transcriptionModel = $state('groq/whisper-large-v3-turbo');
  let cleanupModel = $state('groq/llama-3.3-70b-versatile');

  async function loadModels() {
    try {
      const [tModel, cModel] = await Promise.all([
        invoke<string | null>('get_setting', { key: 'transcription_model' }),
        invoke<string | null>('get_setting', { key: 'cleanup_model' }),
      ]);
      if (tModel) transcriptionModel = tModel;
      if (cModel) cleanupModel = cModel;
    } catch (err) {
      console.error('loadModels failed:', err);
    }
  }

  async function setTranscriptionModel(id: string) {
    transcriptionModel = id;
    try {
      await saveSetting('transcription_model', id);
      await saveSetting('transcription_provider', id.split('/')[0] as ProviderId);
    } catch (err) {
      console.error('setTranscriptionModel failed:', err);
    }
  }

  async function setCleanupModel(id: string) {
    cleanupModel = id;
    try {
      await saveSetting('cleanup_model', id);
      await saveSetting('cleanup_provider', id.split('/')[0] as ProviderId);
    } catch (err) {
      console.error('setCleanupModel failed:', err);
    }
  }

  loadModels();
</script>

<h2 class="settings-h">Models</h2>

<div class="model-section-label">Transcription</div>
<div class="model-desc">Converts audio to raw text</div>
<div class="model-list">
  {#each transcriptionModels as m}
    <button
      class="model-row"
      class:active={transcriptionModel === m.id}
      onclick={() => setTranscriptionModel(m.id)}
    >
      <div class="model-radio"></div>
      <div class="model-info">
        <div class="model-name">
          <span class="model-provider">{m.provider}</span> {m.name}
          {#if m.recommended}<span class="model-badge">recommended</span>{/if}
        </div>
        <div class="model-note">{m.note}</div>
      </div>
    </button>
  {/each}
</div>

<div class="model-section-label" style="margin-top:20px">Cleanup LLM</div>
<div class="model-desc">Rewrites and formats each transcription</div>
<div class="model-list">
  {#each cleanupModels as m}
    <button
      class="model-row"
      class:active={cleanupModel === m.id}
      onclick={() => setCleanupModel(m.id)}
    >
      <div class="model-radio"></div>
      <div class="model-info">
        <div class="model-name">
          <span class="model-provider">{m.provider}</span> {m.name}
          {#if m.recommended}<span class="model-badge">recommended</span>{/if}
        </div>
        <div class="model-note">{m.note}</div>
      </div>
    </button>
  {/each}
</div>

<style>
  .model-section-label { font-size: 12px; font-weight: 500; color: var(--ink-strong); margin-bottom: 2px; }
  .model-desc { font-size: 12px; color: var(--ink-mute); margin-bottom: 10px; }
  .model-list { border: 1px solid var(--line); border-radius: var(--r-sm); overflow: hidden; }
  .model-row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    border: none;
    border-bottom: 1px solid var(--line);
    background: var(--bg-elev);
    cursor: pointer;
    text-align: left;
    transition: background 0.12s;
    font-family: var(--sans);
  }
  .model-row:last-child { border-bottom: none; }
  .model-row:hover { background: var(--paper); }
  .model-row.active { background: var(--accent-soft); }
  .model-radio {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 1.5px solid var(--line-strong);
    flex-shrink: 0;
    position: relative;
    transition: border-color 0.12s;
  }
  .model-row.active .model-radio { border-color: var(--accent); }
  .model-row.active .model-radio::after {
    content: '';
    position: absolute;
    inset: 2px;
    background: var(--accent);
    border-radius: 50%;
  }
  .model-name { font-size: 12.5px; font-weight: 500; color: var(--ink-strong); line-height: 1.3; }
  .model-provider { color: var(--ink-mute); font-weight: 400; }
  .model-info { flex: 1; min-width: 0; }
  .model-note { font-size: 11px; color: var(--ink-mute); font-family: var(--mono); margin-top: 1px; }
  .model-badge {
    display: inline-block;
    font-size: 9.5px;
    font-weight: 500;
    font-family: var(--mono);
    letter-spacing: 0.04em;
    color: var(--success);
    background: var(--success-bg);
    border: 1px solid var(--success-line);
    border-radius: 4px;
    padding: 1px 5px;
    margin-left: 6px;
    vertical-align: middle;
    text-transform: uppercase;
  }
</style>
