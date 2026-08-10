//! Insights aggregation for the Insights page.
//!
//! Mirrors `src/lib/views/insights/types.ts` field for field — serde field
//! names are the frontend contract and must not drift. Day/hour bucketing
//! always goes through `date(created_at, 'localtime')` / `strftime('%H',
//! created_at, 'localtime')` because `created_at` is stored UTC-naive (see
//! `query_stats` in transcriptions.rs).
//!
//! Privacy: this module only ever emits counts, model ids, and normalized
//! word stats. It never logs dictated text, clean_text, or top words.

use std::collections::HashMap;

use anyhow::Result;
use chrono::{NaiveDate, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Insights {
    pub range_days: i64,
    pub generated_at: String,
    pub totals: InsightsTotals,
    pub streak: InsightsStreak,
    pub daily: Vec<InsightsDay>,
    pub hourly: Vec<i64>,
    pub providers: Vec<InsightsProviderUsage>,
    pub cleanup: InsightsCleanup,
    pub words: InsightsWords,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InsightsTotals {
    pub total_words: i64,
    pub total_transcriptions: i64,
    pub total_speaking_ms: i64,
    pub avg_words_per_transcription: i64,
    pub avg_wpm: f64,
    pub best_wpm: i64,
    pub words_in_range: i64,
    pub words_prev_range: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InsightsStreak {
    pub current_days: i64,
    pub longest_days: i64,
    pub longest_started_on: Option<String>,
    pub longest_ended_on: Option<String>,
    pub longest_words: i64,
    pub active_days: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InsightsDay {
    pub day: String,
    pub words: i64,
    pub transcriptions: i64,
    pub speaking_ms: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InsightsProviderUsage {
    pub model: String,
    pub provider: String,
    pub task: String,
    pub calls: i64,
    pub audio_ms: i64,
    pub input_chars: i64,
    pub output_chars: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InsightsCleanup {
    pub raw_words: i64,
    pub clean_words: i64,
    pub edits_applied: i64,
    pub dictionary_fixes: i64,
    pub auto_learned_terms: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InsightsWords {
    pub top: Vec<InsightsWordCount>,
    pub unique_words: i64,
    pub longest_word: Option<String>,
    pub avg_word_length: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InsightsWordCount {
    pub word: String,
    pub count: i64,
}

/// One provider call recorded at pipeline finalize time, aggregated into
/// `Insights.providers`. `audio_ms` is set for transcription tasks only;
/// `input_chars`/`output_chars` for cleanup tasks only.
#[derive(Debug, Clone)]
pub struct ApiCall {
    pub transcription_id: i64,
    pub model: String,
    pub provider: String,
    pub task: String,
    pub audio_ms: i64,
    pub input_chars: i64,
    pub output_chars: i64,
    pub created_at: String,
}

const TOP_WORDS_LIMIT: usize = 12;
const MIN_WORD_CHARS: usize = 3;

// Filler/function words users don't need to see in their distinctive
// vocabulary. Lowercase; `top`/`unique_words`/`avg_word_length` filter on it.
const STOPWORDS: &[&str] = &[
    "a", "about", "after", "all", "also", "am", "an", "and", "any", "are", "as", "at", "be",
    "because", "been", "before", "being", "but", "by", "could", "did", "do", "does", "for",
    "from", "had", "has", "have", "he", "her", "hers", "him", "his", "how", "i", "if", "in",
    "into", "is", "it", "its", "me", "more", "most", "my", "no", "not", "of", "off", "on",
    "or", "our", "ours", "out", "over", "own", "said", "she", "so", "some", "than", "that",
    "the", "their", "them", "then", "there", "these", "they", "this", "those", "through",
    "to", "too", "under", "up", "us", "was", "we", "were", "what", "when", "where", "which",
    "while", "who", "whom", "why", "will", "with", "would", "you", "your", "yours",
];

fn is_stopword(word: &str) -> bool {
    STOPWORDS.binary_search(&word).is_ok()
}

/// Aggregated insights for `days` (`0` = all time). A brand-new install with
/// no transcriptions returns a fully-populated zero payload, never an error.
pub fn query_insights(db: &Db, days: i64) -> Result<Insights> {
    let conn = lock_conn(db)?;
    let (range_start, range_end) = range_bounds(&conn, days)?;

    let totals = query_totals(&conn, &range_start, &range_end, days)?;
    let daily = query_daily(&conn, &range_start, &range_end)?;
    let hourly = query_hourly(&conn, &range_start, &range_end)?;
    let providers = query_providers(&conn, &range_start, &range_end)?;
    let cleanup = query_cleanup(&conn, &range_start, &range_end)?;
    let words = query_words(&conn, &range_start, &range_end)?;
    let streak = compute_streak(&daily);

    Ok(Insights {
        range_days: days,
        generated_at: Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        totals,
        streak,
        daily,
        hourly,
        providers,
        cleanup,
        words,
    })
}

/// Inserts the per-call API usage rows for one transcription.
pub fn insert_api_calls(db: &Db, calls: &[ApiCall]) -> Result<()> {
    if calls.is_empty() {
        return Ok(());
    }
    let mut conn = lock_conn(db)?;
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO api_calls \
             (transcription_id, model, provider, task, audio_ms, input_chars, output_chars, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for call in calls {
            stmt.execute(params![
                call.transcription_id,
                call.model,
                call.provider,
                call.task,
                call.audio_ms,
                call.input_chars,
                call.output_chars,
                call.created_at,
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Builds `ApiCall` rows for one finalized transcription by parsing the
/// `api_used` string the pipeline assembled (formats: `provider/model`,
/// `provider/model/transcription`, `primary=..;secondary=..`, with an
/// optional `;cleanup=..` suffix). Unparseable strings yield no rows, never
/// an error — cost accounting is best-effort and must not fail a dictation.
///
/// `audio_ms` is attributed to every transcription model (each one processed
/// the same clip); cleanup rows carry `input_chars`/`output_chars` derived
/// from the raw and cleaned text.
pub(crate) fn build_api_calls(
    transcription_id: i64,
    created_at: &str,
    api_used: &str,
    duration_ms: i64,
    raw: &str,
    clean: &str,
) -> Vec<ApiCall> {
    let parts = parse_api_usage(api_used);
    let mut calls = Vec::with_capacity(
        parts.transcription_models.len() + parts.cleanup_models.len(),
    );
    for (provider, model) in parts.transcription_models {
        calls.push(ApiCall {
            transcription_id,
            model,
            provider,
            task: "transcription".to_string(),
            audio_ms: duration_ms,
            input_chars: 0,
            output_chars: 0,
            created_at: created_at.to_string(),
        });
    }
    if !parts.cleanup_models.is_empty() {
        let input_chars = raw.chars().count() as i64;
        let output_chars = clean.chars().count() as i64;
        for (provider, model) in parts.cleanup_models {
            calls.push(ApiCall {
                transcription_id,
                model,
                provider,
                task: "cleanup".to_string(),
                audio_ms: 0,
                input_chars,
                output_chars,
                created_at: created_at.to_string(),
            });
        }
    }
    calls
}

/// Models parsed out of an `api_used` string, split by task. Each entry is
/// `(provider, model)`.
struct ApiUsageParts {
    transcription_models: Vec<(String, String)>,
    cleanup_models: Vec<(String, String)>,
}

/// Splits an `api_used` string into transcription and cleanup model pairs.
fn parse_api_usage(api_used: &str) -> ApiUsageParts {
    let (transcription_part, cleanup_part) = match api_used.split_once(";cleanup=") {
        Some((before, after)) => (before, Some(after)),
        None => (api_used, None),
    };

    let mut transcription = Vec::new();
    if let Some(primary) = transcription_part.strip_prefix("primary=") {
        for segment in primary.split(';') {
            let segment = segment.strip_prefix("secondary=").unwrap_or(segment);
            if let Some((provider, model)) = segment.split_once('/') {
                // Uniform with the single-provider fallback below: drop a
                // trailing /transcription so the model key is identical
                // regardless of which format produced the row.
                let model = model.strip_suffix("/transcription").unwrap_or(model);
                if !provider.is_empty() && !model.is_empty() {
                    transcription.push((provider.to_string(), model.to_string()));
                }
            }
        }
    } else if let Some((provider, rest)) = transcription_part.split_once('/') {
        let model = rest.strip_suffix("/transcription").unwrap_or(rest);
        if !provider.is_empty() && !model.is_empty() {
            transcription.push((provider.to_string(), model.to_string()));
        }
    }

    let mut cleanup = Vec::new();
    if let Some(segment) = cleanup_part {
        if let Some((provider, model)) = segment.split_once('/') {
            if !provider.is_empty() && !model.is_empty() {
                cleanup.push((provider.to_string(), model.to_string()));
            }
        }
    }

    ApiUsageParts {
        transcription_models: transcription,
        cleanup_models: cleanup,
    }
}

/// Local calendar-day bounds of the requested range, as `"YYYY-MM-DD"`.
/// `days > 0` spans the last `days` calendar days ending today; `days == 0`
/// spans the first recorded transcription through today.
fn range_bounds(conn: &Connection, days: i64) -> Result<(String, String)> {
    let today: String = conn.query_row("SELECT date('now', 'localtime')", [], |r| r.get(0))?;
    let start = if days > 0 {
        let n = (days - 1).max(0);
        conn.query_row(
            "SELECT date('now', 'localtime', ?1)",
            params![format!("-{n} days")],
            |r| r.get(0),
        )?
    } else {
        conn.query_row(
            "SELECT COALESCE(
               date((SELECT MIN(created_at) FROM transcriptions), 'localtime'),
               date('now', 'localtime')
             )",
            [],
            |r| r.get(0),
        )?
    };
    Ok((start, today))
}

fn query_totals(
    conn: &Connection,
    range_start: &str,
    range_end: &str,
    days: i64,
) -> Result<InsightsTotals> {
    let (total_transcriptions, words_in_range, total_speaking_ms): (i64, i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(words), 0), COALESCE(SUM(duration_ms), 0)
             FROM transcriptions
             WHERE date(created_at, 'localtime') BETWEEN ?1 AND ?2",
            params![range_start, range_end],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;

    // Identical definition to query_stats (average of each clip's own wpm)
    // so the Insights "All time" number matches the Home page exactly;
    // scoped to the range here. Not total_words/total_duration — that
    // aggregates differently and makes the two pages disagree.
    let avg_wpm: f64 = conn.query_row(
        "SELECT COALESCE(AVG(CAST(spoken_words AS REAL) * 60000.0 / duration_ms), 0.0)
         FROM transcriptions
         WHERE date(created_at, 'localtime') BETWEEN ?1 AND ?2
           AND duration_ms > 0 AND spoken_words > 0",
        params![range_start, range_end],
        |r| r.get(0),
    )?;

    let best_wpm: i64 = conn.query_row(
        "SELECT COALESCE(CAST(MAX(CAST(spoken_words AS REAL) * 60000.0 / duration_ms) AS INTEGER), 0)
         FROM transcriptions
         WHERE date(created_at, 'localtime') BETWEEN ?1 AND ?2
           AND duration_ms > 0 AND spoken_words > 0",
        params![range_start, range_end],
        |r| r.get(0),
    )?;

    let total_words: i64 = conn.query_row(
        "SELECT COALESCE((SELECT total_words FROM lifetime_stats WHERE id = 1), 0)",
        [],
        |r| r.get(0),
    )?;

    let words_prev_range: i64 = if days > 0 {
        let prev_start = format!("-{} days", (2 * days - 1).max(1));
        let prev_end = format!("-{} days", days);
        conn.query_row(
            "SELECT COALESCE(SUM(words), 0)
             FROM transcriptions
             WHERE date(created_at, 'localtime') BETWEEN date('now', 'localtime', ?1)
                                                   AND date('now', 'localtime', ?2)",
            params![prev_start, prev_end],
            |r| r.get(0),
        )?
    } else {
        0
    };

    let avg_words_per_transcription = if total_transcriptions > 0 {
        (words_in_range as f64 / total_transcriptions as f64).round() as i64
    } else {
        0
    };

    Ok(InsightsTotals {
        total_words,
        total_transcriptions,
        total_speaking_ms,
        avg_words_per_transcription,
        avg_wpm,
        best_wpm,
        words_in_range,
        words_prev_range,
    })
}

/// One row per calendar day in the range, ascending, zero-filled for idle days.
fn query_daily(
    conn: &Connection,
    range_start: &str,
    range_end: &str,
) -> Result<Vec<InsightsDay>> {
    let mut per_day: HashMap<String, (i64, i64, i64)> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT date(created_at, 'localtime'), SUM(words), COUNT(*), SUM(duration_ms)
             FROM transcriptions
             WHERE date(created_at, 'localtime') BETWEEN ?1 AND ?2
             GROUP BY 1",
        )?;
        let rows = stmt.query_map(params![range_start, range_end], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?),
            ))
        })?;
        for row in rows {
            let (day, sums) = row?;
            per_day.insert(day, sums);
        }
    }

    let start = NaiveDate::parse_from_str(range_start, "%Y-%m-%d")?;
    let end = NaiveDate::parse_from_str(range_end, "%Y-%m-%d")?;
    if start > end {
        // A future-dated transcription (clock drift) can make the range
        // bounds inverted; never spin the zero-fill loop to NaiveDate::MAX.
        return Ok(Vec::new());
    }
    let span_days = (end - start).num_days();
    let mut daily = Vec::with_capacity(span_days as usize + 1);
    let mut current = start;
    loop {
        let day = current.format("%Y-%m-%d").to_string();
        let (words, transcriptions, speaking_ms) =
            per_day.get(&day).copied().unwrap_or((0, 0, 0));
        daily.push(InsightsDay {
            day,
            words,
            transcriptions,
            speaking_ms,
        });
        if current == end {
            break;
        }
        current = current.succ_opt().ok_or_else(|| {
            anyhow::anyhow!("insights: date overflow while zero-filling daily series")
        })?;
    }
    debug_assert_eq!(daily.len() as i64, span_days + 1);
    Ok(daily)
}

/// Words per hour-of-day, local time; exactly 24 entries, zeros included.
fn query_hourly(conn: &Connection, range_start: &str, range_end: &str) -> Result<Vec<i64>> {
    let mut hourly = vec![0i64; 24];
    let mut stmt = conn.prepare(
        "SELECT CAST(strftime('%H', created_at, 'localtime') AS INTEGER), SUM(words)
         FROM transcriptions
         WHERE date(created_at, 'localtime') BETWEEN ?1 AND ?2
         GROUP BY 1",
    )?;
    let rows = stmt.query_map(params![range_start, range_end], |r| {
        Ok((r.get::<_, usize>(0)?, r.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (hour, words) = row?;
        if let Some(slot) = hourly.get_mut(hour) {
            *slot = words;
        }
    }
    Ok(hourly)
}

/// Per-model/per-task usage from the `api_calls` table, in range. Rows
/// predating the table simply contribute nothing.
fn query_providers(
    conn: &Connection,
    range_start: &str,
    range_end: &str,
) -> Result<Vec<InsightsProviderUsage>> {
    let mut stmt = conn.prepare(
        "SELECT model, provider, task, COUNT(*),
                COALESCE(SUM(audio_ms), 0),
                COALESCE(SUM(input_chars), 0),
                COALESCE(SUM(output_chars), 0)
         FROM api_calls
         WHERE date(created_at, 'localtime') BETWEEN ?1 AND ?2
         GROUP BY model, provider, task
         ORDER BY COUNT(*) DESC, model ASC",
    )?;
    let rows = stmt.query_map(params![range_start, range_end], |r| {
        Ok(InsightsProviderUsage {
            model: r.get(0)?,
            provider: r.get(1)?,
            task: r.get(2)?,
            calls: r.get(3)?,
            audio_ms: r.get(4)?,
            input_chars: r.get(5)?,
            output_chars: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn query_cleanup(
    conn: &Connection,
    range_start: &str,
    range_end: &str,
) -> Result<InsightsCleanup> {
    // raw/clean word counts are range-scoped from the transcriptions rows we
    // already load for the vocabulary stats.
    let (raw_words, clean_words) = {
        let mut stmt = conn.prepare(
            "SELECT clean_text, COALESCE(spoken_words, words)
             FROM transcriptions
             WHERE date(created_at, 'localtime') BETWEEN ?1 AND ?2",
        )?;
        let rows = stmt.query_map(params![range_start, range_end], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        let mut raw = 0i64;
        let mut clean = 0i64;
        for row in rows {
            let (clean_text, spoken) = row?;
            raw += spoken;
            clean += clean_text.split_whitespace().count() as i64;
        }
        (raw, clean)
    };

    let dictionary_fixes: i64 = conn.query_row(
        "SELECT COALESCE((SELECT dictionary_fixes FROM lifetime_stats WHERE id = 1), 0)",
        [],
        |r| r.get(0),
    )?;
    let auto_learned_terms: i64 = conn.query_row(
        "SELECT COUNT(*) FROM dictionary WHERE auto_learned = 1",
        [],
        |r| r.get(0),
    )?;
    let snippet_expansions: i64 = conn.query_row(
        "SELECT COALESCE(SUM(use_count), 0) FROM snippets",
        [],
        |r| r.get(0),
    )?;

    Ok(InsightsCleanup {
        raw_words,
        clean_words,
        edits_applied: dictionary_fixes + snippet_expansions,
        dictionary_fixes,
        auto_learned_terms,
    })
}

fn query_words(
    conn: &Connection,
    range_start: &str,
    range_end: &str,
) -> Result<InsightsWords> {
    let mut stmt = conn.prepare(
        "SELECT clean_text FROM transcriptions
         WHERE date(created_at, 'localtime') BETWEEN ?1 AND ?2",
    )?;
    let rows = stmt.query_map(params![range_start, range_end], |r| r.get::<_, String>(0))?;

    let mut counts: HashMap<String, i64> = HashMap::new();
    let mut longest: Option<String> = None;
    let mut length_sum = 0u64;
    let mut length_count = 0u64;

    for row in rows {
        let clean_text = row?;
        for token in clean_text.split_whitespace() {
            let normalized = normalize_word(token);
            if normalized.is_empty() {
                continue;
            }
            // `.len()` is byte length; `chars().count()` is the character
            // count a human word length should measure (non-ASCII words like
            // accented or non-Latin text would otherwise be over-counted).
            let char_len = normalized.chars().count();
            // Stopwords are excluded from the vocabulary insights — check
            // before tracking `longest` so a long filler word like
            // "because"/"themselves" never shows up as the longest word.
            if char_len < MIN_WORD_CHARS || is_stopword(&normalized) {
                continue;
            }
            if char_len > longest.as_ref().map_or(0, |w| w.chars().count()) {
                longest = Some(normalized.clone());
            }
            *counts.entry(normalized.clone()).or_insert(0) += 1;
            length_sum += char_len as u64;
            length_count += 1;
        }
    }

    let mut top: Vec<InsightsWordCount> = counts
        .iter()
        .map(|(word, count)| InsightsWordCount {
            word: word.clone(),
            count: *count,
        })
        .collect();
    top.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.word.cmp(&b.word)));
    top.truncate(TOP_WORDS_LIMIT);

    Ok(InsightsWords {
        top,
        unique_words: counts.len() as i64,
        longest_word: longest,
        avg_word_length: if length_count > 0 {
            length_sum as f64 / length_count as f64
        } else {
            0.0
        },
    })
}

/// Lowercases and strips punctuation, keeping only alphanumeric characters.
fn normalize_word(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Computes current/longest streak over the (already zero-filled, ascending)
/// daily series. A day with `words > 0` counts as active.
fn compute_streak(daily: &[InsightsDay]) -> InsightsStreak {
    let active_days = daily.iter().filter(|d| d.words > 0).count() as i64;

    let mut current_days = 0i64;
    // The series is zero-filled and ends on today. A quiet today (the day
    // isn't over yet) must not zero a streak that's still alive through
    // yesterday — GitHub-style, the streak holds until the day ends. The
    // grace applies only to the trailing day (today) itself: if today is
    // empty it's skipped, and counting continues strictly from yesterday.
    let mut iter = daily.iter().rev();
    if let Some(today) = iter.next() {
        if today.words > 0 {
            current_days += 1;
        }
    }
    for day in iter {
        if day.words > 0 {
            current_days += 1;
        } else {
            break;
        }
    }

    let mut longest_days = 0i64;
    let mut longest_words = 0i64;
    let mut longest_started_on = None;
    let mut longest_ended_on = None;
    let mut run = 0i64;
    let mut run_words = 0i64;
    let mut run_start: Option<&str> = None;
    for day in daily {
        if day.words > 0 {
            if run == 0 {
                run_start = Some(day.day.as_str());
            }
            run += 1;
            run_words += day.words;
        } else {
            run = 0;
            run_words = 0;
            run_start = None;
        }
        if run > longest_days {
            longest_days = run;
            longest_words = run_words;
            longest_started_on = run_start.map(str::to_string);
            longest_ended_on = Some(day.day.clone());
        }
    }

    InsightsStreak {
        current_days,
        longest_days,
        longest_started_on,
        longest_ended_on,
        longest_words,
        active_days,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Local, TimeZone};

    fn test_db() -> Db {
        open(":memory:").expect("test db")
    }

    /// UTC-naive timestamp whose local representation lands on `days_ago`
    /// local calendar days at the given local wall-clock time.
    fn utc_for_local_day(days_ago: i64, hour: u32, minute: u32) -> String {
        let local_date = Local::now().date_naive() - Duration::days(days_ago);
        let wall = local_date
            .and_hms_opt(hour, minute, 0)
            .expect("valid wall time");
        let utc = Local
            .from_local_datetime(&wall)
            .single()
            .expect("local to utc")
            .naive_utc();
        utc.format("%Y-%m-%d %H:%M:%S").to_string()
    }

    fn local_day_string(days_ago: i64) -> String {
        (Local::now().date_naive() - Duration::days(days_ago))
            .format("%Y-%m-%d")
            .to_string()
    }

    fn insert_on_day(db: &Db, days_ago: i64, text: &str, words: i64, duration_ms: i64) -> i64 {
        let entry = insert_transcription_returning(db, text, text, words, duration_ms, "groq/whisper-large-v3-turbo")
            .expect("insert transcription");
        {
            let conn = lock_conn(db).expect("lock");
            conn.execute(
                "UPDATE transcriptions SET created_at = ?1 WHERE id = ?2",
                params![utc_for_local_day(days_ago, 10, 0), entry.id],
            )
            .expect("backdate transcription");
        }
        entry.id
    }

    fn days_in(insights: &Insights) -> Vec<String> {
        insights.daily.iter().map(|d| d.day.clone()).collect()
    }

    #[test]
    fn empty_db_returns_zero_payload_not_an_error() {
        let db = test_db();
        let insights = query_insights(&db, 30).expect("insights on empty db");

        assert_eq!(insights.range_days, 30);
        assert_eq!(insights.totals.total_words, 0);
        assert_eq!(insights.totals.total_transcriptions, 0);
        assert_eq!(insights.totals.total_speaking_ms, 0);
        assert_eq!(insights.totals.avg_words_per_transcription, 0);
        assert_eq!(insights.totals.avg_wpm, 0.0);
        assert_eq!(insights.totals.best_wpm, 0);
        assert_eq!(insights.totals.words_in_range, 0);
        assert_eq!(insights.totals.words_prev_range, 0);
        assert_eq!(insights.streak.current_days, 0);
        assert_eq!(insights.streak.longest_days, 0);
        assert_eq!(insights.streak.longest_started_on, None);
        assert_eq!(insights.streak.longest_ended_on, None);
        assert_eq!(insights.streak.longest_words, 0);
        assert_eq!(insights.streak.active_days, 0);
        assert_eq!(insights.daily.len(), 30);
        assert!(insights.daily.iter().all(|d| d.words == 0));
        assert_eq!(insights.hourly.len(), 24);
        assert!(insights.hourly.iter().all(|h| *h == 0));
        assert!(insights.providers.is_empty());
        assert_eq!(insights.cleanup.raw_words, 0);
        assert_eq!(insights.cleanup.clean_words, 0);
        assert_eq!(insights.cleanup.edits_applied, 0);
        assert_eq!(insights.cleanup.dictionary_fixes, 0);
        assert_eq!(insights.cleanup.auto_learned_terms, 0);
        assert!(insights.words.top.is_empty());
        assert_eq!(insights.words.unique_words, 0);
        assert_eq!(insights.words.longest_word, None);
        assert_eq!(insights.words.avg_word_length, 0.0);
    }

    #[test]
    fn all_time_range_spans_first_transcription_through_today() {
        let db = test_db();
        insert_on_day(&db, 5, "hello world", 2, 1000);
        insert_on_day(&db, 2, "more dictation", 2, 1000);

        let insights = query_insights(&db, 0).expect("insights all time");
        let expected = (0..=5).rev().map(local_day_string).collect::<Vec<_>>();
        assert_eq!(days_in(&insights), expected);
        assert_eq!(insights.range_days, 0);
        assert_eq!(insights.totals.words_in_range, 4);
        assert_eq!(insights.totals.words_prev_range, 0);
    }

    #[test]
    fn daily_zero_fills_a_gap_day_and_stays_ascending() {
        let db = test_db();
        insert_on_day(&db, 3, "day one", 2, 1000);
        insert_on_day(&db, 1, "day three", 3, 1500);

        let insights = query_insights(&db, 7).expect("insights");
        let expected = (0..7).rev().map(local_day_string).collect::<Vec<_>>();
        assert_eq!(days_in(&insights), expected);

        let gap = insights
            .daily
            .iter()
            .find(|d| d.day == local_day_string(2))
            .expect("gap day present");
        assert_eq!(gap.words, 0);
        assert_eq!(gap.transcriptions, 0);
        assert_eq!(gap.speaking_ms, 0);

        let active: Vec<_> = insights.daily.iter().filter(|d| d.words > 0).collect();
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].day, local_day_string(3));
        assert_eq!(active[0].words, 2);
        assert_eq!(active[1].day, local_day_string(1));
        assert_eq!(active[1].words, 3);
        assert_eq!(active[1].speaking_ms, 1500);
    }

    #[test]
    fn streak_counts_a_three_day_run_followed_by_a_gap() {
        let db = test_db();
        insert_on_day(&db, 6, "run one", 2, 1000);
        insert_on_day(&db, 5, "run two", 2, 1000);
        insert_on_day(&db, 4, "run three", 4, 2000);

        let insights = query_insights(&db, 7).expect("insights");

        assert_eq!(insights.streak.longest_days, 3);
        assert_eq!(
            insights.streak.longest_started_on.as_deref(),
            Some(local_day_string(6).as_str())
        );
        assert_eq!(
            insights.streak.longest_ended_on.as_deref(),
            Some(local_day_string(4).as_str())
        );
        assert_eq!(insights.streak.longest_words, 8);
        assert_eq!(insights.streak.current_days, 0, "gap at the range end breaks the current streak");
        assert_eq!(insights.streak.active_days, 3);
    }

    #[test]
    fn streak_survives_a_quiet_today() {
        let db = test_db();
        // Yesterday and the day before were active; today is silent (the day
        // isn't over yet) — the streak must still read as alive.
        insert_on_day(&db, 1, "yesterday", 2, 1000);
        insert_on_day(&db, 2, "two days ago", 2, 1000);

        let insights = query_insights(&db, 7).expect("insights");
        assert_eq!(insights.streak.current_days, 2);
    }

    #[test]
    fn streak_breaks_after_two_quiet_days() {
        let db = test_db();
        // Today and yesterday silent; the last activity is two days back.
        insert_on_day(&db, 2, "two days ago", 2, 1000);

        let insights = query_insights(&db, 7).expect("insights");
        assert_eq!(insights.streak.current_days, 0);
    }

    #[test]
    fn streak_stops_at_a_real_gap_even_when_today_is_active() {
        let db = test_db();
        // Today is active, but yesterday was a gap — the grace for a quiet
        // today must not swallow real historical gaps. Streak is just today.
        insert_on_day(&db, 0, "today", 2, 1000);
        insert_on_day(&db, 2, "two days ago", 2, 1000);

        let insights = query_insights(&db, 7).expect("insights");
        assert_eq!(insights.streak.current_days, 1);
    }

    #[test]
    fn transcription_just_before_local_midnight_buckets_to_the_local_day() {
        let db = test_db();
        // 23:50 local two days ago — within an hour of the local-day boundary.
        let entry = insert_transcription_returning(
            &db,
            "late night words",
            "late night words",
            3,
            1000,
            "groq/whisper-large-v3-turbo",
        )
        .expect("insert transcription");
        let created_at = utc_for_local_day(2, 23, 50);
        {
            let conn = lock_conn(&db).expect("lock");
            conn.execute(
                "UPDATE transcriptions SET created_at = ?1 WHERE id = ?2",
                params![created_at, entry.id],
            )
            .expect("set boundary timestamp");
        }

        let insights = query_insights(&db, 0).expect("insights");

        let expected_local_day = local_day_string(2);
        let bucket = insights
            .daily
            .iter()
            .find(|d| d.words > 0)
            .expect("transcription bucketed somewhere");
        assert_eq!(bucket.day, expected_local_day);

        // When the machine's local offset is non-zero, the UTC calendar day
        // of the stored timestamp must NOT be the bucket (proving the
        // 'localtime' conversion happened).
        let utc_day = created_at[..10].to_string();
        if utc_day != expected_local_day {
            assert!(
                !insights.daily.iter().any(|d| d.day == utc_day && d.words > 0),
                "must bucket to the local day, not the UTC one"
            );
        }
    }

    #[test]
    fn top_words_exclude_stopwords_and_sort_descending() {
        let db = test_db();
        let text = "the the the and and and apple banana banana cherry the";
        insert_transcription_returning(&db, text, text, 11, 1000, "groq/whisper-large-v3-turbo")
            .expect("insert transcription");

        let insights = query_insights(&db, 7).expect("insights");

        let top = &insights.words.top;
        assert_eq!(top.len(), 3);
        assert_eq!(top[0].word, "banana");
        assert_eq!(top[0].count, 2);
        assert_eq!(top[1].word, "apple");
        assert_eq!(top[1].count, 1);
        assert_eq!(top[2].word, "cherry");
        assert_eq!(top[2].count, 1);
        assert!(
            !top.iter().any(|w| w.word == "the" || w.word == "and"),
            "stopwords must not appear in top words"
        );
        assert_eq!(insights.words.unique_words, 3);
        assert_eq!(insights.words.longest_word.as_deref(), Some("banana"));
        assert!(insights.words.avg_word_length > 5.0 && insights.words.avg_word_length < 7.0);
    }

    #[test]
    fn top_words_strip_punctuation_and_drop_short_tokens() {
        let db = test_db();
        let text = "React, Vue! React? Vue. Svelte. i a um go";
        insert_transcription_returning(&db, text, text, 10, 1000, "groq/whisper-large-v3-turbo")
            .expect("insert transcription");

        let insights = query_insights(&db, 7).expect("insights");

        let top = &insights.words.top;
        assert!(top.iter().any(|w| w.word == "react" && w.count == 2));
        assert!(top.iter().any(|w| w.word == "vue" && w.count == 2));
        assert!(top.iter().any(|w| w.word == "svelte" && w.count == 1));
        assert!(
            !top.iter().any(|w| w.word == "i" || w.word == "a" || w.word == "um"),
            "sub-3-char and stopword tokens must be excluded"
        );
    }

    #[test]
    fn api_calls_round_trip_and_aggregate_per_model_and_task() {
        let db = test_db();
        let id = insert_on_day(&db, 1, "cost test", 2, 1000);
        let created_at = utc_for_local_day(1, 10, 0);

        let calls = build_api_calls(id, &created_at, "groq/whisper-large-v3-turbo;cleanup=groq/llama-3.3-70b-versatile", 1234, "raw words", "clean words");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].task, "transcription");
        assert_eq!(calls[0].audio_ms, 1234);
        assert_eq!(calls[1].task, "cleanup");
        assert_eq!(calls[1].input_chars, 9);
        assert_eq!(calls[1].output_chars, 11);

        insert_api_calls(&db, &calls).expect("insert api calls");

        let insights = query_insights(&db, 7).expect("insights");
        assert_eq!(insights.providers.len(), 2);
        let tx = insights
            .providers
            .iter()
            .find(|p| p.task == "transcription")
            .expect("transcription row");
        assert_eq!(tx.model, "whisper-large-v3-turbo");
        assert_eq!(tx.provider, "groq");
        assert_eq!(tx.calls, 1);
        assert_eq!(tx.audio_ms, 1234);
        let cleanup = insights
            .providers
            .iter()
            .find(|p| p.task == "cleanup")
            .expect("cleanup row");
        assert_eq!(cleanup.model, "llama-3.3-70b-versatile");
        assert_eq!(cleanup.calls, 1);
        assert_eq!(cleanup.input_chars, 9);
        assert_eq!(cleanup.output_chars, 11);
    }

    #[test]
    fn serde_field_names_match_the_frontend_contract_exactly() {
        let db = test_db();
        let insights = query_insights(&db, 30).expect("insights");
        let json = serde_json::to_value(&insights).expect("serialize insights");
        let object = json.as_object().expect("object");

        for key in [
            "range_days",
            "generated_at",
            "totals",
            "streak",
            "daily",
            "hourly",
            "providers",
            "cleanup",
            "words",
        ] {
            assert!(object.contains_key(key), "missing top-level key: {key}");
        }
        for key in [
            "total_words",
            "total_transcriptions",
            "total_speaking_ms",
            "avg_words_per_transcription",
            "avg_wpm",
            "best_wpm",
            "words_in_range",
            "words_prev_range",
        ] {
            assert!(
                object["totals"].as_object().expect("totals").contains_key(key),
                "missing totals key: {key}"
            );
        }
        for key in [
            "current_days",
            "longest_days",
            "longest_started_on",
            "longest_ended_on",
            "longest_words",
            "active_days",
        ] {
            assert!(
                object["streak"].as_object().expect("streak").contains_key(key),
                "missing streak key: {key}"
            );
        }
        for key in ["day", "words", "transcriptions", "speaking_ms"] {
            assert!(
                object["daily"]
                    .as_array()
                    .expect("daily")
                    .first()
                    .and_then(|d| d.as_object())
                    .expect("day object")
                    .contains_key(key),
                "missing daily key: {key}"
            );
        }
        assert_eq!(
            object["hourly"].as_array().expect("hourly").len(),
            24,
            "hourly must have exactly 24 entries"
        );
        for key in ["model", "provider", "task", "calls", "audio_ms", "input_chars", "output_chars"]
        {
            assert!(
                object["providers"]
                    .as_array()
                    .expect("providers")
                    .first()
                    .and_then(|p| p.as_object())
                    .map(|o| o.contains_key(key))
                    .unwrap_or(true),
                "missing providers key: {key}"
            );
        }
        for key in ["raw_words", "clean_words", "edits_applied", "dictionary_fixes", "auto_learned_terms"] {
            assert!(
                object["cleanup"].as_object().expect("cleanup").contains_key(key),
                "missing cleanup key: {key}"
            );
        }
        for key in ["top", "unique_words", "longest_word", "avg_word_length"] {
            assert!(
                object["words"].as_object().expect("words").contains_key(key),
                "missing words key: {key}"
            );
        }
        assert!(
            object["words"]["top"].as_array().expect("top").is_empty(),
            "top is an array"
        );
    }

    #[test]
    fn all_time_avg_wpm_matches_query_stats_on_the_same_data() {
        let db = test_db();
        insert_on_day(&db, 1, "two words", 2, 1000);
        insert_on_day(&db, 2, "also two", 2, 2000);

        let stats = query_stats(&db).expect("stats");
        let insights = query_insights(&db, 0).expect("insights");

        assert!(stats.avg_wpm > 0.0);
        assert!(
            (insights.totals.avg_wpm - stats.avg_wpm).abs() < 1e-9,
            "all-time insights avg_wpm ({}) must equal query_stats avg_wpm ({})",
            insights.totals.avg_wpm,
            stats.avg_wpm
        );
    }

    #[test]
    fn cleanup_fixes_count_applied_substitutions_and_snippet_expansions() {
        let db = test_db();
        insert_snippet(&db, "sig", "signature", "").expect("snippet");
        {
            let conn = lock_conn(&db).expect("lock");
            conn.execute("UPDATE snippets SET use_count = 7", [])
                .expect("set use count");
        }
        increment_lifetime_dictionary_fixes(&db, 3).expect("increment");

        let insights = query_insights(&db, 7).expect("insights");
        assert_eq!(insights.cleanup.dictionary_fixes, 3);
        assert_eq!(insights.cleanup.edits_applied, 10);

        increment_lifetime_dictionary_fixes(&db, 2).expect("increment again");
        let insights = query_insights(&db, 7).expect("insights");
        assert_eq!(insights.cleanup.dictionary_fixes, 5);
        assert_eq!(insights.cleanup.edits_applied, 12);
    }

    #[test]
    fn stopwords_stay_sorted_for_binary_search() {
        // `is_stopword` relies on binary_search over a sorted slice — a
        // mis-sorted STOPWORDS would silently fail lookups.
        assert!(STOPWORDS.windows(2).all(|w| w[0] < w[1]));
        // Spot-check membership works for both ends and a middle entry.
        assert!(is_stopword("a"));
        assert!(is_stopword("because"));
        assert!(is_stopword("yours"));
        assert!(!is_stopword("verenu"));
    }

    #[test]
    fn parse_api_usage_handles_all_pipeline_formats() {
        let parts = parse_api_usage("groq/whisper-large-v3-turbo/transcription");
        assert_eq!(
            parts.transcription_models,
            vec![("groq".to_string(), "whisper-large-v3-turbo".to_string())]
        );
        assert!(parts.cleanup_models.is_empty());

        let parts = parse_api_usage(
            "primary=groq/whisper-large-v3-turbo;secondary=openai/gpt-4o-transcribe;cleanup=groq/llama-3.3-70b-versatile",
        );
        assert_eq!(parts.transcription_models.len(), 2);
        assert_eq!(
            parts.transcription_models[0],
            ("groq".to_string(), "whisper-large-v3-turbo".to_string())
        );
        assert_eq!(
            parts.transcription_models[1],
            ("openai".to_string(), "gpt-4o-transcribe".to_string())
        );
        assert_eq!(
            parts.cleanup_models,
            vec![("groq".to_string(), "llama-3.3-70b-versatile".to_string())]
        );

        let parts = parse_api_usage("garbage");
        assert!(parts.transcription_models.is_empty());
        assert!(parts.cleanup_models.is_empty());
    }

    #[test]
    fn parse_api_usage_strips_transcription_suffix_in_primary_branch() {
        // The primary= branch must normalize model names exactly like the
        // single-provider fallback — a trailing /transcription must not leak
        // into the stored model key.
        let parts = parse_api_usage("primary=groq/whisper-large-v3-turbo/transcription");
        assert_eq!(
            parts.transcription_models,
            vec![("groq".to_string(), "whisper-large-v3-turbo".to_string())]
        );
    }
}
