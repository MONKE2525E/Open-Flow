import type { ProviderId, ProviderModelMap } from '../../settings';

export type TaskType = 'transcription' | 'cleanup';
export type UiProviderId = 'groq' | 'openai' | 'google' | 'assemblyai';

export const GROQ_GPT_OSS_20B_MODEL = 'openai/gpt-oss-20b';
export const GROQ_QWEN_3_6_27B_MODEL = 'qwen/qwen3.6-27b';
const DEPRECATED_GROQ_LLAMA_8B_MODEL = 'llama-3.1-8b-instant';
const DEPRECATED_GROQ_LLAMA_70B_MODEL = 'llama-3.3-70b-versatile';

export type ProviderSection = {
  id: UiProviderId;
  label: string;
  storeProvider: ProviderId;
  /** Which tasks this provider's models can be selected for — AssemblyAI is transcription-only. */
  tasks: TaskType[];
};

export type AllSettingsPayload = {
  transcription_model?: string | null;
  cleanup_model?: string | null;
  transcription_models_by_provider?: unknown;
  cleanup_models_by_provider?: unknown;
  transcription_default_model?: string | null;
  cleanup_default_model?: string | null;
  transcription_fallback_models?: string[] | null;
  dual_transcription_enabled?: boolean | null;
  cleanup_fallback_models?: string[] | null;
  cleanup_prompt_overrides?: unknown;
  local_model_memory_policy?: string | null;
};

export const providerSections: ProviderSection[] = [
  { id: 'groq', label: 'Groq', storeProvider: 'groq', tasks: ['transcription', 'cleanup'] },
  { id: 'openai', label: 'OpenAI', storeProvider: 'openai', tasks: ['transcription', 'cleanup'] },
  { id: 'google', label: 'Gemini', storeProvider: 'google', tasks: ['transcription', 'cleanup'] },
  { id: 'assemblyai', label: 'AssemblyAI', storeProvider: 'assemblyai', tasks: ['transcription'] },
];

export const recommendedModels: Record<TaskType, Partial<Record<UiProviderId, { premium: string; standard: string }>>> = {
  transcription: {
    groq: { premium: 'whisper-large-v3', standard: 'whisper-large-v3-turbo' },
    openai: { premium: 'gpt-4o-transcribe', standard: 'gpt-4o-mini-transcribe' },
    google: { premium: 'gemini-3.5-flash', standard: 'gemini-2.5-flash' },
    assemblyai: { premium: 'universal-3-5-pro', standard: 'universal-2' },
  },
  cleanup: {
    groq: { premium: GROQ_QWEN_3_6_27B_MODEL, standard: GROQ_GPT_OSS_20B_MODEL },
    openai: { premium: 'gpt-4o', standard: 'gpt-4o-mini' },
    google: { premium: 'gemini-3.5-flash', standard: 'gemini-2.5-flash' },
  },
};

export function migrateDeprecatedGroqCleanupModel(model: string): string {
  const normalized = model.trim();
  if (normalized === DEPRECATED_GROQ_LLAMA_8B_MODEL) return GROQ_GPT_OSS_20B_MODEL;
  if (normalized === DEPRECATED_GROQ_LLAMA_70B_MODEL) return GROQ_QWEN_3_6_27B_MODEL;
  return normalized;
}

export const emptyProviderModelMap = (): ProviderModelMap => ({
  groq: [],
  openai: [],
  google: [],
  assemblyai: [],
  local: [],
});

export function modelId(provider: ProviderId, modelName: string): string {
  return `${provider}/${modelName.trim()}`;
}

export function splitModelId(id: string): { provider: ProviderId; model: string } | null {
  const idx = id.indexOf('/');
  if (idx <= 0) return null;

  const provider = id.slice(0, idx) as ProviderId;
  const model = id.slice(idx + 1).trim();
  if (!['groq', 'openai', 'google', 'assemblyai', 'local'].includes(provider) || !model) return null;

  return { provider, model };
}

export function mergeProviderModelMap(raw: unknown): ProviderModelMap {
  const base = emptyProviderModelMap();
  if (!raw || typeof raw !== 'object') return base;

  for (const provider of ['groq', 'openai', 'google', 'assemblyai', 'local'] as ProviderId[]) {
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

export function providerDisplayLabel(provider: ProviderId): string {
  switch (provider) {
    case 'openai':
      return 'OpenAI';
    case 'google':
      return 'Gemini';
    case 'assemblyai':
      return 'AssemblyAI';
    case 'local':
      return 'Local';
    default:
      return 'Groq';
  }
}

export function modelDisplayLabel(provider: ProviderId, model: string): string {
  if (provider === 'groq' && model === GROQ_GPT_OSS_20B_MODEL) {
    return 'GPT OSS 20B';
  }
  if (provider === 'groq' && model === GROQ_QWEN_3_6_27B_MODEL) {
    return 'Qwen3.6 27B';
  }
  if (provider === 'assemblyai') {
    switch (model) {
      case 'universal-3-5-pro':
        return 'Universal 3.5 Pro';
      case 'universal-2':
        return 'Universal-2';
      default:
        return model;
    }
  }
  if (provider === 'local') {
    switch (model) {
      case 'gemma-4-e2b':
        return 'Gemma 4 E2B';
      case 'gemma-4-e4b':
        return 'Gemma 4 E4B';
      case 'parakeet-v3':
        return 'Parakeet V3';
      case 'parakeet-v2':
        return 'Parakeet V2';
      case 'moonshine-base':
        return 'Moonshine Base';
      case 'moonshine-tiny':
        return 'Moonshine Tiny';
      case 'moonshine-small':
        return 'Moonshine Small';
      case 'moonshine-medium':
        return 'Moonshine Medium';
      case 'sense-voice':
        return 'SenseVoice';
      case 'gigaam-v3':
        return 'GigaAM v3';
      case 'canary-180m-flash':
        return 'Canary 180M Flash';
      case 'canary-1b-v2':
        return 'Canary 1B v2';
      case 'cohere':
        return 'Cohere';
      case 'qwen2.5-0.5b-instruct':
        return 'Qwen 2.5 0.5B Instruct';
      case 'qwen2.5-1.5b-instruct':
        return 'Qwen 2.5 1.5B Instruct';
      case 'qwen2.5-3b-instruct':
        return 'Qwen 2.5 3B Instruct';
      case 'qwen2.5-7b-instruct':
        return 'Qwen 2.5 7B Instruct';
      case 'phi-3-mini-4k-instruct':
        return 'Phi-3 Mini 4K Instruct';
      case 'smollm2-360m-instruct':
        return 'SmolLM2 360M Instruct';
      case 'smollm2-1.7b-instruct':
        return 'SmolLM2 1.7B Instruct';
      case 'granite-3.3-2b-instruct':
        return 'Granite 3.3 2B Instruct';
      case 'granite-3.3-8b-instruct':
        return 'Granite 3.3 8B Instruct';
      default:
        return model;
    }
  }
  return model;
}
