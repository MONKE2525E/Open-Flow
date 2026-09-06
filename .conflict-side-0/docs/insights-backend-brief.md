# Insights backend brief

The Insights page (`src/lib/views/Insights.svelte`) is built and shipping against a
mock. It calls exactly one Tauri command that does not exist yet. Your job is to
implement that command so the page renders real data. **Do not change the frontend
contract** — the page, its formatting, and its charts are already tuned to it.

---

## The command

```rust
#[tauri::command]
fn get_insights(days: i64, /* state */) -> Result<Insights, String>
```

- Invoked from the frontend as `invoke('get_insights', { days })`.
- `days` is one of `7 | 30 | 90 | 0`, where **`0` means all time**.
- Register it in `src-tauri/src/commands/mod.rs` and in the `invoke_handler` list in
  `src-tauri/src/main.rs`, alongside `get_stats` / `get_recent`.
- Serde field names must match the TypeScript below **exactly** (snake_case as written).

## The payload contract

Source of truth: `src/lib/views/insights/types.ts`. Mirror it field for field.

```ts
export interface InsightsPayload {
  range_days: number;                 // echo the requested window (7 | 30 | 90 | 0)
  generated_at: string;               // "YYYY-MM-DD HH:MM:SS" UTC-naive, like created_at

  totals: {
    total_words: number;              // LIFETIME, from lifetime_stats — not the range
    total_transcriptions: number;     // in range
    total_speaking_ms: number;        // in range
    avg_words_per_transcription: number;
    avg_wpm: number;                  // spoken_words / duration
    best_wpm: number;
    words_in_range: number;
    words_prev_range: number;         // same-length window immediately before the range
  };

  streak: {
    current_days: number;
    longest_days: number;
    longest_started_on: string | null;   // "YYYY-MM-DD"
    longest_ended_on: string | null;
    longest_words: number;               // words dictated during the longest streak
    active_days: number;                 // days with >=1 dictation, all time
  };

  daily: Array<{ day: string; words: number; transcriptions: number; speaking_ms: number }>;
  hourly: number[];                   // exactly 24 entries, words per hour-of-day, local

  providers: Array<{
    model: string;                    // e.g. "whisper-large-v3-turbo"
    provider: 'groq' | 'openai' | 'google' | 'local';
    task: 'transcription' | 'cleanup';
    calls: number;
    audio_ms: number;                 // transcription models; 0 for cleanup
    input_chars: number;              // cleanup models; 0 for transcription
    output_chars: number;
  }>;

  cleanup: {
    raw_words: number;                // pre-cleanup
    clean_words: number;              // post-cleanup
    edits_applied: number;            // dictionary substitutions + snippet expansions
    dictionary_fixes: number;
    auto_learned_terms: number;
  };

  words: {
    top: Array<{ word: string; count: number }>;      // stopword-filtered, top 12
    unique_words: number;
    longest_word: string | null;
    avg_word_length: number;
  };
}
```

## Rules that are easy to get wrong

1. **Local-day bucketing.** `created_at` is stored UTC-naive. Every day and hour bucket
   must go through `date(created_at, 'localtime')` / `strftime('%H', created_at,
   'localtime')`. `query_stats()` in `src-tauri/src/data/db/transcriptions.rs` already
   does this — follow its style for the whole module.
2. **Zero-fill `daily` server-side.** The frontend chart assumes one row per calendar
   day in the range, ascending, with `words: 0` for idle days. Do not send a sparse
   series. For `days == 0`, span the first recorded transcription through today.
3. **`hourly` is always length 24**, index 0 = midnight local, zeros included.
4. **`totals.total_words` is lifetime**, read from the `lifetime_stats` table so it
   never shrinks when retention pruning runs. Everything else is range-scoped from
   `transcriptions` and legitimately does shrink — that asymmetry is intentional.
5. **`words_prev_range`** is the immediately-preceding window of the same length. For
   `days == 0` return `0`; the frontend then hides the delta pill.
