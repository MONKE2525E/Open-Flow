import { writable, get } from 'svelte/store';
import { invoke, listen } from './tauri';
import { saveSetting } from './settings';

// The setup meter intentionally boosts raw RMS by 15x for readability.
// Calibration now uses the raw mic RMS, so convert the historical thresholds
// and targets back to their raw equivalents.
const CALIBRATION_DISPLAY_GAIN = 15;

export const TARGET_CALIBRATION_FACTOR = 2.25 / CALIBRATION_DISPLAY_GAIN;
export const MIN_CALIBRATION_LEVEL = 0.04 / CALIBRATION_DISPLAY_GAIN;
export const MAX_CALIBRATION_GAIN = 8.0;
export const MIN_CALIBRATION_GAIN = 1.0;
export const DEFAULT_CALIBRATION_GAIN = 3.5;
export const SPEECH_DETECTION_THRESHOLD = 0.07 / CALIBRATION_DISPLAY_GAIN;

const PHASE_LOUD_DURATION_MS = 3000;
const PHASE_WHISPER_DURATION_MS = 2000;
const PHASE_LOUD_SECONDS = 3;
const PHASE_WHISPER_SECONDS = 2;
const COUNTDOWN_TICK_MS = 100;
const WHISPER_DETECTION_THRESHOLD = 0.015 / CALIBRATION_DISPLAY_GAIN;
const WHISPER_TARGET_LEVEL = 0.32 / CALIBRATION_DISPLAY_GAIN;

export type CalibrationPhase = 'loud' | 'whisper';

export const isCalibrating = writable(false);
export const calibrationCountdown = writable(PHASE_LOUD_SECONDS);
export const micLevel = writable(0);
export const calibratedGain = writable<number | null>(null);
export const speechDetected = writable<boolean | null>(null);
export const calibrationPhase = writable<CalibrationPhase | null>(null);

let loudMaxLevel = MIN_CALIBRATION_LEVEL;
let whisperMaxLevel = 0;
let currentPhase: CalibrationPhase | null = null;
let calibrationTimer: ReturnType<typeof setTimeout> | null = null;
let calibrationUnlisten: (() => void) | null = null;
let currentCalibrationSession = '';
let calibrationDeadlineMs: number | null = null;

async function cleanupCalibrationResources() {
  if (calibrationTimer) {
    clearTimeout(calibrationTimer);
    calibrationTimer = null;
  }
  calibrationDeadlineMs = null;
  if (calibrationUnlisten) {
    calibrationUnlisten();
    calibrationUnlisten = null;
  }
  try {
    await invoke('stop_calibration_monitoring');
  } catch (e) {
    // Suppress — may not be active
  }
}

