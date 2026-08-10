import type { InsightsProviderUsage } from './types';

/*
 * ponytail: static price table. Published list rates, checked against provider
 * pricing pages; they drift. Upgrade path is a rates file shipped with the app
 * (or fetched from api.verenu.com alongside provider-status) if drift ever
 * matters — not a runtime OpenRouter lookup, which doesn't price audio models.
 *
 * Everything here is an ESTIMATE and must be labelled as such in the UI.
 * Verenu never bills anyone; users pay their provider directly.
 */

type Rate =
  | { kind: 'audio'; usd_per_hour: number }
  | { kind: 'token'; usd_per_m_in: number; usd_per_m_out: number };

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
  'gpt-4o-mini': { kind: 'token', usd_per_m_in: 0.15, usd_per_m_out: 0.6 },
  'gemini-3.5-flash': { kind: 'token', usd_per_m_in: 0.3, usd_per_m_out: 2.5 },
};

/*
 * Gemini's transcription path sends audio inline rather than as text tokens,
 * so the token-priced `gemini-3.5-flash` entry above doesn't apply there —
 * this task-scoped override kicks in instead. Keyed as "model#task".
 */
const TASK_OVERRIDES: Record<string, Rate> = {
  'gemini-3.5-flash#transcription': { kind: 'audio', usd_per_hour: 1.0 },
};

/** Rough tokenizer stand-in. Good to ~±15% for English prose, which is all an estimate needs. */
const CHARS_PER_TOKEN = 4;

/** Backend model ids may carry a provider prefix ("groq/whisper-large-v3-turbo",
 * "models/gemini-3.5-flash") or inconsistent casing — normalize before lookup. */
function normalizeModelId(model: string): string {
  return model.trim().toLowerCase().replace(/^.*\//, '');
}

function lookupRate(usage: InsightsProviderUsage): Rate | null {
  const key = normalizeModelId(usage.model);
  return TASK_OVERRIDES[`${key}#${usage.task}`] ?? PRICING[key] ?? null;
}

/** Cost in USD for one model's usage, or null when the model has no known rate. */
export function modelCost(usage: InsightsProviderUsage): number | null {
  const rate = lookupRate(usage);
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

export function estimateCost(providers: InsightsProviderUsage[]): CostSummary {
  const priced = providers.map((usage) => ({ usage, cost: modelCost(usage) }));
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
