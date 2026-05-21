<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { saveSetting, type ProviderId } from '../../settings';
  import { MOTION_MS, motionMs } from '../../motion';

  const transcriptionModels = [
    { id: 'groq/whisper-large-v3-turbo', provider: 'Groq',   name: 'whisper-large-v3-turbo', note: '~0.5s · free tier · recommended', recommended: true },
    { id: 'openai/gpt-4o-transcribe',    provider: 'OpenAI', name: 'gpt-4o-transcribe',       note: '~1s · best accuracy for accents & noise', recommended: false },
    { id: 'google/gemini-3.5-flash',     provider: 'Google', name: 'gemini-3.5-flash',        note: '~3s · slow for transcription, not recommended', recommended: false },
  ];

  const cleanupModels = [
    { id: 'groq/llama-3.3-70b-versatile', provider: 'Groq',   name: 'llama-3.3-70b-versatile', note: '~0.3s · free tier · recommended', recommended: true },
    { id: 'openai/gpt-4o-mini',           provider: 'OpenAI', name: 'gpt-4o-mini',              note: '~0.5s · best cost/quality balance', recommended: false },
    { id: 'google/gemini-3.5-flash',      provider: 'Google', name: 'gemini-3.5-flash',          note: '~1s · fused with transcription when both Google', recommended: false },
  ];

  let transcriptionModel = $state('groq/whisper-large-v3-turbo');
  let cleanupModel = $state('groq/llama-3.3-70b-versatile');
  let transcriptionListEl = $state<HTMLDivElement | null>(null);
  let cleanupListEl = $state<HTMLDivElement | null>(null);
  let transcriptionRowEls = $state<Record<string, HTMLButtonElement | null>>({});
  let cleanupRowEls = $state<Record<string, HTMLButtonElement | null>>({});
  let transcriptionIndicatorStyle = $state('opacity:0;');
  let cleanupIndicatorStyle = $state('opacity:0;');

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
    setTimeout(updateTranscriptionIndicator, 0);
  }

  async function setCleanupModel(id: string) {
    cleanupModel = id;
    try {
      await saveSetting('cleanup_model', id);
      await saveSetting('cleanup_provider', id.split('/')[0] as ProviderId);
    } catch (err) {
      console.error('setCleanupModel failed:', err);
    }
    setTimeout(updateCleanupIndicator, 0);
  }

  function buildIndicatorStyle(listEl: HTMLDivElement | null, rowEl: HTMLButtonElement | null) {
    if (!listEl || !rowEl) return 'opacity:0;';
    // Use layout offsets instead of viewport rects to avoid subpixel rounding drift.
    const top = Math.max(0, rowEl.offsetTop - 1);
    const height = rowEl.offsetHeight + 2;
    return `opacity:1; transform:translateY(${top}px); height:${height}px; transition: transform ${motionMs(MOTION_MS.panel)}ms cubic-bezier(0.22, 1, 0.36, 1), height ${motionMs(MOTION_MS.panel)}ms cubic-bezier(0.22, 1, 0.36, 1), opacity ${motionMs(MOTION_MS.fast)}ms ease;`;
  }

  function updateTranscriptionIndicator() {
    transcriptionIndicatorStyle = buildIndicatorStyle(transcriptionListEl, transcriptionRowEls[transcriptionModel] ?? null);
  }

  function updateCleanupIndicator() {
    cleanupIndicatorStyle = buildIndicatorStyle(cleanupListEl, cleanupRowEls[cleanupModel] ?? null);
  }

  $effect(() => {
    transcriptionModel;
    setTimeout(updateTranscriptionIndicator, 0);
  });

  $effect(() => {
    cleanupModel;
    setTimeout(updateCleanupIndicator, 0);
  });

  onMount(() => {
    updateTranscriptionIndicator();
    updateCleanupIndicator();
    const onResize = () => {
      updateTranscriptionIndicator();
      updateCleanupIndicator();
    };
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  });

  loadModels();
</script>

<h2 class="settings-h">Models</h2>

<div class="model-section-label">Transcription</div>
<div class="model-desc">Converts audio to raw text</div>
<div class="model-list" bind:this={transcriptionListEl}>
  <span class="model-active-indicator" style={transcriptionIndicatorStyle}></span>
  {#each transcriptionModels as m}
    <button
      class="model-row"
      class:active={transcriptionModel === m.id}
      bind:this={transcriptionRowEls[m.id]}
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

<div class="model-section-label model-section-label-spaced">Cleanup LLM</div>
<div class="model-desc">Rewrites and formats each transcription</div>
<div class="model-list" bind:this={cleanupListEl}>
  <span class="model-active-indicator" style={cleanupIndicatorStyle}></span>
  {#each cleanupModels as m}
    <button
      class="model-row"
      class:active={cleanupModel === m.id}
      bind:this={cleanupRowEls[m.id]}
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
  .model-list { border: 1px solid var(--line); border-radius: var(--r-sm); overflow: hidden; position: relative; }
  .model-active-indicator {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    background: linear-gradient(90deg, color-mix(in srgb, var(--accent) 16%, transparent), color-mix(in srgb, var(--accent) 10%, transparent));
    pointer-events: none;
    z-index: 0;
    opacity: 0;
    border-radius: 0;
  }
  .model-section-label-spaced { margin-top: 20px; }
  .model-row {
    appearance: none;
    -webkit-appearance: none;
    width: 100%;
    margin: 0;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    border: none;
    border-radius: 0;
    border-bottom: 1px solid var(--line);
    background: var(--bg-elev);
    cursor: pointer;
    text-align: left;
    transition: background 0.16s cubic-bezier(0.22, 1, 0.36, 1), color 0.16s cubic-bezier(0.22, 1, 0.36, 1);
    font-family: var(--sans);
    position: relative;
    z-index: 1;
  }
  .model-row:last-child { border-bottom: none; }
  .model-row:hover { background: var(--paper); }
  .model-row.active { background: transparent; }
  .model-radio {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 1.5px solid var(--line-strong);
    flex-shrink: 0;
    position: relative;
    transition: border-color 0.16s cubic-bezier(0.22, 1, 0.36, 1), transform 0.16s cubic-bezier(0.22, 1, 0.36, 1);
  }
  .model-row.active .model-radio { border-color: var(--accent); transform: scale(1.05); }
  .model-row.active .model-radio::after {
    content: '';
    position: absolute;
    inset: 2px;
    background: var(--accent);
    border-radius: 50%;
    animation: radioPop 0.2s cubic-bezier(0.22, 1, 0.36, 1);
  }
  @keyframes radioPop {
    from { transform: scale(0.6); opacity: 0.4; }
    to { transform: scale(1); opacity: 1; }
  }
  .model-name { font-size: 12.5px; font-weight: 500; color: var(--ink-strong); line-height: 1.3; }
  .model-provider { color: var(--ink-mute); font-weight: 400; }
  .model-info { flex: 1; min-width: 0; }
  .model-note { font-size: 11px; color: var(--ink-mute); font-family: var(--mono); margin-top: 1px; transition: color 0.16s cubic-bezier(0.22, 1, 0.36, 1); }
  .model-row.active .model-note { color: var(--ink-soft); }
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
    transition: transform 0.16s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.16s cubic-bezier(0.22, 1, 0.36, 1);
  }
  .model-row.active .model-badge { transform: translateY(-1px); }
</style>
