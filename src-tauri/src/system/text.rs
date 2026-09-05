/// True if `token` contains a feature (non-ASCII, q/x/z, digit/apostrophe/hyphen/underscore,
/// or internal capitalization) that makes it unlikely to be a plain mis-transcribed
/// common word — i.e. likely a brand/technical term or proper noun.
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

pub fn tokenize_lower_alnum(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut buf = String::new();
    for ch in input.chars() {
        if ch.is_alphanumeric() {
            if is_cjk_token_char(ch) {
                if !buf.is_empty() {
                    tokens.push(std::mem::take(&mut buf));
                }
                tokens.push(ch.to_string());
                continue;
            }
            buf.extend(ch.to_lowercase());
        } else if !buf.is_empty() {
            tokens.push(std::mem::take(&mut buf));
        }
    }
    if !buf.is_empty() {
        tokens.push(buf);
    }
    tokens
}

fn is_cjk_token_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{3400}'..='\u{4dbf}'
            | '\u{4e00}'..='\u{9fff}'
            | '\u{f900}'..='\u{faff}'
            | '\u{3040}'..='\u{30ff}'
            | '\u{ac00}'..='\u{d7af}'
    )
}

/// Strips em dashes (—, U+2014) the cleanup model introduced but the
/// speaker never actually said, replacing each with a comma so the
/// surrounding clause still reads naturally. People don't dictate "em
/// dash" out loud — its appearance in cleaned output is a well-known LLM
/// stylistic habit (dramatic pause / parenthetical aside), not derived from
/// real speech. If `raw` already contains an em dash, `cleaned` is returned
/// unchanged, since the speaker may genuinely have meant that exact break
/// (e.g. dictated from text that already used one).
pub fn strip_unspoken_em_dashes(raw: &str, cleaned: &str) -> String {
    const EM_DASH: char = '\u{2014}';
    if raw.contains(EM_DASH) || !cleaned.contains(EM_DASH) {
        return cleaned.to_string();
    }
    let parts: Vec<&str> = cleaned
        .split(EM_DASH)
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    let mut result = String::new();
    for part in parts {
        if !result.is_empty() {
            // A part that already ends in punctuation (e.g. "Hello, —" or
            // "First sentence. —") gets a plain space instead of another
            // ", " — otherwise joining unconditionally produces malformed
            // doubled punctuation like "Hello,, " or "sentence., ".
            let prev_ends_with_punct = result
                .trim_end()
                .chars()
                .last()
                .is_some_and(|c| matches!(c, '.' | '!' | '?' | ',' | ';' | ':'));
            result.push_str(if prev_ends_with_punct { " " } else { ", " });
        }
        result.push_str(part);
    }
    result
}

/// Standalone filler/hesitation words mechanically stripped from cleanup
/// intensity cleanup output as a deterministic backstop. Every prompt
/// already tells the model to remove these, but small local models apply
/// that instruction unreliably — observed in practice on a single real
/// dictation: one "um" was correctly stripped while two other filler
/// words survived untouched elsewhere in the same output. Unlike "like",
/// which has common non-filler meanings, a bare um/uh/ah/erm carries no
/// semantic content in English speech, so mechanical removal here has no real
/// false-positive risk — unlike "like", which is deliberately NOT on this
/// list. The separate "you know" rule only removes an unambiguous discourse
/// filler position.
const FILLER_HESITATION_WORDS: &[&str] = &[
    "um", "umm", "ummm", "uhm", "uh", "uhh", "uhhh", "erm", "err", "ah", "ahh",
];

fn is_filler_hesitation_word(word: &str) -> bool {
    FILLER_HESITATION_WORDS.contains(&word.to_lowercase().as_str())
}

enum FillerTok {
    Word(String),
    Other(String),
}

