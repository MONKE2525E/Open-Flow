import { describe, expect, it } from 'vitest';
import { estimateCost, type PricingSnapshot } from './pricing';
import { fmtUsd } from './helpers';

const usage = (model: string, task: 'cleanup' | 'transcription', input_chars = 0, output_chars = 0) => ({
  model,
  provider: 'google' as const,
  task,
  calls: 1,
  audio_ms: 0,
  input_chars,
  output_chars,
});

describe('Google provider cost estimates', () => {
  it('keeps Flash and Flash-Lite cleanup rates distinct', () => {
    const summary = estimateCost([
      usage('gemini-3.5-flash-lite', 'cleanup', 4_000_000, 4_000_000),
      usage('gemini-3.5-flash', 'cleanup', 4_000_000, 4_000_000),
    ]);

    expect(summary.rows[0].cost).toBeCloseTo(18.9, 8);
    expect(summary.rows[1].cost).toBeCloseTo(2.8, 8);
  });

  it('prices dedicated Gemini Transcribe as audio', () => {
    const summary = estimateCost([
      { ...usage('gemini-3.5-transcribe', 'transcription'), audio_ms: 3_600_000 },
    ]);

    expect(summary.rows[0].cost).toBeCloseTo(0.306, 8);
  });
});


const snapshot: PricingSnapshot = {
  fetched_at: 1,
  rates: [
    {
      model_id: 'qwen/qwen3.8-27b',
      prompt_usd_per_token: 0.00000042,
      completion_usd_per_token: 0.000003,
    },
  ],
};

describe('Insights pricing', () => {
  it('uses the OpenRouter rate for a fully qualified cleanup model id', () => {
    const summary = estimateCost([{
      model: 'qwen/qwen3.8-27b',
      provider: 'groq',
      task: 'cleanup',
      calls: 1,
      audio_ms: 0,
      input_chars: 4_000_000,
      output_chars: 4_000_000,
    }], snapshot);

    expect(summary.rows[0].cost).toBeCloseTo(3.42);
    expect(summary.hasUnpriced).toBe(false);
  });

  it('keeps a real sub-cent estimate visible', () => {
    expect(fmtUsd(0.00123)).toBe('$0.00123');
    expect(fmtUsd(0.000001)).toBe('$0.0000010');
  });
});

