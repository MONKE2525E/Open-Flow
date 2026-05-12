use rusqlite::Connection;

pub fn apply_substitutions(text: &str, _db: &Connection) -> String {
    // TODO: load dictionary from db and apply substitutions
    text.to_string()
}