fn tokenize_words_and_other(text: &str) -> Vec<FillerTok> {
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut in_word = false;
    for ch in text.chars() {
        if is_cjk_token_char(ch) {
            if !buf.is_empty() {
                let taken = std::mem::take(&mut buf);
                tokens.push(if in_word {
                    FillerTok::Word(taken)
                } else {
                    FillerTok::Other(taken)
                });
            }
            tokens.push(FillerTok::Word(ch.to_string()));
            in_word = false;
            continue;
        }
        let is_word_char = ch.is_alphabetic() || ch == '\'';
        if is_word_char != in_word && !buf.is_empty() {
            let taken = std::mem::take(&mut buf);
            tokens.push(if in_word {
                FillerTok::Word(taken)
            } else {
                FillerTok::Other(taken)
            });
        }
        in_word = is_word_char;
        buf.push(ch);
    }
    if !buf.is_empty() {
        tokens.push(if in_word {
            FillerTok::Word(buf)
        } else {
            FillerTok::Other(buf)
        });
    }
    tokens
}

const SENTENCE_END_CHARS: [char; 3] = ['.', '!', '?'];

/// Merges two separator strings that used to be split by a now-removed
/// filler word (e.g. the ". " before "Um" and the ", " after it) into one,
/// returning `true` if the merged separator ends on sentence-ending
/// punctuation (so the caller knows to re-capitalize the word that follows).
/// A sentence-ender on either side always wins and drops any comma —
/// "sentence-end," is never valid punctuation, and blindly gluing the two
/// separators together (as a plain global find-replace would) produces
/// exactly that. Otherwise, at most one comma survives and whitespace
/// collapses to a single space, preferring a newline if either side had one
/// (newlines carry "new paragraph"/"new line" structure and must survive).
fn merge_adjacent_separators(prev: &mut String, cur: &str) -> bool {
    let has_newline = prev.contains('\n') || cur.contains('\n');
    if let Some(end_char) = prev
        .chars()
        .chain(cur.chars())
        .find(|c| SENTENCE_END_CHARS.contains(c))
    {
        *prev = format!("{end_char}{}", if has_newline { '\n' } else { ' ' });
        return true;
    }
    let has_comma = prev.contains(',') || cur.contains(',');
    *prev = format!(
        "{}{}",
        if has_comma { "," } else { "" },
        if has_newline { '\n' } else { ' ' }
    );
    false
}

fn capitalize_first_char(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => word.to_string(),
    }
}

/// Removes standalone filler/hesitation words from `text` and cleans up the
/// spacing/punctuation left behind, without ever touching sentence-ending
/// punctuation or newlines (both structurally meaningful — e.g. "new
/// paragraph"/"new line" voice commands). If removing a filler word merges
/// two separators into a new sentence boundary (e.g. "...it. Um, next..."
/// -> "...it. next..."), the following word is re-capitalized so the
/// sentence still starts correctly.
pub fn strip_filler_hesitations(text: &str) -> String {
    let tokens = tokenize_words_and_other(text);

    let mut drop = vec![false; tokens.len()];
    let mut removed_any = false;
    for (index, tok) in tokens.iter().enumerate() {
        if let FillerTok::Word(word) = tok {
            if is_filler_hesitation_word(word) {
                drop[index] = true;
                removed_any = true;
            }
        }
    }
    // "You know" is meaningful in "you know the answer" and "let them
    // know". Remove it only at the end of a sentence or immediately before a
    // discourse connector, where it is unambiguously a filler.
    for index in 0..tokens.len().saturating_sub(2) {
        let is_you =
            matches!(&tokens[index], FillerTok::Word(word) if word.eq_ignore_ascii_case("you"));
        let is_know = matches!(&tokens[index + 2], FillerTok::Word(word) if word.eq_ignore_ascii_case("know"));
        if !is_you || !matches!(&tokens[index + 1], FillerTok::Other(_)) || !is_know {
            continue;
        }
        let after_know = index + 3;
        let next_word = match tokens.get(after_know) {
            None => Some(""),
            Some(FillerTok::Other(separator)) if separator.contains('?') => None,
            Some(FillerTok::Other(_)) => match tokens.get(after_know + 1) {
                Some(FillerTok::Word(word)) => Some(word.as_str()),
                _ => Some(""),
            },
            Some(FillerTok::Word(word)) => Some(word.as_str()),
        };
        let is_connector = next_word.is_some_and(|word| {
            matches!(
                word.to_ascii_lowercase().as_str(),
                "and" | "because" | "but" | "so" | "then"
            )
        });
        let end_of_sentence = next_word == Some("");
        if end_of_sentence || is_connector {
            drop[index] = true;
            drop[index + 1] = true;
            drop[index + 2] = true;
            removed_any = true;
        }
    }

    if !removed_any {
        return text.to_string();
    }

    let kept: Vec<FillerTok> = tokens
        .into_iter()
        .enumerate()
        .filter_map(|(index, token)| (!drop[index]).then_some(token))
        .collect();

    let mut merged: Vec<FillerTok> = Vec::with_capacity(kept.len());
    let mut recapitalize_next_word = false;
    for tok in kept {
        if let (Some(FillerTok::Other(prev)), FillerTok::Other(cur)) = (merged.last_mut(), &tok) {
            recapitalize_next_word = merge_adjacent_separators(prev, cur);
            continue;
        }
        if recapitalize_next_word {
            if let FillerTok::Word(w) = &tok {
                merged.push(FillerTok::Word(capitalize_first_char(w)));
                recapitalize_next_word = false;
                continue;
            }
        }
        merged.push(tok);
    }

    let mut out = String::with_capacity(text.len());
    for tok in &merged {
        match tok {
            FillerTok::Word(w) => out.push_str(w),
            FillerTok::Other(s) => out.push_str(s),
        }
    }
    out.trim_matches([' ', ',']).to_string()
}

