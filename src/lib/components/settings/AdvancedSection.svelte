<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { slide } from 'svelte/transition';
  import Toggle from '../Toggle.svelte';
  import { saveSetting, type SettingKey } from '../../settings';

  let toggles = $state({
    cleanup: true,
    contextualCaps: true,
    noiseReduction: true,
    apiFallback: false,
    autoLearn: false,
  });
  let micGain = $state(3.5);

  async function loadSettings() {
    try {
      const [cleanup, contextualCaps, noiseReduction, apiFallback, autoLearn, savedGain] =
        await Promise.all([
          invoke<boolean | null>('get_setting', { key: 'cleanup_enabled' }),
          invoke<boolean | null>('get_setting', { key: 'contextual_caps_enabled' }),
          invoke<boolean | null>('get_setting', { key: 'noise_reduction' }),
          invoke<boolean | null>('get_setting', { key: 'api_fallback_enabled' }),
          invoke<boolean | null>('get_setting', { key: 'auto_learn_enabled' }),
          invoke<number | null>('get_setting', { key: 'mic_gain' }),
        ]);
      toggles = {
        cleanup: cleanup ?? true,
        contextualCaps: contextualCaps ?? true,
        noiseReduction: noiseReduction ?? true,
        apiFallback: apiFallback ?? false,
        autoLearn: autoLearn ?? false,
      };
      if (savedGain !== null && savedGain !== undefined) {
        micGain = Math.max(1, Math.min(8, savedGain));
      }
    } catch (err) {
      console.error('AdvancedSection load failed:', err);
    }
  }

  async function handleToggle(key: string, value: boolean) {
    toggles = { ...toggles, [key]: value };
    const invokeKey: Record<string, SettingKey> = {
      cleanup: 'cleanup_enabled',
      contextualCaps: 'contextual_caps_enabled',
      noiseReduction: 'noise_reduction',
      apiFallback: 'api_fallback_enabled',
      autoLearn: 'auto_learn_enabled',
    };
    try {
      await saveSetting(invokeKey[key], value);
    } catch (err) {
      toggles = { ...toggles, [key]: !value };
      console.error(`save toggle ${key} failed:`, err);
    }
  }

  async function saveMicGain() {
    try {
      await saveSetting('mic_gain', micGain);
    } catch (err) {
      console.error('saveMicGain failed:', err);
    }
  }

  loadSettings();
</script>

<h2 class="settings-h">Advanced</h2>
<div class="setting-row">
  <div><div class="label">Auto-cleanup</div><div class="desc">Run LLM cleanup on every transcription</div></div>
  <Toggle checked={toggles.cleanup} onchange={(v) => handleToggle('cleanup', v)} />
</div>
<div class="setting-row">
  <div><div class="label">Contextual capitalization</div><div class="desc">Lowercases the first word when injecting mid-sentence</div></div>
  <Toggle checked={toggles.contextualCaps} onchange={(v) => handleToggle('contextualCaps', v)} />
