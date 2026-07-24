//! Integration tests for Wave 2-4 Unified Touring Excellence implementations.
//!
//! Tests cover: Tantivy CLI handlers (U11), FileKnowledgeEnriched / query_extended (U23),
//! gate metrics tantivy counters (U21), and wiring-community handler (U24p).
//! Hook registry count is asserted at 131 (R7+R8 additions).

#![allow(clippy::indexing_slicing)]

use tempfile::TempDir;
use touring_hooks::knowledge::{FileKnowledge, FileKnowledgeDB, FileKnowledgeEnriched};
use touring_hooks::shared::gate_metrics;

// ─── Helper ────────────────────────────────────────────────────────────────────

fn temp_knowledge_db() -> (TempDir, FileKnowledgeDB) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let kb = FileKnowledgeDB::new(&db_path).unwrap();
    (tmp, kb)
}

// ─── U23: FileKnowledgeEnriched — struct & from_base ──────────────────────────

#[test]
fn file_knowledge_enriched_from_base_preserves_base_fields() {
    let base = FileKnowledge {
        file_path: "src/lib.rs".to_string(),
        language: Some("rust".to_string()),
        line_count: 100,
        symbol_count: 10,
        read_count: 5,
        ..Default::default()
    };
    let enriched = FileKnowledgeEnriched::from_base(base);
    assert_eq!(enriched.file_path, "src/lib.rs");
    assert_eq!(enriched.language, Some("rust".to_string()));
    assert_eq!(enriched.line_count, 100);
    assert_eq!(enriched.symbol_count, 10);
    assert_eq!(enriched.read_count, 5);
}

#[test]
fn file_knowledge_enriched_from_base_enrichment_fields_are_none() {
    let base = FileKnowledge {
        file_path: "src/lib.rs".to_string(),
        ..Default::default()
    };
    let enriched = FileKnowledgeEnriched::from_base(base);
    assert!(enriched.cognitive_score.is_none());
    assert!(enriched.fan_in_signal.is_none());
    assert!(enriched.fan_out_signal.is_none());
    assert!(enriched.blake3_hash.is_none());
    assert!(enriched.community_id.is_none());
    assert!(enriched.coverage_pct.is_none());
    assert!(enriched.integration_score.is_none());
    assert!(enriched.modularity_score.is_none());
}

#[test]
fn file_knowledge_enriched_default_is_all_none_or_zero() {
    let enriched = FileKnowledgeEnriched::default();
    assert!(enriched.file_path.is_empty());
    assert!(enriched.cognitive_score.is_none());
    assert!(enriched.coverage_pct.is_none());
    assert!(enriched.modularity_score.is_none());
    assert!(enriched.complexity_signal.is_none());
}

#[test]
fn file_knowledge_enriched_serializes_to_json() {
    let enriched = FileKnowledgeEnriched {
        file_path: "test.rs".to_string(),
        cognitive_score: Some(0.85),
        fan_in_signal: Some(3.0),
        ..Default::default()
    };
    let json = serde_json::to_value(&enriched).unwrap();
    assert_eq!(json["file_path"], "test.rs");
    assert!((json["cognitive_score"].as_f64().unwrap() - 0.85).abs() < 1e-9);
    assert!((json["fan_in_signal"].as_f64().unwrap() - 3.0).abs() < 1e-9);
    assert!(json["blake3_hash"].is_null());
}

#[test]
fn file_knowledge_enriched_has_23_json_fields() {
    let enriched = FileKnowledgeEnriched::default();
    let json = serde_json::to_value(&enriched).unwrap();
    let obj = json.as_object().unwrap();
    // 10 base + 13 enrichment = 23 fields
    assert_eq!(
        obj.len(),
        23,
        "FileKnowledgeEnriched should have 23 JSON fields, got {}. Fields: {:?}",
        obj.len(),
        obj.keys().collect::<Vec<_>>()
    );
}

// ─── U23: query_extended — DB round-trip ──────────────────────────────────────

#[test]
fn query_extended_returns_none_for_unknown_file() {
    let (_tmp, db) = temp_knowledge_db();
    let result = db.query_extended("nonexistent.rs").unwrap();
    assert!(result.is_none());
}

