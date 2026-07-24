//! Summary Cache — Pre-computed summary tables for O(1) MCP queries.
//!
//! Stores frequently-accessed aggregations (wiring stats, top gotchas,
//! health snapshots) in a dedicated SQLite table with TTL-based expiry.
//! Inspired by code-review-graph's schema v6 summary tables.

use serde_json::Value as JsonValue;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Default TTL for cached summaries (seconds).
const DEFAULT_TTL: u32 = 300;

/// Pre-computed summary cache backed by SQLite.
pub struct SummaryCache {
    conn: rusqlite::Connection,
}

impl SummaryCache {
    /// Open or create the summary cache in the given database file.
    ///
    /// Creates the `touring_summaries` table if it doesn't exist.
    pub fn open(db_path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = rusqlite::Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=1000;")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS touring_summaries (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                computed_at REAL NOT NULL,
                ttl_seconds INTEGER DEFAULT 300
            );",
        )?;
        Ok(Self { conn })
    }

    /// Get a cached summary if it exists and hasn't expired.
    pub fn get(&self, key: &str) -> Option<JsonValue> {
        let now = now_secs();
        let result = self.conn.query_row(
            "SELECT value, computed_at, ttl_seconds FROM touring_summaries WHERE key = ?1",
            [key],
            |row| {
                let value_str: String = row.get(0)?;
                let computed_at: f64 = row.get(1)?;
                let ttl: i64 = row.get(2)?;
                Ok((value_str, computed_at, ttl))
            },
        );

        match result {
            Ok((value_str, computed_at, ttl)) => {
                if now - computed_at > ttl as f64 {
                    return None; // expired
                }
                serde_json::from_str(&value_str).ok()
            }
            Err(_) => None,
        }
    }

    /// Store a summary with the given TTL.
    pub fn set(&self, key: &str, value: &JsonValue, ttl: u32) -> Result<(), rusqlite::Error> {
        let value_str = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
        let now = now_secs();
        self.conn.execute(
            "INSERT OR REPLACE INTO touring_summaries (key, value, computed_at, ttl_seconds) \
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![key, value_str, now, ttl],
        )?;
        Ok(())
    }

    /// Store a summary with the default TTL.
    pub fn set_default(&self, key: &str, value: &JsonValue) -> Result<(), rusqlite::Error> {
        self.set(key, value, DEFAULT_TTL)
    }

    /// Remove a specific cached summary.
    pub fn invalidate(&self, key: &str) -> Result<(), rusqlite::Error> {
        self.conn
            .execute("DELETE FROM touring_summaries WHERE key = ?1", [key])?;
        Ok(())
    }

    /// Remove all cached summaries.
    pub fn invalidate_all(&self) -> Result<(), rusqlite::Error> {
        self.conn.execute("DELETE FROM touring_summaries", [])?;
        Ok(())
    }

    /// Remove expired entries.
    pub fn cleanup_expired(&self) -> Result<usize, rusqlite::Error> {
        let now = now_secs();
        let deleted = self.conn.execute(
            "DELETE FROM touring_summaries WHERE (computed_at + ttl_seconds) < ?1",
            [now],
        )?;
        Ok(deleted)
    }

    /// Count of cached entries (including expired).
    pub fn count(&self) -> usize {
        self.conn
            .query_row("SELECT COUNT(*) FROM touring_summaries", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0) as usize
    }
}

/// Well-known cache keys for pre-computed summaries.
pub mod keys {
    /// Wiring status: orphan_count, module_count, pub_symbols, avg_score. TTL=60s.
    pub const WIRING_STATUS: &str = "wiring:status";
    /// Top 10 gotchas by hit_count. TTL=120s.
    pub const TOP_GOTCHAS: &str = "gotcha:top10";
    /// Project health snapshot: symbol_count, file_count, e2e_score. TTL=30s.
    pub const HEALTH_SNAPSHOT: &str = "health:snapshot";
    /// Minimal context output. TTL=30s.
    pub const MINIMAL_CONTEXT: &str = "context:minimal";
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_cache() -> SummaryCache {
        SummaryCache::open(Path::new(":memory:")).expect("in-memory db")
    }

    #[test]
    fn test_set_and_get() {
        let cache = temp_cache();
        let val = json!({"orphans": 100, "modules": 50});
        cache.set(keys::WIRING_STATUS, &val, 300).expect("set");
        let got = cache.get(keys::WIRING_STATUS);
        assert!(got.is_some());
        assert_eq!(got.unwrap()["orphans"], 100);
    }

    #[test]
    fn test_get_missing_key() {
        let cache = temp_cache();
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn test_ttl_expiry() {
        let cache = temp_cache();
        let val = json!({"data": "old"});
        // Set with TTL=0 (immediately expired)
        cache.set("ephemeral", &val, 0).expect("set");
        // Sleep a tiny bit to ensure time advances
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(cache.get("ephemeral").is_none(), "Should be expired");
    }

    #[test]
    fn test_invalidate_key() {
        let cache = temp_cache();
        cache.set("k1", &json!(1), 300).expect("set");
        cache.set("k2", &json!(2), 300).expect("set");
        assert_eq!(cache.count(), 2);
        cache.invalidate("k1").expect("invalidate");
        assert_eq!(cache.count(), 1);
        assert!(cache.get("k1").is_none());
        assert!(cache.get("k2").is_some());
    }

    #[test]
    fn test_invalidate_all() {
        let cache = temp_cache();
        cache.set("a", &json!(1), 300).expect("set");
        cache.set("b", &json!(2), 300).expect("set");
        cache.invalidate_all().expect("invalidate_all");
        assert_eq!(cache.count(), 0);
    }

    #[test]
    fn test_upsert_overwrites() {
        let cache = temp_cache();
        cache.set("k", &json!({"v": 1}), 300).expect("set");
        cache.set("k", &json!({"v": 2}), 300).expect("set");
        assert_eq!(cache.count(), 1);
        let got = cache.get("k").expect("should exist");
        assert_eq!(got["v"], 2);
    }

    #[test]
    fn test_cleanup_expired() {
        let cache = temp_cache();
        cache.set("fresh", &json!(1), 3600).expect("set");
        cache.set("stale", &json!(2), 0).expect("set");
        std::thread::sleep(std::time::Duration::from_millis(10));
        let deleted = cache.cleanup_expired().expect("cleanup");
        assert_eq!(deleted, 1);
        assert!(cache.get("fresh").is_some());
    }

    #[test]
    fn test_set_default_ttl() {
        let cache = temp_cache();
        cache.set_default("x", &json!("val")).expect("set_default");
        assert!(cache.get("x").is_some());
    }

    #[test]
    fn test_keys_constants() {
        assert_eq!(keys::WIRING_STATUS, "wiring:status");
        assert_eq!(keys::TOP_GOTCHAS, "gotcha:top10");
        assert_eq!(keys::HEALTH_SNAPSHOT, "health:snapshot");
        assert_eq!(keys::MINIMAL_CONTEXT, "context:minimal");
    }
}
