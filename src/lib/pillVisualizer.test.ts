import { describe, expect, it } from 'vitest';
import {
  BARS,
  BAR_MAX_H,
  BAR_MIN_H,
  ENVELOPE_WINDOW_MS,
  HISTORY_SPAN_MS,
  createPillVisualizer,
  mapWindow,
  toDb,
} from './pillVisualizer';

// The pill paints at display refresh; the backend ships envelope batches at
// ~20Hz, each holding several 10ms peaks. The harness drives those two rates
// separately, because decoupling them is the whole point of the design.
const FRAME_MS = 1000 / 60;
const BATCH_MS = 50;

const CENTER = 5; // bars 5 and 6 straddle the centre

interface Frame {
  t: number;
  h: number[];
}

function simulate(
  amplitudeAt: (tMs: number) => number,
  durationMs: number,
  viz = createPillVisualizer()
): Frame[] {
  const frames: Frame[] = [];
  let nextBatch = 0;
  let emitted = 0;
  for (let t = 0; t <= durationMs; t += FRAME_MS) {
    while (nextBatch <= t) {
      const batch: number[] = [];
      const upto = nextBatch + BATCH_MS;
      for (let s = emitted * ENVELOPE_WINDOW_MS; s < upto; s += ENVELOPE_WINDOW_MS) {
        batch.push(Math.max(0, amplitudeAt(s)));
        emitted++;
      }
      viz.pushEnvelope(batch);
      nextBatch += BATCH_MS;
    }
    frames.push({ t, h: [...viz.step(t)] });
  }
  return frames;
}

function lcg(seed: number) {
  let s = seed >>> 0;
  return () => ((s = (s * 1664525 + 1013904223) >>> 0) / 4294967296);
}

/**
 * Steady broadband room noise — a fan, HVAC, a computer.
 *
 * Modelled as the 10ms PEAK of noise, which is the max of several hundred
 * samples and therefore concentrates tightly (Gumbel, ~1.3dB sd) rather than
 * swinging wildly. An earlier version of this generator drew a single Rayleigh
 * sample per window, producing near-zero nulls and ±10dB swings; tuning the
 * filtering against that fictitious signal produced badly wrong constants.
 * Slow mechanical drift is included because real fans have it.
 */
function fan(amp: number, seed: number): (t: number) => number {
  const rand = lcg(seed);
  let n = 0;
  return () => {
    n++;
    const u = Math.max(rand(), 1e-9);
    const gumbel = -Math.log(-Math.log(Math.max(1 - u, 1e-9)));
    const driftDb = Math.sin(((n * ENVELOPE_WINDOW_MS) / 1000) * 2 * Math.PI * 0.3);
    return amp * Math.pow(10, (gumbel * 0.95 + driftDb) / 20);
  };
}

/** Syllabic speech at ~4.5Hz with a pause — the thing that must dominate. */
function speech(t: number): number {
  if (t > 1800 && t < 2300) return 0;
  const syl = Math.pow(Math.abs(Math.sin((t / 1000) * Math.PI * 4.5)), 1.6);
  return 0.5 * (0.12 + 0.88 * syl);
}

const over =
  (bg: (t: number) => number, fg: (t: number) => number) =>
  (t: number) =>
    Math.max(bg(t), fg(t));

const between = (frames: Frame[], from: number, to = Infinity) =>
  frames.filter((f) => f.t >= from && f.t <= to);
const rowMean = (h: number[]) => h.reduce((a, b) => a + b, 0) / h.length;

function centreStats(frames: Frame[]) {
  const c = frames.map((f) => f.h[CENTER]);
  const mean = c.reduce((a, b) => a + b, 0) / c.length;
  const sd = Math.sqrt(c.reduce((a, b) => a + (b - mean) ** 2, 0) / c.length);
  const deltas: number[] = [];
  for (let i = 1; i < c.length; i++) deltas.push(c[i] - c[i - 1]);
  // Direction changes big enough to actually see. This is the churn metric: a
  // loud fan alone used to produce ~13 of these a second.
  let reversals = 0;
  let prev = 0;
  for (const d of deltas) {
    if (Math.abs(d) < 0.03) continue;
    if (prev !== 0 && Math.sign(d) !== prev) reversals++;
    prev = Math.sign(d);
  }
  const seconds = (frames[frames.length - 1].t - frames[0].t) / 1000;
  return {
    mean,
    sd,
    range: Math.max(...c) - Math.min(...c),
    reversalsPerSec: reversals / seconds,
  };
}

function correlate(a: number[], b: number[]): number {
  const n = Math.min(a.length, b.length);
  const ma = a.slice(0, n).reduce((x, y) => x + y, 0) / n;
  const mb = b.slice(0, n).reduce((x, y) => x + y, 0) / n;
  let num = 0,
    da = 0,
    db = 0;
  for (let i = 0; i < n; i++) {
    const x = a[i] - ma,
      y = b[i] - mb;
    num += x * y;
    da += x * x;
    db += y * y;
  }
  return da > 0 && db > 0 ? num / Math.sqrt(da * db) : 0;
}