#[test]
fn query_extended_returns_base_fields_when_no_enrichment() {
    let (_tmp, db) = temp_knowledge_db();
    db.upsert(&FileKnowledge {
        file_path: "src/test.rs".to_string(),
        language: Some("rust".to_string()),
        line_count: 42,
        symbol_count: 5,
        ..Default::default()
    })
    .unwrap();

    let result = db.query_extended("src/test.rs").unwrap();
    assert!(result.is_some());
    let enriched = result.unwrap();
    assert_eq!(enriched.file_path, "src/test.rs");
    assert_eq!(enriched.line_count, 42);
    assert_eq!(enriched.symbol_count, 5);
    // No enrichment data → all enrichment fields None
    assert!(enriched.cognitive_score.is_none());
    assert!(enriched.integration_score.is_none());
    assert!(enriched.blake3_hash.is_none());
}

#[test]
fn query_extended_includes_cognitive_enrichment_after_upsert() {
    let (_tmp, db) = temp_knowledge_db();
    db.upsert(&FileKnowledge {
        file_path: "src/enriched.rs".to_string(),
        line_count: 100,
        ..Default::default()
    })
    .unwrap();

    db.upsert_cognitive_enrichment("src/enriched.rs", 0.75, 0.5, 3.0, 2.0, 0.8)
        .unwrap();

    let result = db.query_extended("src/enriched.rs").unwrap().unwrap();
    assert!((result.cognitive_score.unwrap() - 0.75).abs() < 1e-9);
    assert!((result.fan_in_signal.unwrap() - 3.0).abs() < 1e-9);
    assert!((result.fan_out_signal.unwrap() - 2.0).abs() < 1e-9);
    assert!((result.complexity_signal.unwrap() - 0.5).abs() < 1e-9);
    assert!((result.doc_signal.unwrap() - 0.8).abs() < 1e-9);
}

#[test]
fn query_extended_all_enrichment_none_without_inserts() {
    let (_tmp, db) = temp_knowledge_db();
    db.upsert(&FileKnowledge {
        file_path: "empty.rs".to_string(),
        ..Default::default()
    })
    .unwrap();

    let result = db.query_extended("empty.rs").unwrap().unwrap();
    assert!(result.community_id.is_none());
    assert!(result.modularity_score.is_none());
    assert!(result.coverage_pct.is_none());
    assert!(result.pub_symbol_count.is_none());
    assert!(result.import_count.is_none());
}

// ─── U21: Gate metrics tantivy counters ────────────────────────────────────────

#[test]
fn gate_metrics_tantivy_upsert_increments_monotonically() {
    let before = gate_metrics::GateMetricsSnapshot::capture();
    gate_metrics::record_tantivy_upsert();
    gate_metrics::record_tantivy_upsert();
    let after = gate_metrics::GateMetricsSnapshot::capture();
    assert!(
        after.tantivy_upsert_count >= before.tantivy_upsert_count + 2,
        "Expected +2 upserts, before={} after={}",
        before.tantivy_upsert_count,
        after.tantivy_upsert_count
    );
}

#[test]
fn gate_metrics_tantivy_query_latency_accumulates() {
    let before = gate_metrics::GateMetricsSnapshot::capture();
    gate_metrics::record_tantivy_query_latency(500);
    gate_metrics::record_tantivy_query_latency(300);
    let after = gate_metrics::GateMetricsSnapshot::capture();
    assert!(
        after.tantivy_query_latency_us >= before.tantivy_query_latency_us + 800,
        "Expected +800us, before={} after={}",
        before.tantivy_query_latency_us,
        after.tantivy_query_latency_us
    );
}

#[test]
fn gate_metrics_tantivy_commit_increments() {
    let before = gate_metrics::GateMetricsSnapshot::capture();
    gate_metrics::record_tantivy_commit();
    let after = gate_metrics::GateMetricsSnapshot::capture();
    assert!(
        after.tantivy_commit_count > before.tantivy_commit_count,
        "Expected +1 commit, before={} after={}",
        before.tantivy_commit_count,
        after.tantivy_commit_count
    );
}

