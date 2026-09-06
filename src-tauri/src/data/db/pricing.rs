//! Persistent cache for OpenRouter model rates.

use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::Serialize;

use super::*;

pub const PRICING_CACHE_TTL_SECS: i64 = 48 * 60 * 60;

#[derive(Debug, Serialize, Clone)]
pub struct PricingRate {
    pub model_id: String,
    pub prompt_usd_per_token: f64,
    pub completion_usd_per_token: f64,
}

#[derive(Debug, Serialize, Clone)]
pub struct PricingSnapshot {
    pub fetched_at: i64,
    pub rates: Vec<PricingRate>,
}

pub fn pricing_cache_is_fresh(db: &Db) -> Result<bool> {
    let conn = lock_conn(db)?;
    let fetched_at: Option<i64> = conn.query_row(
        "SELECT MAX(fetched_at) FROM openrouter_pricing",
        [],
        |row| row.get(0),
    )?;
    Ok(fetched_at.is_some_and(|stamp| Utc::now().timestamp() - stamp < PRICING_CACHE_TTL_SECS))
}

pub fn query_pricing_snapshot(db: &Db) -> Result<PricingSnapshot> {
    let conn = lock_conn(db)?;
    query_pricing_snapshot_conn(&conn)
}

pub fn replace_pricing_cache(
    db: &Db,
    fetched_at: i64,
    rates: &[crate::api::openrouter::ModelPricing],
) -> Result<()> {
    let mut conn = lock_conn(db)?;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM openrouter_pricing", [])?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO openrouter_pricing
             (model_id, prompt_usd_per_token, completion_usd_per_token, fetched_at)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for rate in rates {
            stmt.execute(params![
                rate.model_id,
                rate.prompt_usd_per_token,
                rate.completion_usd_per_token,
                fetched_at,
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn query_pricing_snapshot_conn(conn: &Connection) -> Result<PricingSnapshot> {
    let fetched_at: i64 = conn.query_row(
        "SELECT COALESCE(MAX(fetched_at), 0) FROM openrouter_pricing",
        [],
        |row| row.get(0),
    )?;
    let mut stmt = conn.prepare(
        "SELECT model_id, prompt_usd_per_token, completion_usd_per_token
         FROM openrouter_pricing ORDER BY model_id ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(PricingRate {
            model_id: row.get(0)?,
            prompt_usd_per_token: row.get(1)?,
            completion_usd_per_token: row.get(2)?,
        })
    })?;
    Ok(PricingSnapshot {
        fetched_at,
        rates: rows.collect::<rusqlite::Result<Vec<_>>>()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Db {
        let conn = Connection::open_in_memory().expect("open database");
        conn.execute_batch(
            "CREATE TABLE openrouter_pricing (
               model_id TEXT PRIMARY KEY,
               prompt_usd_per_token REAL NOT NULL,
               completion_usd_per_token REAL NOT NULL,
               fetched_at INTEGER NOT NULL
             );",
        )
        .expect("create pricing table");
        std::sync::Arc::new(std::sync::Mutex::new(conn))
    }

    #[test]
    fn empty_cache_is_not_fresh_and_returns_empty_snapshot() {
        let db = test_db();
        assert!(!pricing_cache_is_fresh(&db).expect("freshness"));
        let snapshot = query_pricing_snapshot(&db).expect("snapshot");
        assert_eq!(snapshot.fetched_at, 0);
        assert!(snapshot.rates.is_empty());
    }

    #[test]
    fn replacing_cache_keeps_one_row_per_model() {
        let db = test_db();
        let rates = vec![crate::api::openrouter::ModelPricing {
            model_id: "google/gemini-3.7-flash".to_string(),
            prompt_usd_per_token: 0.1,
            completion_usd_per_token: 0.2,
        }];
        replace_pricing_cache(&db, Utc::now().timestamp(), &rates).expect("replace");
        replace_pricing_cache(&db, Utc::now().timestamp(), &rates).expect("replace again");
        let snapshot = query_pricing_snapshot(&db).expect("snapshot");
        assert_eq!(snapshot.rates.len(), 1);
    }
}
