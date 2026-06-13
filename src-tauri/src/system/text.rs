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
