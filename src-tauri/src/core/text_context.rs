#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SentenceContext {
    NewSentence,
    MidSentence,
    Unknown,
}

impl SentenceContext {
    pub fn as_str(self) -> &'static str {
        match self {
            SentenceContext::NewSentence => "new_sentence",
            SentenceContext::MidSentence => "mid_sentence",
            SentenceContext::Unknown => "unknown",
        }
    }

    pub fn should_capitalize(self) -> bool {
        !matches!(self, SentenceContext::MidSentence)
    }

    pub fn should_add_space(self) -> bool {
        matches!(self, SentenceContext::MidSentence)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectionPrefixClass {
    PlainWordStart,
    SoftPunctuationPrefix,
    HardSentenceTerminator,
    InvisibleOrAmbiguousPrefix,
}

impl InjectionPrefixClass {
    pub fn as_str(self) -> &'static str {
        match self {
            InjectionPrefixClass::PlainWordStart => "plain_word_start",
            InjectionPrefixClass::SoftPunctuationPrefix => "soft_punctuation_prefix",
            InjectionPrefixClass::HardSentenceTerminator => "hard_sentence_terminator",
            InjectionPrefixClass::InvisibleOrAmbiguousPrefix => "invisible_or_ambiguous_prefix",
        }
    }
}

#[cfg(test)]
fn is_sentence_ender(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '\n' | '\r')
}

#[cfg(test)]
fn trim_context_tail(context: &str) -> &str {
    context.trim_end_matches(|c: char| c.is_whitespace() && !is_sentence_ender(c))
}

#[cfg(test)]
pub fn classify_local_context(context: &str) -> SentenceContext {
    let trimmed = trim_context_tail(context);
    if trimmed.is_empty()
        || trimmed
            .chars()
            .next_back()
            .map(is_sentence_ender)
            .unwrap_or(true)
    {
        SentenceContext::NewSentence
    } else {
        SentenceContext::MidSentence
    }
}

fn is_invisible_prefix_char(ch: char) -> bool {
    ch.is_whitespace()
        || ch.is_control()
        || matches!(ch, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}')
}

fn is_context_tail_wrapper_char(ch: char) -> bool {
    matches!(
        ch,
        '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '"' | '\''
            | '\u{201C}' | '\u{201D}'
            | '\u{2018}' | '\u{2019}'
            | '\u{00AB}' | '\u{00BB}'
            | '\u{2039}' | '\u{203A}'
    )
}

fn is_soft_punctuation_prefix_char(ch: char) -> bool {
    matches!(
        ch,
        ',' | ';'
            | ':'
            | '-'
            | '–'
            | '—'
            | '/'
            | '\\'
            | ')'
            | ']'
            | '}'
            | '>'
            | '('
            | '['
            | '{'
            | '<'
            | '"'
            | '\''
            | '“'
            | '”'
            | '‘'
            | '’'
            | '«'
            | '»'
            | '‹'
            | '›'
            | '…'
    )
}

pub fn classify_leading_prefix(text: &str) -> InjectionPrefixClass {
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.peek().copied() {
        if is_invisible_prefix_char(ch) {
            chars.next();
        } else {
            break;
        }
    }

    let Some(first_visible) = chars.next() else {
        return InjectionPrefixClass::InvisibleOrAmbiguousPrefix;
    };

    if first_visible == '.' {
        if matches!(chars.peek().copied(), Some('.')) {
            return InjectionPrefixClass::SoftPunctuationPrefix;
        }
        return InjectionPrefixClass::HardSentenceTerminator;
    }
    if matches!(first_visible, '!' | '?' ) {
        return InjectionPrefixClass::HardSentenceTerminator;
    }
    if is_soft_punctuation_prefix_char(first_visible) {
        return InjectionPrefixClass::SoftPunctuationPrefix;
    }
    if first_visible.is_alphanumeric() {
        return InjectionPrefixClass::PlainWordStart;
    }

    InjectionPrefixClass::InvisibleOrAmbiguousPrefix
}

pub fn classify_context_tail(text: &str) -> SentenceContext {
    for ch in text.chars().rev() {
        if is_invisible_prefix_char(ch) || is_context_tail_wrapper_char(ch) {
            continue;
        }
        if matches!(ch, '.' | '!' | '?' | '\n' | '\r') {
            return SentenceContext::NewSentence;
        }
        if ch.is_alphanumeric()
            || matches!(
                ch,
                ',' | ';' | ':' | '-' | '–' | '—' | '/' | '\\'
            )
        {
            return SentenceContext::MidSentence;
        }
        return SentenceContext::Unknown;
    }

    SentenceContext::Unknown
}

