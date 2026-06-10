// Shared word-diff and candidate-scoring primitives.
// Used by both the post-injection field monitor (api/auto_learn.rs) and the
// in-pipeline raw↔clean divergence detector (pipeline.rs).

pub const MIN_CANDIDATE_NORM_LEN: usize = 2;
pub const MAX_SPAN_GROWTH_WORDS: usize = 5;
pub const MAX_REPLACEMENTS_PER_SPAN: usize = 2;
pub const MAX_CHANGED_OPS_PER_SPAN: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordToken {
    pub raw: String,
    pub norm: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CandidateCorrection {
    pub mistake: String,
    pub correction: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct CorrectionMetrics {
    pub a_len: usize,
    pub b_len: usize,
    pub max_len: usize,
    pub distance: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignOp {
    Equal,
    Replace,
    Insert,
    Delete,
}

pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut row: Vec<usize> = (0..=n).collect();
    for i in 1..=m {
        let mut prev = row[0];
        row[0] = i;
        for j in 1..=n {
            let old = row[j];
            row[j] = if a[i - 1] == b[j - 1] {
                prev
            } else {
                1 + prev.min(row[j]).min(row[j - 1])
            };
            prev = old;
        }
    }
    row[n]
}

pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '\'' | '-' | '_')
}

