import { describe, expect, it } from 'vitest';
import { PACE_TICKS, PACE_TICK_W, paceScale, paceTickOffset } from './helpers';

describe('pace meter scale', () => {
  it('never leaves a lone tick trailing past the best marker', () => {
    // The regression: best 245 on a 250 ceiling put the marker one tick short
    // of the end, so a single orphan hairline sat after it.
    for (let best = 1; best <= 600; best++) {
      const { bestTick } = paceScale(best);
      expect(bestTick, `best=${best}`).not.toBe(PACE_TICKS - 2);
      expect(bestTick).toBeGreaterThanOrEqual(0);
      expect(bestTick).toBeLessThan(PACE_TICKS);
    }
  });

  it('grows the ceiling past the best instead of pinning it to the last tick', () => {
    expect(paceScale(120).max).toBe(200); // floor, so slow talkers keep a stable scale
    expect(paceScale(245).max).toBe(250);
    expect(paceScale(251).max).toBe(250);
    expect(paceScale(254).bestTick).toBe(PACE_TICKS - 1);
  });

  it('has no marker before a best is measured', () => {
    expect(paceScale(0)).toEqual({ max: 200, bestTick: -1 });
  });
});

describe('pace meter tick placement', () => {
  // 1.25 and 1.5 are the Windows display-scaling factors where whole CSS
  // pixels are not whole device pixels.
  it('puts every tick on a device pixel once measured', () => {
    for (const dpr of [1, 1.25, 1.5, 2, 3]) {
      for (const width of [159, 186, 237, 268, 348, 428, 486.7]) {
        for (let i = 0; i < PACE_TICKS; i++) {
          const px = Number(paceTickOffset(i, width, dpr).replace('px', ''));
          expect(Number.isInteger(px * dpr), `dpr=${dpr} w=${width} i=${i}`).toBe(true);
        }
      }
    }
  });

  it('keeps every tick at the same subpixel phase', () => {
    for (const dpr of [1, 1.25, 1.5, 2]) {
      const phases = new Set(
        Array.from({ length: PACE_TICKS }, (_, i) =>
          ((Number(paceTickOffset(i, 237, dpr).replace('px', '')) * dpr) % 1).toFixed(6)
        )
      );
      expect(phases, `dpr=${dpr}`).toHaveLength(1);
    }
  });

  it('spans the full row without overflowing it', () => {
    const width = 237;
    expect(paceTickOffset(0, width, 1)).toBe('0px');
    expect(paceTickOffset(PACE_TICKS - 1, width, 1)).toBe(`${width - PACE_TICK_W}px`);
  });

  it('falls back to percentages before the row is measured', () => {
    expect(paceTickOffset(0, 0, 1)).toBe('0%');
    expect(paceTickOffset(PACE_TICKS - 1, 0, 1)).toBe('100%');
  });
});
