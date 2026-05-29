use crate::system::text::is_number_word_token;

#[cfg(test)]
pub fn normalize_cleanup_cache_key(input: &str) -> String {
    let (tokens, separators) = tokenize_cache_key_parts(input);
    normalize_cleanup_cache_key_parts(&tokens, &separators)
}

pub fn normalize_cleanup_cache_key_parts(tokens: &[String], separators: &[String]) -> String {
    let mut out = String::new();
    let mut i = 0;

    while i < tokens.len() {
        let token = &tokens[i];
        if matches!(token.as_str(), "minus" | "negative")
            && i + 1 < tokens.len()
            && tokens[i + 1].chars().any(|c| c.is_ascii_digit())
        {
            let mut normalized = normalize_digit_token(&tokens[i + 1]);
            let mut j = i + 2;
            while j < tokens.len() && tokens[j].chars().any(|c| c.is_ascii_digit()) {
                let sep = separators.get(j).map(|s| s.trim()).unwrap_or("");
                if sep == "." {
                    normalized.push('.');
                    normalized.push_str(&normalize_digit_token(&tokens[j]));
                    j += 1;
                    continue;
                }
                if sep == ":" {
                    normalized.push_str(&normalize_digit_token(&tokens[j]));
                    j += 1;
                    continue;
                }
                if can_merge_thousands_group(sep, &normalized, &tokens[j]) {
                    normalized.push_str(&normalize_digit_token(&tokens[j]));
                    j += 1;
                    continue;
                }
                break;
            }
            out.push_str("num-");
            out.push_str(&normalized);
            i = j;
            continue;
        }

        if token.chars().any(|c| c.is_ascii_digit()) {
            let mut normalized = normalize_digit_token(token);
            let negative = has_numeric_minus_prefix(tokens, separators, i);
            let mut j = i + 1;
            while j < tokens.len() && tokens[j].chars().any(|c| c.is_ascii_digit()) {
                let sep = separators.get(j).map(|s| s.trim()).unwrap_or("");
                if sep == "." {
                    normalized.push('.');
                    normalized.push_str(&normalize_digit_token(&tokens[j]));
                    j += 1;
                    continue;
                }
                if sep == ":" {
                    normalized.push_str(&normalize_digit_token(&tokens[j]));
                    j += 1;
                    continue;
                }
                if can_merge_thousands_group(sep, &normalized, &tokens[j]) {
                    normalized.push_str(&normalize_digit_token(&tokens[j]));
                    j += 1;
                    continue;
                }
                break;
            }
            out.push_str("num");
            if negative {
                out.push('-');
            }
            out.push_str(&normalized);
            i = j;
            continue;
        }

        if let Some((normalized, next_idx)) = normalize_number_word_run(tokens, i) {
            out.push_str("num");
            out.push_str(&normalized);
            i = next_idx;
            continue;
        }

        out.push_str(token);
        i += 1;
    }

    out
}

pub fn tokenize_cache_key_parts(input: &str) -> (Vec<String>, Vec<String>) {
    let mut tokens = Vec::new();
    let mut separators = Vec::new();
    let mut buf = String::new();
    let mut sep_buf = String::new();

    for ch in input.chars() {
        if ch.is_alphanumeric() {
            if buf.is_empty() {
                separators.push(std::mem::take(&mut sep_buf));
            }
            buf.extend(ch.to_lowercase());
            continue;
        }

        if !buf.is_empty() {
            tokens.push(std::mem::take(&mut buf));
        }
        sep_buf.push(ch);
    }

    if !buf.is_empty() {
        tokens.push(buf);
    }

    (tokens, separators)
}

fn can_merge_thousands_group(sep: &str, normalized: &str, next_token: &str) -> bool {
    sep == ","
        && !normalized.contains('.')
        && next_token.chars().all(|c| c.is_ascii_digit())
        && next_token.len() == 3
}

fn has_numeric_minus_prefix(tokens: &[String], separators: &[String], idx: usize) -> bool {
    let Some(sep) = separators.get(idx) else {
        return false;
    };
    if !sep.trim_end().ends_with('-') {
        return false;
    }
    if idx == 0 {
        return true;
    }
    let prev = tokens[idx - 1].as_str();
    !(prev.chars().any(|c| c.is_ascii_digit()) || is_number_word_token(prev))
}

fn normalize_digit_token(token: &str) -> String {
    let digits: String = token.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        "0".to_string()
    } else {
        digits
    }
}

