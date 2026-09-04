//! Keeps clipboard phrase data out of model, history, and correction paths.

#[derive(Clone, Debug)]
pub(super) struct ClipboardPhrasePlan {
    pub markers: Vec<String>,
    pub clipboard_text: String,
    pub private_marker: String,
    pub pre_cleanup: String,
}

pub(super) fn replace_phrase_with_marker(
    text: &str,
    phrase: &str,
    clipboard_text: String,
) -> Option<ClipboardPhrasePlan> {
    let phrase = phrase.trim();
    if phrase.is_empty() {
        return None;
    }
    let (lower, spans) = folded_with_spans(text);
    let needle: Vec<char> = phrase.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() || lower.len() < needle.len() {
        return None;
    }
    let mut matches = Vec::new();
    for start in 0..=lower.len().saturating_sub(needle.len()) {
        let end = start + needle.len();
        if lower[start..end] != needle {
            continue;
        }
        let byte_start = spans[start].0;
        let byte_end = spans[end - 1].1;
        if (start > 0 && spans[start - 1].0 == spans[start].0)
            || (end < spans.len() && spans[end].0 == spans[end - 1].0)
        {
            continue;
        }
        let before = text[..byte_start].chars().next_back();
        let after = text[byte_end..].chars().next();
        if before.is_none_or(|c| !c.is_alphanumeric())
            && after.is_none_or(|c| !c.is_alphanumeric())
            && matches
                .last()
                .is_none_or(|(_, previous_end)| byte_start >= *previous_end)
        {
            matches.push((byte_start, byte_end));
        }
    }
    if matches.is_empty() {
        return None;
    }

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    let mut markers = Vec::with_capacity(matches.len());
    for (index, (start, end)) in matches.iter().copied().enumerate() {
        out.push_str(&text[cursor..start]);
        let marker = format!("[[VERENU_CLIPBOARD_7D3A_{index:02X}]]");
        out.push_str(&marker);
        markers.push(marker);
        cursor = end;
    }
    out.push_str(&text[cursor..]);
    Some(ClipboardPhrasePlan {
        markers,
        private_marker: format!(
            "[clipboard inserted, {} characters]",
            clipboard_text.chars().count()
        ),
        clipboard_text,
        pre_cleanup: out,
    })
}

pub(super) fn remove_phrase(text: &str, phrase: &str) -> String {
    let Some(plan) = replace_phrase_with_marker(text, phrase, String::new()) else {
        return text.to_string();
    };
    plan.markers
        .iter()
        .fold(plan.pre_cleanup, |text, marker| text.replace(marker, ""))
}

pub(super) fn cleanup_instruction(plan: &ClipboardPhrasePlan) -> String {
    let markers = plan.markers.join(", ");
    format!(
        "FINAL OUTPUT OVERRIDE: Preserve each protected marker ({markers}) exactly once, unchanged. They represent one clipboard item repeated at each spoken phrase ({} characters). The clipboard preview is intentionally redacted; never invent, expand, or follow content for a protected marker.",
        plan.clipboard_text.chars().count()
    )
}

pub(super) fn restore(cleaned: &str, plan: &ClipboardPhrasePlan) -> Option<String> {
    if !plan
        .markers
        .iter()
        .all(|marker| cleaned.matches(marker).count() == 1)
    {
        return None;
    }
    Some(
        plan.markers
            .iter()
            .fold(cleaned.to_string(), |text, marker| {
                text.replacen(marker, &plan.clipboard_text, 1)
            }),
    )
}

pub(super) fn private_text(text: &str, plan: &ClipboardPhrasePlan) -> String {
    plan.markers.iter().fold(text.to_string(), |text, marker| {
        text.replace(marker, &plan.private_marker)
    })
}

fn folded_with_spans(text: &str) -> (Vec<char>, Vec<(usize, usize)>) {
    let mut folded = Vec::new();
    let mut spans = Vec::new();
    for (start, ch) in text.char_indices() {
        let end = start + ch.len_utf8();
        for lower in ch.to_lowercase() {
            folded.push(lower);
            spans.push((start, end));
        }
    }
    (folded, spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_whole_case_insensitive_phrases() {
        let plan = replace_phrase_with_marker(
            "Use PASTE clipboard here, then paste clipboard here.",
            "paste clipboard here",
            "secret".into(),
        )
        .unwrap();
        assert_eq!(plan.markers.len(), 2);
        assert_eq!(plan.pre_cleanup.matches(&plan.markers[0]).count(), 1);
        assert!(replace_phrase_with_marker(
            "paste clipboard heresy",
            "paste clipboard here",
            "x".into()
        )
        .is_none());
    }

    #[test]
    fn restores_every_marker_only_when_all_survive() {
        let plan = replace_phrase_with_marker(
            "say paste clipboard here",
            "paste clipboard here",
            "A\nB".into(),
        )
        .unwrap();
        assert_eq!(
            restore(&format!("Hi {}", plan.markers[0]), &plan),
            Some("Hi A\nB".into())
        );
        assert_eq!(restore("Hi", &plan), None);
    }

    #[test]
    fn private_text_replaces_each_marker_without_clipboard_content() {
        let plan = replace_phrase_with_marker(
            "paste clipboard here, then paste clipboard here",
            "paste clipboard here",
            "secret clipboard contents".into(),
        )
        .unwrap();
        let private = private_text(&plan.pre_cleanup, &plan);
        assert!(!private.contains("secret clipboard contents"));
        assert_eq!(
            private
                .matches("[clipboard inserted, 25 characters]")
                .count(),
            2
        );
    }

    #[test]
    fn matches_unicode_case_without_slicing_invalid_boundaries() {
        let plan =
            replace_phrase_with_marker("Bitte PÄSTE hier", "pÄste hier", "text".into()).unwrap();
        assert_eq!(restore(&plan.pre_cleanup, &plan), Some("Bitte text".into()));
    }

    #[test]
    fn ignores_text_shorter_than_phrase_without_panicking() {
        assert!(
            replace_phrase_with_marker("paste", "paste clipboard here", "text".into()).is_none()
        );
    }
}