fn normalize_word(word: &str) -> String {
    word.chars()
        .filter(|c| is_word_char(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn tokenize_words(text: &str) -> Vec<WordToken> {
    let mut tokens = Vec::new();
    let mut start = None;

    for (idx, ch) in text.char_indices() {
        if is_word_char(ch) {
            if start.is_none() {
                start = Some(idx);
            }
        } else if let Some(s) = start.take() {
            let raw = &text[s..idx];
            let norm = normalize_word(raw);
            if !norm.is_empty() {
                tokens.push(WordToken {
                    raw: raw.to_string(),
                    norm,
                });
            }
        }
    }

    if let Some(s) = start {
        let raw = &text[s..];
        let norm = normalize_word(raw);
        if !norm.is_empty() {
            tokens.push(WordToken {
                raw: raw.to_string(),
                norm,
            });
        }
    }

    tokens
}

pub fn has_distinctive_features(token: &str) -> bool {
    if !token.is_ascii() {
        return true;
    }
    if token.len() >= 4
        && token
            .chars()
            .any(|c| matches!(c.to_ascii_lowercase(), 'q' | 'x' | 'z'))
    {
        return true;
    }
    if token
        .chars()
        .any(|c| c.is_ascii_digit() || matches!(c, '\'' | '-' | '_'))
    {
        return true;
    }

    let uppercase_count = token.chars().filter(|c| c.is_uppercase()).count();
    uppercase_count > 1 || token.chars().skip(1).any(|c| c.is_uppercase())
}

pub fn is_common_word(word: &str) -> bool {
    matches!(
        word,
        "a" | "about"
            | "after"
            | "all"
            | "also"
            | "am"
            | "an"
            | "and"
            | "are"
            | "as"
            | "ask"
            | "at"
            | "be"
            | "but"
            | "by"
            | "can"
            | "do"
            | "for"
            | "from"
            | "get"
            | "go"
            | "had"
            | "has"
            | "have"
            | "he"
            | "her"
            | "him"
            | "his"
            | "i"
            | "if"
            | "in"
            | "is"
            | "it"
            | "its"
            | "just"
            | "me"
            | "my"
            | "no"
            | "not"
            | "of"
            | "on"
            | "or"
            | "our"
            | "out"
            | "so"
            | "ship"
            | "shop"
            | "that"
            | "the"
            | "them"
            | "then"
            | "there"
            | "their"
            | "they"
            | "this"
            | "to"
            | "up"
            | "us"
            | "was"
            | "we"
            | "what"
            | "when"
            | "with"
            | "you"
            | "your"
    )
}

pub fn compute_correction_metrics(
    original: &WordToken,
    corrected: &WordToken,
) -> CorrectionMetrics {
    let a_len = original.norm.chars().count();
    let b_len = corrected.norm.chars().count();
    let max_len = a_len.max(b_len);
    let distance = edit_distance(&original.norm, &corrected.norm);
    CorrectionMetrics {
        a_len,
        b_len,
        max_len,
        distance,
    }
}

fn is_plain_suffix_completion(original: &str, corrected: &str) -> bool {
    if let Some(suffix) = corrected.strip_prefix(original) {
        return is_low_signal_suffix(suffix);
    }

    if let Some(suffix) = original.strip_prefix(corrected) {
        return is_low_signal_suffix(suffix);
    }

    false
}

fn is_low_signal_suffix(suffix: &str) -> bool {
    matches!(suffix, "s" | "d" | "e" | "g" | "ed" | "er" | "es" | "ing")
        || suffix
            .chars()
            .all(|ch| matches!(ch, 'a' | 'e' | 'i' | 'o' | 'u'))
}

pub fn is_candidate_correction(
    original: &WordToken,
    corrected: &WordToken,
    metrics: CorrectionMetrics,
) -> bool {
    if original.norm.is_empty() || corrected.norm.is_empty() {
        return false;
    }
    if original.norm == corrected.norm {
        return false;
    }
    if metrics.a_len < MIN_CANDIDATE_NORM_LEN || metrics.b_len < MIN_CANDIDATE_NORM_LEN {
        return false;
    }

    let original_distinct = has_distinctive_features(&original.raw);
    let corrected_distinct = has_distinctive_features(&corrected.raw);
    if metrics.max_len <= 3 && !original_distinct && !corrected_distinct {
        return false;
    }

    if is_common_word(&original.norm)
        && is_common_word(&corrected.norm)
        && !original_distinct
        && !corrected_distinct
    {
        return false;
    }

    if is_plain_suffix_completion(&original.norm, &corrected.norm)
        && !original_distinct
        && !corrected_distinct
    {
        return false;
    }

    metrics.distance <= 2_usize.max(metrics.max_len / 2)
        || ((original_distinct || corrected_distinct)
            && metrics.max_len >= 4
            && metrics.distance <= 3)
}

pub fn candidate_confidence(
    original: &WordToken,
    corrected: &WordToken,
    metrics: CorrectionMetrics,
    changed_ops: usize,
    replacements_len: usize,
) -> f64 {
    let distance = metrics.distance as f64;
    let max_len = metrics.max_len.max(1) as f64;
    let ratio_score = 1.0 - (distance / max_len).min(1.0);

    let mut score = ratio_score * 0.55;
    if has_distinctive_features(&original.raw) || has_distinctive_features(&corrected.raw) {
        score += 0.25;
        if metrics.max_len <= 5 && metrics.distance <= 3 {
            score += 0.10;
        }
    }
    if is_common_word(&original.norm) && is_common_word(&corrected.norm) {
        score -= 0.2;
    }
    score -= (changed_ops.saturating_sub(1) as f64) * 0.07;
    score -= (replacements_len.saturating_sub(1) as f64) * 0.08;
    score.clamp(0.0, 1.0)
}

pub fn align_word_ops(
    original: &[WordToken],
    current: &[WordToken],
) -> Vec<(AlignOp, Option<usize>, Option<usize>)> {
    let m = original.len();
    let n = current.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    for (i, row) in dp.iter_mut().enumerate().take(m + 1) {
        row[0] = i;
    }
    for (j, cell) in dp[0].iter_mut().enumerate().take(n + 1) {
        *cell = j;
    }

    for i in 1..=m {
        for j in 1..=n {
            let replace_cost = if original[i - 1].norm == current[j - 1].norm {
                0
            } else {
                1
            };
            dp[i][j] = (dp[i - 1][j - 1] + replace_cost)
                .min(dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1);
        }
    }

    let mut ops = Vec::new();
    let (mut i, mut j) = (m, n);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 {
            let replace_cost = if original[i - 1].norm == current[j - 1].norm {
                0
            } else {
                1
            };
            if dp[i][j] == dp[i - 1][j - 1] + replace_cost {
                let op = if replace_cost == 0 {
                    AlignOp::Equal
                } else {
                    AlignOp::Replace
                };
                ops.push((op, Some(i - 1), Some(j - 1)));
                i -= 1;
                j -= 1;
                continue;
            }
        }
        if i > 0 && dp[i][j] == dp[i - 1][j] + 1 {
            ops.push((AlignOp::Delete, Some(i - 1), None));
            i -= 1;
        } else {
            ops.push((AlignOp::Insert, None, Some(j - 1)));
            j -= 1;
        }
    }

    ops.reverse();
    ops
}

pub fn detect_span_corrections(
    original_span: &str,
    current_span: &str,
) -> Vec<CandidateCorrection> {
    let original = tokenize_words(original_span);
    let current = tokenize_words(current_span);

    if original.is_empty() || current.is_empty() {
        return vec![];
    }
    if current.len() > original.len() * 2 + MAX_SPAN_GROWTH_WORDS {
        return vec![];
    }

    let ops = align_word_ops(&original, &current);
    let changed_ops = ops
        .iter()
        .filter(|(op, _, _)| *op != AlignOp::Equal)
        .count();
    if changed_ops > MAX_CHANGED_OPS_PER_SPAN {
        log::debug!("correction_diff: rejected span with too many changed word operations");
        return vec![];
    }

    let replacements: Vec<_> = ops
        .iter()
        .filter_map(|(op, old_idx, new_idx)| {
            if *op == AlignOp::Replace {
                Some((old_idx.unwrap(), new_idx.unwrap()))
            } else {
                None
            }
        })
        .collect();

    if replacements.len() > MAX_REPLACEMENTS_PER_SPAN {
        log::debug!("correction_diff: rejected span with too many replacements");
        return vec![];
    }

    let replacements_len = replacements.len();
    replacements
        .into_iter()
        .filter_map(|(old_idx, new_idx)| {
            let old = &original[old_idx];
            let new = &current[new_idx];
            if old.norm.is_empty() || new.norm.is_empty() || old.norm == new.norm {
                return None;
            }
            let metrics = compute_correction_metrics(old, new);
            if is_candidate_correction(old, new, metrics) {
                Some(CandidateCorrection {
                    mistake: old.raw.clone(),
                    correction: new.raw.clone(),
                    confidence: candidate_confidence(
                        old,
                        new,
                        metrics,
                        changed_ops,
                        replacements_len,
                    ),
                })
            } else {
                log::debug!("correction_diff: rejected low-confidence candidate");
                None
            }
        })
        .collect()
}

/// Computes a simple consonant-skeleton for phonetic similarity gating.
/// Reduces a word to its consonants (minus repeated adjacents) so that
/// "Kubernetes" and "Koobernetes" share a similar skeleton while
/// "running" and "ran" do not.
pub fn consonant_skeleton(word: &str) -> String {
    const VOWELS: &[char] = &[
        'a', 'e', 'i', 'o', 'u', 'y',
        // Common accented Latin vowels (e.g. "café", "naïve") so they are
        // treated as vowels rather than unrelated consonants.
        'à', 'á', 'â', 'ã', 'ä', 'å', 'ā',
        'è', 'é', 'ê', 'ë', 'ē',
        'ì', 'í', 'î', 'ï', 'ī',
        'ò', 'ó', 'ô', 'õ', 'ö', 'ō', 'ø',
        'ù', 'ú', 'û', 'ü', 'ū',
        'ý', 'ÿ',
    ];
    let mut out = String::new();
    let mut prev: Option<char> = None;
    for ch in word.chars().flat_map(char::to_lowercase) {
        if VOWELS.contains(&ch) {
            prev = None;
        } else if ch.is_alphanumeric() {
            if prev != Some(ch) {
                out.push(ch);
            }
            prev = Some(ch);
        }
    }
    out
}

/// Casual contraction → formal-rewrite pairs that the cleanup LLM routinely
/// applies (e.g. "gonna" → "going"). These are style normalizations, not
/// transcription mistakes, so they must never be treated as plausible
/// transcription confusions even though they're phonetically close.
const COLLOQUIAL_NORMALIZATIONS: &[(&str, &str)] = &[
    ("gonna", "going"),
    ("wanna", "want"),
    ("gotta", "got"),
    ("kinda", "kind"),
    ("sorta", "sort"),
    ("lemme", "let"),
    ("gimme", "give"),
    ("dunno", "know"),
    ("hafta", "have"),
    ("cuz", "because"),
    ("cause", "because"),
    ("til", "until"),
    ("yall", "you"),
    ("aint", "isn't"),
];

fn is_known_colloquial_pair(a: &str, b: &str) -> bool {
    COLLOQUIAL_NORMALIZATIONS
        .iter()
        .any(|&(c, f)| (a == c && b == f) || (a == f && b == c))
}

/// Returns true when the pair looks like a plausible transcription confusion
/// (same consonant skeleton, or edit-distance close enough relative to length).
/// This gates Source A (cleanup divergence) from learning grammar/filler rewrites.
pub fn is_plausible_transcription_confusion(mistake: &str, correction: &str) -> bool {
    let m_digits: String = mistake.chars().filter(|c| c.is_ascii_digit()).collect();
    let c_digits: String = correction.chars().filter(|c| c.is_ascii_digit()).collect();
    if m_digits != c_digits {
        return false;
    }

    let m_norm: String = mistake.chars().flat_map(char::to_lowercase).collect();
    let c_norm: String = correction.chars().flat_map(char::to_lowercase).collect();

    let m_apos_stripped: String = m_norm.chars().filter(|&c| c != '\'').collect();
    let c_apos_stripped: String = c_norm.chars().filter(|&c| c != '\'').collect();
    if is_known_colloquial_pair(&m_apos_stripped, &c_apos_stripped) {
        return false;
    }

    let skel_m = consonant_skeleton(&m_norm);
    let skel_c = consonant_skeleton(&c_norm);

    if skel_m == skel_c {
        return true;
    }

    let skel_dist = edit_distance(&skel_m, &skel_c);
    let skel_max = skel_m.chars().count().max(skel_c.chars().count()).max(1);
    if skel_dist <= skel_max / 3 + 1 {
        return true;
    }

    // Fallback: surface-level ratio.
    // Short words (≤5 chars) get a lenient threshold because brand names can look
    // very different phonetically despite sounding identical (e.g. "rock" / "Groq").
    let dist = edit_distance(&m_norm, &c_norm);
    let max_len = m_norm.chars().count().max(c_norm.chars().count()).max(1);
    if max_len <= 5 {
        dist < max_len
    } else {
        dist <= max_len / 3 + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff(original: &str, current: &str) -> Vec<(String, String)> {
        detect_span_corrections(original, current)
            .into_iter()
            .map(|c| (c.mistake, c.correction))
            .collect()
    }

    #[test]
    fn simple_typo_detected() {
        assert_eq!(
            diff(
                "please use Koobernetes today",
                "please use Kubernetes today"
            ),
            vec![("Koobernetes".to_string(), "Kubernetes".to_string())]
        );
    }

    #[test]
    fn casing_only_change_rejected() {
        assert!(diff("Ask.", "Ask").is_empty());
        assert!(diff("ask", "Ask").is_empty());
    }

    #[test]
    fn suffix_completion_rejected() {
        assert!(diff("send the file", "sends the file").is_empty());
        assert!(diff("we should do", "we should doing").is_empty());
    }

    #[test]
    fn short_technical_term_detected() {
        assert_eq!(
            diff("bran rot hosting", "bran qroq hosting"),
            vec![("rot".to_string(), "qroq".to_string())]
        );
    }

    #[test]
    fn phonetic_confusion_kubernetes() {
        assert!(is_plausible_transcription_confusion(
            "Koobernetes",
            "Kubernetes"
        ));
    }

    #[test]
    fn phonetic_confusion_groq() {
        assert!(is_plausible_transcription_confusion("rock", "qroq"));
    }

    #[test]
    fn short_brand_confidence_clears_evidence_gate() {
        let candidates = detect_span_corrections("use rock hosting", "use qroq hosting");
        assert_eq!(candidates.len(), 1);
        assert!(
            candidates[0].confidence >= 0.45,
            "short brand corrections must be strong enough to record as evidence"
        );
    }

    #[test]
    fn phonetic_confusion_rejects_grammar_rewrite() {
        // "running" → "sprint" is not a transcription confusion
        assert!(!is_plausible_transcription_confusion("running", "sprint"));
    }

    #[test]
    fn phonetic_confusion_rejects_gonna_going() {
        // "gonna" → "going" is a cleanup style normalization, not a mistranscription.
        assert!(!is_plausible_transcription_confusion("gonna", "going"));
    }

    #[test]
    fn phonetic_confusion_rejects_wanna_want() {
        assert!(!is_plausible_transcription_confusion("wanna", "want"));
    }

    #[test]
    fn phonetic_confusion_rejects_differing_digits() {
        // "IPv4" vs "IPv6" / "100" vs "200" are distinct values, not
        // transcription confusions, even though their skeletons match.
        assert!(!is_plausible_transcription_confusion("IPv4", "IPv6"));
        assert!(!is_plausible_transcription_confusion("100", "200"));
    }

    #[test]
    fn consonant_skeleton_basic() {
        assert_eq!(consonant_skeleton("kubernetes"), "kbrnts");
        assert_eq!(consonant_skeleton("koobernetes"), "kbrnts");
    }

    #[test]
    fn consonant_skeleton_treats_y_as_vowel() {
        // "y" acts as a vowel in words like "type"/"rhythm" — treating it as a
        // consonant would make phonetically similar words diverge unnecessarily.
        assert_eq!(consonant_skeleton("typo"), consonant_skeleton("tipo"));
    }

    #[test]
    fn consonant_skeleton_treats_accented_vowels_as_vowels() {
        // Accented vowels (e.g. "café"/"naïve") must not be treated as
        // consonants, or their skeletons would diverge from the unaccented form.
        assert_eq!(consonant_skeleton("café"), consonant_skeleton("cafe"));
        assert_eq!(consonant_skeleton("naïve"), consonant_skeleton("naive"));
    }

    #[test]
    fn consonant_skeleton_keeps_consonants_separated_by_vowels() {
        // A repeated consonant separated by a vowel (e.g. "state") must not be
        // collapsed into a single occurrence — only directly-adjacent
        // duplicates (from the same letter, e.g. "koobernetes") collapse.
        assert_eq!(consonant_skeleton("state"), "stt");
        assert_ne!(consonant_skeleton("state"), consonant_skeleton("sat"));
    }

    #[test]
    fn consonant_skeleton_preserves_digits() {
        // Purely numeric words must not collapse to an empty skeleton, or
        // distinct numbers (e.g. "100" vs "200") would look identical.
        assert_eq!(consonant_skeleton("100"), "10");
        assert_ne!(consonant_skeleton("100"), consonant_skeleton("200"));
        assert!(!consonant_skeleton("100").is_empty());
    }
}
