// Deliberate test probe for the Verenu AI reviewer — intentionally violates
// two of the review policy's priority rules so we can confirm the reviewer
// actually catches real issues instead of rubber-stamping every diff.
// This file is never merged; it exists only for one throwaway test PR.

pub fn debug_dump_api_key(provider: &str, api_key: &str) {
    // Rule #1 (secret leaks): writing a raw API key to a log line.
    println!("verenu debug: {} api key = {}", provider, api_key);
}

pub fn get_first_transcription_word(clean_text: &str) -> String {
    // Rule #10 (production crashes): unwrap() on a fallible value
    // (an empty transcription) reachable from user-controlled input.
    clean_text.split_whitespace().next().unwrap().to_string()
}
