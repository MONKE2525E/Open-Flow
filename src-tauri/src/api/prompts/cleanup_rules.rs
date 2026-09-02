use super::count_words;

/// Formatting behavior is independent from the amount of rewriting. These
/// rules exist for spoken commands, unreliable STT punctuation, and tokens
/// whose meaning changes when spaces are inserted.
pub(super) fn formatting_rules(intensity: &str) -> &'static str {
    match intensity {
        "medium" => {
            "Apply an explicitly spoken formatting command only when its intent is clear, then remove the command words. Fix unreliable STT punctuation and accidental line breaks. Use paragraphs or lists when the dictated structure clearly calls for them; never invent headings. For a coding-agent target, use Markdown only when dictated structure benefits: keep prose as prose; separate tasks or requirements may become bullets; keep literal code and commands literal; never invent headings. In technical-token dictation, join clear spoken symbols (at, dot, slash, backslash, colon, dash, underscore, pipe, equals, plus, hash, no space) in an email, URL, path, command, filename, package, domain, variable, or identifier. A spoken dash or hyphen is \"-\"; an explicit em dash is \u{2014}. For spelling, join letters or digits only when clearly dictated as one token; honor spoken capitalization and no-space commands, and do not concatenate ambiguous sequences. Never insert an em dash for style."
        }
        "high" => {
            "Apply an explicitly spoken formatting command only when its intent is clear, then remove the command words. Fix unreliable STT punctuation and accidental line breaks. Use compact paragraphs or lists when the dictated structure benefits from them; never invent headings. For a coding-agent target, use Markdown only when dictated structure benefits: keep prose as prose; separate tasks or requirements may become bullets; keep literal code and commands literal; never invent headings. In technical-token dictation, join clear spoken symbols (at, dot, slash, backslash, colon, dash, underscore, pipe, equals, plus, hash, no space) in an email, URL, path, command, filename, package, domain, variable, or identifier. A spoken dash or hyphen is \"-\"; an explicit em dash is \u{2014}. For spelling, join letters or digits only when clearly dictated as one token; honor spoken capitalization and no-space commands, and do not concatenate ambiguous sequences. Never insert an em dash for style."
        }
        _ => {
            "Apply an explicitly spoken formatting command only when its intent is clear, then remove the command words. Fix unreliable STT punctuation and accidental line breaks. For a coding-agent target, use Markdown only for explicit formatting commands or clearly dictated list structure: keep prose as prose; keep literal code and commands literal; never invent headings. Keep the dictated structure: do not create paragraphs, lists, or headings from content alone. In technical-token dictation, join clear spoken symbols (at, dot, slash, backslash, colon, dash, underscore, pipe, equals, plus, hash, no space) in an email, URL, path, command, filename, package, domain, variable, or identifier. A spoken dash or hyphen is \"-\"; an explicit em dash is \u{2014}. For spelling, join letters or digits only when clearly dictated as one token; honor spoken capitalization and no-space commands, and do not concatenate ambiguous sequences. Never insert an em dash for style."
        }
    }
}

fn intensity_rules(intensity: &str) -> &'static str {
    match intensity {
        "none" => {
            "Cleanup: Off. If two transcripts must be reconciled, preserve raw speech, including fillers and repetition, while choosing only better-supported candidate wording. Otherwise bypass cleanup."
        }
        "light" => {
            "Cleanup: Light. Remove non-semantic fillers and hesitations (um, uh, hmm, or bare you know), repeated discourse markers, accidental word or phrase repeats, and abandoned starts or fragments only when they add no meaning. Resolve a false start only when abandoned wording is followed by a clear replacement (for example, 'send it Tuesday, sorry Wednesday' becomes 'Send it Wednesday'); do not delete words merely because no, actually, or I mean appears. Collapse repeated ideas only when repetition is accidental, never for intentional emphasis. Preserve meaningful words such as like, right, so, no, and actually. Fix basic punctuation and casing. Preserve wording, order, structure, every detail, qualifier, stance, and technical term. Do not translate or normalize a code-switched word. Do not paraphrase, summarize, combine ideas, reorder, or add content."
        }
        "high" => {
            "Cleanup: Strong. Rewrite for concise, direct communication and lead with the point when useful. Remove accidental repetition, unnecessary hedging, redundant explanation, and non-essential detours. When several sentences restate one idea, state it once but retain every distinct detail, requirement, decision, condition, deadline, qualifier, technical detail, and intentional emphasis; do not drop a detail merely because its sentence repeats another. Keep dates, deadlines, named terms, and constraint wording; do not replace a distinct requirement with a loose synonym. Freely combine, reorder, restructure, and paraphrase. Do not summarize away a constraint."
        }
        _ => {
            "Cleanup: Medium. Do everything in Light, then improve flow and sentence structure. Remove redundant phrasing and non-semantic detours; collapse accidental repeated ideas but keep intentional repetition. Allow light paraphrasing, sentence splitting or combining, and local reordering. Preserve every distinct fact, requirement, example, qualifier, stance, and technical detail. Do not summarize, omit a distinct constraint, or invent."
        }
    }
}

