import { getVersion as getTauriVersion } from '@tauri-apps/api/app';
import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { emit as tauriEmit, listen as tauriListen } from '@tauri-apps/api/event';
import { defaultHotkey } from './platform';

declare const __APP_VERSION__: string;

type CommandArgs = Record<string, unknown>;
type EventEnvelope<T> = {
  event: string;
  id: number;
  payload: T;
};
type EventHandler<T> = (event: EventEnvelope<T>) => void;
type UnlistenFn = () => void;
type CreatedRecordMeta = { id: number; created_at: string };
type DevSnippet = {
  id: number;
  trigger: string;
  expansion: string;
  instructions: string;
  use_count: number;
  created_at: string;
};
type DevDictionaryEntry = {
  id: number;
  term: string;
  mistake: string | null;
  auto_learned: boolean;
  correction_count: number;
  confidence_tier: 'manual' | 'low' | 'medium' | 'high';
  last_seen_at: string | null;
  created_at: string;
};
type DevPermissionStatus = 'authorized' | 'needs_permission' | 'not_determined' | 'denied' | 'restricted' | 'unknown';
type DevKeychainStatus = 'authorized' | 'not_configured' | 'denied' | 'unknown';

const DEV_STORAGE_KEY = 'verenu:dev-settings';
const DEV_SNIPPETS_KEY = 'verenu:dev-snippets';
const DEV_DICTIONARY_KEY = 'verenu:dev-dictionary';
let devEventId = 0;

const defaultProviderModels = {
  groq: ['whisper-large-v3-turbo', 'whisper-large-v3'],
  openai: ['gpt-4o-mini-transcribe', 'gpt-4o-transcribe'],
  google: ['gemini-2.5-flash', 'gemini-3.5-flash'],
};

const defaultCleanupModels = {
  groq: ['llama-3.3-70b-versatile', 'llama-3.1-8b-instant'],
  openai: ['gpt-4o-mini', 'gpt-4o'],
  google: ['gemini-2.5-flash', 'gemini-3.5-flash'],
};

const defaultSettings: Record<string, unknown> = {
  setup_complete: true,
  force_setup_on_launch: false,
  appearance_mode: 'system',
  transcription_provider: 'groq',
  transcription_language: 'en',
  cleanup_provider: 'groq',
  transcription_model: 'groq/whisper-large-v3-turbo',
  cleanup_model: 'groq/llama-3.3-70b-versatile',
  transcription_default_model: 'groq/whisper-large-v3-turbo',
  cleanup_default_model: 'groq/llama-3.3-70b-versatile',
  transcription_models_by_provider: defaultProviderModels,
  cleanup_models_by_provider: defaultCleanupModels,
  transcription_fallback_models: [],
  cleanup_fallback_models: [],
  cleanup_enabled: true,
  default_tone: 'casual',
  cleanup_intensity: 'medium',
  app_mappings: [],
  noise_reduction: true,
  mute_audio: false,
  pause_media_during_dictation: false,
  autostart_enabled: false,
  mic_gain: 3.5,
  app_context_hint: false,
  auto_learn_enabled: false,
  contextual_caps_enabled: true,
  auto_spacing_enabled: true,
  history_retention: '30 days',
  microphone_device: null,
  update_dismissed_version: null,
  update_notified_version: null,
  advanced_model_ui: false,
  hotkey: defaultHotkey,
};

function hasTauriInternals(): boolean {
  if (typeof window === 'undefined') return false;
  const maybeWindow = window as Window & {
    __TAURI_INTERNALS__?: { invoke?: unknown };
  };
  return typeof maybeWindow.__TAURI_INTERNALS__?.invoke === 'function';
}

