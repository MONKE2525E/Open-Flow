import { invoke } from './tauri';
import type { TranscriptionLanguageCode } from './transcriptionLanguages';

export type ProviderId = 'groq' | 'openai' | 'google' | 'assemblyai' | 'local';
export type ProviderModelMap = Record<ProviderId, string[]>;
export type ToneId = 'casual' | 'formal' | 'very_casual';
export type CleanupIntensity = 'none' | 'light' | 'medium' | 'high';
export type HistoryRetention = '7 days' | '30 days' | '90 days' | 'Forever';
export type AppearanceMode = 'system' | 'light' | 'dark';
export type LocalModelMemoryPolicy =
  | 'keep_loaded'
  | 'unload_after_5m'
  | 'unload_after_15m'
  | 'unload_immediately';
export type { TranscriptionLanguageCode } from './transcriptionLanguages';

export interface AppMapping {
  exe: string;
  profile: string;
  name?: string;
  cleanup_intensity?: CleanupIntensity;
}

type SettingsValueMap = {
  transcription_provider: ProviderId;
  transcription_language: TranscriptionLanguageCode;
  cleanup_provider: ProviderId;
  transcription_model: string;
  cleanup_model: string;
  transcription_models_by_provider: ProviderModelMap;
  cleanup_models_by_provider: ProviderModelMap;
  transcription_default_model: string;
  cleanup_default_model: string;
  transcription_fallback_models: string[];
  dual_transcription_enabled: boolean;
  cleanup_fallback_models: string[];
  cleanup_enabled: boolean;
  repair_hotkey: string[];
  default_tone: ToneId;
  cleanup_intensity: CleanupIntensity;
  app_mappings: AppMapping[];
  noise_reduction: boolean;
  mute_audio: boolean;
  exclusive_mic: boolean;
  pause_media_during_dictation: boolean;
  play_start_stop_sounds: boolean;
  sound_effects_volume: number;
  mic_gain: number;
  setup_complete: boolean;
  force_setup_on_launch: boolean;
  app_context_hint: boolean;
  auto_learn_enabled: boolean;
  contextual_formatting_enabled: boolean;
  /** @deprecated Compatibility mirror for one downgrade cycle. */
  contextual_caps_enabled: boolean;
  /** @deprecated Compatibility mirror for one downgrade cycle. */
  auto_spacing_enabled: boolean;
  caps_lock_uppercase_enabled: boolean;
  clipboard_phrase_enabled: boolean;
  clipboard_phrase: string;
  history_retention: HistoryRetention;
  local_model_memory_policy: LocalModelMemoryPolicy;
  microphone_device: string | null;
  update_dismissed_version: string | null;
  update_notified_version: string | null;
  beta_updates_enabled: boolean;
  verenu_service_checks_enabled: boolean;
  appearance_mode: AppearanceMode;
  accent_color: string | null;
  advanced_model_ui: boolean;
  /** One cleanup prompt for every model — see stores.svelte.ts. */
  cleanup_prompt_override: string;
  /** Derived cache of each provider's live model list. Written only by modelCatalogStore. */
  provider_model_cache: Record<string, unknown>;
  legacy_features_enabled: boolean;
  sync_enabled: boolean;
};

type SettingKey = keyof SettingsValueMap;

export function saveSetting<K extends SettingKey>(key: K, value: SettingsValueMap[K]) {
  return invoke('save_setting', { key, value });
}