</div>
<div class="setting-row gain-row">
  <div class="gain-header">
    <div>
      <div class="label">Microphone gain</div>
      <div class="desc">Boost signal strength before sending audio to the voice model</div>
    </div>
    <span class="gain-value">{micGain.toFixed(1)}×</span>
  </div>
  <div class="gain-slider-wrap">
    <input
      type="range"
      class="gain-slider"
      min="1" max="8" step="0.1"
      bind:value={micGain}
      oninput={saveMicGain}
      style="--pct: {((micGain - 1) / 7 * 100).toFixed(1)}%"
      aria-label="Microphone gain"
    />
    <div class="gain-ticks">
      <span>1×</span>
      <span>4×</span>
      <span>8×</span>
    </div>
  </div>
  {#if micGain >= 5}
    <div class="gain-tip" transition:slide={{ duration: 220 }}>
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
      At high gain, enable <strong>noise reduction</strong> to avoid amplifying background noise.
    </div>
  {/if}
</div>
<div class="setting-row">
  <div><div class="label">Noise reduction</div><div class="desc">Suppress background noise before transcription (RNNoise)</div></div>
  <Toggle checked={toggles.noiseReduction} onchange={(v) => handleToggle('noiseReduction', v)} />
</div>
<div class="setting-row">
  <div>
    <div class="label">API fallback</div>
    <div class="desc">If your primary provider hits its quota, automatically retry with another configured API key</div>
  </div>
  <Toggle checked={toggles.apiFallback} onchange={(v) => handleToggle('apiFallback', v)} />
</div>
<div class="setting-row">
  <div>
    <div class="label" style="display:flex;align-items:center;gap:7px;">
      Auto-learn corrections
      <span class="privacy-eye-wrap">
        <svg class="privacy-eye" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
          <circle cx="12" cy="12" r="3"/>
        </svg>
        <span class="privacy-tooltip">Entirely on-device — no text is sent to any API.</span>
      </span>
    </div>
    <div class="desc">Add confirmed corrections to dictionary automatically</div>
  </div>
  <Toggle checked={toggles.autoLearn} onchange={(v) => handleToggle('autoLearn', v)} />
</div>

<style>
  .gain-row { flex-direction: column; align-items: stretch; gap: 0; }
  .gain-header { display: flex; align-items: center; justify-content: space-between; width: 100%; }
  .gain-value {
    font-family: var(--mono);
    font-size: 13px;
    font-weight: 500;
    color: var(--accent);
    min-width: 36px;
    text-align: right;
    flex-shrink: 0;
  }
  .gain-slider-wrap { margin-top: 10px; width: 100%; }
  .gain-slider {
    -webkit-appearance: none;
    appearance: none;
    width: 100%;
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(
      to right,
      var(--accent) 0%, var(--accent) var(--pct),
      var(--line-strong) var(--pct), var(--line-strong) 100%
    );
    outline: none;
    cursor: pointer;
    border: none;
    display: block;
  }
  .gain-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--bg-elev);
    border: 2px solid var(--accent);
    box-shadow: 0 1px 4px color-mix(in srgb, var(--accent) 35%, transparent);
    cursor: pointer;
    transition: box-shadow 0.15s ease, transform 0.15s ease;
  }
  .gain-slider::-webkit-slider-thumb:hover { box-shadow: 0 2px 8px color-mix(in srgb, var(--accent) 45%, transparent); transform: scale(1.1); }
  .gain-slider::-webkit-slider-thumb:active { box-shadow: 0 2px 10px color-mix(in srgb, var(--accent) 55%, transparent); transform: scale(1.15); }
  .gain-slider::-moz-range-thumb {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--bg-elev);
    border: 2px solid var(--accent);
    box-shadow: 0 1px 4px color-mix(in srgb, var(--accent) 35%, transparent);
    cursor: pointer;
  }
  .gain-ticks { display: flex; justify-content: space-between; margin-top: 5px; font-size: 10px; color: var(--ink-mute); font-family: var(--mono); user-select: none; }
  .gain-tip {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 10px;
    padding: 7px 10px;
    background: var(--warning-bg);
    border: 1px solid var(--warning-line);
    border-radius: 7px;
    font-size: 11.5px;
    color: var(--warning);
    line-height: 1.4;
  }
  .gain-tip svg { flex-shrink: 0; color: var(--warning); }
  .gain-tip strong { font-weight: 600; }
  .privacy-eye-wrap { position: relative; display: inline-flex; align-items: center; }
  .privacy-eye { color: var(--ink-mute); cursor: default; flex-shrink: 0; transition: color 0.15s ease, transform 0.15s ease; }
  .privacy-eye-wrap:hover .privacy-eye { color: var(--ink-soft); transform: scale(1.18); }
  .privacy-tooltip {
    position: absolute;
    left: 50%;
    bottom: calc(100% + 7px);
    transform: translateX(-50%) translateY(4px);
    background: var(--ink);
    color: var(--paper);
    font-size: 11px;
    font-family: var(--sans);
    font-weight: 400;
    white-space: nowrap;
    padding: 4px 9px;
    border-radius: 6px;
    pointer-events: none;
    z-index: 20;
    box-shadow: var(--shadow-popover);
    opacity: 0;
    transition: opacity 0.16s ease, transform 0.16s ease;
  }
  .privacy-eye-wrap:hover .privacy-tooltip { opacity: 1; transform: translateX(-50%) translateY(0); }
</style>
