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
  provider_model_cache?: unknown;
};

export const providerSections: ProviderSection[] = [
  { id: 'groq', label: 'Groq', storeProvider: 'groq', tasks: ['transcription', 'cleanup'] },
  { id: 'openai', label: 'OpenAI', storeProvider: 'openai', tasks: ['transcription', 'cleanup'] },
  { id: 'google', label: 'Gemini', storeProvider: 'google', tasks: ['transcription', 'cleanup'] },
  { id: 'assemblyai', label: 'AssemblyAI', storeProvider: 'assemblyai', tasks: ['transcription'] },
];

export type ModelTag = 'accurate' | 'fast' | 'cheap';
export type ModelTier = 'premium' | 'standard';

export type CatalogEntry = {
  provider: ProviderId;
  /** Wire id, exactly as the provider names it. */
  id: string;
  label: string;
  tasks: TaskType[];
  tags: ModelTag[];
  /**
   * Presets index models by tier, so every (task, provider, tier) triple must
   * resolve to exactly one entry — see `buildRecommended`. Local models and any
   * future extras carry no tier and never take part in preset matching.
   */
  tier?: ModelTier;
};

/**
 * The models Verenu vouches for: display name, which task they serve, and how
 * they trade accuracy for speed.
 *
 * Deliberately short. It is not a mirror of what each provider offers — the
 * live `/v1/models` fetch supplies that, and the picker shows anything a
 * provider returns that isn't listed here under "All models". Adding a model
 * to the curated list is a data edit here, never a UI change.
 */
