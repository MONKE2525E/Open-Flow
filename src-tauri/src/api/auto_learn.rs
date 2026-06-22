use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};
use tauri_plugin_store::StoreExt;

#[cfg(windows)]
use crate::core::context_probe::{
    describe_selection_state, resolve_context_from_tail, stable_metadata_hash, ContextProbeSource,
    InjectionContextProbe, SelectionState,
};
#[cfg(not(windows))]
use crate::core::context_probe::{ContextProbeSource, InjectionContextProbe};
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use crate::core::text_context::is_invisible_prefix_char as is_invisible_probe_char;
#[cfg(any(windows, test))]
use crate::core::text_context::SentenceContext;
use crate::data::{db, store};
use crate::DbHandle;

mod correction;
mod focused_text;
mod monitor;
mod rejection;
use correction::*;
// Glob feeds the `pub(super)` helpers (e.g. classify_caret_char,
// resolve_injection_context) to the test module via `use super::*`; on macOS
// the lib build uses none of them directly, so silence the unused-import lint.
#[allow(unused_imports)]
use focused_text::*;
use monitor::*;
#[allow(unused_imports)]
pub use focused_text::{
    read_focused_text, read_focused_text_probe, read_injection_context_probe, FocusedTextProbe,
};
pub use monitor::start_monitor;
pub use rejection::{start_cache_rejection_monitor, start_rejection_monitor};

const MONITOR_WINDOW_SECS: u64 = 60;
const POLL_INTERVAL_SECS: u64 = 2;
const BASELINE_CAPTURE_DELAY_MS: u64 = 250;
const BASELINE_RETRY_DELAY_MS: u64 = 500;
const EVENT_MONITOR_POLL_MS: u64 = 250;
const REJECTION_WINDOW_SECS: u64 = 8;
const REJECTION_POLL_MS: u64 = 500;
const CACHE_REJECTION_WINDOW_SECS: u64 = 10;
const PENDING_RETENTION_DAYS: i64 = 2;
const PROMOTION_THRESHOLD_DEFAULT: i64 = 2;
const PROMOTION_THRESHOLD_FAST: i64 = 1;
// candidate_confidence() tops out around 0.80; this is reachable only by
// distinctive corrections (brand/technical terms) with a small edit distance.
const FAST_PROMOTION_CONFIDENCE: f64 = 0.70;
const HIGH_CONFIDENCE_TIER: f64 = 0.70;
const MEDIUM_CONFIDENCE_TIER: f64 = 0.55;
const STABLE_TEXT_OBSERVATIONS_REQUIRED: usize = 2;
const MIN_CANDIDATE_NORM_LEN: usize = 2;
const MAX_SPAN_GROWTH_WORDS: usize = 5;
const MAX_REPLACEMENTS_PER_SPAN: usize = 2;
const MAX_CHANGED_OPS_PER_SPAN: usize = 4;
const MIN_CANDIDATE_CONFIDENCE: f64 = 0.45;



#[derive(Debug, Clone, PartialEq, Eq)]
struct WordToken {
    raw: String,
    norm: String,
}

#[derive(Debug, Clone, PartialEq)]
struct CandidateCorrection {
    mistake: String,
    correction: String,
    confidence: f64,
}