function readDevSettings(): Record<string, unknown> {
  if (typeof localStorage === 'undefined') return {};
  try {
    const raw = localStorage.getItem(DEV_STORAGE_KEY);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

function writeDevSetting(key: string, value: unknown) {
  if (typeof localStorage === 'undefined' || !key) return;
  try {
    const next = { ...readDevSettings(), [key]: value };
    localStorage.setItem(DEV_STORAGE_KEY, JSON.stringify(next));
  } catch {
    // Browser dev mode should keep working even when persistent storage is blocked.
  }
}

function getDevSetting(key: string): unknown {
  const saved = readDevSettings();
  return key in saved ? saved[key] : defaultSettings[key] ?? null;
}

function readDevList<T>(key: string): T[] {
  if (typeof localStorage === 'undefined') return [];
  try {
    const raw = localStorage.getItem(key);
    const parsed = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function writeDevList<T>(key: string, rows: T[]) {
  if (typeof localStorage === 'undefined') return;
  try {
    localStorage.setItem(key, JSON.stringify(rows));
  } catch {
    // Browser dev mode should keep working even when persistent storage is blocked.
  }
}

function nextDevId(rows: { id: number }[]): number {
  return rows.reduce((max, row) => Math.max(max, row.id), 0) + 1;
}

function devCreated(id: number): CreatedRecordMeta {
  return { id, created_at: new Date().toISOString() };
}

function devPermissionSnapshot(provider?: unknown) {
  const accessibility = String(getDevSetting('accessibility_permission_status') ?? 'authorized') as DevPermissionStatus;
  const microphone = String(getDevSetting('microphone_permission_status') ?? 'authorized') as DevPermissionStatus;
  const saved = (getDevSetting('__provider_connected') as Record<string, boolean> | null) ?? {};
  const providerKey = typeof provider === 'string' ? provider : '';
  const keychain = providerKey && saved[providerKey]
    ? String(getDevSetting('keychain_permission_status') ?? 'authorized') as DevKeychainStatus
    : 'not_configured';

  return {
    accessibility,
    microphone,
    keychain,
    allCoreGranted: accessibility === 'authorized' && microphone === 'authorized',
    lastCheckedAt: new Date().toISOString(),
    sourceHints: {
      microphoneVerified: Boolean(getDevSetting('microphone_verified') ?? microphone === 'authorized'),
      accessibilityVerified: Boolean(getDevSetting('accessibility_verified') ?? accessibility === 'authorized'),
    },
    diagnostics: {
      bundleIdentifier: String(getDevSetting('bundle_identifier') ?? 'com.verenu.app'),
      bundlePath: String(getDevSetting('bundle_path') ?? '/Applications/Verenu.app'),
      executablePath: String(getDevSetting('executable_path') ?? '/Applications/Verenu.app/Contents/MacOS/Verenu'),
      processId: 12345,
      accessibilityTrusted: accessibility === 'authorized',
    },
  };
}

function assertDevText(value: unknown, field: string): string {
  if (typeof value !== 'string') {
    throw new Error(`${field} must be text.`);
  }
  return value;
}

async function devInvoke<T>(command: string, args?: CommandArgs): Promise<T> {
  switch (command) {
    case 'get_setting':
      return getDevSetting(String(args?.key ?? '')) as T;
    case 'save_setting':
      if (typeof args?.key !== 'string' || args.key.length === 0) {
        return undefined as T;
      }
      writeDevSetting(args.key, args?.value);
      return undefined as T;
    case 'get_all_settings':
      return { ...defaultSettings, ...readDevSettings() } as T;
    case 'get_app_mappings':
      return getDevSetting('app_mappings') as T;
    case 'get_snippets':
      return readDevList<DevSnippet>(DEV_SNIPPETS_KEY) as T;
    case 'get_dictionary':
      return readDevList<DevDictionaryEntry>(DEV_DICTIONARY_KEY) as T;
    case 'get_recent':
    case 'get_recent_auto_learn_activity':
    case 'get_microphones':
    case 'get_recent_logs':
    case 'get_installed_apps':
      return [] as T;
    case 'get_stats':
      return { total_words: 0, avg_wpm: 0, day_streak: 0 } as T;
    case 'get_memory_mb':
      return 0 as T;
    case 'count_old_transcriptions':
      return 0 as T;
    case 'get_api_key_status':
      return {
        groq: false,
        openai: false,
        google: false,
        ...(getDevSetting('__provider_connected') as Record<string, boolean> | null),
      } as T;
    case 'validate_api_key':
      return { ok: true, status: 'valid', message: 'Key verified (dev mode).' } as T;
    case 'get_accessibility_permission_status':
      return String(getDevSetting('accessibility_permission_status') ?? 'authorized') as T;
    case 'get_microphone_permission_status':
      return String(getDevSetting('microphone_permission_status') ?? 'authorized') as T;
    case 'get_macos_permission_snapshot':
      return devPermissionSnapshot(args?.provider) as T;
    case 'request_accessibility_permission':
      writeDevSetting('accessibility_permission_status', 'authorized');
      return devPermissionSnapshot(args?.provider) as T;
    case 'request_microphone_permission':
      writeDevSetting('microphone_permission_status', 'authorized');
      return 'authorized' as T;
    case 'request_microphone_permission_snapshot':
      writeDevSetting('microphone_permission_status', 'authorized');
      writeDevSetting('microphone_verified', true);
      return devPermissionSnapshot(args?.provider) as T;
    case 'check_keychain_access':
      return 'authorized' as T;
    case 'reset_macos_core_permissions':
      writeDevSetting('accessibility_permission_status', 'not_determined');
      return {
        bundleIdentifier: 'com.verenu.app',
        steps: [
          { service: 'Accessibility', ok: true, message: 'Reset' },
        ],
      } as T;
    case 'check_for_update':
      return null as T;
    case 'check_provider_status':
      return [] as T;
    case 'check_provider_status_raw':
      return { dev: true, note: 'Not running in Tauri — no real fetch performed.' } as T;
    case 'check_verenu_api_health':
      return true as T;
    case 'check_connectivity':
      return (typeof navigator === 'undefined' ? true : navigator.onLine) as T;
    case 'get_dev_logging_enabled':
      return Boolean(getDevSetting('dev_logging_enabled') ?? false) as T;
    case 'get_cleanup_cache_status':
      return { entry_count: 0, is_space_constrained: false, free_bytes: null } as T;
    case 'get_auto_learn_status_summary':
      return {
        monitors_started: 0,
        anchor_misses: 0,
        low_confidence_rejections: 0,
        promotions: 0,
        duplicate_monitor_skips: 0,
        timeout_finishes: 0,
      } as T;
    case 'clear_cleanup_cache':
      return 0 as T;
    case 'check_hotkey':
      return true as T;
    case 'stop_and_transcribe_input':
      return '' as T;
    case 'save_api_key':
    case 'delete_api_key': {
      // Round-trip "saved" state through dev storage so the API Keys section
      // (saved indicator + Save⇄Clear flip) is actually demoable in browser dev.
      const provider = String(args?.provider ?? '');
      if (provider) {
        const current = (getDevSetting('__provider_connected') as Record<string, boolean> | null) ?? {};
        writeDevSetting('__provider_connected', { ...current, [provider]: command === 'save_api_key' });
      }
      return undefined as T;
    }
    case 'set_dev_logging_enabled':
      writeDevSetting('dev_logging_enabled', Boolean(args?.enabled));
      return undefined as T;
    case 'set_autostart':
    case 'save_hotkey':
    case 'hide_main':
    case 'open_accessibility_settings':
    case 'open_microphone_settings':
    case 'open_privacy_security_settings':
    case 'restart_app':
    case 'start_input_recording':
    case 'start_setup_try_recording':
    case 'stop_setup_try_recording':
    case 'retry_transcription':
    case 'install_update':
    case 'start_calibration_monitoring':
    case 'stop_calibration_monitoring':
      return undefined as T;
    case 'create_snippet': {
      const trigger = assertDevText(args?.trigger, 'Trigger').trim();
      const expansion = assertDevText(args?.expansion, 'Expansion');
      const instructions = assertDevText(args?.instructions ?? '', 'Cleanup instructions');
      if (!trigger) throw new Error('Trigger cannot be empty');
      if (!expansion.trim()) throw new Error('Expansion cannot be empty');
      if ([...trigger].length > 300) throw new Error('Trigger must be 300 characters or fewer');

      const rows = readDevList<DevSnippet>(DEV_SNIPPETS_KEY);
      if (rows.some((row) => row.trigger === trigger)) {
        throw new Error('UNIQUE constraint failed: snippets.trigger');
      }
      const id = nextDevId(rows);
      const created = devCreated(id);
      rows.unshift({
        id,
        trigger,
        expansion,
        instructions,
        use_count: 0,
        created_at: created.created_at,
      });
      writeDevList(DEV_SNIPPETS_KEY, rows);
      return created as T;
    }
    case 'edit_snippet': {
      const id = Number(args?.id);
      const trigger = assertDevText(args?.trigger, 'Trigger').trim();
      const expansion = assertDevText(args?.expansion, 'Expansion');
      const instructions = assertDevText(args?.instructions ?? '', 'Cleanup instructions');
      if (!Number.isFinite(id)) throw new Error('Snippet id is required.');
      if (!trigger) throw new Error('Trigger cannot be empty');
      if (!expansion.trim()) throw new Error('Expansion cannot be empty');
      if ([...trigger].length > 300) throw new Error('Trigger must be 300 characters or fewer');

      const rows = readDevList<DevSnippet>(DEV_SNIPPETS_KEY);
      if (rows.some((row) => row.id !== id && row.trigger === trigger)) {
        throw new Error('UNIQUE constraint failed: snippets.trigger');
      }
      const index = rows.findIndex((row) => row.id === id);
      if (index === -1) throw new Error(`Snippet ${id} was not found`);
      rows[index] = { ...rows[index], trigger, expansion, instructions };
      writeDevList(DEV_SNIPPETS_KEY, rows);
      return undefined as T;
    }
    case 'remove_snippet': {
      const id = Number(args?.id);
      const rows = readDevList<DevSnippet>(DEV_SNIPPETS_KEY);
      const next = rows.filter((row) => row.id !== id);
      if (next.length === rows.length) throw new Error(`Snippet ${id} was not found`);
      writeDevList(DEV_SNIPPETS_KEY, next);
      return undefined as T;
    }
    case 'create_dictionary_entry': {
      const term = assertDevText(args?.term, 'Term').trim();
      const mistakeText = typeof args?.mistake === 'string' ? args.mistake.trim() : '';
      const mistake = mistakeText || null;
      if (!term) throw new Error('Term cannot be empty');
      if ([...term].length > 120) throw new Error('Term must be 120 characters or fewer');
      if (mistake && [...mistake].length > 120) {
        throw new Error('Often mistranscribed as must be 120 characters or fewer');
      }

      const rows = readDevList<DevDictionaryEntry>(DEV_DICTIONARY_KEY);
      if (rows.some((row) => row.term === term)) {
        throw new Error('UNIQUE constraint failed: dictionary.term');
      }
      const id = nextDevId(rows);
      const created = devCreated(id);
      rows.unshift({
        id,
        term,
        mistake,
        auto_learned: false,
        correction_count: 0,
        confidence_tier: 'manual',
        last_seen_at: null,
        created_at: created.created_at,
      });
      writeDevList(DEV_DICTIONARY_KEY, rows);
      return created as T;
    }
    case 'edit_dictionary_entry': {
      const id = Number(args?.id);
      const term = assertDevText(args?.term, 'Term').trim();
      const mistakeText = typeof args?.mistake === 'string' ? args.mistake.trim() : '';
      const mistake = mistakeText || null;
      if (!Number.isFinite(id)) throw new Error('Dictionary entry id is required.');
      if (!term) throw new Error('Term cannot be empty');
      if ([...term].length > 120) throw new Error('Term must be 120 characters or fewer');
      if (mistake && [...mistake].length > 120) {
        throw new Error('Often mistranscribed as must be 120 characters or fewer');
      }

      const rows = readDevList<DevDictionaryEntry>(DEV_DICTIONARY_KEY);
      if (rows.some((row) => row.id !== id && row.term === term)) {
        throw new Error('UNIQUE constraint failed: dictionary.term');
      }
      const index = rows.findIndex((row) => row.id === id);
      if (index === -1) throw new Error(`Dictionary entry ${id} was not found`);
      rows[index] = { ...rows[index], term, mistake };
      writeDevList(DEV_DICTIONARY_KEY, rows);
      return undefined as T;
    }
    case 'remove_dictionary_entry': {
      const id = Number(args?.id);
      const rows = readDevList<DevDictionaryEntry>(DEV_DICTIONARY_KEY);
      const next = rows.filter((row) => row.id !== id);
      if (next.length === rows.length) throw new Error(`Dictionary entry ${id} was not found`);
      writeDevList(DEV_DICTIONARY_KEY, next);
      return undefined as T;
    }
    case 'log_frontend':
      return undefined as T;
    case 'check_accessibility_permission':
      if (args?.prompt) {
        writeDevSetting('accessibility_permission_status', 'authorized');
      }
      return true as T;
    case 'save_app_mappings':
      writeDevSetting('app_mappings', args?.mappings ?? []);
      return undefined as T;
    case 'download_logs':
      return 'browser-dev://verenu-logs.txt' as T;
    default:
      throw new Error(`Tauri command "${command}" is unavailable in browser dev mode.`);
  }
}

export function isTauriRuntime(): boolean {
  return hasTauriInternals();
}

export function invoke<T = unknown>(command: string, args?: CommandArgs): Promise<T> {
  if (hasTauriInternals()) {
    return tauriInvoke<T>(command, args);
  }
  return devInvoke<T>(command, args);
}

export function listen<T>(
  event: string,
  handler: EventHandler<T>,
): Promise<UnlistenFn> {
  if (hasTauriInternals()) {
    return tauriListen<T>(event, handler as Parameters<typeof tauriListen<T>>[1]);
  }
  if (typeof window === 'undefined') return Promise.resolve(() => {});
  const eventName = `tauri:${event}`;
  const listener = (ev: Event) => {
    handler({
      event,
      id: ++devEventId,
      payload: (ev as CustomEvent<T>).detail,
    });
  };
  window.addEventListener(eventName, listener);
  return Promise.resolve(() => window.removeEventListener(eventName, listener));
}

export function emit<T>(event: string, payload?: T): Promise<void> {
  if (hasTauriInternals()) {
    return tauriEmit(event, payload);
  }
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new CustomEvent(`tauri:${event}`, { detail: payload }));
  }
  return Promise.resolve();
}

export function flog(level: 'info' | 'warn' | 'error', message: string): void {
  invoke('log_frontend', { level, message }).catch(() => {});
}

export function getVersion(): Promise<string> {
  if (hasTauriInternals()) {
    return getTauriVersion();
  }
  return Promise.resolve(__APP_VERSION__);
}
