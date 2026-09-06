/** Average words in a published novel — the unit behind "you've written N books". */
const WORDS_PER_BOOK = 80_000;

export function fmtNumber(n: number): string {
  return Math.round(n).toLocaleString();
}

/** Compact form for hero tiles: 1.2k, 3.4M. Exact below 1000. */
export function fmtCompact(n: number): { value: string; suffix: string } {
  const abs = Math.abs(n);
  // Each tier's threshold accounts for the rounding of the tier below it:
  // 999.6 renders as "1.0k" (not "1000") and 999_950 as "1.0M" (not
  // "1000.0k").
  if (Math.round(abs / 1000) >= 1000) return { value: (n / 1e6).toFixed(1), suffix: 'M' };
  if (Math.round(abs) >= 1000) return { value: (n / 1000).toFixed(1), suffix: 'k' };
  return { value: String(Math.round(n)), suffix: '' };
}

export function fmtUsd(n: number | null): string {
  if (n === null) return '—';
  if (n === 0) return '$0.00';
  // Only small *positive* amounts render as "<$0.01" — a negative amount
  // must format normally instead of being swallowed by the tiny-value branch.
  if (n > 0 && n < 0.01) return '<$0.01';
  if (n < 0) {
    // Keep the minus before the dollar sign, and let a value that rounds to
    // zero (e.g. -0.004) render as plain $0.00 rather than "$-0.00".
    const abs = n.toFixed(2).replace('-', '');
    return abs === '0.00' ? '$0.00' : `-$${abs}`;
  }
  return `$${n.toFixed(2)}`;
}

