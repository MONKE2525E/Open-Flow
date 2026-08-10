/** Average words in a published novel — the unit behind "you've written N books". */
const WORDS_PER_BOOK = 80_000;

export function fmtNumber(n: number): string {
  return Math.round(n).toLocaleString();
}

/** Compact form for hero tiles: 1.2k, 3.4M. Exact below 1000. */
export function fmtCompact(n: number): { value: string; suffix: string } {
  const abs = Math.abs(n);
  if (abs >= 1e6) return { value: (n / 1e6).toFixed(1), suffix: 'M' };
  if (abs >= 1000) return { value: (n / 1000).toFixed(1), suffix: 'k' };
  return { value: String(Math.round(n)), suffix: '' };
}

export function fmtUsd(n: number | null): string {
  if (n === null) return '—';
  if (n === 0) return '$0.00';
  // Only small *positive* amounts render as "<$0.01" — a negative amount
  // must format normally instead of being swallowed by the tiny-value branch.
  if (n > 0 && n < 0.01) return '<$0.01';
  return `$${n.toFixed(2)}`;
}

/** "3h 12m" / "12m 04s" / "48s" */
export function fmtDuration(ms: number): string {
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
  if (previous <= 0) return null;
  return ((current - previous) / previous) * 100;
}

export function bookEquivalent(words: number): number {
  return words / WORDS_PER_BOOK;
}

/** Round a max value up to a readable axis ceiling (1/2/5 × 10^n). */
export function niceCeiling(max: number): number {
  if (max <= 0) return 1;
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
  return new Date(y, (m ?? 1) - 1, d ?? 1);
}

export function fmtDay(day: string): string {
  try {
    return parseLocalDay(day).toLocaleDateString([], { month: 'short', day: 'numeric' });
  } catch {
    return day;
  }
}

export function fmtDayLong(day: string): string {
  try {
    return parseLocalDay(day).toLocaleDateString([], {
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