export const CATALOG: CatalogEntry[] = [
  { provider: 'groq', id: 'whisper-large-v3', label: 'Whisper Large v3', tasks: ['transcription'], tags: ['accurate'], tier: 'premium' },
  { provider: 'groq', id: 'whisper-large-v3-turbo', label: 'Whisper Large v3 Turbo', tasks: ['transcription'], tags: ['fast', 'cheap'], tier: 'standard' },
  { provider: 'groq', id: GROQ_QWEN_3_6_27B_MODEL, label: 'Qwen3.6 27B', tasks: ['cleanup'], tags: ['accurate'], tier: 'premium' },
  { provider: 'groq', id: GROQ_GPT_OSS_20B_MODEL, label: 'GPT OSS 20B', tasks: ['cleanup'], tags: ['fast', 'cheap'], tier: 'standard' },
  { provider: 'groq', id: 'openai/gpt-oss-120b', label: 'GPT OSS 120B', tasks: ['cleanup'], tags: ['accurate'] },
  { provider: 'groq', id: 'groq/compound', label: 'Compound', tasks: ['cleanup'], tags: ['accurate'] },
  { provider: 'groq', id: 'groq/compound-mini', label: 'Compound Mini', tasks: ['cleanup'], tags: ['fast'] },
  { provider: 'openai', id: 'gpt-transcribe', label: 'GPT Transcribe', tasks: ['transcription'], tags: ['accurate'], tier: 'premium' },
  { provider: 'openai', id: 'gpt-4o-mini-transcribe', label: 'GPT-4o mini Transcribe', tasks: ['transcription'], tags: ['fast', 'cheap'], tier: 'standard' },
  { provider: 'openai', id: 'gpt-4o-transcribe', label: 'GPT-4o Transcribe', tasks: ['transcription'], tags: ['accurate'] },
  { provider: 'openai', id: 'gpt-live-transcribe', label: 'GPT Live Transcribe', tasks: ['transcription'], tags: ['fast'] },
  { provider: 'openai', id: 'whisper-1', label: 'Whisper v1', tasks: ['transcription'], tags: ['cheap'] },
  { provider: 'openai', id: 'gpt-5.6', label: 'GPT-5.6', tasks: ['cleanup'], tags: ['accurate'], tier: 'premium' },
  { provider: 'openai', id: 'gpt-5.6-luna', label: 'GPT-5.6 Luna', tasks: ['cleanup'], tags: ['fast', 'cheap'], tier: 'standard' },
  { provider: 'openai', id: 'gpt-5.6-sol', label: 'GPT-5.6 Sol', tasks: ['cleanup'], tags: ['accurate'] },
  { provider: 'openai', id: 'gpt-5.6-terra', label: 'GPT-5.6 Terra', tasks: ['cleanup'], tags: ['fast'] },
  { provider: 'google', id: 'gemini-3.7-flash', label: 'Gemini 3.7 Flash', tasks: ['transcription', 'cleanup'], tags: ['accurate'], tier: 'premium' },
  { provider: 'google', id: 'gemini-3.5-transcribe', label: 'Gemini 3.5 Transcribe', tasks: ['transcription'], tags: ['accurate'] },
  { provider: 'google', id: 'gemini-2.5-flash', label: 'Gemini 2.5 Flash', tasks: ['transcription', 'cleanup'], tags: ['fast', 'cheap'], tier: 'standard' },
  { provider: 'google', id: 'gemini-3.6-flash', label: 'Gemini 3.6 Flash', tasks: ['transcription', 'cleanup'], tags: ['accurate'] },
  { provider: 'google', id: 'gemini-3.5-flash', label: 'Gemini 3.5 Flash', tasks: ['transcription', 'cleanup'], tags: ['fast'] },
  { provider: 'google', id: 'gemini-3.5-flash-lite', label: 'Gemini 3.5 Flash Lite', tasks: ['transcription', 'cleanup'], tags: ['fast', 'cheap'] },
  { provider: 'google', id: 'gemini-3.1-flash-lite', label: 'Gemini 3.1 Flash Lite', tasks: ['transcription', 'cleanup'], tags: ['fast', 'cheap'] },
  { provider: 'google', id: 'gemini-3.1-pro-preview', label: 'Gemini 3.1 Pro', tasks: ['cleanup'], tags: ['accurate'] },
  { provider: 'google', id: 'gemini-2.5-pro', label: 'Gemini 2.5 Pro', tasks: ['cleanup'], tags: ['accurate'] },
  { provider: 'google', id: 'gemini-2.5-flash-lite', label: 'Gemini 2.5 Flash Lite', tasks: ['transcription', 'cleanup'], tags: ['fast', 'cheap'] },
  { provider: 'google', id: 'gemini-2.0-flash', label: 'Gemini 2.0 Flash', tasks: ['transcription', 'cleanup'], tags: ['fast'] },
  { provider: 'google', id: 'gemini-2.0-flash-lite', label: 'Gemini 2.0 Flash Lite', tasks: ['transcription', 'cleanup'], tags: ['fast', 'cheap'] },
  { provider: 'google', id: 'gemini-flash-latest', label: 'Gemini Flash (latest)', tasks: ['transcription', 'cleanup'], tags: ['fast'] },
  { provider: 'google', id: 'gemini-omni-flash', label: 'Gemini Omni Flash', tasks: ['transcription', 'cleanup'], tags: ['accurate'] },
  { provider: 'assemblyai', id: 'universal-3-5-pro', label: 'Universal 3.5 Pro', tasks: ['transcription'], tags: ['accurate'], tier: 'premium' },
  { provider: 'assemblyai', id: 'universal-2', label: 'Universal-2', tasks: ['transcription'], tags: ['fast', 'cheap'], tier: 'standard' },
  { provider: 'local', id: 'parakeet-v3', label: 'Parakeet V3', tasks: ['transcription'], tags: ['accurate'] },
  { provider: 'local', id: 'parakeet-v2', label: 'Parakeet V2', tasks: ['transcription'], tags: ['accurate'] },
  { provider: 'local', id: 'moonshine-base', label: 'Moonshine Base', tasks: ['transcription'], tags: ['fast'] },
  { provider: 'local', id: 'moonshine-tiny', label: 'Moonshine Tiny', tasks: ['transcription'], tags: ['fast'] },
  { provider: 'local', id: 'moonshine-small', label: 'Moonshine Small', tasks: ['transcription'], tags: ['fast'] },
  { provider: 'local', id: 'moonshine-medium', label: 'Moonshine Medium', tasks: ['transcription'], tags: ['accurate'] },
  { provider: 'local', id: 'sense-voice', label: 'SenseVoice', tasks: ['transcription'], tags: ['fast'] },
  { provider: 'local', id: 'gigaam-v3', label: 'GigaAM v3', tasks: ['transcription'], tags: ['accurate'] },
  { provider: 'local', id: 'canary-180m-flash', label: 'Canary 180M Flash', tasks: ['transcription'], tags: ['fast'] },
  { provider: 'local', id: 'canary-1b-v2', label: 'Canary 1B v2', tasks: ['transcription'], tags: ['accurate'] },
  { provider: 'local', id: 'cohere', label: 'Cohere', tasks: ['transcription'], tags: ['accurate'] },
  { provider: 'local', id: 'gemma-4-e2b', label: 'Gemma 4 E2B', tasks: ['cleanup'], tags: ['fast'] },
  { provider: 'local', id: 'gemma-4-e4b', label: 'Gemma 4 E4B', tasks: ['cleanup'], tags: ['accurate'] },
  { provider: 'local', id: 'qwen2.5-0.5b-instruct', label: 'Qwen 2.5 0.5B Instruct', tasks: ['cleanup'], tags: ['fast'] },
  { provider: 'local', id: 'qwen2.5-1.5b-instruct', label: 'Qwen 2.5 1.5B Instruct', tasks: ['cleanup'], tags: ['fast'] },
  { provider: 'local', id: 'qwen2.5-3b-instruct', label: 'Qwen 2.5 3B Instruct', tasks: ['cleanup'], tags: ['accurate'] },
  { provider: 'local', id: 'qwen2.5-7b-instruct', label: 'Qwen 2.5 7B Instruct', tasks: ['cleanup'], tags: ['accurate'] },
  { provider: 'local', id: 'phi-3-mini-4k-instruct', label: 'Phi-3 Mini 4K Instruct', tasks: ['cleanup'], tags: ['fast'] },
  { provider: 'local', id: 'smollm2-360m-instruct', label: 'SmolLM2 360M Instruct', tasks: ['cleanup'], tags: ['fast'] },
  { provider: 'local', id: 'smollm2-1.7b-instruct', label: 'SmolLM2 1.7B Instruct', tasks: ['cleanup'], tags: ['fast'] },
  { provider: 'local', id: 'granite-3.3-2b-instruct', label: 'Granite 3.3 2B Instruct', tasks: ['cleanup'], tags: ['fast'] },
  { provider: 'local', id: 'granite-3.3-8b-instruct', label: 'Granite 3.3 8B Instruct', tasks: ['cleanup'], tags: ['accurate'] },
];

