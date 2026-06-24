use super::{PromptTier, count_words};

pub(super) const FORMATTING_RULES: &str = "FORMATTING COMMANDS: If speech includes literal commands like \
'new paragraph', 'new line', 'bullet point', 'numbered list', 'open quote', or 'close quote', \
apply the formatting.";

fn role_line(intensity: &str) -> &'static str {
    match intensity {
        "none" => "You are a transcription mirror for <raw_dictation>.",
        "light" => "You make a minimal edit to <raw_dictation>, removing only speech noise.",
        "medium" => {
            "You do a normal dictation cleanup of <raw_dictation>, preserving detail and intent."
        }
        "high" => "You concisely rewrite <raw_dictation> into a punchy result.",
        _ => "You do a normal dictation cleanup of <raw_dictation>, preserving detail and intent.",
    }
}

fn intensity_rules(
    intensity: &str,
    tier: PromptTier,
    has_overrides: bool,
    profile: &str,
) -> String {
    let base = match (intensity, tier) {
        ("none", _) => {
            if profile == "formal" {
                "CLEANUP: Keep wording and structure unchanged by default. \
                You may only change wording where needed to apply FORMAL profanity policy replacements."
                    .to_string()
            } else {
                "CLEANUP: Return input unchanged, character-for-character.".to_string()
            }
        }
        ("light", PromptTier::Short) => {
            "CLEANUP (LIGHT): MUST remove filler words (um, uh, like, you know), immediate duplicated words, and immediate false starts. \
            MUST NOT summarize, compress, reorder, or rewrite personality away. \
            MUST keep sentence structure and almost all content."
                .to_string()
        }
        ("light", _) => {
            "CLEANUP (LIGHT): MUST remove filler words (um, uh, like, you know), immediate duplicated words, and immediate false starts. \
            MUST NOT summarize, compress, reorder, or rewrite personality away. \
            MUST preserve sentence structure and almost all content."
                .to_string()
        }
        ("high", PromptTier::Short) => {
            "CLEANUP (DIRECT): MUST produce a concise, punchy rewrite. MUST cut hedges, circular phrasing, repeated ideas, throat-clearing, and unnecessary qualifiers. \
            MUST preserve core meaning and important specifics. \
            MUST NOT invent content or over-summarize technical details."
                .to_string()
        }
        ("high", PromptTier::Medium) => {
            "CLEANUP (DIRECT): MUST produce a concise, punchy rewrite targeting roughly 30-50% of input words. \
            MUST cut filler (um, uh, like, you know), hedges (I think, maybe, probably), repeated ideas, false starts, circular phrasing, throat-clearing, and unnecessary qualifiers. \
            MUST preserve core meaning and important specifics. \
            MUST NOT invent content or over-summarize technical details."
                .to_string()
        }
        ("high", PromptTier::Detailed) => {
            "CLEANUP (DIRECT): MUST produce a concise, punchy rewrite targeting roughly 30-50% of input words, keeping only core meaning. \
            MUST cut filler, hedges, repeated ideas, false starts, circular phrasing, throat-clearing, and unnecessary qualifiers. MAY merge or reorder sentences when it improves clarity. \
            MUST preserve important specifics. \
            MUST NOT invent content or over-summarize technical details."
                .to_string()
        }
        ("medium", PromptTier::Short) => {
            "CLEANUP (MEDIUM): MUST remove filler, repetition, and obvious speech artifacts, and smooth sentence flow. \
            MUST preserve detail and speaker intent. \
            MUST NOT aggressively compress or drop specifics."
                .to_string()
        }
        ("medium", PromptTier::Medium) => {
            "CLEANUP (MEDIUM): MUST remove filler, repetition, rambling loops, and obvious speech artifacts, and smooth sentence flow. \
            MAY lightly merge or reorder sentences when clarity improves. \
            MUST preserve detail and speaker intent. \
            MUST NOT aggressively compress or drop specifics."
                .to_string()
        }
        ("medium", PromptTier::Detailed) | (_, _) => {
            "CLEANUP (MEDIUM): MUST remove filler, repetition, rambling loops, and obvious speech artifacts, and smooth sentence flow. \
            MAY restructure for clarity while preserving meaning. \
            MUST preserve detail and important specifics and speaker intent. \
            MUST NOT aggressively compress."
                .to_string()
        }
    };

    if has_overrides {
        format!(
            "{base}\nSNIPPET OVERRIDES: If FINAL OUTPUT OVERRIDES conflict with cleanup rules, overrides win."
        )
    } else {
        base
    }
}

