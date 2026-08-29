<script lang="ts">
  import { invoke } from '../../tauri';
  import { slide, fly, fade } from 'svelte/transition';
  import { expoOut } from 'svelte/easing';
  import { onDestroy } from 'svelte';
  import Toggle from '../Toggle.svelte';
  import { isMac, isWindows } from '../../platform';
  import { saveSetting } from '../../settings';
  import { MOTION_MS, MOTION_PX, motionMs, motionPx } from '../../motion';

  let noiseReduction = $state(true);
  let muteAudio = $state(false);
  let exclusiveMic = $state(false);
  let pauseMediaDuringDictation = $state(false);
  let soundEffectsVolume = $state(100);

  // Voice detection has no settings — it learns a sensitivity per input device
  // from real usage. The only control is the escape hatch.
  let resetState = $state<'idle' | 'working' | 'done' | 'error'>('idle');
  let resetTimer: ReturnType<typeof setTimeout> | null = null;

  async function resetVoiceDetection() {
    if (resetState === 'working') return;
    if (resetTimer) clearTimeout(resetTimer);
    resetState = 'working';
    try {
      await invoke('reset_voice_detection');
      resetState = 'done';
    } catch (err) {
      console.error('reset_voice_detection failed:', err);
      resetState = 'error';
    }
    resetTimer = setTimeout(() => { resetState = 'idle'; }, 2600);
  }

  async function loadSettings() {
    try {
      const [nr, mute, exclusive, pauseMedia, legacySounds, savedVolume] = await Promise.all([
        invoke<boolean | null>('get_setting', { key: 'noise_reduction' }),
        invoke<boolean | null>('get_setting', { key: 'mute_audio' }),
        invoke<boolean | null>('get_setting', { key: 'exclusive_mic' }),
        invoke<boolean | null>('get_setting', { key: 'pause_media_during_dictation' }),
        invoke<boolean | null>('get_setting', { key: 'play_start_stop_sounds' }),
        invoke<number | null>('get_setting', { key: 'sound_effects_volume' }),
      ]);
      noiseReduction = nr ?? true;
      muteAudio = mute ?? false;
      exclusiveMic = exclusive ?? false;
      pauseMediaDuringDictation = pauseMedia ?? false;
      soundEffectsVolume = savedVolume !== null && savedVolume !== undefined
        ? Math.max(0, Math.min(100, savedVolume))
        : legacySounds === false ? 0 : 100;
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

  async function handleMuteAudio(value: boolean) {
    muteAudio = value;
    try {
      await saveSetting('mute_audio', value);
    } catch (err) {
      muteAudio = !value;
      console.error('save mute_audio failed:', err);
    }
  }

  async function handleExclusiveMic(value: boolean) {
    exclusiveMic = value;
    try {
      await saveSetting('exclusive_mic', value);
    } catch (err) {
      exclusiveMic = !value;
      console.error('save exclusive_mic failed:', err);
    }
  }

  async function handlePauseMedia(value: boolean) {
    pauseMediaDuringDictation = value;
    try {
      await saveSetting('pause_media_during_dictation', value);
    } catch (err) {
      pauseMediaDuringDictation = !value;
      console.error('save pause_media_during_dictation failed:', err);
    }
  }

  async function saveSoundEffectsVolume() {
    try {
      await saveSetting('sound_effects_volume', soundEffectsVolume);
    } catch (err) {
      console.error('save sound_effects_volume failed:', err);
    }
  }

  onDestroy(() => {
    if (resetTimer) clearTimeout(resetTimer);
  });

  loadSettings();
</script>

<h2 class="settings-h">
  Audio
  {#if import.meta.env.DEV}
    <span class="legacy-label" aria-hidden="true">Microphone</span>
  {/if}
</h2>

<h3 class="settings-subhead first">Input</h3>
<div class="setting-row">
  <div><div class="label">{isMac ? 'Mute System Audio' : 'Mute PC Audio'}</div><div class="desc">{isMac ? 'Mutes system volume while dictating to prevent audio interference' : 'Mutes Windows volume while dictating to prevent audio interference'}</div></div>
  <Toggle checked={muteAudio} onchange={handleMuteAudio} label={isMac ? 'Mute system audio' : 'Mute PC audio'} />
</div>
{#if isMac}
  <div class="setting-row">
    <div><div class="label">Exclusive microphone access</div><div class="desc">Reserves the mic for Verenu while dictating, muting it for all other apps</div></div>
    <Toggle checked={exclusiveMic} onchange={handleExclusiveMic} label="Exclusive microphone access" />
  </div>
{/if}
{#if isWindows}
  <div class="setting-row">
    <div><div class="label">Pause media while dictating</div><div class="desc">Pauses active Windows media sessions and resumes them after transcription finishes. Works with apps that expose Windows media controls.</div></div>
    <Toggle checked={pauseMediaDuringDictation} onchange={handlePauseMedia} label="Pause media while dictating" />
  </div>
{/if}
<div class="setting-row">
  <div><div class="label">Noise reduction</div><div class="desc">Suppress background noise before transcription (RNNoise)</div></div>
  <Toggle checked={noiseReduction} onchange={handleNoiseReduction} label="Noise reduction" />
</div>
<div class="setting-row">
  <div>
    <div class="label">Voice detection</div>
    <div class="desc">Adapts to each microphone on its own, favouring quiet speech and whispers</div>
  </div>
  <button class="btn-ghost" onclick={resetVoiceDetection} disabled={resetState === 'working'}>
    {resetState === 'working' ? 'Resetting…'
      : resetState === 'done' ? 'Reset'
      : resetState === 'error' ? 'Try again'
      : 'Reset sensitivity'}
  </button>
</div>
{#if resetState === 'done' || resetState === 'error'}
  <div class="vd-status" class:vd-status-error={resetState === 'error'} role="status" transition:slide={{ duration: motionMs(200) }}>
    {resetState === 'done'
      ? 'Learned sensitivity cleared for all microphones. Your other audio settings are unchanged.'
      : 'Could not reset learned sensitivity. Check the app logs.'}
  </div>
{/if}

<h3 class="settings-subhead">Sound effects</h3>
<div class="setting-row sound-volume-row">
  <div class="sound-volume-header">
    <div>
      <div class="label">Sound effects volume</div>
      <div class="desc">Set to 0% to silence dictation chimes</div>
    </div>
    <span class="sound-volume-value">{Math.round(soundEffectsVolume)}%</span>
  </div>
  <div class="sound-volume-slider-wrap">
    <input
      type="range"
      class="sound-volume-slider"
      min="0" max="100" step="1"
      bind:value={soundEffectsVolume}
      onchange={saveSoundEffectsVolume}
      style="--pct: {soundEffectsVolume.toFixed(1)}%"
      aria-label="Sound effects volume"
    />
    <div class="sound-volume-ticks">
      <span>0%</span>
      <span>50%</span>
      <span>100%</span>
    </div>
  </div>
</div>

<style>
  .vd-status {
    margin-top: 10px;
    padding: 7px 10px;
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: 7px;
    font-size: 11.5px;
    color: var(--ink-mute);
    line-height: 1.4;
  }
  .vd-status-error {
    background: var(--warning-bg);
    border-color: var(--warning-line);
    color: var(--warning);
  }

  .sound-volume-row { flex-direction: column; align-items: stretch; gap: 0; }
  .sound-volume-header { display: flex; align-items: center; justify-content: space-between; width: 100%; }
  .sound-volume-value {
    font-family: var(--mono);
    font-size: 13px;
    font-weight: 500;
    color: var(--accent);
    min-width: 44px;
    text-align: right;
    flex-shrink: 0;
  }
  .sound-volume-slider-wrap { margin-top: 10px; width: 100%; }
  .sound-volume-slider {
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
    margin: 0;
  }
  .sound-volume-slider::-webkit-slider-thumb {
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
  .sound-volume-slider::-webkit-slider-thumb:hover { box-shadow: 0 2px 8px color-mix(in srgb, var(--accent) 45%, transparent); transform: scale(1.1); }
  .sound-volume-slider::-webkit-slider-thumb:active { box-shadow: 0 2px 10px color-mix(in srgb, var(--accent) 55%, transparent); transform: scale(1.15); }
  .sound-volume-slider::-moz-range-thumb {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--bg-elev);
    border: 2px solid var(--accent);
    box-shadow: 0 1px 4px color-mix(in srgb, var(--accent) 35%, transparent);
    cursor: pointer;
  }
  .sound-volume-ticks {
    position: relative;
    height: 16px;
    margin-top: 5px;
    font-size: 10px;
    color: var(--ink-mute);
    font-family: var(--mono);
    user-select: none;
  }
  .sound-volume-ticks span { position: absolute; top: 0; white-space: nowrap; }
  .sound-volume-ticks span:first-child { left: 0; }
  .sound-volume-ticks span:nth-child(2) { left: 50%; transform: translateX(-50%); }
  .sound-volume-ticks span:last-child { right: 0; }

  .legacy-label {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }

</style>
