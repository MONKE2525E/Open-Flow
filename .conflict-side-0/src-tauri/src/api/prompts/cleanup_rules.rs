use super::{count_words, PromptTier};

pub(super) const FORMATTING_RULES: &str = r#"<formatting>
- Execute spoken paragraph, line, list, punctuation, symbol, capitalization, and spacing commands, then remove the command words. Accept equivalent wording. Preserve such phrases when discussed, quoted, or named.
- Commands include comma, period/full stop, colon, semicolon, question/exclamation mark; open/close quote, parenthesis, bracket, brace; slash, backslash, pipe, underscore, hyphen, dash, em dash, percent, ampersand; all caps on/off, no space on/off, and numeral.
- Split distinct ideas into paragraphs separated by a blank line.
- Create a list only when explicitly requested or when several clearly parallel items are dictated as separate entries. Keep rhetorical "first" or "second" sequencing and ordinary prose as prose unless items are clearly enumerated.
- "Hyphen" or "dash" → "-"; "em dash" or an unambiguous equivalent → "—". Never introduce an em dash stylistically. Preserve punctuation and line breaks.
- In clear technical-token dictation, compact spoken at, dot, slash, backslash, colon, dash/hyphen, underscore, pipe, equals, plus, hash, or no space into an email, URL, path, command, filename, package, domain, variable, or identifier. Interpret these components specially only there; preserve correct tokens without restyling.
- For clear spelling, join dictated letters and digits. Honor explicitly spoken capitalization instructions and phonetic letter names. By default preserve the spelled sequence as letters; use conventional proper-name casing only when vocabulary or context strongly supports it. Never concatenate ambiguous sequences.
</formatting>"#;

fn intensity_rules(
    intensity: &str,
    tier: PromptTier,
    has_overrides: bool,
    profile: &str,
) -> String {
    let base = match (intensity, tier) {
        ("none", _) if profile == "formal" =>
            "Cleanup: Off. Keep wording and structure unchanged except for the formal profanity rule. Do not add content.".to_string(),
        ("none", _) =>
            "Cleanup: Off. Return the dictation unchanged, character-for-character.".to_string(),
        ("light", _) =>
            "Cleanup: Light. Remove filler words, immediate duplicates, and abandoned false starts only. Keep sentence order, personality, emphasis, qualifiers, and every distinct point. Do not summarize, paraphrase, pad, or noticeably change length.".to_string(),
        ("high", PromptTier::Short) =>
            "Cleanup: Strong. Lead with the main point and rewrite to the shortest clear version, aiming for about half the words unless already concise. Cut filler, false starts, repetition, throat-clearing, unnecessary setup, hedges, and weak qualifiers. Keep concrete facts, names, numbers, and required context.".to_string(),
        ("high", PromptTier::Medium) =>
            "Cleanup: Strong. Lead with the main point and target 30-50% of the words. Cut filler, false starts, repetition, circular phrasing, throat-clearing, unnecessary setup, hedges, and weak qualifiers. Keep concrete facts, names, numbers, and required context.".to_string(),
        ("high", PromptTier::Detailed) =>
            "Cleanup: Strong. Lead with the main point and target 30-50% of the words. Merge related sentences when useful, but preserve the speaker's sequence of reasoning unless the original is clearly scrambled. Cut filler, false starts, repetition, circular phrasing, throat-clearing, unnecessary setup, hedges, and weak qualifiers. Keep concrete facts, names, numbers, and required context.".to_string(),
        (_, PromptTier::Short) =>
            "Cleanup: Medium. Remove filler, repetition, rambling, false starts, and speech artifacts. Tighten roundabout wording into clear sentences while keeping meaning, specifics, personality, and intent. Do not reduce it to a terse summary.".to_string(),
        (_, PromptTier::Medium) =>
            "Cleanup: Medium. Remove filler, repetition, rambling loops, false starts, and speech artifacts. Smooth boundaries and merge adjacent thoughts when helpful. Reorder words or nearby clauses for grammar, but do not reorganize distinct ideas or change the speaker's reasoning sequence unless clearly scrambled. Preserve details, personality, and intent; do not aggressively compress.".to_string(),
        (_, PromptTier::Detailed) =>
            "Cleanup: Medium. Remove filler, repetition, rambling loops, false starts, and speech artifacts. Smooth boundaries and merge adjacent thoughts when helpful. Reorder words or nearby clauses for grammar, but do not reorganize distinct ideas or change the speaker's reasoning sequence unless clearly scrambled. Preserve details, important specifics, personality, and intent; do not aggressively compress.".to_string(),
    };

    if has_overrides {
        format!("{base}\nPriority: Final output overrides win if they conflict with this cleanup setting.")
    } else {
        base
    }
}

