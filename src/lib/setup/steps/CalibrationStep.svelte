<script lang="ts">
  import { onDestroy } from 'svelte';
  import { fly, fade, slide } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { motionMs } from '../../motion';
  import { getSetupCalibrationCopy } from '../../calibrationCopy';
  import type { TranscriptionLanguageCode } from '../../transcriptionLanguages';
  import {
    isCalibrating,
    calibrationCountdown,
    calibratedGain,
    micLevel,
    startCalibration,
    cancelCalibration,
    speechDetected,
    calibrationPhase,
  } from '../../calibration';

  let { language }: { language: TranscriptionLanguageCode } = $props();

  let copy = $derived(getSetupCalibrationCopy(language));

  onDestroy(() => {
    cancelCalibration();
  });
</script>

<div class="step">
  <div class="calibration-box">
    {#if !$isCalibrating && $calibratedGain === null}
      <div class="cal-start-state" out:fade={{ duration: motionMs(160) }}>
        <div class="cal-steps-preview">
          <div class="cal-step-row">
            <span class="cal-step-num">1</span>
            <span class="cal-step-text">{copy.step1Text}</span>
          </div>
          <div class="cal-step-row">
            <span class="cal-step-num">2</span>
            <span class="cal-step-text">{copy.step2Text}</span>
          </div>
        </div>
        <button class="btn-primary" onclick={startCalibration}>{copy.startButton}</button>
      </div>
    {:else if $isCalibrating}
      <div class="cal-active-state" in:slide={{ duration: motionMs(260), easing: expoOut }} out:fade={{ duration: motionMs(160) }}>
        <div class="cal-phase-header">
          <div class="cal-label-stack">
            {#key $calibrationPhase}
              <span class="cal-phase-label" in:fade={{ duration: motionMs(200), delay: motionMs(80) }} out:fade={{ duration: motionMs(80) }}>
                {$calibrationPhase === 'loud' ? copy.phase1Label : copy.phase2Label}
              </span>
            {/key}
          </div>
          <div class="cal-timer-ring">
            <span class="cal-countdown">{$calibrationCountdown}s</span>
          </div>
        </div>
        <div class="cal-content-stack">
          {#key $calibrationPhase}
            <div class="cal-phase-content" in:fade={{ duration: motionMs(200), delay: motionMs(80) }} out:fade={{ duration: motionMs(80) }}>
              <p class="cal-prompt">{$calibrationPhase === 'loud' ? copy.readPrompt : copy.whisperPrompt}</p>
              <blockquote class="cal-phrase">"{$calibrationPhase === 'loud' ? copy.readPhrase : copy.whisperPhrase}"</blockquote>
            </div>
          {/key}
        </div>

        <div class="cal-meter-container">
          <div class="cal-meter-track">
            <div class="cal-meter-fill" style="width: {($micLevel * 100).toFixed(0)}%"></div>
          </div>
        </div>
        <button class="cal-cancel-btn" onclick={cancelCalibration} in:fly={{ y: 6, duration: motionMs(200), delay: motionMs(240), easing: expoOut }}>
          {copy.cancelButton}
        </button>
      </div>
    {:else if $calibratedGain !== null}
      <div class="cal-result-state" in:slide={{ duration: motionMs(280), easing: expoOut }}>
        {#if $speechDetected === false}
          <div class="cal-warning-icon">
            <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <circle cx="12" cy="12" r="10"/>
              <line x1="12" x2="12" y1="8" y2="12"/>
              <line x1="12" x2="12" y1="16" y2="16"/>
            </svg>
          </div>
          <h3 class="cal-result-title">{copy.silenceTitle}</h3>
          <p class="cal-result-desc">{copy.silenceDescription} <strong>{$calibratedGain.toFixed(1)}×</strong>.</p>
        {:else}
          <div class="cal-success-icon">
            <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/>
              <polyline points="22 4 12 14.01 9 11.01"/>
            </svg>
          </div>
          <h3 class="cal-result-title">{copy.successTitle}</h3>
          <p class="cal-result-desc">{copy.successDescription} <strong>{$calibratedGain.toFixed(1)}×</strong>. {copy.successTail}</p>
        {/if}
        <button class="cal-recalibrate-btn" onclick={startCalibration}>{copy.recalibrateButton}</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .calibration-box {
    background: var(--paper-2);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    padding: 20px 24px;
    display: grid;
    grid-template-columns: 1fr;
    align-items: center;
    justify-items: center;
    text-align: center;
    width: 100%;
    overflow: hidden;
  }

  .calibration-box > * { grid-column: 1; grid-row: 1; }

  .cal-label-stack, .cal-content-stack { display: grid; width: 100%; }
  .cal-label-stack > *, .cal-content-stack > * { grid-column: 1; grid-row: 1; }

  .cal-start-state, .cal-active-state, .cal-result-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    width: 100%;
  }

  .cal-steps-preview { display: flex; flex-direction: column; gap: 8px; width: 100%; text-align: left; }

  .cal-step-row { display: flex; align-items: center; gap: 10px; animation: calStepIn 0.28s ease both; }
  .cal-step-row:nth-child(2) { animation-delay: 0.07s; }

  @keyframes calStepIn {
    from { opacity: 0; transform: translateY(6px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  @media (prefers-reduced-motion: reduce) {
    .cal-step-row, .cal-success-icon, .cal-warning-icon { animation: none; }
  }

  .cal-phase-content { display: flex; flex-direction: column; align-items: center; gap: 6px; width: 100%; }

  .cal-step-num {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent-ink);
    font-size: 11px;
    font-weight: 600;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .cal-step-text { font-size: 13px; color: var(--ink-soft); }

  .cal-phase-header { display: flex; align-items: center; justify-content: space-between; width: 100%; }

  .cal-phase-label {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--ink-faint);
  }

  .cal-timer-ring {
    width: 44px;
    height: 44px;
    aspect-ratio: 1;
    flex-shrink: 0;
    border-radius: 50%;
    border: 3px solid var(--accent);
    display: flex;
    align-items: center;
    justify-content: center;
    animation: pulseCal 1.5s infinite;
  }

  @keyframes pulseCal {
    0%, 100% { border-color: var(--accent); transform: scale(1); }
    50% { border-color: color-mix(in srgb, var(--accent) 50%, transparent); transform: scale(1.03); }
  }

  .cal-countdown {
    font-size: 15px;
    font-weight: 600;
    color: var(--accent-ink);
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    text-align: center;
  }

  .cal-prompt {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--ink-faint);
    margin: 0;
  }

  .cal-phrase {
    font-family: var(--serif);
    font-size: 18px;
    font-weight: 500;
    font-style: italic;
    color: var(--ink-strong);
    margin: 0;
    line-height: 1.4;
  }

  .cal-meter-container { width: 100%; max-width: 280px; margin-top: 8px; }

  .cal-meter-track {
    width: 100%;
    height: 6px;
    background: var(--line-strong);
    border-radius: 999px;
    overflow: hidden;
    position: relative;
  }

  .cal-meter-fill {
    height: 100%;
    background: linear-gradient(90deg, var(--accent) 0%, color-mix(in srgb, var(--accent) 70%, white 30%) 100%);
    border-radius: 999px;
    transition: width 0.05s ease-out;
  }

  .cal-success-icon {
    width: 56px;
    height: 56px;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent);
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 4px;
    animation: iconPop 0.38s 0.18s cubic-bezier(0.16, 1, 0.3, 1) both;
  }

  .cal-warning-icon {
    width: 56px;
    height: 56px;
    border-radius: 50%;
    background: var(--warning-bg);
    color: var(--warning);
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 4px;
    animation: iconShake 0.42s 0.18s cubic-bezier(0.16, 1, 0.3, 1) both;
  }

  @keyframes iconPop {
    0%   { transform: scale(0.5); opacity: 0; }
    60%  { transform: scale(1.08); opacity: 1; }
    100% { transform: scale(1); opacity: 1; }
  }

  @keyframes iconShake {
    0%   { transform: translateY(-10px) scale(0.8); opacity: 0; }
    45%  { transform: translateY(4px) scale(1.02); opacity: 1; }
    70%  { transform: translateY(-2px) scale(1); }
    100% { transform: translateY(0) scale(1); opacity: 1; }
  }

  .cal-result-title { font-family: var(--serif); font-size: 18px; font-weight: 500; color: var(--ink-strong); margin: 0; }

  .cal-result-desc { font-size: 13px; color: var(--ink-soft); line-height: 1.5; margin: 0; max-width: 360px; }
  .cal-result-desc strong { color: var(--accent); font-family: var(--mono); font-size: 13.5px; }

  .cal-cancel-btn {
    margin-top: 4px;
    padding: 6px 16px;
    border-radius: var(--r-md);
    font-size: 13px;
    font-weight: 500;
    font-family: var(--sans);
    color: var(--ink-mute);
    cursor: pointer;
    background: transparent;
    border: 1px solid var(--line-strong);
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }

  .cal-cancel-btn:hover { color: var(--ink-strong); background: var(--paper-3); border-color: var(--line-strong); }

  .cal-recalibrate-btn {
    padding: 7px 18px;
    border-radius: var(--r-md);
    font-size: 13px;
    font-weight: 500;
    font-family: var(--sans);
    border: 1px solid var(--line-strong);
    color: var(--ink-mute);
    background: transparent;
    cursor: pointer;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }

  .cal-recalibrate-btn:hover { background: var(--paper-2); color: var(--ink-strong); border-color: var(--accent); }
</style>
