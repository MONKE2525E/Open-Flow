import { invoke } from '@tauri-apps/api/core';
import type { TranscriptionLanguageCode } from './transcriptionLanguages';

export type ProviderId = 'groq' | 'openai' | 'google';
export type ToneId = 'casual' | 'formal' | 'very_casual';
export type CleanupIntensity = 'none' | 'light' | 'medium' | 'high';
export type HistoryRetention = '7 days' | '30 days' | '90 days' | 'Forever';
export type AppearanceMode = 'system' | 'light' | 'dark';
export type { TranscriptionLanguageCode } from './transcriptionLanguages';

export interface AppMapping {
  exe: string;
  profile: string;
  name?: string;
}

type SettingsValueMap = {
  transcription_provider: ProviderId;
  transcription_language: TranscriptionLanguageCode;
  cleanup_provider: ProviderId;
  transcription_model: string;
  cleanup_model: string;
  cleanup_enabled: boolean;
  default_tone: ToneId;
  cleanup_intensity: CleanupIntensity;
  app_mappings: AppMapping[];
  noise_reduction: boolean;
  mute_audio: boolean;
  mic_gain: number;
  setup_complete: boolean;
  force_setup_on_launch: boolean;
  app_context_hint: boolean;
  api_fallback_enabled: boolean;
  auto_learn_enabled: boolean;
  contextual_caps_enabled: boolean;
  auto_spacing_enabled: boolean;
  history_retention: HistoryRetention;
  microphone_device: string | null;
  update_dismissed_version: string | null;
  appearance_mode: AppearanceMode;
};

export type SettingKey = keyof SettingsValueMap;

export function saveSetting<K extends SettingKey>(key: K, value: SettingsValueMap[K]) {
  return invoke('save_setting', { key, value });
}
