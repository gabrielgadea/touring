//! U4 embedding migration utilities for `recall_embeddings` table.
//!
//! Provides batch migration of existing f32 embeddings to 4-bit quantized
//! representation, achieving 8× storage reduction with minimal precision loss.
//!
//! # Feature Gate
//!
//! All items in this module require the `u4-quantization` feature.

/// Batch-migrate existing f32 embeddings to U4 in the `recall_embeddings` table.
///
/// Safe to run multiple times: skips rows where `embedding_u4` is already populated.
/// Processes rows in batches to bound memory usage.
///
/// # Errors
///
/// Returns an error if any SQLite operation fails (schema mismatch, I/O, etc.).
///
/// # Example
///
/// ```no_run
/// # use rusqlite::Connection;
/// # use touring_intelligence::rl::memory::embedding_u4::migrate_embeddings_to_u4;
/// # let conn = Connection::open_in_memory().expect("in-memory db");
/// let count = migrate_embeddings_to_u4(&conn, 256).expect("migration failed");
/// println!("Migrated {count} embeddings");
/// ```
#[cfg(feature = "u4-quantization")]
pub fn migrate_embeddings_to_u4(
    conn: &rusqlite::Connection,
    batch_size: usize,
) -> Result<u64, rusqlite::Error> {
    use touring_simd::quantization::EmbeddingU4;

    let mut migrated = 0u64;
    loop {
        let ids: Vec<i64> = conn
            .prepare(
                "SELECT id FROM recall_embeddings
                 WHERE embedding IS NOT NULL AND embedding_u4 IS NULL
                 LIMIT ?1",
            )?
            .query_map([batch_size as i64], |r| r.get(0))?
            .flatten()
            .collect();

        if ids.is_empty() {
            break;
        }

        for id in &ids {
            let blob: Vec<u8> = conn.query_row(
                "SELECT embedding FROM recall_embeddings WHERE id = ?1",
                [id],
                |r| r.get(0),
            )?;

            // Decode raw le_bytes (standard format written by store_chunk).
            // chunks_exact(4) guarantees each slice is exactly 4 bytes; the
            // try_into cannot fail here, so the expect message is a safety net.
            let f32_vals: Vec<f32> = blob
                .chunks_exact(4)
                .map(|b| {
                    let arr: [u8; 4] = b
                        .try_into()
                        .expect("chunks_exact(4) guarantees 4-byte slices");
                    f32::from_le_bytes(arr)
                })
                .collect();

            let q = EmbeddingU4::from_f32(&f32_vals);
            conn.execute(
                "UPDATE recall_embeddings
                 SET embedding_u4 = ?1, quant_scale = ?2, quant_zero = ?3
                 WHERE id = ?4",
                rusqlite::params![q.to_bytes(), q.scale, q.zero, id],
            )?;
        }

        migrated += ids.len() as u64;
    }

    Ok(migrated)
}

#[cfg(all(test, feature = "u4-quantization"))]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn create_test_db(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE recall_embeddings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                embedding BLOB,
                embedding_u4 BLOB,
                quant_scale REAL,
                quant_zero REAL
            )",
        )
        .expect("create table");
    }

    fn insert_embedding(conn: &Connection, vals: &[f32]) -> i64 {
        let blob: Vec<u8> = vals.iter().flat_map(|f| f.to_le_bytes()).collect();
        conn.execute(
            "INSERT INTO recall_embeddings (embedding) VALUES (?1)",
            [&blob],
        )
        .expect("insert");
        conn.last_insert_rowid()
    }

    #[test]
    fn migrate_empty_table_returns_zero() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        create_test_db(&conn);
        let count = migrate_embeddings_to_u4(&conn, 64).expect("migrate");
        assert_eq!(count, 0);
    }

    #[test]
    fn migrate_single_embedding() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        create_test_db(&conn);
        let vals: Vec<f32> = (0..8).map(|i| i as f32 * 0.1).collect();
        let id = insert_embedding(&conn, &vals);

        let count = migrate_embeddings_to_u4(&conn, 64).expect("migrate");
        assert_eq!(count, 1);

        let (blob, scale, zero): (Vec<u8>, f32, f32) = conn
            .query_row(
                "SELECT embedding_u4, quant_scale, quant_zero FROM recall_embeddings WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("query");

        assert!(!blob.is_empty(), "embedding_u4 should be set");
        assert!(scale > 0.0, "scale should be positive");
        let _ = zero; // zero can be any value
    }

    #[test]
    fn migrate_idempotent() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        create_test_db(&conn);
        let vals: Vec<f32> = (0..16).map(|i| i as f32 * 0.05).collect();
        insert_embedding(&conn, &vals);

        let count1 = migrate_embeddings_to_u4(&conn, 64).expect("first migrate");
        let count2 = migrate_embeddings_to_u4(&conn, 64).expect("second migrate");

        assert_eq!(count1, 1, "first run should migrate 1 row");
        assert_eq!(count2, 0, "second run should skip already-migrated rows");
    }

    #[test]
    fn migrate_batch_processes_all() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        create_test_db(&conn);
        for i in 0..10i32 {
            let vals: Vec<f32> = (0..4).map(|j| i as f32 + j as f32 * 0.1).collect();
            insert_embedding(&conn, &vals);
        }

        let count = migrate_embeddings_to_u4(&conn, 3).expect("migrate with batch_size=3");
        assert_eq!(count, 10, "all 10 embeddings should be migrated");
    }
}
