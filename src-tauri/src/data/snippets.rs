use rusqlite::Connection;

pub fn expand_snippets(text: &str, _db: &Connection) -> String {
    // TODO: load snippets from db and expand
    text.to_string()
}
