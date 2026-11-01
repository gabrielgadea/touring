//! SCIP ingest (H2, 2026-08-12) — type-resolved wiring edges from `rust-analyzer scip`.
//!
//! The name-matching heuristic (`contract_source='ast_inferred'`, ~66% of
//! `wiring_map`) fabricates cross-module cycles: measured 2026-08-12,
//! excluding it takes the SCC count from 7 (including a 917-module giant)
//! to **zero**. This module ingests the compiler's own answer instead: every
//! reference occurrence in a SCIP index resolves to the exact definition the
//! type-checker picked, so homonymous forks and re-export chains can no
//! longer alias into phantom edges.
//!
//! Trust ordering: `scip_resolved` ranks above every AST origin — it is the
//! only one carrying rustc type resolution (method dispatch, generics,
//! re-export identity). Heuristic edges are preserved (REGRA #0: they serve
//! non-Rust files); trusted-mode cycle/orphan queries exclude them.

use prost::Message;

/// `contract_source` value written by this ingest (trusted tier).
pub const SCIP_CONTRACT_SOURCE: &str = "scip_resolved";

/// SCIP `symbol_roles` bit marking a definition occurrence.
const SCIP_ROLE_DEFINITION: i32 = 0x1;

/// Minimal wire-compatible view of `scip.proto` carrying only the fields the
/// ingest reads; prost skips everything else (metadata, symbol documentation,
/// relationships, hover text).
#[derive(Clone, PartialEq, Message)]
struct ScipIndexIngest {
    #[prost(message, repeated, tag = "2")]
    documents: Vec<ScipDocumentIngest>,
}

#[derive(Clone, PartialEq, Message)]
struct ScipDocumentIngest {
    #[prost(string, tag = "1")]
    relative_path: String,
    /// Real scip.proto: `occurrences` is tag 2 (tag 3 is `symbols`; reading
    /// SymbolInformation AS occurrences is what produced the misleading
    /// "symbol_roles: invalid wire type" decode errors during bring-up).
    #[prost(message, repeated, tag = "2")]
    occurrences: Vec<ScipOccurrenceIngest>,
}

#[derive(Clone, PartialEq, Message)]
struct ScipOccurrenceIngest {
    #[prost(string, tag = "2")]
    symbol: String,
    /// Singular varint bitfield on the real wire (verified by raw protobuf
    /// walk of `rust-analyzer scip` output, 2026-08-12).
    #[prost(int32, tag = "3")]
    symbol_roles: i32,
}

impl ScipOccurrenceIngest {
    fn is_definition(&self) -> bool {
        self.symbol_roles & SCIP_ROLE_DEFINITION != 0
    }
}

