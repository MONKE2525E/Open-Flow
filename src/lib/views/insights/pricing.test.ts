import { describe, expect, it } from 'vitest';
import { estimateCost } from './pricing';

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