/** "3h 12m" / "12m 04s" / "48s" */
export function fmtDuration(ms: number): string {
  // Guard non-finite/negative inputs — Math.floor of a negative would
  // produce malformed strings like "-5s".
  if (!Number.isFinite(ms) || ms < 0) return '0s';
  const totalSeconds = Math.round(ms / 1000);
  const h = Math.floor(totalSeconds / 3600);
  const m = Math.floor((totalSeconds % 3600) / 60);
  const s = totalSeconds % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${String(s).padStart(2, '0')}s`;
  return `${s}s`;
}

/** null when there is no previous window to compare against. */
export function pctDelta(current: number, previous: number): number | null {
  if (!Number.isFinite(current) || !Number.isFinite(previous) || previous <= 0) return null;
  return ((current - previous) / previous) * 100;
}

export function bookEquivalent(words: number): number {
  return words / WORDS_PER_BOOK;
}

/** Round a max value up to a readable axis ceiling (1/2/5 × 10^n). */
export function niceCeiling(max: number): number {
  if (!Number.isFinite(max) || max <= 0) return 1;
  const magnitude = 10 ** Math.floor(Math.log10(max));
  // IEEE-754 rounding can nudge an exact boundary (0.2/0.1) just past it, so
  // compare with a small tolerance instead of raw <=.
  const normalized = max / magnitude;
  let step: number;
  if (normalized <= 1 + 1e-9) {
    step = 1;
  } else if (normalized <= 2 + 1e-9) {
    step = 2;
  } else if (normalized <= 5 + 1e-9) {
    step = 5;
  } else {
    step = 10;
  }
  return step * magnitude;
}

/** Parse a local "YYYY-MM-DD" without the UTC shift `new Date(str)` would apply. */
export function parseLocalDay(day: string): Date {
  const [y, m, d] = day.split('-').map(Number);
  // Guard malformed/empty strings: `new Date(0, 0, 1)` silently becomes the
  // year 1900 rather than throwing, so the callers' try/catch fallback would
  // never fire. Reject with an Invalid Date so `toLocaleDateString` throws
  // and `fmtDay`/`fmtDayLong` fall back to the raw string.
  if (!Number.isInteger(y) || !Number.isInteger(m) || !Number.isInteger(d) || y < 1) {
    return new Date(NaN);
  }
  const date = new Date(y, m - 1, d);
  // Reject invalid calendar dates (e.g. month 13, day 32) that JS normalizes.
  return date.getFullYear() === y && date.getMonth() === m - 1 && date.getDate() === d
    ? date
    : new Date(NaN);
}

export function fmtDay(day: string): string {
  try {
    const date = parseLocalDay(day);
    if (Number.isNaN(date.getTime())) return day;
    return date.toLocaleDateString([], { month: 'short', day: 'numeric' });
  } catch {
    return day;
  }
}

export function fmtDayLong(day: string): string {
  try {
    const date = parseLocalDay(day);
    if (Number.isNaN(date.getTime())) return day;
    return date.toLocaleDateString([], {
      weekday: 'short',
      month: 'short',
      day: 'numeric',
    });
  } catch {
    return day;
  }
}

/** 0 → "12 AM", 13 → "1 PM" */
export function fmtHour(hour: number): string {
  const suffix = hour < 12 ? 'AM' : 'PM';
  const h = hour % 12 === 0 ? 12 : hour % 12;
  return `${h} ${suffix}`;
}

/**
 * Four accent-derived intensity steps. The accent is user-swappable, so every
 * chart colour is mixed from var(--accent) rather than hardcoded.
 */
export function accentStep(level: 0 | 1 | 2 | 3 | 4): string {
  if (level === 0) return 'var(--control-hover)';
  const pct = [0, 22, 45, 70, 100][level];
  return `color-mix(in srgb, var(--accent) ${pct}%, var(--paper-2))`;
}

/** Bucket a value into 0–4 against the series max, where 0 means "no activity". */
export function intensityLevel(value: number, max: number): 0 | 1 | 2 | 3 | 4 {
  if (value <= 0) return 0;
  if (max <= 0) return 1;
  const ratio = value / max;
  if (ratio > 0.66) return 4;
  if (ratio > 0.33) return 3;
  if (ratio > 0.12) return 2;
  return 1;
}

/** Hairline count on the Insights pace meter. */
export const PACE_TICKS = 44;

/**
 * Scale for the pace meter: a round ceiling that always clears the personal
 * best, plus the tick the best marker sits on (-1 when there is no best yet).
 *
 * Because the ceiling rounds up to the next 50, the best always lands in the
 * last handful of ticks. Landing on the second-to-last one leaves a single
 * orphan hairline trailing past the marker, which reads as a rendering
 * artifact rather than scale — so within a tick of the end, the marker takes
 * the end.
 */
export function paceScale(bestWpm: number, ticks = PACE_TICKS): { max: number; bestTick: number } {
  const max = Math.min(250, Math.max(200, Math.ceil(bestWpm / 50) * 50));
  if (bestWpm <= 0) return { max, bestTick: -1 };
  // Lower clamp: a best under half a tick still deserves a marker on tick 0
  // rather than rounding away to "no best".
  const tick = Math.max(0, Math.round((bestWpm / max) * ticks) - 1);
  return { max, bestTick: tick >= ticks - 2 ? ticks - 1 : tick };
}

/** Hairline width on the pace meter, in CSS px. Mirrors `.tick { width }`. */
export const PACE_TICK_W = 2;

/**
 * Left offset for pace-meter tick `i`, snapped to the device pixel grid.
 *
 * Letting flexbox spread the fractional slack put each hairline at a different
 * subpixel phase: one starting mid-pixel antialiases across three columns and
 * renders visibly wider and paler than a neighbour that happens to land on the
 * grid, so an evenly-spaced row reads as mismatched line weights. Snapping is
 * against *device* pixels, not CSS ones — under Windows display scaling (1.25×,
 * 1.5×) whole CSS pixels still straddle the real grid. Costs at most one device
 * pixel of spacing variance, which is invisible, and buys identical weight on
 * every tick.
 *
 * Falls back to percentages until the row has been measured, so the first frame
 * isn't every tick piled up at zero.
 */
export function paceTickOffset(
  i: number,
  rulerWidth: number,
  dpr: number = (typeof window === 'undefined' ? 1 : window.devicePixelRatio) || 1,
  ticks = PACE_TICKS
): string {
  const span = ticks - 1;
  if (rulerWidth <= PACE_TICK_W) return `${(i * 100) / span}%`;
  const raw = (i * (rulerWidth - PACE_TICK_W)) / span;
  return `${Math.round(raw * dpr) / dpr}px`;
}
