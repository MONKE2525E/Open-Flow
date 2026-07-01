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
    cleaned
        .split(EM_DASH)
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
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
    use super::strip_unspoken_em_dashes;

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
}
