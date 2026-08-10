/*
 * Contract for the `get_insights` command. The backend owns every number here;
 * the Insights page does no aggregation of its own beyond formatting.
 *
 * Day bucketing is the backend's job and must use date(created_at, 'localtime')
 * — created_at is stored UTC-naive (see query_stats in data/db/transcriptions.rs).
 */

export type InsightsRange = 7 | 30 | 90 | 0; // 0 = all time

export interface InsightsTotals {
  /** Lifetime, from the lifetime_stats table — never shrinks with retention pruning. */
  total_words: number;
  total_transcriptions: number;
  total_speaking_ms: number;
  avg_words_per_transcription: number;
  avg_wpm: number;
  best_wpm: number;
  words_in_range: number;
  /** Same-length window immediately before the range, for the delta pill. */
  words_prev_range: number;
}

export interface InsightsStreak {
  current_days: number;
  longest_days: number;
  longest_started_on: string | null; // "YYYY-MM-DD"
  longest_ended_on: string | null;
  longest_words: number;
  active_days: number;
}

export interface InsightsDay {
  day: string; // "YYYY-MM-DD", local
  words: number;
  transcriptions: number;
  speaking_ms: number;
}

export interface InsightsProviderUsage {
  model: string;
  provider: 'groq' | 'openai' | 'google' | 'assemblyai' | 'local';
  task: 'transcription' | 'cleanup';
  calls: number;
  audio_ms: number;
  input_chars: number;
  output_chars: number;
}

export interface InsightsCleanup {
  raw_words: number;
  clean_words: number;
  edits_applied: number;
  dictionary_fixes: number;
  auto_learned_terms: number;
}

export interface InsightsWords {
  top: Array<{ word: string; count: number }>;
  unique_words: number;
  longest_word: string | null;
  avg_word_length: number;
}

export interface InsightsPayload {
  range_days: number;
  generated_at: string; // "YYYY-MM-DD HH:MM:SS", UTC-naive like created_at
  totals: InsightsTotals;
  streak: InsightsStreak;
  /** One row per calendar day in range, zero days included, ascending. */
  daily: InsightsDay[];
  /** Length 24 — words per hour-of-day, local time. */
  hourly: number[];
  providers: InsightsProviderUsage[];
  cleanup: InsightsCleanup;
  words: InsightsWords;
}

export const EMPTY_INSIGHTS: InsightsPayload = {
  range_days: 30,
  generated_at: '',
  totals: {
    total_words: 0,
    total_transcriptions: 0,
    total_speaking_ms: 0,
    avg_words_per_transcription: 0,
    avg_wpm: 0,
    best_wpm: 0,
    words_in_range: 0,
    words_prev_range: 0,
  },
  streak: {
    current_days: 0,
    longest_days: 0,
    longest_started_on: null,
    longest_ended_on: null,
    longest_words: 0,
    active_days: 0,
  },
  daily: [],
  hourly: new Array(24).fill(0),
  providers: [],
  cleanup: {
    raw_words: 0,
    clean_words: 0,
    edits_applied: 0,
    dictionary_fixes: 0,
    auto_learned_terms: 0,
  },
  words: { top: [], unique_words: 0, longest_word: null, avg_word_length: 0 },
};

export const RANGE_OPTIONS: Array<{ value: InsightsRange; label: string }> = [
  { value: 7, label: 'Last 7 days' },
  { value: 30, label: 'Last 30 days' },
  { value: 90, label: 'Last 90 days' },
  { value: 0, label: 'All time' },
];
