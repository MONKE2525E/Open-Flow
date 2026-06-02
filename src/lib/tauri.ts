import { getVersion as getTauriVersion } from '@tauri-apps/api/app';
import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { emit as tauriEmit, listen as tauriListen } from '@tauri-apps/api/event';

declare const __APP_VERSION__: string;

type CommandArgs = Record<string, unknown>;
type EventEnvelope<T> = {
  event: string;
  id: number;
  payload: T;
};
type EventHandler<T> = (event: EventEnvelope<T>) => void;
type UnlistenFn = () => void;

const DEV_STORAGE_KEY = 'open-flow:dev-settings';

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
  autostart_enabled: false,
  mic_gain: 3.5,
  app_context_hint: false,
  auto_learn_enabled: false,
  contextual_caps_enabled: true,
  auto_spacing_enabled: true,
  history_retention: '30 days',
  microphone_device: null,
  update_dismissed_version: null,
  advanced_model_ui: false,
  hotkey: ['ControlLeft', 'MetaLeft'],
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
    case 'get_recent':
    case 'get_snippets':
    case 'get_dictionary':
    case 'get_recent_auto_learn_activity':
    case 'get_microphones':
    case 'get_recent_logs':
    case 'get_installed_apps':
      return [] as T;
    case 'get_stats':
      return { total_words: 0, avg_wpm: 0, day_streak: 0 } as T;
    case 'get_memory_mb':
      return 0 as T;
    case 'get_api_key_status':
      return { groq: false, openai: false, google: false } as T;
    case 'check_for_update':
      return null as T;
    case 'check_connectivity':
      return (typeof navigator === 'undefined' ? true : navigator.onLine) as T;
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
    case 'delete_api_key':
    case 'set_autostart':
    case 'save_hotkey':
    case 'hide_main':
    case 'start_input_recording':
    case 'retry_transcription':
    case 'install_update':
    case 'set_dev_logging_enabled':
    case 'create_dictionary_entry':
    case 'edit_dictionary_entry':
    case 'remove_dictionary_entry':
    case 'create_snippet':
    case 'edit_snippet':
    case 'remove_snippet':
    case 'start_calibration_monitoring':
    case 'stop_calibration_monitoring':
      return undefined as T;
    case 'save_app_mappings':
      writeDevSetting('app_mappings', args?.mappings ?? []);
      return undefined as T;
    case 'download_logs':
      return 'browser-dev://open-flow-logs.txt' as T;
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
  return Promise.resolve(() => {});
}

export function emit<T>(event: string, payload?: T): Promise<void> {
  if (hasTauriInternals()) {
    return tauriEmit(event, payload);
  }
  return Promise.resolve();
}

export function getVersion(): Promise<string> {
  if (hasTauriInternals()) {
    return getTauriVersion();
  }
  return Promise.resolve(__APP_VERSION__);
}