fn transform_first_cased_char(text: &str, capitalize: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut transformed = false;

    for ch in text.chars() {
        if !transformed && ch.is_alphabetic() {
            if capitalize {
                out.extend(ch.to_uppercase());
            } else {
                out.extend(ch.to_lowercase());
            }
            transformed = true;
        } else {
            out.push(ch);
        }
    }

    if transformed {
        out
    } else {
        text.to_owned()
    }
}

pub fn apply_contextual_casing(text: &str, context: SentenceContext) -> String {
    transform_first_cased_char(text, context.should_capitalize())
}

pub fn should_add_injection_space(context: SentenceContext, prefix: InjectionPrefixClass) -> bool {
    matches!(prefix, InjectionPrefixClass::PlainWordStart) && context.should_add_space()
}

fn is_opening_prefix_char(ch: char) -> bool {
    matches!(
        ch,
        '(' | '[' | '{' | '<' | '"' | '\'' | '“' | '‘' | '«' | '‹'
    )
}

fn first_visible_char(text: &str) -> Option<char> {
    text.chars().find(|ch| !is_invisible_prefix_char(*ch))
}

fn tail_ends_with_visible_char(text: &str) -> bool {
    text.chars()
        .next_back()
        .map(|ch| !is_invisible_prefix_char(ch))
        .unwrap_or(false)
}

pub fn should_add_leading_injection_space(
    text: &str,
    context: SentenceContext,
    prefix: InjectionPrefixClass,
    source_is_caret_local: bool,
    context_tail: &str,
) -> bool {
    if !source_is_caret_local {
        return false;
    }
    if !tail_ends_with_visible_char(context_tail) {
        return false;
    }

    match prefix {
        InjectionPrefixClass::PlainWordStart => {
            matches!(context, SentenceContext::MidSentence | SentenceContext::NewSentence)
        }
        InjectionPrefixClass::SoftPunctuationPrefix => {
            matches!(first_visible_char(text), Some(ch) if is_opening_prefix_char(ch))
        }
        InjectionPrefixClass::HardSentenceTerminator
        | InjectionPrefixClass::InvisibleOrAmbiguousPrefix => false,
    }
}

pub fn should_capitalize_injection(context: SentenceContext, prefix: InjectionPrefixClass) -> bool {
    match prefix {
        InjectionPrefixClass::PlainWordStart => context.should_capitalize(),
        InjectionPrefixClass::HardSentenceTerminator => true,
        InjectionPrefixClass::SoftPunctuationPrefix
        | InjectionPrefixClass::InvisibleOrAmbiguousPrefix => false,
    }
}