fn normalize_number_word_run(tokens: &[String], start: usize) -> Option<(String, usize)> {
    if start >= tokens.len() {
        return None;
    }

    let mut i = start;
    let mut negative = false;
    if matches!(tokens[i].as_str(), "minus" | "negative") {
        negative = true;
        i += 1;
    }
    if i >= tokens.len() || !is_number_word_token(&tokens[i]) {
        return None;
    }

    let (int_value, mut next, seen_any) = parse_number_word_integer(tokens, i);
    if !seen_any {
        return None;
    }

    let mut normalized = if negative {
        format!("-{int_value}")
    } else {
        int_value.to_string()
    };

    if next < tokens.len() && tokens[next] == "point" {
        let mut temp_next = next + 1;
        let mut frac = String::new();
        while temp_next < tokens.len() {
            let t = tokens[temp_next].as_str();
            if t.chars().all(|c| c.is_ascii_digit()) {
                frac.push_str(t);
                temp_next += 1;
                continue;
            }
            if t == "oh" {
                frac.push('0');
                temp_next += 1;
                continue;
            }
            if let Some(d) = unit_word_value(t) {
                frac.push(char::from(b'0' + d as u8));
                temp_next += 1;
                continue;
            }
            break;
        }
        if !frac.is_empty() {
            normalized.push('.');
            normalized.push_str(&frac);
            next = temp_next;
        }
    }

    Some((normalized, next))
}

fn parse_number_word_integer(tokens: &[String], mut i: usize) -> (i64, usize, bool) {
    let mut total: i64 = 0;
    let mut current: i64 = 0;
    let mut seen_any = false;
    let mut allow_and = false;

    while i < tokens.len() {
        let t = tokens[i].as_str();
        if t == "and" {
            if allow_and {
                allow_and = false;
                i += 1;
                continue;
            }
            break;
        }
        if let Some(v) = unit_word_value(t) {
            current = current.saturating_add(i64::from(v));
            seen_any = true;
            allow_and = false;
            i += 1;
            continue;
        }
        if let Some(v) = teen_or_tens_word_value(t) {
            current = current.saturating_add(i64::from(v));
            seen_any = true;
            allow_and = false;
            i += 1;
            continue;
        }
        if t == "hundred" {
            current = if current == 0 {
                100
            } else {
                current.saturating_mul(100)
            };
            seen_any = true;
            allow_and = true;
            i += 1;
            continue;
        }
        if let Some(scale) = large_scale_word_value(t) {
            let part = if current == 0 { 1 } else { current };
            total = total.saturating_add(part.saturating_mul(scale));
            current = 0;
            seen_any = true;
            allow_and = true;
            i += 1;
            continue;
        }
        if let Some(v) = ordinal_word_value(t) {
            current = current.saturating_add(i64::from(v));
            seen_any = true;
            allow_and = false;
            i += 1;
            continue;
        }
        break;
    }

    (total.saturating_add(current), i, seen_any)
}

fn unit_word_value(token: &str) -> Option<i32> {
    match token {
        "zero" => Some(0),
        "one" | "first" => Some(1),
        "two" | "second" => Some(2),
        "three" | "third" => Some(3),
        "four" | "fourth" => Some(4),
        "five" | "fifth" => Some(5),
        "six" | "sixth" => Some(6),
        "seven" | "seventh" => Some(7),
        "eight" | "eighth" => Some(8),
        "nine" | "ninth" => Some(9),
        _ => None,
    }
}

fn teen_or_tens_word_value(token: &str) -> Option<i32> {
    match token {
        "ten" | "tenth" => Some(10),
        "eleven" | "eleventh" => Some(11),
        "twelve" | "twelfth" => Some(12),
        "thirteen" | "thirteenth" => Some(13),
        "fourteen" | "fourteenth" => Some(14),
        "fifteen" | "fifteenth" => Some(15),
        "sixteen" | "sixteenth" => Some(16),
        "seventeen" | "seventeenth" => Some(17),
        "eighteen" | "eighteenth" => Some(18),
        "nineteen" | "nineteenth" => Some(19),
        "twenty" | "twentieth" => Some(20),
        "thirty" | "thirtieth" => Some(30),
        "forty" | "fortieth" => Some(40),
        "fifty" | "fiftieth" => Some(50),
        "sixty" | "sixtieth" => Some(60),
        "seventy" | "seventieth" => Some(70),
        "eighty" | "eightieth" => Some(80),
        "ninety" | "ninetieth" => Some(90),
        _ => None,
    }
}

