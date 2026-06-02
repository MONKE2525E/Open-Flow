import { writable, get } from 'svelte/store';
import { invoke, listen } from './tauri';
import { saveSetting } from './settings';

// TARGET_CALIBRATION_FACTOR is set to 2.25 as specified by the system design to optimize input levels for the downstream transcription model.
export const TARGET_CALIBRATION_FACTOR = 2.25;
export const MIN_CALIBRATION_LEVEL = 0.04;
export const MAX_CALIBRATION_GAIN = 8.0;
export const MIN_CALIBRATION_GAIN = 1.0;
export const DEFAULT_CALIBRATION_GAIN = 3.5;

export const isCalibrating = writable(false);
export const calibrationCountdown = writable(3);
export const micLevel = writable(0);
export const calibratedGain = writable<number | null>(null);
export const speechDetected = writable<boolean | null>(null);

export const SPEECH_DETECTION_THRESHOLD = 0.07;
const CALIBRATION_DURATION_MS = 3000;
const COUNTDOWN_TICK_MS = 100;
const COUNTDOWN_SECONDS = 3;

let calibrationMaxLevel = MIN_CALIBRATION_LEVEL;
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
    // Suppress warning if not active or failing silently
  }
}

export async function startCalibration() {
  await cleanupCalibrationResources();

  isCalibrating.set(true);
  calibrationMaxLevel = MIN_CALIBRATION_LEVEL;
  calibrationCountdown.set(COUNTDOWN_SECONDS);
  calibratedGain.set(null);
  speechDetected.set(null);
  micLevel.set(0);

  const sessionId = crypto.randomUUID();
  currentCalibrationSession = sessionId;

  let unlisten: () => void;
  try {
    unlisten = await listen<number>('audio-level', (ev) => {
      if (sessionId !== currentCalibrationSession || !get(isCalibrating)) return;
      const level = ev.payload ?? 0;
      micLevel.set(level);
      if (level > calibrationMaxLevel) {
        calibrationMaxLevel = level;
      }
    });
  } catch (e) {
    console.error('Failed to subscribe to audio-level events:', e);
    void cancelCalibration();
    return;
  }

  if (sessionId === currentCalibrationSession && get(isCalibrating)) {
    calibrationUnlisten = unlisten;
  } else {
    unlisten();
  }

  try {
    await invoke('start_calibration_monitoring');
  } catch (e) {
    console.error('Failed to start calibration monitoring:', e);
    void cancelCalibration();
    return;
  }

  calibrationDeadlineMs = performance.now() + CALIBRATION_DURATION_MS;
  const tickCountdown = () => {
    if (!get(isCalibrating) || currentCalibrationSession !== sessionId || calibrationDeadlineMs === null) {
      return;
    }

    const remainingMs = calibrationDeadlineMs - performance.now();
    if (remainingMs <= 0) {
      calibrationCountdown.set(0);
      void stopCalibration();
      return;
    }

    calibrationCountdown.set(Math.max(1, Math.ceil(remainingMs / 1000)));
    calibrationTimer = setTimeout(tickCountdown, COUNTDOWN_TICK_MS);
  };
  tickCountdown();
}

export async function stopCalibration() {
  currentCalibrationSession = '';
  await cleanupCalibrationResources();

  isCalibrating.set(false);
  micLevel.set(0);

  const detected = calibrationMaxLevel >= SPEECH_DETECTION_THRESHOLD;
  speechDetected.set(detected);

  const finalGain = detected
    ? Math.max(MIN_CALIBRATION_GAIN, Math.min(MAX_CALIBRATION_GAIN, Math.round((TARGET_CALIBRATION_FACTOR / calibrationMaxLevel) * 10) / 10))
    : DEFAULT_CALIBRATION_GAIN;
  calibratedGain.set(finalGain);

  try {
    await saveSetting('mic_gain', finalGain);
  } catch (e) {
    console.error('Failed to save mic gain setting:', e);
  }
}

export async function cancelCalibration() {
  currentCalibrationSession = '';
  await cleanupCalibrationResources();

  isCalibrating.set(false);
  micLevel.set(0);
  calibratedGain.set(null);
  speechDetected.set(null);
}