/// Collapses a run of 6+ identical consecutive words down to a single
/// occurrence. Small local ASR models occasionally get stuck re-predicting
/// the same short token under acoustic ambiguity (a CTC/RNNT "repetition
/// collapse" failure) — no real speaker dictates the same word six-plus
/// times in a row, so this is always a transcription artifact, never
/// genuine content. This runs on the RAW transcription, before cleanup:
/// a run baked into the raw text is invisible to
/// `looks_like_degenerate_repetition`'s usefulness in one specific way — if
/// the cleanup model reproduces it too (plausible under "light" intensity,
/// which explicitly asks it to preserve wording verbatim) and the pipeline
/// falls back to the raw text as a safety net, that raw text still has the
/// run; there is nothing safer left to fall back to. Collapsing at the
/// source means a repetition glitch corrupts only the handful of words it
/// actually hit, instead of leaking downstream. Threshold matches
/// `looks_like_degenerate_repetition` in `api/prompts/cleanup_templates.rs`.
pub fn collapse_degenerate_word_runs(text: &str) -> String {
    const RUN_THRESHOLD: usize = 6;
    let tokens = tokenize_words_and_other(text);

    let word_positions: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| matches!(t, FillerTok::Word(_)).then_some(i))
        .collect();

    let mut drop = vec![false; tokens.len()];
    let mut any_dropped = false;
    let mut run_start = 0usize;
    while run_start < word_positions.len() {
        let first_word = match &tokens[word_positions[run_start]] {
            FillerTok::Word(w) => w.to_lowercase(),
            FillerTok::Other(_) => unreachable!("word_positions only indexes Word tokens"),
        };
        let mut run_end = run_start + 1;
        while run_end < word_positions.len() {
            let next_word = match &tokens[word_positions[run_end]] {
                FillerTok::Word(w) => w.to_lowercase(),
                FillerTok::Other(_) => unreachable!("word_positions only indexes Word tokens"),
            };
            if next_word != first_word {
                break;
            }
            run_end += 1;
        }
        if run_end - run_start >= RUN_THRESHOLD {
            for &idx in &word_positions[run_start + 1..run_end] {
                drop[idx] = true;
            }
            any_dropped = true;
        }
        run_start = run_end;
    }

    if !any_dropped {
        return text.to_string();
    }

    let kept: Vec<FillerTok> = tokens
        .into_iter()
        .enumerate()
        .filter_map(|(i, tok)| (!drop[i]).then_some(tok))
        .collect();

    let mut merged: Vec<FillerTok> = Vec::with_capacity(kept.len());
    let mut recapitalize_next_word = false;
    for tok in kept {
        if let (Some(FillerTok::Other(prev)), FillerTok::Other(cur)) = (merged.last_mut(), &tok) {
            recapitalize_next_word = merge_adjacent_separators(prev, cur);
            continue;
        }
        if recapitalize_next_word {
            if let FillerTok::Word(w) = &tok {
                merged.push(FillerTok::Word(capitalize_first_char(w)));
                recapitalize_next_word = false;
                continue;
            }
        }
        merged.push(tok);
    }

    let mut out = String::with_capacity(text.len());
    for tok in &merged {
        match tok {
            FillerTok::Word(w) => out.push_str(w),
            FillerTok::Other(s) => out.push_str(s),
        }
    }
    // Unlike strip_filler_hesitations, a run can be dropped at the very end
    // of the text, leaving a trailing separator with nothing after it to
    // merge into — trim both ends here.
    out.trim_matches([' ', ',']).to_string()
}

