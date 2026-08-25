import { isMac } from './platform';
import type { SettingsSectionId } from './settingsSections';

/**
 * Shared frontend error classification.
 *
 * Backend errors arrive over IPC as strings (command `Err` payloads or the
 * `verenu:error` event). Some of those strings are already user-crafted, some
 * carry structured markers (AUTH_401|..., QUOTA_EXCEEDED:...), and some could
 * still embed provider response bodies. This module is the single place that
 * turns any of those into a stable `ErrorKind` plus a safe, human message, so
 * components stop substring-matching backend strings in their own scripts.
 */

export type ErrorKind =
  | 'unknown'
  | 'already-recording'
  | 'mic-permission'
  | 'accessibility-permission'
  | 'auth-401'
  | 'quota'
  | 'too-short'
  | 'too-quiet'
  | 'nothing-transcribed'
  | 'local-model-missing'
  | 'no-backend'
  | 'invalid-backup'
  | 'unsupported-backup'
  | 'duplicate';

interface ClassifiedError {
  kind: ErrorKind;
  message: string;
}

/** Order matters: checked top to bottom, first match wins. */
const KIND_HINTS: ReadonlyArray<readonly [ErrorKind, readonly string[]]> = [
  ['already-recording', ['already recording']],
  [
    'mic-permission',
    ['microphone access is blocked', 'microphone access denied', 'microphone permission'],
  ],
  ['accessibility-permission', ['accessibility permission']],
  ['auth-401', ['auth_401', 'invalid or revoked', 'rejected this key', 'rejected authentication']],
  ['quota', ['quota exceeded', 'quota reached']],
  // "Recording was too quiet — nothing was transcribed" must classify as
  // nothing-transcribed (the original mic-button wording); keep it before
  // the too-quiet hint.
  ['nothing-transcribed', ['nothing was transcribed', 'nothing transcribed']],
  ['too-short', ['too short']],
  ['too-quiet', ['too quiet']],
  ['local-model-missing', ['download the selected local model']],
  ['no-backend', ['no configured transcription backend']],
  ['invalid-backup', ['invalid backup']],
  ['unsupported-backup', ['unsupported backup']],
  ['duplicate', ['unique constraint']],
];

/** Copy for kinds whose wording is fixed and shared across surfaces. */
const KIND_MESSAGES: Partial<Record<ErrorKind, string>> = {
  'already-recording': 'Hotkey recording is active',
  'mic-permission': 'Enable microphone permission in System Settings',
  'accessibility-permission': 'Enable Accessibility permission in System Settings',
  'too-short': 'Too short — try again',
  'too-quiet': 'Too quiet — try again',
  'nothing-transcribed': 'Nothing detected — try again',
  'local-model-missing': 'Download the local model first',
  'no-backend': 'Choose a transcription backend first',
  'invalid-backup': "That file isn't a valid Verenu backup.",
  'unsupported-backup':
    "This backup was made by a newer Verenu version and can't be read. Update Verenu to import it.",
  duplicate: 'That term already exists.',
};

/** Extracts a displayable string from any IPC/JS error shape. */
export function extractIpcErrorMessage(err: unknown): string {
  if (typeof err === 'object' && err !== null) {
    if ('message' in err) {
      const message = (err as { message?: unknown }).message;
      if (typeof message === 'string' && message.trim()) {
        return message.trim();
      }
    }
    if ('error' in err) {
      const error = (err as { error?: unknown }).error;
      if (typeof error === 'string' && error.trim()) {
        return error.trim();
      }
    }
  }
  if (err instanceof Error && err.message?.trim()) {
    return err.message.trim();
  }
  const raw = String(err ?? '').trim();
  if (!raw || raw === '[object Object]') {
    return 'The backend is unavailable.';
  }
  return raw;
}

/**
 * Defense-in-depth for strings that bypass the Rust-side sanitizer (e.g. a
 * future command that returns a raw provider context string). Strips response
 * bodies and internal request ids, and caps length so a raw backend string can
 * never flood the UI (the Rust side truncates to 120 chars; match that).
 */
function stripInternalMarkers(message: string): string {
  const cleaned = message
    .replace(/body_preview=[^|]*$/g, '')
    .replace(/request_id=[^\s|]+/g, '')
    .replace(/[ \t]{2,}/g, ' ')
    .trim();
  return cleaned.length > 120 ? `${cleaned.slice(0, 120)}…` : cleaned;
}

function messageForKind(kind: ErrorKind, raw: string): string {
  switch (kind) {
    case 'auth-401': {
      // Wire format: "AUTH_401|...|status=401: {user message}". The backend
      // message after the colon is already user-facing; anything else here is
      // metadata and must not reach the UI.
      if (raw.includes('AUTH_401')) {
        const after = raw.split(': ').slice(1).join(': ').trim();
        if (after) return after;
      }
      return raw;
    }
    case 'quota': {
      // "QUOTA_EXCEEDED: Groq quota reached" or the already-friendly message.
      const provider = raw.replace(/^QUOTA_EXCEEDED:\s*/i, '').split(/\s/)[0];
      const label = provider && !provider.includes('quota') ? provider : 'Your provider';
      return `${label} quota reached — wait for it to reset or add credits, then try again.`;
    }
    default: {
      return KIND_MESSAGES[kind] ?? raw;
    }
  }
}

export function classifyIpcError(err: unknown): ClassifiedError {
  const raw = extractIpcErrorMessage(err);
  const lower = raw.toLowerCase();
  for (const [kind, hints] of KIND_HINTS) {
    if (hints.some((hint) => lower.includes(hint))) {
      return { kind, message: messageForKind(kind, raw) };
    }
  }
  return { kind: 'unknown', message: stripInternalMarkers(raw) };
}

/**
 * Which settings section, if any, is the useful next step for this error.
 * `null` means there is nothing actionable to jump to.
 */
export function settingsSectionForKind(kind: ErrorKind): SettingsSectionId | null {
  switch (kind) {
    case 'auth-401':
    case 'quota':
      return 'keys';
    case 'no-backend':
    case 'local-model-missing':
      return 'models';
    case 'mic-permission':
    case 'accessibility-permission':
      return isMac ? 'permissions' : 'advanced';
    default:
      return null;
  }
}
