import { writable, get } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { saveSetting } from './settings';

// TARGET_CALIBRATION_FACTOR is set to 2.25 as specified by the system design to optimize input levels for the downstream transcription model.
export const TARGET_CALIBRATION_FACTOR = 2.25;
export const MIN_CALIBRATION_LEVEL = 0.04;
export const MAX_CALIBRATION_GAIN = 8.0;
export const MIN_CALIBRATION_GAIN = 1.0;

export const isCalibrating = writable(false);
export const calibrationCountdown = writable(3);
export const micLevel = writable(0);
export const calibratedGain = writable<number | null>(null);

let calibrationMaxLevel = MIN_CALIBRATION_LEVEL;
let calibrationTimer: ReturnType<typeof setInterval> | null = null;
let calibrationUnlisten: (() => void) | null = null;
let currentCalibrationSession = '';

async function cleanupCalibrationResources() {
  if (calibrationTimer) {
    clearInterval(calibrationTimer);
    calibrationTimer = null;
  }
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
  calibrationCountdown.set(3);
  calibratedGain.set(null);
  micLevel.set(0);

  const sessionId = Math.random().toString(36).substring(2) + '-' + Date.now();
  currentCalibrationSession = sessionId;

  const unlisten = await listen<number>('audio-level', (ev) => {
    if (sessionId !== currentCalibrationSession || !get(isCalibrating)) return;
    const level = ev.payload ?? 0;
    micLevel.set(level);
    if (level > calibrationMaxLevel) {
      calibrationMaxLevel = level;
    }
  });

  if (sessionId === currentCalibrationSession && get(isCalibrating)) {
    calibrationUnlisten = unlisten;
  } else {
    unlisten();
  }

  try {
    await invoke('start_calibration_monitoring');
  } catch (e) {
    console.error('Failed to start calibration monitoring:', e);
    cancelCalibration();
    return;
  }

  calibrationTimer = setInterval(() => {
    calibrationCountdown.update((c) => {
      if (c <= 1) {
        stopCalibration();
        return 0;
      }
      return c - 1;
    });
  }, 1000);
}

export async function stopCalibration() {
  currentCalibrationSession = '';
  await cleanupCalibrationResources();

  isCalibrating.set(false);
  micLevel.set(0);

  const rawGain = TARGET_CALIBRATION_FACTOR / Math.max(MIN_CALIBRATION_LEVEL, calibrationMaxLevel);
  const finalGain = Math.max(MIN_CALIBRATION_GAIN, Math.min(MAX_CALIBRATION_GAIN, Math.round(rawGain * 10) / 10));
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
}
