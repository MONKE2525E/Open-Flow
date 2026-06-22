import type { ProviderId, ProviderModelMap } from '../../settings';

export type TaskType = 'transcription' | 'cleanup';
export type UiProviderId = 'groq' | 'openai' | 'google';

export type ProviderSection = {
  id: UiProviderId;
  label: string;
  storeProvider: ProviderId;
};

export type AllSettingsPayload = {
  transcription_model?: string | null;
  cleanup_model?: string | null;
  transcription_models_by_provider?: unknown;
  cleanup_models_by_provider?: unknown;
  transcription_default_model?: string | null;
  cleanup_default_model?: string | null;
  transcription_fallback_models?: string[] | null;
  cleanup_fallback_models?: string[] | null;
  cleanup_prompt_overrides?: unknown;
};

export const providerSections: ProviderSection[] = [
  { id: 'groq', label: 'Groq', storeProvider: 'groq' },
  { id: 'openai', label: 'OpenAI', storeProvider: 'openai' },
  { id: 'google', label: 'Gemini', storeProvider: 'google' },
];

export const recommendedModels: Record<TaskType, Record<UiProviderId, { premium: string; standard: string }>> = {
  transcription: {
    groq: { premium: 'whisper-large-v3', standard: 'whisper-large-v3-turbo' },
    openai: { premium: 'gpt-4o-transcribe', standard: 'gpt-4o-mini-transcribe' },
    google: { premium: 'gemini-3.5-flash', standard: 'gemini-2.5-flash' },
  },
  cleanup: {
    groq: { premium: 'llama-3.3-70b-versatile', standard: 'llama-3.1-8b-instant' },
    openai: { premium: 'gpt-4o', standard: 'gpt-4o-mini' },
    google: { premium: 'gemini-3.5-flash', standard: 'gemini-2.5-flash' },
  },
};

export const emptyProviderModelMap = (): ProviderModelMap => ({ groq: [], openai: [], google: [] });

export function modelId(provider: ProviderId, modelName: string): string {
  return `${provider}/${modelName.trim()}`;
}

export function splitModelId(id: string): { provider: ProviderId; model: string } | null {
  const idx = id.indexOf('/');
  if (idx <= 0) return null;

  const provider = id.slice(0, idx) as ProviderId;
  const model = id.slice(idx + 1).trim();
  if (!['groq', 'openai', 'google'].includes(provider) || !model) return null;

  return { provider, model };
}

export function mergeProviderModelMap(raw: unknown): ProviderModelMap {
  const base = emptyProviderModelMap();
  if (!raw || typeof raw !== 'object') return base;

  for (const provider of ['groq', 'openai', 'google'] as ProviderId[]) {
    const values = (raw as Record<string, unknown>)[provider];
    if (Array.isArray(values)) {
      base[provider] = values.map((value) => String(value).trim()).filter(Boolean);
    }
  }

  return base;
}

export function taskLabel(type: TaskType): string {
  return type === 'transcription' ? 'Transcription' : 'Clean-up';
}