6. **Stopword-filter `words.top`.** Strip the obvious English filler ("the", "a",
   "and", "to", "of", "it", "is", …) and anything under 3 characters, lowercase and
   punctuation-strip before counting. The point is showing the user their distinctive
   vocabulary, not "the ×4,102". Count from `clean_text`.
7. **Unknown/empty is not an error.** A brand-new install with no transcriptions must
   return a fully-populated zero payload, not an `Err`. The page has a dedicated empty
   state keyed off `totals.total_transcriptions == 0`.
8. **Privacy.** This runs entirely locally. Do not log dictated text, top words, or any
   `clean_text` content — counts, ids, and model names only (see the logging rules in
   `AGENTS.md`).

## Schema — what already exists

`src-tauri/src/data/db/schema.rs`:

```sql
CREATE TABLE transcriptions (
  id INTEGER PRIMARY KEY AUTOINCREMENT, raw_text TEXT, clean_text TEXT,
  words INTEGER, spoken_words INTEGER, duration_ms INTEGER,
  api_used TEXT, created_at DATETIME DEFAULT (datetime('now'))
);
CREATE INDEX idx_transcriptions_created_at ON transcriptions(created_at);
CREATE TABLE lifetime_stats (id PK CHECK(id=1), total_words INTEGER);
```

So `spoken_words`, `duration_ms`, `api_used`, `raw_text` and `clean_text` are already
recorded and unused by the UI — most of the payload comes straight out of this table
with no migration.

**The one gap is cost.** `providers[]` needs per-call `audio_ms`, `input_chars`, and
`output_chars` split by model *and* task. `api_used` today records a single string and
doesn't separate the transcription model from the cleanup model, nor carry token/char
counts. Pick one:

- **Preferred:** add an `api_calls` table (`id, transcription_id, model, provider,
  task, audio_ms, input_chars, output_chars, created_at`) written from
  `src-tauri/src/pipeline/finalize.rs`, and aggregate it here. Historical rows simply
  won't have cost data, which is fine — the card degrades to "No priced API usage in
  this range."
- **Cheaper:** derive approximate values from `duration_ms` (audio) and
  `length(raw_text)` / `length(clean_text)` (chars), attributing them to the models
  named in the current settings. Less accurate, no migration.

Either way the *pricing* is the frontend's problem — `src/lib/views/insights/pricing.ts`
holds the static rate table and does the arithmetic. Send raw usage, not dollars.

## `cleanup` block sourcing

- `raw_words` / `clean_words`: word counts of `raw_text` and `clean_text` in range.
  `spoken_words` may already serve for `raw_words` — check before recomputing.
- `dictionary_fixes`: `SUM(correction_count)` from the `dictionary` table.
- `auto_learned_terms`: `COUNT(*) WHERE auto_learned = 1`.
- `edits_applied`: dictionary substitutions + snippet expansions. If there's no counter
  today, add one rather than guessing — `SUM(use_count)` on `snippets` plus
  `dictionary_fixes` is an acceptable first cut.

## Where the code goes

- Queries: a new `src-tauri/src/data/db/insights.rs`, exported from `data/db/mod.rs`.
- Command handler: `src-tauri/src/commands/mod.rs` (or a small `commands/insights.rs`).
- Match the existing `anyhow::Result` + `.map_err(|e| e.to_string())` convention.

## Tests

Add Rust unit tests against an in-memory DB covering, at minimum:

- Zero-fill: a range with a gap day yields a contiguous ascending `daily` with a zero.
- Streak: a 3-day run followed by a gap gives `longest_days == 3` with the right
  start/end dates, and `current_days == 0`.
- Local-day boundary: a transcription written just before local midnight buckets to the
  local day, not the UTC one.
- Empty DB returns a zero payload, not an error.
- `words.top` excludes stopwords and is sorted descending.

## Definition of done

`npm run test:rust` and `npm test` pass, and the Insights page shows real numbers in
`npm run tauri dev`. Once real data flows, the dev mock in `devInvoke`
(`src/lib/tauri.ts`, `case 'get_insights'` → `devInsights()`) **stays** — it's what
keeps browser dev and the Playwright smoke tests working without a backend.