fn tone_rules(profile: &str) -> &'static str {
    match profile {
        "formal" => "Tone: Formal. Use professional wording, complete sentences and punctuation, standard capitalization, and expanded contractions.",
        "very_casual" => "Tone: Very casual. Use mostly lowercase, minimal punctuation, and contractions. Tone changes voice and casing only; cleanup intensity still controls what may be removed.",
        _ => "Tone: Casual. Use natural conversational wording, sentence capitalization, contractions, and normal punctuation.",
    }
}

fn profanity_policy(profile: &str, intensity: &str) -> String {
    if profile == "formal" {
        return "Profanity: Replace most profanity with professional wording while preserving meaning and emphasis. Do not use asterisk censorship."
            .to_string();
    }

    let intensity_label = match intensity {
        "none" => "Off",
        "light" => "Light",
        "high" => "Strong",
        _ => "Medium",
    };
    format!(
        "Profanity ({intensity_label}): Keep profanity and its intensity as spoken. Do not sanitize, euphemize, or censor it."
    )
}

fn number_style_block(tier: PromptTier, has_numeric_content: bool) -> String {
    if tier == PromptTier::Short && !has_numeric_content {
        String::new()
    } else {
        "Numbers: Preserve the speaker's apparent numeric style when it matters. In ordinary prose, spell out simple cardinal numbers below 10 and use digits for 10 or above. Prefer digits for technical discussion, comparisons, quantities, measurements, dates, times, versions, addresses, identifiers, and when the dictated form is clearly numeric."
            .to_string()
    }
}

pub(super) fn build_preset_block(
    profile: &str,
    intensity: &str,
    tier: PromptTier,
    has_numeric_content: bool,
    has_overrides: bool,
) -> String {
    let mut lines = vec![
        "<active_settings>".to_string(),
        intensity_rules(intensity, tier, has_overrides, profile),
        tone_rules(profile).to_string(),
        profanity_policy(profile, intensity),
    ];
    let number_style = number_style_block(tier, has_numeric_content);
    if !number_style.is_empty() {
        lines.push(number_style);
    }
    lines.push("</active_settings>".to_string());
    lines.join("\n")
}

fn to_imperative(raw: &str) -> String {
    let value = raw.trim();
    let value = value.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ')');
    let value = value.trim();

    if value.to_uppercase().starts_with("MUST") {
        return value.to_owned();
    }
    for negative in &["don't ", "do not ", "never ", "avoid "] {
        if value.to_lowercase().starts_with(negative) {
            let rest = &value[negative.len()..];
            let mut chars = rest.chars();
            let capitalized = match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            };
            return format!("MUST NOT {capitalized}");
        }
    }
    format!("MUST {value}")
}

pub(super) fn snippet_overrides_block(extra_rules: &str) -> String {
    if extra_rules.trim().is_empty() {
        return String::new();
    }

    let override_lines = extra_rules
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
        .map(|(index, line)| format!("{}. {}", index + 1, to_imperative(line)))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "<final_output_overrides>\n\
        These user-authored rules have final say over cleanup, tone, punctuation, and formatting.\n\
        {override_lines}\n\
        </final_output_overrides>"
    )
}

pub(super) fn render_cleanup_template(
    template: &str,
    active_app: &str,
    cleanup_preset: &str,
    formatting_rules: &str,
    snippet_overrides: &str,
) -> String {
    // Replace untrusted target context last. A window title containing a
    // template token must remain data rather than expanding into instructions.
    template
        .replace("{{ cleanup_preset }}", cleanup_preset)
        .replace("{{ formatting_rules }}", formatting_rules)
        .replace("{{ snippet_overrides }}", snippet_overrides)
        .replace("{{ active_app }}", active_app)
}

pub(super) fn collapse_blank_lines(value: &str) -> String {
    let value = value.replace("\r\n", "\n");
    let mut result = String::with_capacity(value.len());
    let mut newline_run = 0;
    for character in value.chars() {
        if character == '\n' {
            newline_run += 1;
            if newline_run <= 2 {
                result.push(character);
            }
        } else {
            newline_run = 0;
            result.push(character);
        }
    }
    result.trim_end().to_string()
}

pub fn cleanup_max_output_tokens(intensity: &str, input_text: &str) -> u32 {
    let input_words = count_words(input_text) as u32;
    match intensity {
        "none" => (input_words + 32).clamp(64, 512),
        "light" => (input_words * 2 + 32).clamp(96, 768),
        "high" => (input_words + 64).clamp(96, 768),
        "medium" => (input_words * 2 + 64).clamp(128, 1024),
        _ => (input_words * 2 + 64).clamp(128, 1024),
    }
}
