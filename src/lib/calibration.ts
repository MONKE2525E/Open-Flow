import { writable, get } from 'svelte/store';
import { invoke, listen } from './tauri';
import { saveSetting } from './settings';

export const TARGET_CALIBRATION_FACTOR = 2.25;
export const MIN_CALIBRATION_LEVEL = 0.04;
export const MAX_CALIBRATION_GAIN = 8.0;
export const MIN_CALIBRATION_GAIN = 1.0;
export const DEFAULT_CALIBRATION_GAIN = 3.5;
export const SPEECH_DETECTION_THRESHOLD = 0.07;

const PHASE_LOUD_DURATION_MS = 3000;
const PHASE_WHISPER_DURATION_MS = 2500;
const PHASE_LOUD_SECONDS = 3;
const PHASE_WHISPER_SECONDS = 3;
const COUNTDOWN_TICK_MS = 100;
const WHISPER_DETECTION_THRESHOLD = 0.015;
const WHISPER_MIN_TRANSCRIBABLE = 0.05;

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

  let unlisten: () => void;
  try {
    unlisten = await listen<number>('audio-level', (ev) => {
      if (sessionId !== currentCalibrationSession || !get(isCalibrating)) return;
      const level = ev.payload ?? 0;
      micLevel.set(level);
      if (currentPhase === 'loud' && level > loudMaxLevel) {
        loudMaxLevel = level;
      } else if (currentPhase === 'whisper' && level > whisperMaxLevel) {
        whisperMaxLevel = level;
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
      if (whisperPostGain < WHISPER_MIN_TRANSCRIBABLE) {
        const gainFromWhisper = Math.max(
          MIN_CALIBRATION_GAIN,
          Math.min(MAX_CALIBRATION_GAIN, WHISPER_MIN_TRANSCRIBABLE / whisperMaxLevel)
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