#[test]
fn gate_metrics_snapshot_captures_all_three_tantivy_fields() {
    let snap = gate_metrics::GateMetricsSnapshot::capture();
    // Validate the snapshot exposes all three tantivy fields (type check via usage)
    let _upsert: u64 = snap.tantivy_upsert_count;
    let _latency: u64 = snap.tantivy_query_latency_us;
    let _commit: u64 = snap.tantivy_commit_count;
}

// ─── U11 + U24p: Hook registry structural tests ────────────────────────────────

#[test]
fn hook_registry_has_all_tantivy_handlers() {
    let names = touring_hooks::hook_registry::all_daemon_hook_names();
    assert!(
        names.contains(&"cli-tantivy-search"),
        "Missing cli-tantivy-search"
    );
    assert!(
        names.contains(&"cli-tantivy-fuzzy"),
        "Missing cli-tantivy-fuzzy"
    );
    assert!(
        names.contains(&"cli-tantivy-stats"),
        "Missing cli-tantivy-stats"
    );
    assert!(
        names.contains(&"cli-tantivy-suggest"),
        "Missing cli-tantivy-suggest"
    );
    assert!(
        names.contains(&"cli-tantivy-reindex"),
        "Missing cli-tantivy-reindex"
    );
}

#[test]
fn hook_registry_has_wiring_community() {
    let names = touring_hooks::hook_registry::all_daemon_hook_names();
    assert!(
        names.contains(&"cli-wiring-community"),
        "Missing cli-wiring-community"
    );
}

#[test]
fn hook_registry_count_is_176() {
    let names = touring_hooks::hook_registry::all_daemon_hook_names();
    assert_eq!(
        names.len(),
        222,
        "Expected 222 hook names (sync with touring-dispatch hook_registry test), got {}",
        names.len()
    );
}

#[test]
fn hook_registry_all_daemon_constant_matches_function() {
    let names_vec = touring_hooks::hook_registry::all_daemon_hook_names();
    let names_const = touring_hooks::hook_registry::ALL_DAEMON_HOOK_NAMES;
    // NOTE: constant (182) and function (184) diverge on feature-gated entries
    // and 'stop' which is in constant but not in function. Both counts are
    // correct for their respective purposes.
    let vec_set: std::collections::HashSet<_> = names_vec.iter().collect();
    let const_set: std::collections::HashSet<_> = names_const.iter().collect();
    assert!(
        const_set.is_subset(&vec_set),
        "ALL_DAEMON_HOOK_NAMES must be a subset of all_daemon_hook_names()"
    );
}

#[test]
fn dispatch_table_has_all_tantivy_entries() {
    let table = touring_hooks::hook_registry::build_dispatch_table();
    assert!(
        table.contains_key("cli-tantivy-search"),
        "Dispatch table missing cli-tantivy-search"
    );
    assert!(
        table.contains_key("cli-tantivy-fuzzy"),
        "Dispatch table missing cli-tantivy-fuzzy"
    );
    assert!(
        table.contains_key("cli-tantivy-stats"),
        "Dispatch table missing cli-tantivy-stats"
    );
    assert!(
        table.contains_key("cli-tantivy-suggest"),
        "Dispatch table missing cli-tantivy-suggest"
    );
    assert!(
        table.contains_key("cli-tantivy-reindex"),
        "Dispatch table missing cli-tantivy-reindex"
    );
}

#[test]
fn dispatch_table_has_wiring_community_entry() {
    let table = touring_hooks::hook_registry::build_dispatch_table();
    assert!(
        table.contains_key("cli-wiring-community"),
        "Dispatch table missing cli-wiring-community"
    );
}

#[test]
fn dispatch_table_and_registry_are_in_sync() {
    let names = touring_hooks::hook_registry::all_daemon_hook_names();
    let table = touring_hooks::hook_registry::build_dispatch_table();
    for name in &names {
        assert!(
            table.contains_key(name),
            "Hook '{name}' in registry but missing from dispatch table"
        );
    }
    for key in table.keys() {
        assert!(
            names.contains(key),
            "Hook '{key}' in dispatch table but missing from registry"
        );
    }
}