pub fn is_number_word_token(token: &str) -> bool {
    matches!(
        token,
        "zero"
            | "one"
            | "two"
            | "three"
            | "four"
            | "five"
            | "six"
            | "seven"
            | "eight"
            | "nine"
            | "ten"
            | "eleven"
            | "twelve"
            | "thirteen"
            | "fourteen"
            | "fifteen"
            | "sixteen"
            | "seventeen"
            | "eighteen"
            | "nineteen"
            | "twenty"
            | "thirty"
            | "forty"
            | "fifty"
            | "sixty"
            | "seventy"
            | "eighty"
            | "ninety"
            | "hundred"
            | "thousand"
            | "million"
            | "billion"
            | "trillion"
            | "first"
            | "second"
            | "third"
            | "fourth"
            | "fifth"
            | "sixth"
            | "seventh"
            | "eighth"
            | "ninth"
            | "tenth"
            | "eleventh"
            | "twelfth"
            | "thirteenth"
            | "fourteenth"
            | "fifteenth"
            | "sixteenth"
            | "seventeenth"
            | "eighteenth"
            | "nineteenth"
            | "twentieth"
            | "thirtieth"
            | "fortieth"
            | "fiftieth"
            | "sixtieth"
            | "seventieth"
            | "eightieth"
            | "ninetieth"
            | "hundredth"
            | "thousandth"
            | "millionth"
            | "billionth"
            | "trillionth"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        collapse_degenerate_word_runs, strip_filler_hesitations, strip_unspoken_em_dashes,
    };

    #[test]
    fn strips_an_em_dash_the_model_introduced_mid_sentence() {
        let raw = "wait actually I changed my mind";
        let cleaned = "Wait\u{2014}actually, I changed my mind.";
        assert_eq!(
            strip_unspoken_em_dashes(raw, cleaned),
            "Wait, actually, I changed my mind."
        );
    }

    #[test]
    fn drops_a_dangling_em_dash_at_the_end_instead_of_a_trailing_comma() {
        let raw = "something";
        let cleaned = "Something\u{2014}";
        assert_eq!(strip_unspoken_em_dashes(raw, cleaned), "Something");
    }

    #[test]
    fn joins_a_parenthetical_pair_of_em_dashes_into_commas() {
        let raw = "my brother john called";
        let cleaned = "My brother\u{2014}John\u{2014}called.";
        assert_eq!(
            strip_unspoken_em_dashes(raw, cleaned),
            "My brother, John, called."
        );
    }

    #[test]
    fn leaves_cleaned_text_unchanged_when_the_speaker_already_used_an_em_dash() {
        let raw = "the meeting\u{2014}if it happens\u{2014}is at noon";
        let cleaned = "The meeting\u{2014}if it happens\u{2014}is at noon.";
        assert_eq!(strip_unspoken_em_dashes(raw, cleaned), cleaned);
    }

    #[test]
    fn leaves_text_without_any_em_dash_unchanged() {
        let raw = "hello there";
        let cleaned = "Hello there.";
        assert_eq!(strip_unspoken_em_dashes(raw, cleaned), cleaned);
    }

    #[test]
    fn strips_a_bare_um_left_in_by_the_model() {
        // The actual observed bug: the model correctly removed one "um" but
        // left this one untouched in the same output.
        assert_eq!(
            strip_filler_hesitations("Just have um basically sync users"),
            "Just have basically sync users"
        );
    }

    #[test]
    fn strips_multiple_filler_words_in_one_pass() {
        assert_eq!(
            strip_filler_hesitations("So uh I think, ah, we should go um now"),
            "So I think, we should go now"
        );
    }

    #[test]
    fn strips_a_leading_filler_and_the_orphaned_comma() {
        assert_eq!(
            strip_filler_hesitations("Um, I think we should leave"),
            "I think we should leave"
        );
    }

    #[test]
    fn does_not_touch_words_that_merely_contain_a_filler_as_a_substring() {
        assert_eq!(
            strip_filler_hesitations("The umbrella and the album are here"),
            "The umbrella and the album are here"
        );
    }

    #[test]
    fn does_not_strip_like_since_it_has_common_non_filler_meanings() {
        assert_eq!(
            strip_filler_hesitations("I like pizza and things like that"),
            "I like pizza and things like that"
        );
    }

    #[test]
    fn strips_you_know_only_as_a_clear_discourse_filler() {
        assert_eq!(
            strip_filler_hesitations("There will be traffic you know"),
            "There will be traffic"
        );
        assert_eq!(
            strip_filler_hesitations("There will be traffic you know, because it is raining"),
            "There will be traffic, because it is raining"
        );
    }

    #[test]
    fn preserves_semantic_you_know_phrases() {
        assert_eq!(
            strip_filler_hesitations("You know the answer"),
            "You know the answer"
        );
        assert_eq!(strip_filler_hesitations("You know?"), "You know?");
        assert_eq!(
            strip_filler_hesitations("Please let them know tomorrow"),
            "Please let them know tomorrow"
        );
    }

    #[test]
    fn never_collapses_sentence_ending_punctuation_or_newlines() {
        assert_eq!(
            strip_filler_hesitations("That's it. Um, next point.\nNew line here."),
            "That's it. Next point.\nNew line here."
        );
    }

    #[test]
    fn returns_input_unchanged_when_no_filler_words_present() {
        let text = "This sentence has no hesitation words at all.";
        assert_eq!(strip_filler_hesitations(text), text);
    }

    #[test]
    fn collapses_a_stuck_asr_run_of_a_single_letter_word() {
        // Observed live: Parakeet got stuck and emitted "d" nine times in a
        // row in the middle of an otherwise normal dictation.
        assert_eq!(
            collapse_degenerate_word_runs(
                "Make sure if you see any d d d d d d d d d dictation with this you tell me"
            ),
            "Make sure if you see any d dictation with this you tell me"
        );
    }

    #[test]
    fn collapses_repeated_cjk_characters() {
        assert_eq!(collapse_degenerate_word_runs("啊啊啊啊啊啊"), "啊");
    }

    #[test]
    fn collapses_a_run_regardless_of_case() {
        assert_eq!(
            collapse_degenerate_word_runs("the The the THE the the ok"),
            "the ok"
        );
    }

    #[test]
    fn leaves_a_short_run_below_threshold_untouched() {
        let text = "the the the cat sat down";
        assert_eq!(collapse_degenerate_word_runs(text), text);
    }

    #[test]
    fn returns_input_unchanged_when_no_run_present() {
        let text = "This is a perfectly normal dictation with no repeats.";
        assert_eq!(collapse_degenerate_word_runs(text), text);
    }

    #[test]
    fn collapses_a_run_at_the_very_start_of_the_text() {
        assert_eq!(
            collapse_degenerate_word_runs("no no no no no no no way that happened"),
            "no way that happened"
        );
    }

    #[test]
    fn collapses_a_run_at_the_very_end_of_the_text() {
        assert_eq!(
            collapse_degenerate_word_runs("that is so weird stop stop stop stop stop stop stop"),
            "that is so weird stop"
        );
    }

    #[test]
    fn collapses_two_separate_runs_in_the_same_text() {
        assert_eq!(
            collapse_degenerate_word_runs(
                "go go go go go go left then stop stop stop stop stop stop right"
            ),
            "go left then stop right"
        );
    }
}