describe('pill visualizer — mapping', () => {
  it('maps dB into an explicit window', () => {
    expect(toDb(1)).toBeCloseTo(0, 5);
    expect(toDb(0)).toBeLessThan(-80);
    expect(mapWindow(-20, -20, -2)).toBe(0);
    expect(mapWindow(-2, -20, -2)).toBe(1);
    expect(mapWindow(-11, -20, -2)).toBeCloseTo(0.5, 1);
    expect(mapWindow(-40, -20, -2)).toBe(0);
  });

  it('keeps every bar in range and finite for any input', () => {
    const bg = fan(0.03, 5);
    const frames = simulate((t) => Math.max(bg(t), t > 1000 && t < 2000 ? 1 : 0), 4000);
    for (const f of frames) {
      for (const h of f.h) {
        expect(Number.isFinite(h)).toBe(true);
        expect(h).toBeGreaterThanOrEqual(BAR_MIN_H - 1e-6);
        expect(h).toBeLessThanOrEqual(BAR_MAX_H + 1e-6);
      }
    }
  });
});

describe('pill visualizer — background noise', () => {
  // Before the adaptive noise floor, a loud fan ALONE sat at a mean of 9.5px in
  // a 3..16px row and churned at 13 direction reversals a second.
  it.each([
    ['a quiet room', 0.004],
    ['a moderate fan', 0.03],
    ['a loud fan', 0.08],
  ])('stays calm and stable with %s', (_label, amp) => {
    const s = centreStats(between(simulate(fan(amp as number, 42), 6000), 2500));
    expect(s.reversalsPerSec).toBeLessThan(2);
    expect(s.sd).toBeLessThan(0.4);
    expect(s.range).toBeLessThan(1.5);
  });

  it('does not render background as a dead flat line', () => {
    // Calm, but alive: a perfectly static row reads as the visualizer being off.
    const s = centreStats(between(simulate(fan(0.03, 8), 6000), 2500));
    expect(s.range).toBeGreaterThan(0.05);
  });

  it('rests at the same height whatever the background level is', () => {
    // The floor is adaptive, so a loud fan must not sit higher than a quiet one.
    const quiet = centreStats(between(simulate(fan(0.01, 3), 6000), 2500));
    const loud = centreStats(between(simulate(fan(0.12, 3), 6000), 2500));
    expect(Math.abs(loud.mean - quiet.mean)).toBeLessThan(1);
  });
});

describe('pill visualizer — speech over noise', () => {
  it('is clearly dominated by the voice over a fan', () => {
    const s = centreStats(between(simulate(over(fan(0.03, 7), speech), 5000), 2000));
    expect(s.range).toBeGreaterThan(8);
    expect(s.mean).toBeGreaterThan(6);
  });

  it('still shows speech over a loud fan', () => {
    // Regression: the noise floor fed back on itself here. Speech inflated the
    // measured spread, which raised the window bottom, which drove the level to
    // zero, which closed the gate, which is what permitted the floor to climb
    // further. The row died completely.
    const s = centreStats(between(simulate(over(fan(0.08, 7), speech), 5000), 2000));
    expect(s.range).toBeGreaterThan(4);
    expect(s.mean).toBeGreaterThan(4.5);
  });

  it('does not flatten speech that starts the recording', () => {
    // Regression: seeding the noise floor from the first sample declared an
    // opening word to be the background.
    const s = centreStats(between(simulate(speech, 5000), 400));
    expect(s.range).toBeGreaterThan(8);
  });

  it('does not flatten a low-gain microphone', () => {
    // Regression: reference acquisition used an absolute -35dBFS threshold.
    // A healthy but quiet input below it never initialized the display window.
    const gain = 0.01;
    const s = centreStats(
      between(
        simulate(over(fan(0.03 * gain, 17), (t) => speech(t) * gain), 5000),
        800
      )
    );
    expect(s.range).toBeGreaterThan(5);
    expect(s.mean).toBeGreaterThan(4);
  });

  it('does not let continuous speech walk the noise floor upward', () => {
    // The floor must not creep up under sustained talking and shrink the
    // waveform mid-sentence.
    const frames = simulate(over(fan(0.03, 12), speech), 8000);
    const early = centreStats(between(frames, 1000, 3000));
    const late = centreStats(between(frames, 6000, 8000));
    expect(late.range).toBeGreaterThan(early.range * 0.6);
  });

  it('responds to a speech onset without visible latency', () => {
    const bg = fan(0.03, 3);
    const frames = simulate((t) => (t < 2500 ? bg(t) : Math.max(bg(t), 0.6)), 3500);
    const baseWin = between(frames, 2000, 2500);
    const base = baseWin.reduce((a, f) => a + f.h[CENTER], 0) / baseWin.length;
    const peak = Math.max(...between(frames, 2500).map((f) => f.h[CENTER]));
    const firstVisible = between(frames, 2500).find(
      (f) => f.h[CENTER] >= base + (peak - base) * 0.1
    );
    expect(firstVisible).toBeDefined();
    // ~90ms of any figure here is the playhead buffer plus the centre bar's own
    // age, which are structural rather than filtering.
    expect((firstVisible as Frame).t - 2500).toBeLessThan(160);
  });

  it('settles smoothly back to the background when speech stops', () => {
    const bg = fan(0.03, 5);
    const frames = simulate((t) => (t < 2500 ? Math.max(bg(t), speech(t)) : bg(t)), 6000);
    const tail = centreStats(between(frames, 5000));
    expect(tail.sd).toBeLessThan(0.3);
    expect(tail.reversalsPerSec).toBeLessThan(2);
    expect(tail.mean).toBeLessThan(BAR_MIN_H + 1.5);
  });
});