export async function startCalibration() {
  await cleanupCalibrationResources();

  isCalibrating.set(true);
  loudMaxLevel = MIN_CALIBRATION_LEVEL;
  whisperMaxLevel = 0;
  currentPhase = 'loud';
  calibrationPhase.set('loud');
  calibrationCountdown.set(PHASE_LOUD_SECONDS);
  calibratedGain.set(null);
  speechDetected.set(null);
  micLevel.set(0);

  const sessionId = crypto.randomUUID();
  currentCalibrationSession = sessionId;

  let unlistenDisplay: (() => void) | undefined;
  let unlistenRaw: (() => void) | undefined;
  try {
    unlistenDisplay = await listen<number>('audio-level', (ev) => {
      if (sessionId !== currentCalibrationSession || !get(isCalibrating)) return;
      micLevel.set(ev.payload ?? 0);
    });
    unlistenRaw = await listen<number>('audio-level-raw', (ev) => {
      if (sessionId !== currentCalibrationSession || !get(isCalibrating)) return;
      const level = ev.payload ?? 0;
      if (currentPhase === 'loud' && level > loudMaxLevel) {
        loudMaxLevel = level;
      } else if (currentPhase === 'whisper' && level > whisperMaxLevel) {
        whisperMaxLevel = level;
      }
    });
  } catch (e) {
    unlistenDisplay?.();
    unlistenRaw?.();
    console.error('Failed to subscribe to audio-level events:', e);
    void cancelCalibration();
    return;
  }

  if (sessionId === currentCalibrationSession && get(isCalibrating)) {
    calibrationUnlisten = () => {
      unlistenDisplay?.();
      unlistenRaw?.();
    };
  } else {
    unlistenDisplay?.();
    unlistenRaw?.();
  }

  try {
    await invoke('start_calibration_monitoring');
  } catch (e) {
    console.error('Failed to start calibration monitoring:', e);
    void cancelCalibration();
    return;
  }

  calibrationDeadlineMs = performance.now() + PHASE_LOUD_DURATION_MS;

  const tickCountdown = () => {
    if (!get(isCalibrating) || currentCalibrationSession !== sessionId || calibrationDeadlineMs === null) {
      return;
    }

    const remainingMs = calibrationDeadlineMs - performance.now();

    if (remainingMs <= 0) {
      if (currentPhase === 'loud') {
        // Transition to whisper phase — audio stream stays open
        currentPhase = 'whisper';
        calibrationPhase.set('whisper');
        calibrationDeadlineMs = performance.now() + PHASE_WHISPER_DURATION_MS;
        calibrationCountdown.set(PHASE_WHISPER_SECONDS);
        micLevel.set(0);
        calibrationTimer = setTimeout(tickCountdown, COUNTDOWN_TICK_MS);
      } else {
        calibrationCountdown.set(0);
        void stopCalibration();
      }
      return;
    }

    calibrationCountdown.set(Math.max(1, Math.ceil(remainingMs / 1000)));
    calibrationTimer = setTimeout(tickCountdown, COUNTDOWN_TICK_MS);
  };

  tickCountdown();
}

export async function stopCalibration() {
  currentCalibrationSession = '';
  currentPhase = null;
  await cleanupCalibrationResources();

  isCalibrating.set(false);
  calibrationPhase.set(null);
  micLevel.set(0);

  const loudDetected = loudMaxLevel >= SPEECH_DETECTION_THRESHOLD;
  speechDetected.set(loudDetected);

  let finalGain: number;

  if (loudDetected) {
    const gainFromLoud = Math.max(
      MIN_CALIBRATION_GAIN,
      Math.min(MAX_CALIBRATION_GAIN, TARGET_CALIBRATION_FACTOR / loudMaxLevel)
    );

    const whisperPresent = whisperMaxLevel >= WHISPER_DETECTION_THRESHOLD;
    if (whisperPresent) {
      const whisperPostGain = whisperMaxLevel * gainFromLoud;
      if (whisperPostGain < WHISPER_TARGET_LEVEL) {
        const gainFromWhisper = Math.max(
          MIN_CALIBRATION_GAIN,
          Math.min(MAX_CALIBRATION_GAIN, WHISPER_TARGET_LEVEL / whisperMaxLevel)
        );
        finalGain = Math.min(MAX_CALIBRATION_GAIN, Math.max(gainFromLoud, gainFromWhisper));
      } else {
        finalGain = gainFromLoud;
      }
    } else {
      finalGain = gainFromLoud;
    }
  } else {
    finalGain = DEFAULT_CALIBRATION_GAIN;
  }

  finalGain = Math.round(finalGain * 10) / 10;
  calibratedGain.set(finalGain);

  try {
    await saveSetting('mic_gain', finalGain);
  } catch (e) {
    console.error('Failed to save mic gain setting:', e);
  }
}

export async function cancelCalibration() {
  currentCalibrationSession = '';
  currentPhase = null;
  await cleanupCalibrationResources();

  isCalibrating.set(false);
  calibrationPhase.set(null);
  micLevel.set(0);
  calibratedGain.set(null);
  speechDetected.set(null);
}