#[derive(Debug, Clone, Copy)]
struct CorrectionMetrics {
    a_len: usize,
    b_len: usize,
    max_len: usize,
    distance: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TextAnchor {
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlignOp {
    Equal,
    Replace,
    Insert,
    Delete,
}

#[derive(Debug, Default)]
struct StableTextGate {
    pending_text: Option<String>,
    pending_observations: usize,
    processed_text: Option<String>,
}

impl StableTextGate {
    fn observe(&mut self, text: String) -> Option<&str> {
        if self.pending_text.as_deref() == Some(text.as_str()) {
            self.pending_observations += 1;
        } else {
            self.pending_text = Some(text);
            self.pending_observations = 1;
        }

        if self.pending_observations < STABLE_TEXT_OBSERVATIONS_REQUIRED {
            return None;
        }

        let stable_text = self.pending_text.as_deref()?;
        if self.processed_text.as_deref() == Some(stable_text) {
            return None;
        }

        self.processed_text = Some(stable_text.to_string());
        self.processed_text.as_deref()
    }
}





#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::text_context::SentenceContext;
    use crate::data::db;

    #[derive(Debug, serde::Deserialize)]
    struct AutoLearnCase {
        name: String,
        original: String,
        corrected: String,
        expect: Vec<[String; 2]>,
        /// Number of sessions needed to promote this pair, or `null` if the
        /// case isn't expected to promote at all. 1 exercises the
        /// fast-promotion path (confidence >= FAST_PROMOTION_CONFIDENCE);
        /// 2 exercises the default path.
        #[serde(default)]
        promotion_sessions: Option<u8>,
    }

    #[test]
    fn anchored_text_ignores_surrounding_content() {
        let injected = "Ask me about Koobernetes today";
        let baseline = "Before this. Ask me about Koobernetes today After this.";
        let current = "Before this. Ask me about Kubernetes today After this.";

        let diffs = detect_corrections_from_anchored_text(injected, baseline, current);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].mistake, "Koobernetes");
        assert_eq!(diffs[0].correction, "Kubernetes");
        assert!(diffs[0].confidence > 0.0);
    }

    #[test]
    fn edits_before_injected_span_are_ignored() {
        let injected = "Ask me about Koobernetes today";
        let baseline = "Before this. Ask me about Koobernetes today After this.";
        let current = "Before this please. Ask me about Kubernetes today After this.";

        assert!(detect_corrections_from_anchored_text(injected, baseline, current).is_empty());
    }

    #[test]
    fn simple_typo_correction_is_detected() {
        assert_eq!(
            diff_words(
                "please use Koobernetes today",
                "please use Kubernetes today"
            ),
            vec![("Koobernetes".to_string(), "Kubernetes".to_string())]
        );
    }

    #[test]
    fn casing_and_punctuation_only_changes_are_rejected() {
        assert!(diff_words("Ask.", "Ask").is_empty());
        assert!(diff_words("ask", "Ask").is_empty());
    }

    #[test]
    fn inserted_words_do_not_shift_later_alignment() {
        assert_eq!(
            diff_words(
                "please use Koobernetes today",
                "please use the Kubernetes today"
            ),
            vec![("Koobernetes".to_string(), "Kubernetes".to_string())]
        );
    }

    #[test]
    fn full_sentence_rewrites_are_ignored() {
        assert!(diff_words(
            "please use Koobernetes in the deployment tomorrow",
            "rewrite this whole sentence into something else"
        )
        .is_empty());
    }

    #[test]
    fn short_common_word_swaps_are_rejected() {
        assert!(diff_words("as me later", "ask me later").is_empty());
    }

    #[test]
    fn plain_suffix_completion_is_rejected() {
        assert!(diff_words("bran rot hostin", "bran rot hosting").is_empty());
        assert!(diff_words("send the file", "sends the file").is_empty());
        assert!(diff_words("say nugga", "say nuggaaaa").is_empty());
        assert!(diff_words("scratch the cat", "scratch the cats").is_empty());
        assert!(diff_words("we should do", "we should doing").is_empty());
    }

    #[test]
    fn short_technical_brand_term_correction_is_detected() {
        assert_eq!(
            diff_words("bran rock hosting", "bran qroq hosting"),
            vec![("rock".to_string(), "qroq".to_string())]
        );
        assert_eq!(
            diff_words("bran rot hosting", "bran qroq hosting"),
            vec![("rot".to_string(), "qroq".to_string())]
        );
    }

    #[test]
    fn one_letter_candidate_is_rejected() {
        assert!(diff_words("use x hosting", "use qroq hosting").is_empty());
        assert!(diff_words("use rock hosting", "use q hosting").is_empty());
    }

    #[test]
    fn repeated_candidate_counts_once_per_session() {
        let db = db::open(":memory:").expect("test db");
        let mut recorded = HashSet::new();

        assert!(!record_candidate(
            &db,
            &mut recorded,
            "test-app",
            "Koobernetes".to_string(),
            "Kubernetes".to_string(),
            0.6,
        ));
        assert!(!record_candidate(
            &db,
            &mut recorded,
            "test-app",
            "Koobernetes".to_string(),
            "Kubernetes".to_string(),
            0.6,
        ));

        let count = db::count_pending_corrections_recent(
            &db,
            "Koobernetes",
            "Kubernetes",
            PENDING_RETENTION_DAYS,
        )
        .expect("pending count");
        assert_eq!(count, 1);
    }

    #[test]
    fn candidate_promotes_after_two_sessions() {
        let db = db::open(":memory:").expect("test db");

        for expected in [false, true] {
            let mut recorded = HashSet::new();
            assert_eq!(
                record_candidate(
                    &db,
                    &mut recorded,
                    "test-app",
                    "Koobernetes".to_string(),
                    "Kubernetes".to_string(),
                    0.6,
                ),
                expected
            );
        }

        let entries = db::query_dictionary(&db).expect("dictionary");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].term, "Kubernetes");
        assert_eq!(entries[0].mistake.as_deref(), Some("Koobernetes"));
        assert!(entries[0].auto_learned);
    }

    #[test]
    fn short_technical_term_promotes_only_after_two_sessions() {
        let db = db::open(":memory:").expect("test db");

        for expected in [false, true] {
            let mut recorded = HashSet::new();
            assert_eq!(
                record_candidate(
                    &db,
                    &mut recorded,
                    "test-app",
                    "rock".to_string(),
                    "qroq".to_string(),
                    0.6,
                ),
                expected
            );
        }

        let entries = db::query_dictionary(&db).expect("dictionary");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].term, "qroq");
        assert_eq!(entries[0].mistake.as_deref(), Some("rock"));
        assert!(entries[0].auto_learned);
    }

    #[test]
    fn high_confidence_candidate_promotes_after_one_session() {
        let db = db::open(":memory:").expect("test db");
        let mut recorded = HashSet::new();

        assert!(record_candidate(
            &db,
            &mut recorded,
            "test-app",
            "vsc0de".to_string(),
            "vscode".to_string(),
            0.75,
        ));

        let entries = db::query_dictionary(&db).expect("dictionary");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].term, "vscode");
        assert_eq!(entries[0].mistake.as_deref(), Some("vsc0de"));
        assert!(entries[0].auto_learned);
        assert_eq!(entries[0].confidence_tier, "high");
    }

    #[test]
    fn stable_text_gate_waits_for_consecutive_identical_reads() {
        let mut gate = StableTextGate::default();

        assert_eq!(gate.observe("say nugga".to_string()), None);
        assert_eq!(gate.observe("say nuggaaaa".to_string()), None);
        assert_eq!(gate.observe("say nuggaaaaa".to_string()), None);
        assert_eq!(gate.observe("say nuggaaaa".to_string()), None);
        assert_eq!(
            gate.observe("say nuggaaaa".to_string()),
            Some("say nuggaaaa")
        );
        assert_eq!(gate.observe("say nuggaaaa".to_string()), None);
    }

    #[test]
    fn blank_field_probe_capitalizes() {
        let context = resolve_injection_context(true, None);
        assert_eq!(context, SentenceContext::NewSentence);
        assert!(context.should_capitalize());
    }

    #[test]
    fn unknown_probe_capitalizes() {
        let context = resolve_injection_context(false, None);
        assert_eq!(context, SentenceContext::Unknown);
        assert!(context.should_capitalize());
    }

    #[test]
    fn sentence_ending_characters_capitalize() {
        assert_eq!(
            resolve_injection_context(false, Some('.')),
            SentenceContext::NewSentence
        );
        assert_eq!(
            resolve_injection_context(false, Some('!')),
            SentenceContext::NewSentence
        );
        assert_eq!(
            resolve_injection_context(false, Some('?')),
            SentenceContext::NewSentence
        );
    }

    #[test]
    fn mid_sentence_probe_lowercases() {
        let context = resolve_injection_context(false, Some('a'));
        assert_eq!(context, SentenceContext::MidSentence);
        assert!(!context.should_capitalize());
    }

    #[test]
    fn blank_field_overrides_stale_context() {
        assert_eq!(
            resolve_injection_context(true, Some('a')),
            SentenceContext::NewSentence
        );
    }

    #[test]
    fn invisible_probe_characters_do_not_force_lowercase() {
        assert_eq!(classify_caret_char('\u{200b}'), None);
        assert_eq!(classify_caret_char('\u{feff}'), None);
        assert_eq!(classify_caret_char('"'), None);
        assert_eq!(classify_caret_char('('), None);
        assert_eq!(
            resolve_injection_context(false, Some('\u{200b}')),
            SentenceContext::Unknown
        );
        assert_eq!(
            resolve_injection_context(false, Some('"')),
            SentenceContext::Unknown
        );
    }

    #[test]
    fn event_mode_hook_unavailable_uses_poll_interval_sleep() {
        assert_eq!(
            event_mode_poll_sleep_duration(false),
            std::time::Duration::from_secs(POLL_INTERVAL_SECS)
        );
    }

    #[test]
    fn event_mode_hook_ready_uses_fast_poll_sleep() {
        assert_eq!(
            event_mode_poll_sleep_duration(true),
            std::time::Duration::from_millis(EVENT_MONITOR_POLL_MS)
        );
    }

    #[test]
    fn auto_learn_regression_matrix() {
        let raw = include_str!("../../testdata/auto_learn_cases.json");
        let cases: Vec<AutoLearnCase> = serde_json::from_str(raw).expect("valid cases");

        for case in cases {
            let actual = diff_words(&case.original, &case.corrected);
            let expected: Vec<(String, String)> = case
                .expect
                .iter()
                .map(|pair| (pair[0].clone(), pair[1].clone()))
                .collect();
            assert_eq!(actual, expected, "case {}", case.name);

            if let Some(sessions) = case.promotion_sessions {
                assert!(
                    !expected.is_empty(),
                    "case {} marked promotable without expected pair",
                    case.name
                );
                let db = db::open(":memory:").expect("test db");
                let (mistake, correction) = expected[0].clone();

                // 1 session exercises the fast-promotion path (confidence >=
                // FAST_PROMOTION_CONFIDENCE); 2 exercises the default path.
                let confidence = if sessions == 1 {
                    FAST_PROMOTION_CONFIDENCE + 0.05
                } else {
                    FAST_PROMOTION_CONFIDENCE - 0.1
                };

                for session in 1..=sessions {
                    let mut recorded = HashSet::new();
                    let promoted = record_candidate(
                        &db,
                        &mut recorded,
                        "test-app",
                        mistake.clone(),
                        correction.clone(),
                        confidence,
                    );
                    assert_eq!(
                        promoted,
                        session == sessions,
                        "case {} promotion threshold (session {session}/{sessions})",
                        case.name
                    );
                }
            }
        }
    }
}
