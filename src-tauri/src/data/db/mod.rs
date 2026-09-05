//! SQLite access layer for Verenu.
//!
//! Split by domain into submodules; every public item is re-exported here so the
//! external surface (`crate::data::db::*`) is unchanged. Add new queries to the
//! submodule that matches their table. Shared low-level pieces (`Db`, `lock_conn`,
//! `CreatedRecordMeta`) live in this file; reusable validation/normalization
//! helpers live in `validation`. API keys are never stored here — see
//! `data/credentials.rs`.

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, MutexGuard};

mod cleanup_cache;
mod contexts;
mod dictionary;
mod insights;
mod schema;
mod snippets;
mod transcriptions;
mod validation;

pub use cleanup_cache::*;
pub use contexts::*;
pub use dictionary::*;
pub use insights::*;
pub use schema::*;
pub use snippets::*;
pub use transcriptions::*;
pub use validation::*;

pub type Db = Arc<Mutex<Connection>>;

fn lock_conn(db: &Db) -> Result<MutexGuard<'_, Connection>> {
    db.lock()
        .map_err(|_| anyhow::anyhow!("Database lock was poisoned"))
}

/// Metadata returned after inserting a row: the new id and its `created_at`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreatedRecordMeta {
    pub id: i64,
    pub created_at: String,
}

#[cfg(test)]
mod tests;
