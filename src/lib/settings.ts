import { invoke } from './tauri';
import type { TranscriptionLanguageCode } from './transcriptionLanguages';

export type ProviderId = 'groq' | 'openai' | 'google';
export type ProviderModelMap = Record<ProviderId, string[]>;
export type ToneId = 'casual' | 'formal' | 'very_casual';
export type CleanupIntensity = 'none' | 'light' | 'medium' | 'high';
export type HistoryRetention = '7 days' | '30 days' | '90 days' | 'Forever';
export type AppearanceMode = 'system' | 'light' | 'dark';
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
  cleanup_fallback_models: string[];
  cleanup_enabled: boolean;
  default_tone: ToneId;
  cleanup_intensity: CleanupIntensity;
  app_mappings: AppMapping[];
  noise_reduction: boolean;
  mute_audio: boolean;
  exclusive_mic: boolean;
  mic_gain: number;
  setup_complete: boolean;
  force_setup_on_launch: boolean;
  app_context_hint: boolean;
  auto_learn_enabled: boolean;
  contextual_caps_enabled: boolean;
  auto_spacing_enabled: boolean;
  caps_lock_uppercase_enabled: boolean;
  history_retention: HistoryRetention;
  microphone_device: string | null;
  update_dismissed_version: string | null;
  update_notified_version: string | null;
  appearance_mode: AppearanceMode;
  advanced_model_ui: boolean;
  cleanup_prompt_overrides: Record<string, string>;
};

export type SettingKey = keyof SettingsValueMap;

export function saveSetting<K extends SettingKey>(key: K, value: SettingsValueMap[K]) {
  return invoke('save_setting', { key, value });
}
