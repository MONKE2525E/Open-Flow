import { writable, get } from 'svelte/store';
import { invoke, listen } from './tauri';
import { saveSetting } from './settings';

// The setup meter intentionally boosts raw RMS by 15x for readability.
// Calibration uses the raw mic RMS, so the historical thresholds and targets
// below are written in that display space and divided back to raw.
const CALIBRATION_DISPLAY_GAIN = 15;

export const TARGET_CALIBRATION_FACTOR = 2.25 / CALIBRATION_DISPLAY_GAIN;
export const MIN_CALIBRATION_LEVEL = 0.04 / CALIBRATION_DISPLAY_GAIN;
export const MAX_CALIBRATION_GAIN = 8.0;
export const MIN_CALIBRATION_GAIN = 1.0;
export const DEFAULT_CALIBRATION_GAIN = 3.5;

/**
 * Level fallback for "did they speak?", used only when Silero VAD could not run
 * (model staging failure). The backend's `containsSpeech` is the real answer —
 * a peak-RMS threshold can't tell a voice from a chair scrape, and the scrape
 * also inflates `loudMaxLevel` and drags the computed gain down with it.
 */
const SPEECH_LEVEL_FALLBACK = 0.07 / CALIBRATION_DISPLAY_GAIN;

const PHASE_AMBIENT_DURATION_MS = 1500;
const PHASE_LOUD_DURATION_MS = 3000;
const PHASE_WHISPER_DURATION_MS = 2000;
const PHASE_AMBIENT_SECONDS = 2;
const PHASE_LOUD_SECONDS = 3;
const PHASE_WHISPER_SECONDS = 2;
const COUNTDOWN_TICK_MS = 100;
const WHISPER_DETECTION_THRESHOLD = 0.015 / CALIBRATION_DISPLAY_GAIN;
const WHISPER_TARGET_LEVEL = 0.32 / CALIBRATION_DISPLAY_GAIN;

/**
 * Ambient noise above this while the user is asked to stay silent means the
 * room, not the mic, is the problem. Same floor as whisper detection: if the
 * background is as loud as a whisper, a whisper can't be distinguished from it.
 */
const ROOM_NOISE_THRESHOLD = WHISPER_DETECTION_THRESHOLD;

export type CalibrationPhase = 'ambient' | 'loud' | 'whisper';

/** Ordered phases, so the UI can render "Step N of 3" without hardcoding. */
export const CALIBRATION_PHASES: CalibrationPhase[] = ['ambient', 'loud', 'whisper'];

type CalibrationResult = {
  containsSpeech: boolean | null;
  speechMs: number;
  speechRatio: number;
  peakProbability: number;
  longestSegmentMs: number;
  durationMs: number;
};

export const isCalibrating = writable(false);
export const calibrationCountdown = writable(PHASE_AMBIENT_SECONDS);
export const micLevel = writable(0);
export const calibratedGain = writable<number | null>(null);
export const speechDetected = writable<boolean | null>(null);
export const calibrationPhase = writable<CalibrationPhase | null>(null);
export const calibrationError = writable<string | null>(null);
/** True when the ambient phase heard enough background noise to matter. */
export const roomNoisy = writable(false);

let ambientMaxLevel = 0;
let loudMaxLevel = MIN_CALIBRATION_LEVEL;
let whisperMaxLevel = 0;
let currentPhase: CalibrationPhase | null = null;
let calibrationTimer: ReturnType<typeof setTimeout> | null = null;
let calibrationUnlisten: (() => void) | null = null;
let currentCalibrationSession = '';
let calibrationDeadlineMs: number | null = null;

/**
 * Tears down timers/listeners and stops the capture. Returns what the backend
 * learned from the capture, or null if it could not be analysed.
 */
async function cleanupCalibrationResources(): Promise<CalibrationResult | null> {
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
    return await invoke<CalibrationResult>('stop_calibration_monitoring');
  } catch (e) {
    // Suppress — may not be active.
    return null;
  }
}

export async function startCalibration() {
  await cleanupCalibrationResources();

  calibrationError.set(null);
  isCalibrating.set(true);
  ambientMaxLevel = 0;
  loudMaxLevel = MIN_CALIBRATION_LEVEL;
  whisperMaxLevel = 0;
  currentPhase = 'ambient';
  calibrationPhase.set('ambient');
  calibrationCountdown.set(PHASE_AMBIENT_SECONDS);
  calibratedGain.set(null);
  speechDetected.set(null);
  roomNoisy.set(false);
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
      if (currentPhase === 'ambient') {
        if (level > ambientMaxLevel) ambientMaxLevel = level;
      } else if (currentPhase === 'loud') {
        if (level > loudMaxLevel) loudMaxLevel = level;
      } else if (currentPhase === 'whisper') {
        if (level > whisperMaxLevel) whisperMaxLevel = level;
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
    const msg = e && typeof e === 'object' && 'message' in e ? String((e as any).message) : (e ? String(e) : "");
    if (msg && msg.includes("Microphone access denied")) {
      // Backend already emits a detailed, user-friendly message — pass it through.
      calibrationError.set(msg);
    } else {
      calibrationError.set(
        msg && msg !== "undefined" && msg !== "[object Object]"
          ? msg
          : "Could not start calibration. Make sure no other app is using the microphone and try again."
      );
    }
    void cancelCalibration();
    return;
  }

  calibrationDeadlineMs = performance.now() + PHASE_AMBIENT_DURATION_MS;

  const tickCountdown = () => {
    if (!get(isCalibrating) || currentCalibrationSession !== sessionId || calibrationDeadlineMs === null) {
      return;
    }

    const remainingMs = calibrationDeadlineMs - performance.now();

    if (remainingMs <= 0) {
      // Phases advance without reopening the audio stream, so the whole
      // capture stays as one recording for the backend's VAD pass.
      if (currentPhase === 'ambient') {
        currentPhase = 'loud';
        calibrationPhase.set('loud');
        calibrationDeadlineMs = performance.now() + PHASE_LOUD_DURATION_MS;
        calibrationCountdown.set(PHASE_LOUD_SECONDS);
        micLevel.set(0);
        calibrationTimer = setTimeout(tickCountdown, COUNTDOWN_TICK_MS);
      } else if (currentPhase === 'loud') {
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
  const analysis = await cleanupCalibrationResources();

  isCalibrating.set(false);
  calibrationPhase.set(null);
  micLevel.set(0);

  // Silero's verdict wins; the level threshold is only a fallback for when
  // VAD could not be loaded at all.
  const heardSpeech = analysis?.containsSpeech ?? loudMaxLevel >= SPEECH_LEVEL_FALLBACK;
  speechDetected.set(heardSpeech);
  roomNoisy.set(ambientMaxLevel >= ROOM_NOISE_THRESHOLD);

  let finalGain: number;

  if (heardSpeech && loudMaxLevel > MIN_CALIBRATION_LEVEL) {
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
  roomNoisy.set(false);
  // Do NOT clear calibrationError here — it should persist so the UI can display it.
}