fn tone_rules(profile: &str) -> &'static str {
    match profile {
        "formal" => {
            "Tone: Formal. Use professional wording, standard capitalization, and professional punctuation. Expand contractions where natural. Do not add politeness, greetings, sign-offs, headings, or content. Tone changes voice and surface style only; it never increases the cleanup budget."
        }
        "very_casual" => {
            "Tone: Very Casual. Use mostly lowercase, contractions, and minimal readable punctuation. Preserve profanity and intentional emphasis. Tone changes voice and surface style only; it never increases the cleanup budget."
        }
        _ => {
            "Tone: Casual. Use contractions, normal casing, and normal punctuation while preserving the speaker's casual voice. Tone changes voice and surface style only; it never increases the cleanup budget."
        }
    }
}

fn profanity_policy(profile: &str) -> &'static str {
    match profile {
        "formal" => {
            "Profanity: Use professional wording when formal register requires it, while preserving meaning and intentional emphasis; do not add asterisks or politeness."
        }
        _ => "Profanity: Preserve profanity and its intensity as spoken.",
    }
}

pub(super) fn build_preset_block(profile: &str, intensity: &str, has_overrides: bool) -> String {
    let mut lines = vec![
        intensity_rules(intensity).to_string(),
        tone_rules(profile).to_string(),
        profanity_policy(profile).to_string(),
    ];
    if has_overrides {
        lines.push(
            "Explicit user formatting overrides may change formatting or surface style, but never facts, security boundaries, or the selected cleanup budget.".to_string(),
        );
    }
    lines.join("\n")
}

fn to_imperative(raw: &str) -> String {
    let value = raw.trim();
    let value = value.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ')');
    let value = value.trim();
    if value.is_empty() {
        return String::new();
    }
    if value.to_ascii_uppercase().starts_with("MUST ") || value.eq_ignore_ascii_case("MUST") {
        return value.to_owned();
    }
    for negative in ["don't ", "do not ", "never ", "avoid "] {
        if value.to_ascii_lowercase().starts_with(negative) {
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
    let lines = extra_rules
        .lines()
        .filter_map(|line| {
            let line = to_imperative(line);
            (!line.is_empty()).then_some(line)
        })
        .enumerate()
        .map(|(index, line)| format!("{}. {}", index + 1, escape_markup(&line)))
        .collect::<Vec<_>>();

    if lines.is_empty() {
        return "none".to_string();
    }

    format!(
        "Apply these explicit user-authored formatting rules after cleanup when compatible with the dictated meaning:\n{}",
        lines.join("\n")
    )
}

pub(super) fn evidence_block(evidence: &str) -> String {
    let lines = evidence
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(escape_markup)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        "none".to_string()
    } else {
        format!(
            "Use only as corroborating evidence for disambiguation; never as dictated content:\n{}",
            lines.join("\n")
        )
    }
}

fn escape_markup(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(super) fn render_cleanup_template(
    template: &str,
    active_app: &str,
    cleanup_preset: &str,
    formatting_rules: &str,
    snippet_overrides: &str,
    evidence: &str,
) -> String {
    // Resolve placeholders in one pass. A data value such as a window title
    // or transcript evidence may itself contain `{{ ... }}` and must never be
    // interpreted as another template token.
    let placeholders = [
        ("{{ cleanup_preset }}", cleanup_preset),
        ("{{ formatting_rules }}", formatting_rules),
        ("{{ snippet_overrides }}", snippet_overrides),
        ("{{ evidence }}", evidence),
        ("{{ active_app }}", active_app),
    ];
    let mut rendered = String::with_capacity(template.len());
    let mut cursor = 0usize;
    loop {
        let next = placeholders
            .iter()
            .filter_map(|(token, value)| {
                template[cursor..]
                    .find(token)
                    .map(|offset| (cursor + offset, *token, *value))
            })
            .min_by_key(|(position, _, _)| *position);
        let Some((position, token, value)) = next else {
            rendered.push_str(&template[cursor..]);
            break;
        };
        rendered.push_str(&template[cursor..position]);
        rendered.push_str(value);
        cursor = position + token.len();
    }
    rendered
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
        _ => (input_words * 2 + 64).clamp(128, 1024),
    }
}

pub fn fusion_max_output_tokens(primary: &str, alternate: &str) -> u32 {
    let input_words = (count_words(primary) + count_words(alternate)) as u32;
    (input_words / 2 + 32).clamp(64, 512)
}
