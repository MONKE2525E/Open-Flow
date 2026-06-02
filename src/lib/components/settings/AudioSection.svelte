<script lang="ts">
  import { invoke } from '../../tauri';
  import { slide, fly, fade } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { onDestroy } from 'svelte';
  import Toggle from '../Toggle.svelte';
  import { saveSetting } from '../../settings';
  import { MOTION_MS, MOTION_PX, motionMs, motionPx } from '../../motion';
  import { getAudioCalibrationCopy } from '../../calibrationCopy';
  import type { TranscriptionLanguageCode } from '../../transcriptionLanguages';

  import {
    isCalibrating,
    calibrationCountdown,
    calibratedGain,
    micLevel,
    startCalibration,
    cancelCalibration,
    speechDetected
  } from '../../calibration';

  let noiseReduction = $state(true);
  let micGain = $state(3.5);
  let selectedLanguage = $state<TranscriptionLanguageCode>('en');
  const audioCopy = $derived(getAudioCalibrationCopy(selectedLanguage));

  $effect(() => {
    if ($calibratedGain !== null) {
      micGain = $calibratedGain;
    }
  });

  async function loadSettings() {
    try {
      const [nr, savedGain, language] = await Promise.all([
        invoke<boolean | null>('get_setting', { key: 'noise_reduction' }),
        invoke<number | null>('get_setting', { key: 'mic_gain' }),
        invoke<TranscriptionLanguageCode | null>('get_setting', { key: 'transcription_language' }),
      ]);
      noiseReduction = nr ?? true;
      if (savedGain !== null && savedGain !== undefined) {
        micGain = Math.max(1, Math.min(8, savedGain));
      }
      if (language) selectedLanguage = language;
    } catch (err) {
      console.error('AudioSection load failed:', err);
    }
  }

  async function handleNoiseReduction(value: boolean) {
    noiseReduction = value;
    try {
      await saveSetting('noise_reduction', value);
    } catch (err) {
      noiseReduction = !value;
      console.error('save noise_reduction failed:', err);
    }
  }

  async function saveMicGain() {
    try {
      await saveSetting('mic_gain', micGain);
    } catch (err) {
      console.error('saveMicGain failed:', err);
    }
  }

  onDestroy(() => {
    cancelCalibration();
  });

  loadSettings();
</script>

<h2 class="settings-h">Advanced</h2>

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
    <div class="gain-tip" transition:slide={{ duration: motionMs(MOTION_MS.base) }}>
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
      At high gain, enable <strong>noise reduction</strong> to avoid amplifying background noise.
    </div>
  {/if}
</div>

<div class="setting-row">
  <div><div class="label">Noise reduction</div><div class="desc">Suppress background noise before transcription (RNNoise)</div></div>
  <Toggle checked={noiseReduction} onchange={handleNoiseReduction} label="Noise reduction" />
</div>

<div class="setting-row cal-row" class:calibrating={$isCalibrating}>
  <div class="cal-copy">
    <div class="label">Auto calibration</div>
    <div class="desc">Speak naturally to automatically set the ideal microphone gain</div>
    {#if $speechDetected === false}
      <div class="gain-tip" style="margin-top: 10px;" transition:slide={{ duration: motionMs(200) }}>
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <circle cx="12" cy="12" r="10"/>
          <line x1="12" x2="12" y1="8" y2="12"/>
          <line x1="12" x2="12" y1="16" y2="16"/>
        </svg>
        {audioCopy.noSpeechDetected}
      </div>
    {/if}
  </div>

  <div class="cal-control">
    {#if !$isCalibrating}
      <button class="btn-ghost cal-btn" onclick={startCalibration}
        out:fade={{ duration: motionMs(80) }}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/>
          <path d="M19 10v2a7 7 0 0 1-14 0v-2"/>
          <line x1="12" x2="12" y1="19" y2="22"/>
        </svg>
        <span>{audioCopy.autoCalibrateButton}</span>
      </button>
    {:else}
      <div class="cal-active-panel"
        in:fly={{ x: motionPx(MOTION_PX.lift), duration: motionMs(180), delay: motionMs(100), easing: expoOut }}>
        <span class="cal-timer">{$calibrationCountdown}s</span>
        <span class="cal-phrase-hint">{audioCopy.speakingHint}</span>
        <div class="cal-level-bar">
          <div class="cal-level-fill" style="width: {($micLevel * 100).toFixed(0)}%"></div>
        </div>
        <button class="cal-cancel-icon-btn" onclick={cancelCalibration} aria-label="Cancel calibration">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
            <line x1="18" y1="6" x2="6" y2="18"/>
            <line x1="6" y1="6" x2="18" y2="18"/>
          </svg>
        </button>
      </div>
    {/if}
  </div>
</div>

<style>

  /* Calibration row styles */
  .cal-row {
    align-items: center;
    transition: border-top-color 90ms ease;
  }
  .cal-row.calibrating {
    border-top-color: transparent;
  }
  .cal-copy {
    flex: 1;
    min-width: 0;
    max-height: 80px;
    overflow: hidden;
    transition:
      flex-basis 260ms cubic-bezier(0.16, 1, 0.3, 1),
      max-height 260ms cubic-bezier(0.16, 1, 0.3, 1),
      opacity 90ms ease;
  }
  .cal-row.calibrating .cal-copy {
    flex-basis: 0;
    max-height: 0;
    opacity: 0;
    pointer-events: none;
  }
  .cal-control {
    flex-shrink: 0;
    width: 136px;
    overflow: hidden;
    transition: width 280ms cubic-bezier(0.16, 1, 0.3, 1);
  }
  .cal-row.calibrating .cal-control {
    width: min(440px, 100%);
  }
  .cal-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 32px;
    padding: 0 14px;
    border-radius: var(--r-md);
    border: 1px solid var(--accent);
    color: var(--accent);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    width: 100%;
    background: transparent;
    transition:
      background 0.15s ease,
      color 0.15s ease,
      transform 0.1s ease;
  }
  .cal-btn:hover {
    background: var(--accent-soft);
    color: var(--accent-ink);
  }
  .cal-btn:active {
    transform: scale(0.985);
  }
  .cal-active-panel {
    display: flex;
    align-items: center;
    gap: 12px;
    background: var(--paper-2);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    padding: 4px 12px;
    height: 32px;
    width: 100%;
  }
  .cal-timer {
    font-family: var(--mono);
    font-weight: 600;
    color: var(--accent);
    font-size: 13px;
  }
  .cal-phrase-hint {
    font-size: 12px;
    color: var(--ink-soft);
    font-style: italic;
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cal-level-bar {
    width: 80px;
    height: 6px;
    background: var(--line-strong);
    border-radius: 999px;
    overflow: hidden;
    flex-shrink: 0;
  }
  .cal-level-fill {
    height: 100%;
    background: var(--accent);
    border-radius: 999px;
    transition: width 80ms ease-out;
  }
  .cal-cancel-icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    color: var(--ink-mute);
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 0;
    margin-left: 4px;
    flex-shrink: 0;
    transition:
      color 0.15s ease,
      background 0.15s ease;
  }
  .cal-cancel-icon-btn:hover {
    color: var(--ink-strong);
    background: var(--paper-3);
  }
  @media (prefers-reduced-motion: reduce) {
    .cal-row,
    .cal-copy,
    .cal-control,
    .cal-level-fill,
    .cal-btn { transition: none !important; }
  }

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
</style>