pub fn format_injection_text(
    text: &str,
    context: SentenceContext,
    prefix: InjectionPrefixClass,
) -> String {
    match prefix {
        InjectionPrefixClass::PlainWordStart => apply_contextual_casing(text, context),
        InjectionPrefixClass::HardSentenceTerminator => transform_first_cased_char(text, true),
        InjectionPrefixClass::SoftPunctuationPrefix
        | InjectionPrefixClass::InvisibleOrAmbiguousPrefix => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_context_capitalizes() {
        assert_eq!(classify_local_context(""), SentenceContext::NewSentence);
        assert!(SentenceContext::NewSentence.should_capitalize());
        assert!(!SentenceContext::NewSentence.should_add_space());
    }

    #[test]
    fn unknown_context_capitalizes() {
        assert!(SentenceContext::Unknown.should_capitalize());
        assert!(!SentenceContext::Unknown.should_add_space());
    }

    #[test]
    fn sentence_enders_capitalize() {
        assert_eq!(classify_local_context("hello."), SentenceContext::NewSentence);
        assert_eq!(classify_local_context("hello?"), SentenceContext::NewSentence);
        assert_eq!(classify_local_context("hello!"), SentenceContext::NewSentence);
        assert_eq!(classify_local_context("hello.\n"), SentenceContext::NewSentence);
    }

    #[test]
    fn mid_sentence_context_lowercases() {
        assert_eq!(classify_local_context("hello"), SentenceContext::MidSentence);
        assert_eq!(classify_local_context("hello,"), SentenceContext::MidSentence);
        assert_eq!(classify_local_context("hello /"), SentenceContext::MidSentence);
    }

    #[test]
    fn context_tail_ignores_trailing_spaces_and_wrappers() {
        assert_eq!(classify_context_tail("hi "), SentenceContext::MidSentence);
        assert_eq!(classify_context_tail("hi\""), SentenceContext::MidSentence);
        assert_eq!(classify_context_tail("hi)"), SentenceContext::MidSentence);
        assert_eq!(classify_context_tail("hi.\""), SentenceContext::NewSentence);
        assert_eq!(classify_context_tail("hi.\u{200B} "), SentenceContext::NewSentence);
        assert_eq!(classify_context_tail("\u{200B}  "), SentenceContext::Unknown);
    }

    #[test]
    fn leading_prefix_classifier_finds_plain_words() {
        assert_eq!(
            classify_leading_prefix("hello"),
            InjectionPrefixClass::PlainWordStart
        );
        assert_eq!(
            classify_leading_prefix("  hello"),
            InjectionPrefixClass::PlainWordStart
        );
    }

    #[test]
    fn leading_prefix_classifier_finds_soft_punctuation() {
        assert_eq!(
            classify_leading_prefix(", hello"),
            InjectionPrefixClass::SoftPunctuationPrefix
        );
        assert_eq!(
            classify_leading_prefix("\"hello\""),
            InjectionPrefixClass::SoftPunctuationPrefix
        );
        assert_eq!(
            classify_leading_prefix("(hello"),
            InjectionPrefixClass::SoftPunctuationPrefix
        );
        assert_eq!(
            classify_leading_prefix("...hello"),
            InjectionPrefixClass::SoftPunctuationPrefix
        );
        assert_eq!(
            classify_leading_prefix("…hello"),
            InjectionPrefixClass::SoftPunctuationPrefix
        );
    }

    #[test]
    fn leading_prefix_classifier_finds_sentence_terminators() {
        assert_eq!(
            classify_leading_prefix("!hello"),
            InjectionPrefixClass::HardSentenceTerminator
        );
        assert_eq!(
            classify_leading_prefix("?hello"),
            InjectionPrefixClass::HardSentenceTerminator
        );
        assert_eq!(
            classify_leading_prefix(".hello"),
            InjectionPrefixClass::HardSentenceTerminator
        );
    }

    #[test]
    fn leading_prefix_classifier_treats_invisible_only_input_as_ambiguous() {
        assert_eq!(
            classify_leading_prefix(""),
            InjectionPrefixClass::InvisibleOrAmbiguousPrefix
        );
        assert_eq!(
            classify_leading_prefix("\u{200B}\u{FEFF}"),
            InjectionPrefixClass::InvisibleOrAmbiguousPrefix
        );
    }

    #[test]
    fn trailing_whitespace_after_sentence_end_keeps_capitalization() {
        assert_eq!(classify_local_context("hello. "), SentenceContext::NewSentence);
        assert_eq!(classify_local_context("hello?\t"), SentenceContext::NewSentence);
    }

    #[test]
    fn spacing_follows_sentence_context() {
        assert!(!SentenceContext::NewSentence.should_add_space());
        assert!(SentenceContext::MidSentence.should_add_space());
        assert!(!SentenceContext::Unknown.should_add_space());
    }

    #[test]
    fn contextual_casing_capitalizes_sentence_starts() {
        assert_eq!(
            apply_contextual_casing("hello world", SentenceContext::NewSentence),
            "Hello world"
        );
        assert_eq!(
            apply_contextual_casing("\"hello world\"", SentenceContext::NewSentence),
            "\"Hello world\""
        );
        assert_eq!(
            apply_contextual_casing("123 hello", SentenceContext::NewSentence),
            "123 Hello"
        );
    }

    #[test]
    fn contextual_casing_lowercases_mid_sentence_starts() {
        assert_eq!(
            apply_contextual_casing("Hello world", SentenceContext::MidSentence),
            "hello world"
        );
        assert_eq!(
            apply_contextual_casing("\"Hello world\"", SentenceContext::MidSentence),
            "\"hello world\""
        );
        assert_eq!(
            apply_contextual_casing("123 Hello", SentenceContext::MidSentence),
            "123 hello"
        );
    }

    #[test]
    fn punctuation_prefixes_keep_model_casing_and_skip_extra_spacing() {
        let prefix = classify_leading_prefix("\"Hello\"");
        assert_eq!(prefix, InjectionPrefixClass::SoftPunctuationPrefix);
        assert_eq!(
            format_injection_text("\"Hello\"", SentenceContext::MidSentence, prefix),
            "\"Hello\""
        );
        assert!(!should_add_injection_space(SentenceContext::MidSentence, prefix));
        assert!(!should_capitalize_injection(SentenceContext::MidSentence, prefix));
    }

    #[test]
    fn sentence_boundary_adds_space_before_plain_words() {
        assert!(should_add_leading_injection_space(
            "Hello world",
            SentenceContext::NewSentence,
            InjectionPrefixClass::PlainWordStart,
            true,
            "hello.",
        ));
        assert!(should_add_leading_injection_space(
            "Hello world",
            SentenceContext::MidSentence,
            InjectionPrefixClass::PlainWordStart,
            true,
            "hello",
        ));
        assert!(!should_add_leading_injection_space(
            "Hello world",
            SentenceContext::NewSentence,
            InjectionPrefixClass::PlainWordStart,
            false,
            "hello.",
        ));
        assert!(!should_add_leading_injection_space(
            "Hello world",
            SentenceContext::NewSentence,
            InjectionPrefixClass::PlainWordStart,
            true,
            "hello ",
        ));
    }

    #[test]
    fn sentence_boundary_adds_space_before_opening_punctuation() {
        assert!(should_add_leading_injection_space(
            "\"Hello\"",
            SentenceContext::NewSentence,
            InjectionPrefixClass::SoftPunctuationPrefix,
            true,
            "hello.",
        ));
        assert!(should_add_leading_injection_space(
            "(Hello)",
            SentenceContext::NewSentence,
            InjectionPrefixClass::SoftPunctuationPrefix,
            true,
            "hello.",
        ));
        assert!(!should_add_leading_injection_space(
            ", hello",
            SentenceContext::NewSentence,
            InjectionPrefixClass::SoftPunctuationPrefix,
            true,
            "hello.",
        ));
        assert!(!should_add_leading_injection_space(
            "\"Hello\"",
            SentenceContext::NewSentence,
            InjectionPrefixClass::SoftPunctuationPrefix,
            false,
            "hello.",
        ));
        assert!(!should_add_leading_injection_space(
            "\"Hello\"",
            SentenceContext::NewSentence,
            InjectionPrefixClass::SoftPunctuationPrefix,
            true,
            "hello ",
        ));
    }

    #[test]
    fn comma_prefix_keeps_model_casing_and_skips_leading_space() {
        let prefix = classify_leading_prefix(", I think");
        assert_eq!(prefix, InjectionPrefixClass::SoftPunctuationPrefix);
        assert_eq!(
            format_injection_text(", I think", SentenceContext::NewSentence, prefix),
            ", I think"
        );
        assert!(!should_add_injection_space(SentenceContext::MidSentence, prefix));
    }

    #[test]
    fn dash_and_ellipsis_prefixes_preserve_model_casing() {
        let dash_prefix = classify_leading_prefix("\u{2014}Hello");
        assert_eq!(dash_prefix, InjectionPrefixClass::SoftPunctuationPrefix);
        assert_eq!(
            format_injection_text("\u{2014}Hello", SentenceContext::MidSentence, dash_prefix),
            "\u{2014}Hello"
        );

        let ellipsis_prefix = classify_leading_prefix("\u{2026}Hello");
        assert_eq!(ellipsis_prefix, InjectionPrefixClass::SoftPunctuationPrefix);
        assert_eq!(
            format_injection_text(
                "\u{2026}Hello",
                SentenceContext::MidSentence,
                ellipsis_prefix
            ),
            "\u{2026}Hello"
        );
    }

    #[test]
    fn hard_terminators_still_capitalize_after_they_start_the_chunk() {
        let prefix = classify_leading_prefix("!hello");
        assert_eq!(prefix, InjectionPrefixClass::HardSentenceTerminator);
        assert_eq!(
            format_injection_text("!hello", SentenceContext::MidSentence, prefix),
            "!Hello"
        );
        assert!(should_capitalize_injection(SentenceContext::MidSentence, prefix));
        assert!(!should_add_injection_space(SentenceContext::MidSentence, prefix));
    }
}