/// Errors the ingest can fail with.
#[derive(Debug, thiserror::Error)]
pub enum ScipIngestError {
    /// The input bytes are not a decodable SCIP index.
    #[error("SCIP decode failed: {0}")]
    Decode(#[from] prost::DecodeError),
    /// Writing the resolved edges into `wiring_map` failed.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// What an ingest run did (returned to the CLI caller verbatim).
#[derive(Debug, Default, serde::Serialize)]
pub struct ScipIngestReport {
    /// Documents (files) seen in the index.
    pub documents: usize,
    /// Definition occurrences indexed.
    pub definitions: usize,
    /// Reference occurrences examined.
    pub references_seen: usize,
    /// Cross-file edges written into `wiring_map`.
    pub edges_written: usize,
    /// References skipped because producer == consumer file.
    pub self_edges_skipped: usize,
    /// References to symbols with no definition in the index (std, crates.io
    /// deps, locals) — correctly out of the workspace graph.
    pub external_refs_skipped: usize,
    /// Symbols defined more than once (first definition wins — REGRA #17
    /// determinism: the document order is the tiebreak, never a hash map's).
    pub duplicate_definitions: usize,
    /// Repeat references to an already-written (producer, consumer, symbol)
    /// edge — one row per triple is the whole signal.
    pub duplicate_edges_skipped: usize,
    /// Edges the wiring write gate refuses (a producer outside every wiring
    /// vocabulary, or a companion on either side) — the ingest used to write
    /// them straight past the gate (cross-audit 14/09/2026, B11).
    pub gated_edges_skipped: usize,
}

/// Extracts the human-readable descriptor from a SCIP symbol string:
/// `<scheme> <manager> <package> <version> <descriptor>` — e.g.
/// `rust-analyzer cargo touring-cli 30.4.0 cli/memory/derive_tags().` yields
/// `cli/memory/derive_tags().`.
fn descriptor_of(symbol: &str) -> &str {
    symbol.splitn(5, ' ').nth(4).unwrap_or(symbol)
}

/// Reduces a SCIP descriptor to the plain identifier AST rows use as
/// `symbol_name`: `cli/memory/derive_tags().` → `derive_tags`,
/// `foo/Bar#` → `Bar`, `foo/Bar#method().` → `method`.
///
/// Orphan queries correlate producer and consumer rows via
/// `symbol_name` equality (`orphan_symbols_with_trust`), and AST producer
/// rows store the bare identifier — a descriptor-shaped consumer row can
/// never match one, so SCIP edges would count for cycles but silently never
/// clear an orphan (S2, cross-audit 2026-08-23).
fn identifier_of(descriptor: &str) -> &str {
    let trimmed = descriptor.trim_end_matches(|c: char| "()./#!:`".contains(c));
    let start = trimmed
        .rfind(|c: char| "/#.".contains(c))
        .map_or(0, |i| i + 1);
    let id = &trimmed[start..];
    if id.is_empty() { descriptor } else { id }
}

/// Decodes a SCIP index and writes its type-resolved cross-file edges into
/// `wiring_map`, replacing this workspace's previous `scip_resolved` set
/// (idempotent re-ingest: delete-then-insert inside one transaction).
pub fn ingest_scip_bytes(
    conn: &rusqlite::Connection,
    data: &[u8],
    workspace_root: &str,
) -> Result<ScipIngestReport, ScipIngestError> {
    let index = ScipIndexIngest::decode(data)?;
    let mut report = ScipIngestReport {
        documents: index.documents.len(),
        ..ScipIngestReport::default()
    };

    // Pass 1 — definition map (symbol → defining file). Iteration follows the
    // document order of the index; the first definition wins, so the mapping
    // is deterministic for identical input bytes.
    let mut defs: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for doc in &index.documents {
        for occ in &doc.occurrences {
            if occ.is_definition() {
                report.definitions += 1;
                if defs.insert(&occ.symbol, &doc.relative_path).is_some() {
                    report.duplicate_definitions += 1;
                }
            }
        }
    }

    // Pass 2 — references become edges consumer_file → producer_file.
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM wiring_map WHERE contract_source = ?1 AND workspace_root = ?2",
        rusqlite::params![SCIP_CONTRACT_SOURCE, workspace_root],
    )?;
    write_edges(&tx, &index.documents, &defs, workspace_root, &mut report)?;
    tx.commit()?;
    Ok(report)
}

/// Writes one `wiring_map` row per reference that resolves to a workspace
/// definition in another file (pass 2 of the ingest, extracted to keep
/// `ingest_scip_bytes` under the complexity budget).
fn write_edges(
    tx: &rusqlite::Transaction,
    documents: &[ScipDocumentIngest],
    defs: &std::collections::HashMap<&str, &str>,
    workspace_root: &str,
    report: &mut ScipIngestReport,
) -> Result<(), ScipIngestError> {
    let mut insert = tx.prepare(
        "INSERT INTO wiring_map
         (module_file, symbol_name, symbol_kind, visibility, consumer_file,
          contract_source, consumer_type, workspace_root)
         VALUES (?1, ?2, 'unknown', 'public', ?3, ?4, 'rust_scip', ?5)",
    )?;
    // One row per (producer, consumer, symbol): a file that references the
    // same symbol N times contributes ONE edge — `idx_wiring_unique` forbids
    // the duplicates, and N identical rows carry no extra signal.
    let mut seen: std::collections::HashSet<(&str, &str, &str)> = std::collections::HashSet::new();
    for doc in documents {
        for occ in &doc.occurrences {
            if occ.is_definition() {
                continue;
            }
            if occ.symbol.starts_with("local ") {
                report.external_refs_skipped += 1;
                continue;
            }
            report.references_seen += 1;
            let Some(producer) = defs.get(occ.symbol.as_str()) else {
                report.external_refs_skipped += 1;
                continue;
            };
            if *producer == doc.relative_path {
                report.self_edges_skipped += 1;
                continue;
            }
            // The write gate the other writers pass through, under the maximal
            // vocabulary the open-time eviction also uses.
            if !crate::knowledge_wiring::is_wireable_source(producer, true, &[])
                || touring_foundation::config::is_companion_key(&doc.relative_path)
            {
                report.gated_edges_skipped += 1;
                continue;
            }
            let symbol = identifier_of(descriptor_of(&occ.symbol));
            if !seen.insert((*producer, doc.relative_path.as_str(), symbol)) {
                report.duplicate_edges_skipped += 1;
                continue;
            }
            insert.execute(rusqlite::params![
                *producer,
                symbol,
                doc.relative_path,
                SCIP_CONTRACT_SOURCE,
                workspace_root,
            ])?;
            report.edges_written += 1;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_index() -> Vec<u8> {
        // Two documents: `src/a.rs` defines `pkg::compute`, `src/b.rs` calls it
        // and also references a std symbol (no definition in the index).
        let index = ScipIndexIngest {
            documents: vec![
                ScipDocumentIngest {
                    relative_path: "src/a.rs".to_string(),
                    occurrences: vec![ScipOccurrenceIngest {
                        symbol: "rust-analyzer cargo probe 0.1.0 a/compute().".to_string(),
                        symbol_roles: SCIP_ROLE_DEFINITION,
                    }],
                },
                ScipDocumentIngest {
                    relative_path: "src/b.rs".to_string(),
                    occurrences: vec![
                        ScipOccurrenceIngest {
                            symbol: "rust-analyzer cargo probe 0.1.0 a/compute().".to_string(),
                            symbol_roles: 0,
                        },
                        ScipOccurrenceIngest {
                            symbol: "rust-analyzer cargo std x macros/println!".to_string(),
                            symbol_roles: 0,
                        },
                        ScipOccurrenceIngest {
                            symbol: "local 0".to_string(),
                            symbol_roles: 0,
                        },
                    ],
                },
            ],
        };
        index.encode_to_vec()
    }

    fn wiring_conn() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE wiring_map (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                module_file TEXT NOT NULL,
                symbol_name TEXT NOT NULL,
                symbol_kind TEXT NOT NULL DEFAULT 'unknown',
                visibility TEXT NOT NULL DEFAULT 'public',
                consumer_file TEXT,
                import_line INTEGER,
                contract_source TEXT DEFAULT 'ast_read',
                resolved_at TEXT,
                consumer_type TEXT DEFAULT 'rust_import',
                workspace_root TEXT
            )",
        )
        .unwrap();
        conn
    }

    /// Cross-audit 14/09/2026 (B11): the ingest passes the write gate the other
    /// writers use — a producer under `tests/` or a companion consumer is skipped.
    #[test]
    fn ingest_skips_the_edges_the_write_gate_refuses() {
        let def = "rust-analyzer cargo probe 0.1.0 it/helper().";
        let doc = |path: &str, roles| ScipDocumentIngest {
            relative_path: path.to_string(),
            occurrences: vec![ScipOccurrenceIngest {
                symbol: def.to_string(),
                symbol_roles: roles,
            }],
        };
        let index = ScipIndexIngest {
            documents: vec![
                doc("crates/a/tests/it.rs", SCIP_ROLE_DEFINITION),
                doc("src/uses.rs", 0),
            ],
        };
        let conn = wiring_conn();
        let report = ingest_scip_bytes(&conn, &index.encode_to_vec(), "/ws").unwrap();
        assert_eq!(report.edges_written, 0, "{report:?}");
        assert_eq!(report.gated_edges_skipped, 1);

        let index = ScipIndexIngest {
            documents: vec![
                doc("src/a.rs", SCIP_ROLE_DEFINITION),
                doc("@companion/skills/x/b.rs", 0),
            ],
        };
        let report = ingest_scip_bytes(&conn, &index.encode_to_vec(), "/ws").unwrap();
        assert_eq!(report.edges_written, 0, "{report:?}");
        assert_eq!(report.gated_edges_skipped, 1);
    }

    #[test]
    fn ingest_writes_only_the_true_cross_file_edge() {
        let conn = wiring_conn();
        let report = ingest_scip_bytes(&conn, &fixture_index(), "/ws").unwrap();
        assert_eq!(report.documents, 2);
        assert_eq!(report.definitions, 1);
        assert_eq!(
            report.edges_written, 1,
            "only the resolved call becomes an edge"
        );
        assert_eq!(report.external_refs_skipped, 2, "std macro + local skipped");
        assert_eq!(report.self_edges_skipped, 0);

        let row: (String, String, String, String) = conn
            .query_row(
                "SELECT module_file, consumer_file, symbol_name, contract_source FROM wiring_map",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(row.0, "src/a.rs");
        assert_eq!(row.1, "src/b.rs");
        assert_eq!(
            row.2, "compute",
            "symbol_name must be the bare identifier AST producer rows use — \
             a descriptor here can never clear an orphan (S2)"
        );
        assert_eq!(row.3, "scip_resolved");

        // Re-ingest replaces rather than duplicates.
        let again = ingest_scip_bytes(&conn, &fixture_index(), "/ws").unwrap();
        assert_eq!(again.edges_written, 1);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM wiring_map", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    /// Opt-in proof against real `rust-analyzer scip` output (not CI-run: the
    /// file is machine-local). Run explicitly:
    /// `cargo test -p touring-hook-runtime decode_real -- --ignored`
    #[test]
    #[ignore = "requires a generated index.scip at the workspace root"]
    fn decode_real_rust_analyzer_index() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../index.scip");
        let data = std::fs::read(&path).expect("index.scip at workspace root");
        let index = ScipIndexIngest::decode(data.as_slice()).expect("real SCIP decodes");
        assert!(
            index.documents.len() > 1_000,
            "workspace has thousands of documents"
        );
        let defs = index
            .documents
            .iter()
            .flat_map(|d| d.occurrences.iter())
            .filter(|o| o.is_definition())
            .count();
        assert!(
            defs > 10_000,
            "workspace has tens of thousands of definitions"
        );
        println!("documents={} definitions={defs}", index.documents.len());
    }

    #[test]
    fn descriptor_extraction_handles_canonical_and_fallback() {
        assert_eq!(
            descriptor_of("rust-analyzer cargo touring-cli 30.4.0 cli/memory/derive_tags()."),
            "cli/memory/derive_tags()."
        );
        assert_eq!(descriptor_of("local 0"), "local 0");
    }

    #[test]
    fn identifier_extraction_matches_ast_symbol_names() {
        assert_eq!(identifier_of("cli/memory/derive_tags()."), "derive_tags");
        assert_eq!(identifier_of("a/compute()."), "compute");
        assert_eq!(identifier_of("foo/Bar#"), "Bar");
        assert_eq!(identifier_of("foo/Bar#method()."), "method");
        assert_eq!(identifier_of("macros/println!"), "println");
        assert_eq!(identifier_of("plain"), "plain");
    }
}
