import type { InsightsProviderUsage } from './types';

/*
 * Bundled fallback rates for audio models and providers that OpenRouter does
 * not publish. Token-priced cleanup models use the cached OpenRouter snapshot.
 * The refresh is performed by the desktop backend and persisted locally.
 *
 * Everything here is an ESTIMATE and must be labelled as such in the UI.
 * Verenu never bills anyone; users pay their provider directly.
 */

type Rate =
  | { kind: 'audio'; usd_per_hour: number }
  | { kind: 'token'; usd_per_m_in: number; usd_per_m_out: number };

export interface PricingRate {
  model_id: string;
  prompt_usd_per_token: number;
  completion_usd_per_token: number;
}

export interface PricingSnapshot {
  fetched_at: number;
  rates: PricingRate[];
}

export const PRICING: Record<string, Rate> = {
  // Transcription — billed on audio duration
  'whisper-large-v3-turbo': { kind: 'audio', usd_per_hour: 0.04 },
  'whisper-large-v3': { kind: 'audio', usd_per_hour: 0.111 },
  'distil-whisper-large-v3-en': { kind: 'audio', usd_per_hour: 0.02 },
  'whisper-1': { kind: 'audio', usd_per_hour: 0.36 },
  'gpt-4o-transcribe': { kind: 'audio', usd_per_hour: 0.36 },
  'gpt-4o-mini-transcribe': { kind: 'audio', usd_per_hour: 0.18 },

  // Cleanup — billed on tokens
  'llama-3.3-70b-versatile': { kind: 'token', usd_per_m_in: 0.59, usd_per_m_out: 0.79 },
  'qwen3.6-27b': { kind: 'token', usd_per_m_in: 0.60, usd_per_m_out: 3.00 },
  'qwen3.8-27b': { kind: 'token', usd_per_m_in: 0.80, usd_per_m_out: 4.00 },
  'gpt-4o-mini': { kind: 'token', usd_per_m_in: 0.15, usd_per_m_out: 0.6 },
  'gemini-3.5-flash': { kind: 'token', usd_per_m_in: 2.70, usd_per_m_out: 16.20 },
  'gemini-3.5-flash-lite': { kind: 'token', usd_per_m_in: 0.30, usd_per_m_out: 2.50 },
  'gemini-2.5-flash': { kind: 'token', usd_per_m_in: 0.15, usd_per_m_out: 1.25 },
  'gemini-2.5-flash-lite': { kind: 'token', usd_per_m_in: 0.10, usd_per_m_out: 0.40 },
};

/*
 * Gemini's transcription path sends audio inline rather than as text tokens,
 * so the token-priced `gemini-3.5-flash` entry above doesn't apply there —
 * this task-scoped override kicks in instead. Keyed as "model#task".
 */
const TASK_OVERRIDES: Record<string, Rate> = {
  'gemini-3.5-transcribe#transcription': { kind: 'audio', usd_per_hour: 0.306 },
};

/** Rough tokenizer stand-in. Good to ~±15% for English prose, which is all an estimate needs. */
const CHARS_PER_TOKEN = 4;

/** Backend model ids may carry a provider prefix ("groq/whisper-large-v3-turbo",
 * "models/gemini-3.5-flash") or inconsistent casing — normalize before lookup. */
function normalizeModelId(model: string | null | undefined): string {
  return String(model ?? '').trim().toLowerCase();
}

function shortModelId(model: string): string {
  return model.replace(/^.*\//, '');
}

function lookupOpenRouterRate(usage: InsightsProviderUsage, snapshot: PricingSnapshot | null): Rate | null {
  if (usage.task === 'transcription' || !snapshot) return null;
  const model = normalizeModelId(usage.model);
  const short = shortModelId(model);
  const provider = String(usage.provider ?? '').trim().toLowerCase();
  const providerQualified = provider ? `${provider}/${short}` : null;
  const exact = snapshot.rates.find((rate) => {
    const id = normalizeModelId(rate.model_id);
    return id === model || (providerQualified !== null && id === providerQualified);
  });
  const published = exact ?? snapshot.rates.find((rate) => shortModelId(normalizeModelId(rate.model_id)) === short);
  if (!published) return null;
  return {
    kind: 'token',
    usd_per_m_in: published.prompt_usd_per_token * 1e6,
    usd_per_m_out: published.completion_usd_per_token * 1e6,
  };
}

function lookupRate(usage: InsightsProviderUsage, snapshot: PricingSnapshot | null): Rate | null {
  const key = shortModelId(normalizeModelId(usage.model));
  const openRouterRate = lookupOpenRouterRate(usage, snapshot);
  if (openRouterRate) return openRouterRate;
  // The backend writes tasks in lowercase, but normalize defensively so a
  // mixed-case or whitespace-padded value still matches the override keys.
  const task = String(usage.task ?? '').trim().toLowerCase();
  return TASK_OVERRIDES[`${key}#${task}`] ?? PRICING[key] ?? null;
}

/** Cost in USD for one model's usage, or null when the model has no known rate. */
function modelCost(usage: InsightsProviderUsage, snapshot: PricingSnapshot | null): number | null {
  const rate = lookupRate(usage, snapshot);
  if (!rate) return null;
  if (rate.kind === 'audio') {
    return (usage.audio_ms / 3_600_000) * rate.usd_per_hour;
  }
  const inTokens = usage.input_chars / CHARS_PER_TOKEN;
  const outTokens = usage.output_chars / CHARS_PER_TOKEN;
  return (inTokens / 1e6) * rate.usd_per_m_in + (outTokens / 1e6) * rate.usd_per_m_out;
}

export interface CostRow extends InsightsProviderUsage {
  cost: number | null;
  /** Fraction of the priced total, 0–1. Zero for unpriced models. */
  share: number;
}

export interface CostSummary {
  rows: CostRow[];
  total: number;
  /** True when at least one model had no rate, so `total` understates reality. */
  hasUnpriced: boolean;
}

export function estimateCost(providers: InsightsProviderUsage[], snapshot: PricingSnapshot | null = null): CostSummary {
  const priced = providers.map((usage) => ({ usage, cost: modelCost(usage, snapshot) }));
  const total = priced.reduce((sum, p) => sum + (p.cost ?? 0), 0);

  const rows: CostRow[] = priced
    .map(({ usage, cost }) => ({
      ...usage,
      cost,
      share: cost !== null && total > 0 ? cost / total : 0,
    }))
    .sort((a, b) => (b.cost ?? -1) - (a.cost ?? -1));

  return { rows, total, hasUnpriced: priced.some((p) => p.cost === null) };
}