const CATALOG_BY_KEY = new Map(CATALOG.map((entry) => [`${entry.provider}/${entry.id}`, entry]));

export function catalogEntry(provider: ProviderId, model: string): CatalogEntry | undefined {
  return CATALOG_BY_KEY.get(`${provider}/${model.trim()}`);
}

export function catalogFor(task: TaskType, provider?: ProviderId): CatalogEntry[] {
  return CATALOG.filter(
    (entry) => entry.tasks.includes(task) && (!provider || entry.provider === provider),
  );
}

/**
 * Derives the `{ premium, standard }` shape the preset code indexes directly.
 *
 * Throws on a duplicate (task, provider, tier) rather than letting array order
 * silently pick a winner — a preset quietly changing model during an unrelated
 * catalog edit is exactly the bug this guards.
 */
export function buildRecommended(
  catalog: CatalogEntry[],
): Record<TaskType, Partial<Record<UiProviderId, { premium: string; standard: string }>>> {
  const built: Record<string, Record<string, Record<string, string>>> = {
    transcription: {},
    cleanup: {},
  };

  for (const entry of catalog) {
    if (!entry.tier || entry.provider === 'local') continue;
    for (const task of entry.tasks) {
      const provider = (built[task][entry.provider] ??= {});
      if (provider[entry.tier]) {
        throw new Error(
          `Two ${entry.tier} models for ${entry.provider} ${task}: ${provider[entry.tier]} and ${entry.id}`,
        );
      }
      provider[entry.tier] = entry.id;
    }
  }

  return built as Record<
    TaskType,
    Partial<Record<UiProviderId, { premium: string; standard: string }>>
  >;
}

export const recommendedModels = buildRecommended(CATALOG);

export function migrateDeprecatedGroqCleanupModel(model: string): string {
  const normalized = model.trim();
  if (normalized === DEPRECATED_GROQ_LLAMA_8B_MODEL) return GROQ_GPT_OSS_20B_MODEL;
  if (normalized === DEPRECATED_GROQ_LLAMA_70B_MODEL) return GROQ_QWEN_3_6_27B_MODEL;
  return normalized;
}

export function migrateDeprecatedGoogleModel(model: string): string {
  return model.trim() === 'gemini-3.5-flash'
    ? 'gemini-3.7-flash'
    : model.trim();
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
      base[provider] = values
        .map((value) => String(value).trim())
        .filter(Boolean)
        .map((value) => provider === 'google' ? migrateDeprecatedGoogleModel(value) : value)
        .filter((value, index, models) => models.indexOf(value) === index);
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

/**
 * "Provider Model", without saying the provider twice. Gemini's models are
 * literally named "Gemini 3.7 Flash", so a blind prefix reads "Gemini Gemini
 * 3.7 Flash"; AssemblyAI's don't, so they need the prefix to be identifiable.
 */
export function qualifiedModelLabel(provider: ProviderId, model: string): string {
  const label = modelDisplayLabel(provider, model);
  const prefix = providerDisplayLabel(provider);
  return label.toLowerCase().startsWith(prefix.toLowerCase()) ? label : `${prefix} ${label}`;
}

/**
 * Pretty name for a model, or the raw wire id when it isn't curated — the
 * "All models" section shows plenty of ids Verenu has never heard of.
 */
export function modelDisplayLabel(provider: ProviderId, model: string): string {
  return catalogEntry(provider, model)?.label ?? model;
}