describe('pill visualizer — flow', () => {
  it('carries content outward: the same shape reaches each bar in turn', () => {
    const frames = between(simulate(over(fan(0.02, 4), speech), 6000), 800);
    const at = (idx: number) => frames.map((f) => f.h[idx]);
    const step = Math.round(HISTORY_SPAN_MS / ((BARS - 1) / 2) / FRAME_MS);
    const inner = at(CENTER);
    const outer = at(CENTER - 1);
    const aligned = correlate(inner.slice(0, inner.length - step), outer.slice(step));
    expect(aligned).toBeGreaterThan(0.9);
    // Delayed must beat undelayed, or the bars are merely moving together —
    // which is the level-meter look this design exists to avoid.
    expect(aligned).toBeGreaterThan(correlate(inner, outer));
  });

  it('advances every frame, not only when a packet arrives', () => {
    const frames = between(simulate(over(fan(0.02, 6), speech), 3000), 800, 1700);
    const identical = frames.filter((f, i) => i > 0 && f.h[CENTER] === frames[i - 1].h[CENTER]);
    expect(identical.length).toBe(0);
  });

  it('shows different audio at different bars at the same instant', () => {
    const frames = between(simulate(over(fan(0.02, 9), speech), 4000), 1000);
    const spreads = frames.map((f) => Math.max(...f.h) - Math.min(...f.h));
    expect(spreads.reduce((a, b) => a + b, 0) / spreads.length).toBeGreaterThan(0.6);
  });

  it('stays mirrored left/right at every frame', () => {
    const frames = simulate(over(fan(0.02, 2), speech), 2500);
    for (const f of frames) {
      for (let i = 0; i < BARS / 2; i++) {
        expect(f.h[i]).toBeCloseTo(f.h[BARS - 1 - i], 9);
      }
    }
  });
});

describe('pill visualizer — loudness and motion quality', () => {
  it('draws a bigger waveform for louder speech', () => {
    const meanOf = (fs: Frame[]) => {
      const late = between(fs, 2500, 4000);
      return late.reduce((a, f) => a + rowMean(f.h), 0) / late.length;
    };
    const quiet = simulate(over(fan(0.01, 1), (t) => speech(t) * 0.15), 4000);
    const loud = simulate(over(fan(0.01, 1), speech), 4000);
    expect(meanOf(loud)).toBeGreaterThan(meanOf(quiet) + 1);
  });

  it('moves smoothly, with no snapping or stair-stepping', () => {
    const frames = between(simulate(over(fan(0.02, 13), speech), 4000), 600);
    const deltas: number[] = [];
    for (let i = 1; i < frames.length; i++) {
      deltas.push(Math.abs(frames[i].h[CENTER] - frames[i - 1].h[CENTER]));
    }
    // Generous enough to allow the attack when speech resumes after the pause,
    // which is legitimately the fastest motion in the run; a snap would be a
    // large fraction of the 13px range in a single frame.
    expect(Math.max(...deltas)).toBeLessThan(3.2);
    // Sub-pixel heights: device-pixel snapping quantized this range to ~13 steps.
    const distinct = new Set(frames.map((f) => f.h[CENTER].toFixed(3)));
    expect(distinct.size / frames.length).toBeGreaterThan(0.75);
  });

  it('rises without overshooting (critically damped)', () => {
    const frames = simulate((t) => (t < 600 ? 0.02 : 0.6), 1100);
    const rising = between(frames, 700, 1000).map((f) => f.h[CENTER]);
    for (let i = 1; i < rising.length; i++) {
      expect(rising[i]).toBeGreaterThanOrEqual(rising[i - 1] - 1e-6);
    }
  });

  it('rests on the idle floor in true silence', () => {
    const frames = simulate(() => 0, 1500);
    const settled = frames[frames.length - 1];
    for (const h of settled.h) expect(h).toBeCloseTo(BAR_MIN_H, 4);
  });

  it('resets every stage between recordings', () => {
    const viz = createPillVisualizer();
    simulate(over(fan(0.03, 21), speech), 2500, viz);
    viz.reset();
    const after = viz.step(0);
    for (const h of after) expect(h).toBeCloseTo(BAR_MIN_H, 6);
  });
});