fn large_scale_word_value(token: &str) -> Option<i64> {
    match token {
        "thousand" | "thousandth" => Some(1_000),
        "million" | "millionth" => Some(1_000_000),
        "billion" | "billionth" => Some(1_000_000_000),
        "trillion" | "trillionth" => Some(1_000_000_000_000),
        _ => None,
    }
}

fn ordinal_word_value(token: &str) -> Option<i32> {
    match token {
        "hundredth" => Some(100),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_cleanup_cache_key;

    #[test]
    fn cache_key_normalizes_digit_vs_word_numbers() {
        let a = normalize_cleanup_cache_key("I have 12 apples");
        let b = normalize_cleanup_cache_key("I have twelve apples");
        assert_eq!(a, b);
    }

    #[test]
    fn cache_key_normalizes_decimal_digit_vs_word_form() {
        let a = normalize_cleanup_cache_key("version 2.5");
        let b = normalize_cleanup_cache_key("version two point five");
        assert_eq!(a, b);
    }

    #[test]
    fn cache_key_keeps_point_when_not_followed_by_fraction_digits() {
        let a = normalize_cleanup_cache_key("one point");
        let b = normalize_cleanup_cache_key("one");
        assert_ne!(a, b);
        assert!(a.contains("point"));
    }

    #[test]
    fn cache_key_normalizes_time_and_date_like_forms() {
        let a = normalize_cleanup_cache_key("meet at 10:30 on 20260517");
        let b = normalize_cleanup_cache_key("meet at 1030 on 20260517");
        assert_eq!(a, b);
    }

    #[test]
    fn cache_key_keeps_non_numeric_text_distinct() {
        let a = normalize_cleanup_cache_key("model x");
        let b = normalize_cleanup_cache_key("model y");
        assert_ne!(a, b);
    }

    #[test]
    fn cache_key_still_ignores_case_and_punctuation() {
        let a = normalize_cleanup_cache_key("Hello, WORLD!");
        let b = normalize_cleanup_cache_key("hello world");
        assert_eq!(a, b);
    }

    #[test]
    fn cache_key_matches_digit_and_word_same_number() {
        let a = normalize_cleanup_cache_key("What's 45 plus 45?");
        let b = normalize_cleanup_cache_key("What's forty five plus forty five?");
        assert_eq!(a, b);
    }

    #[test]
    fn cache_key_distinguishes_different_numeric_values() {
        let a = normalize_cleanup_cache_key("What's 45 plus 45?");
        let b = normalize_cleanup_cache_key("What's 6 plus 6?");
        assert_ne!(a, b);
    }

    #[test]
    fn cache_key_distinguishes_decimal_from_whole_number() {
        let a = normalize_cleanup_cache_key("version 2.5");
        let b = normalize_cleanup_cache_key("version 25");
        assert_ne!(a, b);
    }

    #[test]
    fn cache_key_distinguishes_negative_and_positive_digits() {
        let a = normalize_cleanup_cache_key("temperature is -5");
        let b = normalize_cleanup_cache_key("temperature is 5");
        assert_ne!(a, b);
    }

    #[test]
    fn cache_key_matches_negative_digit_and_word_forms() {
        let a = normalize_cleanup_cache_key("temperature is -5");
        let b = normalize_cleanup_cache_key("temperature is minus five");
        assert_eq!(a, b);
    }

    #[test]
    fn cache_key_does_not_merge_comma_separated_digits() {
        let a = normalize_cleanup_cache_key("What's 4, 5 plus 4, 5?");
        let b = normalize_cleanup_cache_key("What's 45 plus 45?");
        assert_ne!(a, b);
    }

    #[test]
    fn cache_key_merges_thousands_separators_without_spaces() {
        let a = normalize_cleanup_cache_key("population is 1,000,000");
        let b = normalize_cleanup_cache_key("population is 1000000");
        assert_eq!(a, b);
    }

    #[test]
    fn cache_key_does_not_collapse_one_and_two_to_three() {
        let a = normalize_cleanup_cache_key("one and two");
        let b = normalize_cleanup_cache_key("three");
        assert_ne!(a, b);
    }

    #[test]
    fn cache_key_keeps_hundred_and_form_equivalent() {
        let a = normalize_cleanup_cache_key("one hundred and two");
        let b = normalize_cleanup_cache_key("102");
        assert_eq!(a, b);
    }

    #[test]
    fn cache_key_handles_large_number_word_runs_without_overflow() {
        let key = normalize_cleanup_cache_key(
            "one hundred hundred hundred hundred hundred hundred hundred hundred hundred hundred",
        );
        assert!(key.starts_with("num"));
    }
}