fn tone_rules(profile: &str) -> &'static str {
    match profile {
        "formal" => {
            "TONE: Formal. Full sentences, proper capitalization, complete punctuation, expanded contractions."
        }
        "very_casual" => {
            "TONE: Very casual. Mostly lowercase, minimal punctuation, keep contractions. \
            Affects voice and capitalization only; do not change how much content is removed."
        }
        _ => {
            "TONE: Casual. Natural conversational phrasing, sentence capitalization, light punctuation."
        }
    }
}

fn profanity_policy(profile: &str, intensity: &str) -> String {
    if profile == "formal" {
        return "PROFANITY (FORMAL): Soften most profanity to professional wording, preserving meaning and emphasis. No asterisk censorship. This overrides intensity profanity defaults."
            .to_string();
    }

    let intensity_label = match intensity {
        "none" => "VERBATIM",
        "light" => "LIGHT",
        "high" => "DIRECT",
        _ => "MEDIUM",
    };

    let tone_line = match profile {
        "very_casual" => "PROFANITY TONE (VERY CASUAL): Keep swear words and speaker intensity.",
        _ => "PROFANITY TONE (CASUAL): Keep swear words and speaker intensity.",
    };

    format!(
        "PROFANITY ({intensity_label}): Keep profanity as spoken. Do not sanitize or euphemize.\n{tone_line}"
    )
}

fn number_style_block(tier: PromptTier, has_numeric_content: bool) -> String {
    if tier == PromptTier::Short && !has_numeric_content {
        String::new()
    } else {
        "NUMBER STYLE: Plain-language cardinal numbers below 10 must be words. \
        Cardinal numbers 10 or above must be digits. \
        Do not apply this rule inside preserved technical tokens."
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
        role_line(intensity).to_string(),
        intensity_rules(intensity, tier, has_overrides, profile),
        tone_rules(profile).to_string(),
        profanity_policy(profile, intensity),
    ];
    let number_style = number_style_block(tier, has_numeric_content);
    if !number_style.is_empty() {
        lines.push(number_style);
    }
    lines.join("\n")
}

fn to_imperative(raw: &str) -> String {
    let s = raw.trim();
    let s = s.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ')');
    let s = s.trim();

    if s.to_uppercase().starts_with("MUST") {
        return s.to_owned();
    }
    for neg in &["don't ", "do not ", "never ", "avoid "] {
        if s.to_lowercase().starts_with(neg) {
            let rest = &s[neg.len()..];
            let mut chars = rest.chars();
            let capitalized = match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            };
            return format!("MUST NOT {capitalized}");
        }
    }
    format!("MUST {s}")
}

pub(super) fn snippet_overrides_block(extra_rules: &str) -> String {
    if extra_rules.trim().is_empty() {
        return String::new();
    }

    let override_lines: String = extra_rules
        .lines()
        .filter(|l| !l.trim().is_empty())
        .enumerate()
        .map(|(i, line)| format!("{}. {}", i + 1, to_imperative(line)))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "FINAL OUTPUT OVERRIDES\n\
        Apply these rules last. They override cleanup, tone, punctuation, and preserve-syntax rules.\n\
        Follow every rule exactly.\n\
        {override_lines}"
    )
}

pub(super) fn render_cleanup_template(
    template: &str,
    active_app: &str,
    cleanup_preset: &str,
    formatting_rules: &str,
    snippet_overrides: &str,
) -> String {
    template
        .replace("{{ active_app }}", active_app)
        .replace("{{ cleanup_preset }}", cleanup_preset)
        .replace("{{ formatting_rules }}", formatting_rules)
        .replace("{{ snippet_overrides }}", snippet_overrides)
}

pub(super) fn collapse_blank_lines(s: &str) -> String {
    let s = s.replace("\r\n", "\n");
    let mut result = String::with_capacity(s.len());
    let mut newline_run = 0;
    for c in s.chars() {
        if c == '\n' {
            newline_run += 1;
            if newline_run <= 2 {
                result.push(c);
            }
        } else {
            newline_run = 0;
            result.push(c);
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
