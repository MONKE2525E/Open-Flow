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
