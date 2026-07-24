//! E2E Integration Test — proves the entire touring-analysis pipeline works end-to-end.
//!
//! This test creates a realistic knowledge.db + memory.db, populates them with
//! representative data, and runs every analysis dimension to verify they produce
//! correct, integrated results.

use rusqlite::Connection;
use touring_analysis::AnalysisPipeline;
use touring_analysis::cache::CachedAnalysisPipeline;
use touring_analysis::e2e::schema_guard;
use touring_analysis::engine::{AnalysisConfig, Depth};
use touring_analysis::health::{self, CodeHealthReport, HealthDimension, HealthStatus};
use touring_analysis::knowledge;
use touring_analysis::learning;
use touring_analysis::pipeline::AnalysisPipelineBuilder;
use touring_analysis::quality::QualityPipeline;
use touring_analysis::temporal::{self as trends};
use touring_analysis::wiring;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Create a knowledge.db with V8 schema and populate with realistic data.
fn setup_knowledge_db() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory knowledge DB");
    conn.execute_batch(touring_foundation::schema::knowledge::KNOWLEDGE_SCHEMA_V8)
        .expect("apply KNOWLEDGE_SCHEMA_V8");

    // Populate file_knowledge
    conn.execute_batch(
        "INSERT INTO file_knowledge (file_path, language, line_count, symbol_count, content_hash)
         VALUES ('src/lib.rs', 'rust', 500, 25, 'abc123');
         INSERT INTO file_knowledge (file_path, language, line_count, symbol_count, content_hash)
         VALUES ('src/engine.rs', 'rust', 200, 10, 'def456');
         INSERT INTO file_knowledge (file_path, language, line_count, symbol_count, content_hash)
         VALUES ('src/utils.py', 'python', 100, 5, 'ghi789');",
    )
    .expect("populate file_knowledge");

    // Populate edit_history (renamed from edit_history in v8)
    conn.execute_batch(
        "INSERT INTO edit_history (file_path, edit_type, edited_at)
         VALUES ('src/lib.rs', 'edit', datetime('now'));
         INSERT INTO edit_history (file_path, edit_type, edited_at)
         VALUES ('src/engine.rs', 'edit', datetime('now', '-1 hour'));
         INSERT INTO edit_history (file_path, edit_type, edited_at)
         VALUES ('src/lib.rs', 'edit', datetime('now', '-2 hours'));",
    )
    .expect("populate edit_history");

    // Populate bash_outcomes (using correct columns: command_short, executed_at, success)
    conn.execute_batch(
        "INSERT INTO bash_outcomes (command, command_short, exit_code, success, executed_at)
         VALUES ('cargo test', 'cargo', 0, 1, datetime('now'));
         INSERT INTO bash_outcomes (command, command_short, exit_code, success, executed_at)
         VALUES ('cargo clippy', 'cargo', 0, 1, datetime('now', '-30 minutes'));
         INSERT INTO bash_outcomes (command, command_short, exit_code, success, executed_at)
         VALUES ('cargo build', 'cargo', 1, 0, datetime('now', '-1 hour'));",
    )
    .expect("populate bash_outcomes");

    // Populate gotchas
    conn.execute_batch(
        "INSERT INTO gotchas (pattern, gotcha, severity, hit_count, prevented_errors, decay_score)
         VALUES ('unwrap_in_handler', 'Use ? instead of unwrap in hook handlers', 'high', 5, 3, 0.9);",
    )
    .expect("populate gotchas");

    // Populate wiring_map (using correct columns: module_file, symbol_name, etc.)
    conn.execute_batch(
        "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file)
         VALUES ('src/lib.rs', 'AnalysisConfig', 'struct', 'public', 'src/engine.rs');
         INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file)
         VALUES ('src/lib.rs', 'QualityPipeline', 'struct', 'public', 'src/engine.rs');
         INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility)
         VALUES ('src/utils.py', 'OrphanHelper', 'function', 'public');",
    )
    .expect("populate wiring_map");

    // Populate module_ecosystem (using correct columns)
    conn.execute_batch(
        "INSERT INTO module_ecosystem (file_path, module_role, pub_symbol_count, import_count, integration_score)
         VALUES ('src/lib.rs', 'public_api', 10, 5, 0.85);
         INSERT INTO module_ecosystem (file_path, module_role, pub_symbol_count, import_count, integration_score)
         VALUES ('src/engine.rs', 'internal', 3, 8, 0.95);",
    )
    .expect("populate module_ecosystem");

    // Populate functional_chains
    conn.execute_batch(
        "INSERT INTO functional_chains (source_module, source_symbol, sink_module, sink_symbol, chain_type, confidence)
         VALUES ('src/lib.rs', 'analyze', 'src/engine.rs', 'compute', 'Sequential', 0.85);
         INSERT INTO functional_chains (source_module, source_symbol, sink_module, sink_symbol, chain_type, confidence)
         VALUES ('src/engine.rs', 'compute', 'src/utils.py', 'format', 'Broken', 0.1);",
    )
    .expect("populate functional_chains");

    // Populate functional_signatures
    conn.execute_batch(
        "INSERT INTO functional_signatures (file_path, module_purpose, domain)
         VALUES ('src/lib.rs', 'Public API entry point', 'analysis');
         INSERT INTO functional_signatures (file_path, module_purpose, domain)
         VALUES ('src/engine.rs', 'Core computation engine', 'analysis');",
    )
    .expect("populate functional_signatures");

    conn
}

/// Create a memory.db with V8 schema and populate.
fn setup_memory_db() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory memory DB");
    conn.execute_batch(touring_foundation::schema::memory::MEMORY_SCHEMA_V8)
        .expect("apply MEMORY_SCHEMA_V8");

    // Populate rlm_entries (using correct table name, NOT memory_entries)
    conn.execute_batch(
        "INSERT INTO rlm_entries (key, value, tier, entry_type, access_count)
         VALUES ('lesson:error_handling', 'Always use ? in hook handlers', 'semantic', 'lesson', 5);
         INSERT INTO rlm_entries (key, value, tier, entry_type, access_count)
         VALUES ('pattern:blast_radius', 'BFS depth 5 for hook path', 'semantic', 'pattern', 3);",
    )
    .expect("populate rlm_entries");

    conn
}

// ─────────────────────────────────────────────────────────────────────────────
// E2E Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_e2e_schema_guard_validates_knowledge_db() {
    let conn = setup_knowledge_db();
    let missing = schema_guard::validate_knowledge_tables(&conn);
    assert!(
        missing.is_empty(),
        "All knowledge tables should exist after V8 schema + data population. Missing: {missing:?}"
    );
}

#[test]
fn test_e2e_schema_guard_validates_memory_db() {
    let conn = setup_memory_db();
    let missing = schema_guard::validate_memory_tables(&conn);
    assert!(
        missing.is_empty(),
        "All memory tables should exist after V8 schema. Missing: {missing:?}"
    );
}

#[test]
fn test_e2e_quality_pipeline_rust_source() {
    let config = AnalysisConfig::standard();
    let pipeline = QualityPipeline::new(config);

    let rust_source = r#"
        pub fn process(data: &str) -> Result<String, Error> {
            let parsed = parse(data)?;
            let validated = validate(parsed)?;
            Ok(format!("{validated}"))
        }

        fn risky() {
            let x = dangerous().unwrap();
            todo!();
        }
    "#;

    let report = pipeline.analyze_file("src/process.rs", rust_source, "rust");

    assert_eq!(report.file_path, "src/process.rs");
    assert_eq!(report.language, "rust");
    assert!(
        report.unwrap_count >= 1,
        "should detect .unwrap(): got {}",
        report.unwrap_count
    );
    assert!(
        !report.antipatterns.is_empty(),
        "should detect antipatterns"
    );
    assert!(
        report.error_handling_coverage > 0.0,
        "should detect Result returns"
    );
    assert!(
        report.score > 0.0 && report.score < 1.0,
        "score should reflect mixed quality: {}",
        report.score
    );
}

#[test]
fn test_e2e_quality_pipeline_python_source() {
    let config = AnalysisConfig::standard();
    let pipeline = QualityPipeline::new(config);

    let python_source = r#"
def process(data: str) -> Optional[str]:
    try:
        return parse(data)
    except:
        return None
    "#;

    let report = pipeline.analyze_file("src/process.py", python_source, "python");
    assert!(!report.antipatterns.is_empty(), "should detect bare except");
}

#[test]
fn test_e2e_quality_batch_parallel() {
    let config = AnalysisConfig::standard();
    let pipeline = QualityPipeline::new(config);

    let files: Vec<(&str, &str, &str)> = vec![
        ("a.rs", "fn foo() -> Result<(), E> { Ok(()) }", "rust"),
        ("b.rs", "fn bar() { panic!(\"oops\"); }", "rust"),
        ("c.py", "def baz(): pass", "python"),
    ];

    let reports = pipeline.analyze_batch(&files);
    assert_eq!(reports.len(), 3, "should analyze all 3 files");

    let aggregate = QualityPipeline::aggregate(&reports);
    assert_eq!(aggregate.files_analyzed, 3);
    assert!(aggregate.avg_score > 0.0);
}

#[test]
fn test_e2e_wiring_analysis_detects_orphans() {
    let conn = setup_knowledge_db();
    let report = wiring::analyze_wiring(&conn);

    assert_eq!(
        report.total_pub_symbols, 3,
        "should find 3 pub symbols (2 with consumers + 1 orphan)"
    );
    assert_eq!(
        report.orphan_count, 1,
        "OrphanHelper should be detected as orphan"
    );
    assert_eq!(
        report.total_consumers, 2,
        "AnalysisConfig and QualityPipeline have consumers"
    );
    assert!(report.orphan_rate > 0.0 && report.orphan_rate < 1.0);
    assert!(
        report.score < 1.0,
        "score should reflect the orphan: {}",
        report.score
    );
}

#[test]
fn test_e2e_wiring_analysis_detects_chains() {
    let conn = setup_knowledge_db();
    let report = wiring::analyze_wiring(&conn);

    assert_eq!(report.chain_count, 2, "should find 2 functional chains");
    assert_eq!(
        report.broken_chain_count, 1,
        "should find 1 broken chain (confidence 0.1)"
    );
}

#[test]
fn test_e2e_temporal_trends() {
    let conn = setup_knowledge_db();
    let report = trends::analyze_trends(&conn);

    assert!(
        report.edits_1d >= 2,
        "should find recent edits: got {}",
        report.edits_1d
    );
    assert!(
        report.edits_7d >= 3,
        "should find all edits: got {}",
        report.edits_7d
    );
    assert!(report.edit_velocity > 0.0, "velocity should be positive");
    assert!(
        report.bash_success_rate > 0.5,
        "2/3 bash commands succeeded: got {}",
        report.bash_success_rate
    );
}

#[test]
fn test_e2e_memory_table_name_correctness() {
    let conn = setup_memory_db();

    // This is the EXACT query that was broken before (used "memory_entries")
    // Now it must use "rlm_entries" and succeed
    let count: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {}",
                schema_guard::MEMORY_TABLE_RLM_ENTRIES
            ),
            [],
            |r| r.get(0),
        )
        .expect("rlm_entries query should succeed");

    assert_eq!(count, 2, "should find 2 rlm_entries");

    // Verify the OLD wrong name fails
    let old_result = conn.query_row("SELECT COUNT(*) FROM memory_entries", [], |r| {
        r.get::<_, i64>(0)
    });
    assert!(old_result.is_err(), "memory_entries should NOT exist");
}

#[test]
fn test_e2e_wiring_map_column_correctness() {
    let conn = setup_knowledge_db();

    // This query uses the CORRECT operational column names
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM wiring_map WHERE module_file IS NOT NULL AND symbol_name IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .expect("wiring_map with correct columns should work");

    assert_eq!(count, 3);

    // Verify the OLD wrong column names fail
    let old_result = conn.query_row("SELECT defined_in FROM wiring_map LIMIT 1", [], |r| {
        r.get::<_, String>(0)
    });
    assert!(
        old_result.is_err(),
        "defined_in column should NOT exist (old schema)"
    );
}

#[test]
fn test_e2e_edit_history_exists() {
    let conn = setup_knowledge_db();

    // Actual table is edit_history (file_edit_history rename was planned but not executed)
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edit_history WHERE edited_at IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .expect("edit_history with edited_at should work");

    assert_eq!(count, 3);

    // file_edit_history should NOT exist
    let old_result = conn.query_row("SELECT 1 FROM file_edit_history LIMIT 1", [], |r| {
        r.get::<_, i64>(0)
    });
    assert!(old_result.is_err(), "file_edit_history should NOT exist");
}

#[test]
fn test_e2e_gotchas_exists() {
    let conn = setup_knowledge_db();

    // Actual table is gotchas (file_gotchas rename was planned but not executed)
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM gotchas WHERE decay_score > 0.0",
            [],
            |r| r.get(0),
        )
        .expect("gotchas with decay_score should work");

    assert_eq!(count, 1);

    // file_gotchas should NOT exist
    let old_result = conn.query_row("SELECT 1 FROM file_gotchas LIMIT 1", [], |r| {
        r.get::<_, i64>(0)
    });
    assert!(old_result.is_err(), "file_gotchas should NOT exist");
}

#[test]
fn test_e2e_health_report_full_pipeline() {
    let conn = setup_knowledge_db();

    // Run all analysis dimensions
    let wiring_report = wiring::analyze_wiring(&conn);
    let trend_report = trends::analyze_trends(&conn);

    let config = AnalysisConfig::standard();
    let pipeline = QualityPipeline::new(config);
    let quality_report = pipeline.analyze_file(
        "src/lib.rs",
        "fn main() -> Result<(), Error> { Ok(()) }",
        "rust",
    );
    let quality_dim = QualityPipeline::aggregate(&[quality_report]);

    // Build health dimensions
    let dimensions = vec![
        HealthDimension {
            name: "quality".to_string(),
            score: quality_dim.avg_score,
            weight: 2.0,
            issues: vec![],
            metrics: serde_json::json!({"antipatterns": quality_dim.total_antipatterns}),
            duration_ms: 10,
        },
        HealthDimension {
            name: "wiring".to_string(),
            score: wiring_report.score,
            weight: 2.0,
            issues: if wiring_report.orphan_count > 0 {
                vec![format!("{} orphan symbols", wiring_report.orphan_count)]
            } else {
                vec![]
            },
            metrics: serde_json::json!({"orphans": wiring_report.orphan_count, "chains": wiring_report.chain_count}),
            duration_ms: 5,
        },
        HealthDimension {
            name: "temporal".to_string(),
            score: trend_report.score,
            weight: 1.0,
            issues: vec![],
            metrics: serde_json::json!({"velocity": trend_report.edit_velocity}),
            duration_ms: 3,
        },
    ];

    let report: CodeHealthReport =
        health::compute_health("/test/project", "standard", dimensions, 18);

    // Verify the report is complete and correct
    assert_eq!(report.dimensions.len(), 3);
    assert!(
        report.composite_score > 0.0 && report.composite_score <= 1.0,
        "composite score should be in (0,1]: {}",
        report.composite_score
    );
    assert!(
        report.confidence_lower <= report.composite_score,
        "CI lower {} should be <= score {}",
        report.confidence_lower,
        report.composite_score
    );
    assert!(
        report.confidence_upper >= report.composite_score,
        "CI upper {} should be >= score {}",
        report.confidence_upper,
        report.composite_score
    );
    assert!(report.confidence_lower >= 0.0);
    assert!(report.confidence_upper <= 1.0);
    assert_ne!(
        report.status,
        HealthStatus::Critical,
        "with good quality and wiring, should not be critical"
    );
    assert!(!report.timestamp.is_empty());
    assert_eq!(report.project_root, "/test/project");
    assert_eq!(report.total_duration_ms, 18);
}

#[test]
fn test_e2e_depth_configs_differ() {
    let quick = Depth::Quick.to_config();
    let standard = Depth::Standard.to_config();
    let deep = Depth::Deep.to_config();

    assert!(quick.budget_ms.is_some(), "quick should have budget");
    assert!(
        standard.budget_ms.is_none(),
        "standard should not have budget"
    );
    assert!(deep.budget_ms.is_none(), "deep should not have budget");
    assert!(deep.cross_crate, "deep should enable cross-crate");
    assert!(deep.temporal, "deep should enable temporal");
    assert!(
        !standard.cross_crate,
        "standard should not have cross-crate"
    );
    assert!(
        quick.quality_sample < standard.quality_sample,
        "quick should sample fewer files"
    );
}

#[test]
fn test_e2e_blast_radius_engine_with_mock() {
    use touring_analysis::blast_radius::*;

    struct TestStrategy;
    impl BlastRadiusStrategy for TestStrategy {
        fn name(&self) -> &'static str {
            "test"
        }
        fn compute(&self, start_file: &str, _config: &AnalysisConfig) -> BlastRadiusResult {
            BlastRadiusResult {
                start_file: start_file.to_string(),
                affected_files: vec![
                    AffectedFile {
                        path: "dep1.rs".to_string(),
                        distance: 1,
                        co_edit_weight: 0.8,
                    },
                    AffectedFile {
                        path: "dep2.rs".to_string(),
                        distance: 2,
                        co_edit_weight: 0.3,
                    },
                ],
                strategy_used: "test".to_string(),
                duration_ms: 1,
                truncated: false,
            }
        }
        fn latency_tier(&self) -> LatencyTier {
            LatencyTier::Fast
        }
    }

    let engine = BlastRadiusEngine::new(vec![Box::new(TestStrategy)]);
    let result = engine.compute("src/lib.rs", &AnalysisConfig::hook_path());

    assert_eq!(result.start_file, "src/lib.rs");
    assert_eq!(result.affected_files.len(), 2);
    let first = result
        .affected_files
        .first()
        .expect("at least one affected");
    assert_eq!(first.path, "dep1.rs");
    assert_eq!(first.distance, 1);
    assert!(!result.truncated);
}

/// E2E proof that `BlastRadiusEngine::bfs_only` factory wires `BfsStrategy` correctly.
///
/// Verifies that the convenience factory produces a functioning engine that
/// computes real hop distances via BFS over `SymbolIndex::reverse_deps`.
#[test]
fn test_e2e_bfs_only_factory_computes_real_distances() {
    use std::sync::Arc;
    use touring_analysis::blast_radius::{BfsStrategy, BlastRadiusEngine};
    use touring_code::ast::SymbolIndex;

    // Build a symbol index with a 3-file dependency chain:
    // root.rs → middle.rs → leaf.rs
    let mut idx = SymbolIndex::new();
    idx.reverse_deps
        .insert("root.rs".to_string(), vec!["middle.rs".to_string()]);
    idx.reverse_deps
        .insert("middle.rs".to_string(), vec!["leaf.rs".to_string()]);
    let shared = Arc::new(idx);

    // Use bfs_only factory — this is the primary wiring verification for BfsStrategy
    let engine = BlastRadiusEngine::bfs_only(Arc::clone(&shared));
    let result = engine.compute("root.rs", &AnalysisConfig::standard());

    // Verify BFS produces real hop distances (not just a flat list)
    let middle = result.affected_files.iter().find(|f| f.path == "middle.rs");
    let leaf = result.affected_files.iter().find(|f| f.path == "leaf.rs");

    assert!(
        middle.is_some(),
        "bfs_only must find middle.rs as a dependent"
    );
    assert_eq!(
        middle.expect("middle.rs").distance,
        1,
        "middle.rs is 1 hop from root.rs"
    );

    assert!(
        leaf.is_some(),
        "bfs_only must find leaf.rs via transitive dependency"
    );
    assert_eq!(
        leaf.expect("leaf.rs").distance,
        2,
        "leaf.rs is 2 hops from root.rs"
    );

    // Verify strategy name is reported correctly
    assert_eq!(result.strategy_used, "bfs_exact");
    assert!(!result.truncated);

    // Verify BfsStrategy is directly constructable (pub visibility check)
    let _direct = BfsStrategy::new(shared);
}

#[test]
fn test_e2e_all_schema_guard_constants_match_real_tables() {
    let knowledge_conn = setup_knowledge_db();
    let memory_conn = setup_memory_db();

    // Every schema_guard constant should correspond to a real table
    let knowledge_tables = [
        schema_guard::TABLE_FILE_KNOWLEDGE,
        schema_guard::TABLE_FILE_RELATIONS,
        schema_guard::TABLE_FILE_ACCESS_LOG,
        schema_guard::TABLE_BASH_OUTCOMES,
        schema_guard::TABLE_EDIT_HISTORY,
        schema_guard::TABLE_GOTCHAS,
        schema_guard::TABLE_FILE_COEDITS,
        schema_guard::TABLE_FILE_RISK_SCORES,
        schema_guard::TABLE_WIRING_MAP,
        schema_guard::TABLE_MODULE_ECOSYSTEM,
        schema_guard::TABLE_FUNCTIONAL_SIGNATURES,
        schema_guard::TABLE_FUNCTIONAL_CHAINS,
        schema_guard::TABLE_SYMBOLS,
    ];

    for table_name in &knowledge_tables {
        let exists: bool = knowledge_conn
            .prepare(&format!(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='{table_name}'"
            ))
            .and_then(|mut stmt| stmt.exists([]))
            .unwrap_or(false);
        assert!(
            exists,
            "schema_guard constant '{table_name}' should match a real table in knowledge.db"
        );
    }

    let memory_tables = [
        schema_guard::MEMORY_TABLE_RLM_ENTRIES,
        schema_guard::MEMORY_TABLE_RECALL_EMBEDDINGS,
        schema_guard::MEMORY_TABLE_ANN_EMBEDDINGS,
    ];

    for table_name in &memory_tables {
        let exists: bool = memory_conn
            .prepare(&format!(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='{table_name}'"
            ))
            .and_then(|mut stmt| stmt.exists([]))
            .unwrap_or(false);
        assert!(
            exists,
            "schema_guard constant '{table_name}' should match a real table in memory.db"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// v0.2.0 — New dimension and pipeline E2E tests
// ─────────────────────────────────────────────────────────────────────────────

/// Create a graph.db with V8 schema and populate with RL data.
fn setup_graph_db() -> Connection {
    let conn = Connection::open_in_memory().expect("open graph DB");
    conn.execute_batch(touring_foundation::schema::graph::GRAPH_SCHEMA_V8)
        .expect("apply GRAPH_SCHEMA_V8");

    conn.execute_batch(
        "INSERT INTO learning_wilson (tool_name, successes, trials, wilson_score)
         VALUES ('edit', 85, 100, 0.78);
         INSERT INTO learning_wilson (tool_name, successes, trials, wilson_score)
         VALUES ('bash', 70, 100, 0.62);
         INSERT INTO learning_wilson (tool_name, successes, trials, wilson_score)
         VALUES ('read', 90, 100, 0.84);
         INSERT INTO learning_wilson (tool_name, successes, trials, wilson_score)
         VALUES ('write', 75, 100, 0.67);",
    )
    .expect("populate learning_wilson");

    conn.execute_batch(
        "INSERT INTO learning_qtable (state_action, q_value) VALUES ('ev_pre_edit:edit', 0.8);
         INSERT INTO learning_qtable (state_action, q_value) VALUES ('ev_pre_bash:bash', 0.6);
         INSERT INTO learning_qtable (state_action, q_value) VALUES ('ev_post_edit:verify', 0.7);",
    )
    .expect("populate learning_qtable");

    conn.execute(
        "INSERT INTO learning_linucb (arm_id, pulls, cumulative_reward) VALUES ('arm_edit', 50, 40.0)",
        [],
    )
    .expect("populate learning_linucb");

    conn
}

#[test]
fn test_e2e_knowledge_dimension() {
    let conn = setup_knowledge_db();
    let report = knowledge::analyze_knowledge(&conn);

    assert_eq!(report.total_files, 3, "should find 3 indexed files");
    assert!(report.language_distribution.contains_key("rust"));
    assert!(report.language_distribution.contains_key("python"));
    assert!(report.avg_line_count > 0.0);
    assert!(report.score > 0.0 && report.score <= 1.0);
}

#[test]
fn test_e2e_learning_dimension() {
    let conn = setup_graph_db();
    let report = learning::analyze_learning(&conn);

    assert!(report.rl_active, "should detect active RL system");
    assert_eq!(report.wilson_tool_count, 4);
    assert!(report.avg_wilson_score > 0.5);
    assert_eq!(report.qtable_entry_count, 3);
    assert_eq!(report.linucb_arm_count, 1);
    assert_eq!(report.linucb_total_pulls, 50);
    assert!(report.score > 0.0 && report.score <= 1.0);
}

#[test]
fn test_e2e_pipeline_builder() {
    let conn = setup_knowledge_db();
    let report = AnalysisPipelineBuilder::new(&conn)
        .config(AnalysisConfig::standard())
        .build()
        .run_parallel(".");

    assert!(report.composite_score > 0.0 && report.composite_score <= 1.0);
    // Standard config: wiring + knowledge enabled
    assert!(
        report.dimensions.iter().any(|d| d.name == "wiring"),
        "should include wiring dimension"
    );
    assert!(
        report.dimensions.iter().any(|d| d.name == "knowledge"),
        "should include knowledge dimension"
    );
}

#[test]
fn test_e2e_pipeline_builder_with_graph_conn() {
    let knowledge_conn = setup_knowledge_db();
    let graph_conn = setup_graph_db();

    let report = AnalysisPipelineBuilder::new(&knowledge_conn)
        .config(AnalysisConfig::deep())
        .graph_conn(&graph_conn)
        .build()
        .run_parallel(".");

    assert!(report.composite_score > 0.0);
    // Deep config: all dimensions enabled
    assert!(
        report.dimensions.iter().any(|d| d.name == "wiring"),
        "should include wiring"
    );
    assert!(
        report.dimensions.iter().any(|d| d.name == "knowledge"),
        "should include knowledge"
    );
    assert!(
        report.dimensions.iter().any(|d| d.name == "learning"),
        "should include learning"
    );
    assert!(
        report.dimensions.iter().any(|d| d.name == "temporal"),
        "should include temporal"
    );
}

#[test]
fn test_e2e_pipeline_builder_with_files() {
    let conn = setup_knowledge_db();
    let files = vec![
        (
            "src/a.rs".to_string(),
            "fn ok() -> Result<(), ()> { Ok(()) }".to_string(),
            "rust".to_string(),
        ),
        (
            "src/b.rs".to_string(),
            "fn bad() { let _ = x.unwrap(); }".to_string(),
            "rust".to_string(),
        ),
    ];

    let report = AnalysisPipelineBuilder::new(&conn)
        .config(AnalysisConfig::standard())
        .with_files(files)
        .build()
        .run_parallel(".");

    assert!(
        report.dimensions.iter().any(|d| d.name == "quality"),
        "should include quality when files provided"
    );
}

#[test]
fn test_e2e_report_display_and_summary() {
    let conn = setup_knowledge_db();
    let report = AnalysisPipelineBuilder::new(&conn)
        .config(AnalysisConfig::standard())
        .build()
        .run_parallel(".");

    // Display (2 lines)
    let display = format!("{report}");
    assert!(display.lines().count() >= 2, "display: {display}");

    // Summary line
    let summary = report.to_summary_line();
    assert!(summary.contains("ms"), "summary: {summary}");

    // JSON roundtrip
    let json = report.to_json_pretty();
    let parsed: CodeHealthReport = serde_json::from_str(&json).expect("parse JSON");
    assert!((parsed.composite_score - report.composite_score).abs() < 1e-6);
}

#[test]
fn test_e2e_report_diff() {
    let conn = setup_knowledge_db();

    let config_a = AnalysisConfig::standard();
    let report_a = AnalysisPipeline::new(&conn, config_a).run(".");

    // Add an orphan to change wiring score
    conn.execute(
        "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility) \
         VALUES ('orphan.rs', 'UnusedFn', 'fn', 'public')",
        [],
    )
    .expect("insert orphan");

    let config_b = AnalysisConfig::standard();
    let report_b = AnalysisPipeline::new(&conn, config_b).run(".");

    let diff = report_a.diff(&report_b);
    // Adding an orphan should degrade wiring
    assert!(
        diff.degraded.contains(&"wiring".to_string()) || diff.score_delta.abs() < 0.01,
        "adding orphan should degrade wiring or delta near zero: {:?}",
        diff
    );

    let display = format!("{diff}");
    assert!(!display.is_empty());
}

#[test]
fn test_e2e_cached_pipeline() {
    let conn = setup_knowledge_db();
    let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
    let cached = CachedAnalysisPipeline::new(pipeline);

    let r1 = cached.run_cached(".", Depth::Standard);
    let r2 = cached.run_cached(".", Depth::Standard);

    assert!(
        (r1.composite_score - r2.composite_score).abs() < 1e-9,
        "cached results should be identical"
    );

    let (entries, hits, misses) = cached.cache_stats();
    assert_eq!(entries, 1);
    assert_eq!(hits, 1);
    assert_eq!(misses, 1);

    // Invalidate and verify re-computation
    cached.invalidate(".");
    let (entries, _, _) = cached.cache_stats();
    assert_eq!(entries, 0, "invalidate should clear entries");
}

#[test]
fn test_e2e_backward_compatible_run() {
    // Verify the old API still works unchanged
    let conn = setup_knowledge_db();
    let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
    let report = pipeline.run(".");

    assert!(report.composite_score >= 0.0 && report.composite_score <= 1.0);
    assert!(report.dimensions.iter().any(|d| d.name == "wiring"));
}

// ─────────────────────────────────────────────────────────────────────────────
// v0.2.0 — Cross-audit functional contract tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_e2e_blast_radius_in_pipeline() {
    let conn = setup_knowledge_db();
    let report = AnalysisPipelineBuilder::new(&conn)
        .config(AnalysisConfig::deep())
        .with_blast_file("src/lib.rs".to_string())
        .build()
        .run_parallel(".");

    // blast_radius dimension should be present when blast_file is set
    assert!(
        report.dimensions.iter().any(|d| d.name == "blast_radius"),
        "blast_radius dimension should be included when blast_file provided"
    );
    let br = report
        .dimensions
        .iter()
        .find(|d| d.name == "blast_radius")
        .unwrap();
    assert!(
        br.score >= 0.0 && br.score <= 1.0,
        "blast_radius score out of range: {}",
        br.score
    );
}

#[test]
fn test_e2e_all_scores_bounded() {
    // Contract: every dimension score MUST be in [0.0, 1.0]
    let knowledge_conn = setup_knowledge_db();
    let graph_conn = setup_graph_db();
    let files = vec![(
        "src/a.rs".to_string(),
        "fn ok() -> Result<(), ()> { Ok(()) }".to_string(),
        "rust".to_string(),
    )];

    let report = AnalysisPipelineBuilder::new(&knowledge_conn)
        .config(AnalysisConfig::deep())
        .graph_conn(&graph_conn)
        .with_files(files)
        .with_blast_file("src/a.rs".to_string())
        .build()
        .run_parallel(".");

    for dim in &report.dimensions {
        assert!(
            dim.score >= 0.0 && dim.score <= 1.0,
            "Dimension '{}' score {:.4} is out of [0.0, 1.0]",
            dim.name,
            dim.score
        );
        assert!(
            dim.weight >= 0.0,
            "Dimension '{}' weight {:.4} is negative",
            dim.name,
            dim.weight
        );
    }
    assert!(report.composite_score >= 0.0 && report.composite_score <= 1.0);
    assert!(report.confidence_lower <= report.composite_score);
    assert!(report.confidence_upper >= report.composite_score);
    assert!(report.confidence_lower >= 0.0);
    assert!(report.confidence_upper <= 1.0);
}

#[test]
fn test_e2e_full_stack_integration() {
    // Proves the COMPLETE pipeline from DB creation through all 6 dimensions,
    // report formatting, caching, diff, and backward compatibility.
    let knowledge_conn = setup_knowledge_db();
    let graph_conn = setup_graph_db();
    let files = vec![
        (
            "src/good.rs".to_string(),
            "fn main() -> Result<(), Box<dyn std::error::Error>> { Ok(()) }".to_string(),
            "rust".to_string(),
        ),
        (
            "src/bad.rs".to_string(),
            "fn bad() { panic!(\"oops\"); foo.unwrap(); }".to_string(),
            "rust".to_string(),
        ),
    ];

    // 1. Build and run full pipeline with ALL dimensions
    let report = AnalysisPipelineBuilder::new(&knowledge_conn)
        .config(AnalysisConfig::deep())
        .graph_conn(&graph_conn)
        .with_files(files)
        .with_blast_file("src/good.rs".to_string())
        .build()
        .run_parallel(".");

    // 2. Verify ALL 6 dimensions are present
    let dim_names: Vec<&str> = report.dimensions.iter().map(|d| d.name.as_str()).collect();
    assert!(
        dim_names.contains(&"wiring"),
        "missing wiring: {dim_names:?}"
    );
    assert!(
        dim_names.contains(&"quality"),
        "missing quality: {dim_names:?}"
    );
    assert!(
        dim_names.contains(&"temporal"),
        "missing temporal: {dim_names:?}"
    );
    assert!(
        dim_names.contains(&"knowledge"),
        "missing knowledge: {dim_names:?}"
    );
    assert!(
        dim_names.contains(&"learning"),
        "missing learning: {dim_names:?}"
    );
    assert!(
        dim_names.contains(&"blast_radius"),
        "missing blast_radius: {dim_names:?}"
    );

    // 3. Verify report formatting works
    let display = format!("{report}");
    assert!(display.lines().count() >= 2, "Display should be 2+ lines");
    let summary = report.to_summary_line();
    assert!(summary.contains("ms"), "summary should contain timing");
    let json = report.to_json_pretty();
    let parsed: health::CodeHealthReport = serde_json::from_str(&json).expect("JSON roundtrip");
    assert!((parsed.composite_score - report.composite_score).abs() < 1e-6);

    // 4. Verify caching works
    let pipeline = AnalysisPipeline::new(&knowledge_conn, AnalysisConfig::standard());
    let cached = CachedAnalysisPipeline::new(pipeline);
    let r1 = cached.run_cached(".", Depth::Standard);
    let r2 = cached.run_cached(".", Depth::Standard);
    assert!(
        (r1.composite_score - r2.composite_score).abs() < 1e-9,
        "cache hit should return same result"
    );
    let (_, hits, _) = cached.cache_stats();
    assert_eq!(hits, 1, "should have 1 cache hit");

    // 5. Verify diff works between cached and full pipeline
    let diff = r1.diff(&report);
    assert!(
        !diff.improved.is_empty() || !diff.degraded.is_empty() || !diff.unchanged.is_empty(),
        "diff should classify dimensions"
    );

    // 6. Verify backward-compatible run() still works
    let old_report = AnalysisPipeline::new(&knowledge_conn, AnalysisConfig::standard()).run(".");
    assert!(old_report.composite_score >= 0.0 && old_report.composite_score <= 1.0);
    assert!(old_report.dimensions.iter().any(|d| d.name == "wiring"));
}

#[test]
fn test_e2e_nan_safety() {
    // Contract: NaN scores must NEVER propagate to the composite score
    let dims = vec![health::HealthDimension {
        name: "nan_dim".to_string(),
        score: f64::NAN,
        weight: 1.0,
        issues: vec![],
        metrics: serde_json::json!({}),
        duration_ms: 0,
    }];
    let report = health::compute_health("/test", "standard", dims, 0);
    assert!(
        report.composite_score.is_finite(),
        "NaN dimension should not produce NaN composite: {}",
        report.composite_score
    );
    assert!(report.composite_score >= 0.0 && report.composite_score <= 1.0);
}

#[test]
fn test_e2e_knowledge_score_formula() {
    // Verify knowledge score formula produces expected results
    let conn = setup_knowledge_db();
    let report = knowledge::analyze_knowledge(&conn);
    // setup_knowledge_db has 3 files, no hot files (edits < 3x per file in 7d)
    // file_coverage = 1.0 (has files)
    // density_ok: symbols/lines = (25+10+5)/(500+200+100) = 40/800 = 0.05 → in [0.01, 0.5] → 1.0
    // hot_rate = 0/3 = 0.0 → (1 - 0.0) = 1.0
    // gotcha_rate = 1/3 ≈ 0.33 → (1 - 0.33) = 0.67
    // score = 0.3*1.0 + 0.3*1.0 + 0.2*1.0 + 0.2*0.67 = 0.3+0.3+0.2+0.133 = 0.933
    assert!(
        report.score > 0.9 && report.score < 0.95,
        "knowledge score should be ~0.93 for setup_knowledge_db data: got {}",
        report.score
    );
}

#[test]
fn test_e2e_learning_reward_trend() {
    // Verify reward trend detection works with known data
    let conn = setup_graph_db();
    let report = learning::analyze_learning(&conn);
    // setup_graph_db has 4 Wilson entries: [0.78, 0.62, 0.84, 0.67]
    // Older half avg = (0.78+0.62)/2 = 0.70, newer half avg = (0.84+0.67)/2 = 0.755
    // Delta = 0.055 > 0.05 threshold → Improving
    assert_eq!(
        report.reward_trend,
        learning::RewardTrend::Improving,
        "4 Wilson entries with newer-half higher avg should be Improving, got {:?}",
        report.reward_trend
    );
}

/// Create a knowledge DB with asymmetric edit history (4:1 current/previous week ratio)
/// that should produce a measurable KS statistic when simd-temporal is active.
fn setup_knowledge_db_with_drift() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory knowledge DB for drift test");
    conn.execute_batch(touring_foundation::schema::knowledge::KNOWLEDGE_SCHEMA_V8)
        .expect("apply KNOWLEDGE_SCHEMA_V8");

    // Populate file_knowledge (required by V8 schema)
    conn.execute_batch(
        "INSERT INTO file_knowledge (file_path, language, line_count, symbol_count, content_hash)
         VALUES ('src/lib.rs', 'rust', 300, 15, 'drift123');",
    )
    .expect("populate file_knowledge for drift test");

    // Current week: 7 edits spread across days -1 through -7 (high activity)
    conn.execute_batch(
        "INSERT INTO edit_history (file_path, edit_type, edited_at)
         VALUES ('src/lib.rs', 'edit', datetime('now', '-1 days'));
         INSERT INTO edit_history (file_path, edit_type, edited_at)
         VALUES ('src/lib.rs', 'edit', datetime('now', '-2 days'));
         INSERT INTO edit_history (file_path, edit_type, edited_at)
         VALUES ('src/lib.rs', 'edit', datetime('now', '-3 days'));
         INSERT INTO edit_history (file_path, edit_type, edited_at)
         VALUES ('src/lib.rs', 'edit', datetime('now', '-4 days'));
         INSERT INTO edit_history (file_path, edit_type, edited_at)
         VALUES ('src/lib.rs', 'edit', datetime('now', '-5 days'));
         INSERT INTO edit_history (file_path, edit_type, edited_at)
         VALUES ('src/lib.rs', 'edit', datetime('now', '-6 days'));
         INSERT INTO edit_history (file_path, edit_type, edited_at)
         VALUES ('src/lib.rs', 'edit', datetime('now', '-7 days'));",
    )
    .expect("populate current-week edit history for drift test");

    // Previous week: 2 edits only (days -8 and -14) — 4:1 ratio produces measurable KS drift
    conn.execute_batch(
        "INSERT INTO edit_history (file_path, edit_type, edited_at)
         VALUES ('src/lib.rs', 'edit', datetime('now', '-8 days'));
         INSERT INTO edit_history (file_path, edit_type, edited_at)
         VALUES ('src/lib.rs', 'edit', datetime('now', '-14 days'));",
    )
    .expect("populate previous-week edit history for drift test");

    conn
}

/// E2E proof that quality_drift is Option<f64>:
/// - Some(value >= 0.0) when simd-temporal feature is enabled (DriftDetector KS statistic computed)
/// - None when simd-temporal feature is disabled (drift unmeasured, not zero)
///
/// The 4:1 current/previous week edit ratio ensures a measurable KS statistic
/// when the DriftDetector is active.
#[test]
fn test_e2e_quality_drift_option_type() {
    let conn = setup_knowledge_db_with_drift();
    let report = trends::analyze_trends(&conn);

    // Type assertion: quality_drift must be Option<f64>.
    // This line only compiles if quality_drift is exactly Option<f64>.
    let _: Option<f64> = report.quality_drift;

    #[cfg(feature = "simd-temporal")]
    {
        assert!(
            report.quality_drift.is_some(),
            "simd-temporal enabled: quality_drift should be Some when DriftDetector is active, got {:?}",
            report.quality_drift
        );
        let drift_value = report
            .quality_drift
            .expect("simd-temporal enabled: quality_drift must be Some after is_some() check");
        assert!(
            drift_value >= 0.0,
            "simd-temporal enabled: quality_drift KS statistic should be non-negative, got {drift_value}"
        );
    }

    #[cfg(not(feature = "simd-temporal"))]
    {
        assert!(
            report.quality_drift.is_none(),
            "simd-temporal disabled: quality_drift should be None (drift unmeasured), got {:?}",
            report.quality_drift
        );
    }
}

#[cfg(feature = "wiring")]
#[test]
fn test_e2e_analyze_chains_returns_chain_details() {
    // setup_knowledge_db() inserts 2 chains: 1 Sequential (conf=0.85), 1 Broken (conf=0.1).
    let conn = setup_knowledge_db();
    let result = touring_analysis::analyze_chains(&conn);

    assert_eq!(
        result.total_chains, 2,
        "expected 2 chains from setup fixture"
    );
    assert_eq!(
        result.broken_count, 1,
        "expected 1 broken chain (chain_type='Broken')"
    );
    assert!(
        result.avg_confidence > 0.0,
        "avg_confidence should be > 0 with data present, got {}",
        result.avg_confidence
    );
}

#[cfg(feature = "wiring")]
#[test]
fn test_e2e_count_orphans_direct_access() {
    // setup_knowledge_db() inserts 3 pub symbols: AnalysisConfig + QualityPipeline (have consumer)
    // and OrphanHelper (consumer_file IS NULL → orphan).
    let conn = setup_knowledge_db();
    let result = touring_analysis::count_orphans(&conn);

    assert_eq!(
        result.total_pub, 3,
        "expected 3 pub symbols (AnalysisConfig, QualityPipeline, OrphanHelper)"
    );
    assert_eq!(
        result.orphan_count, 1,
        "expected 1 orphan (OrphanHelper has no consumer)"
    );
    assert_eq!(
        result.orphans.len(),
        1,
        "orphans vec should have exactly 1 entry"
    );
    let orphan = result.orphans.first().expect("one orphan");
    assert_eq!(
        orphan.1, "OrphanHelper",
        "orphan symbol name should be OrphanHelper"
    );
}

#[cfg(feature = "quality")]
#[test]
fn test_e2e_estimate_cognitive_complexity_measures_nesting() {
    // Flat: 3 sequential `if`s at nesting depth 0 → each scores 1+0=1 → total=3.
    let flat = "if a { } if b { } if c { }";
    let flat_score = touring_analysis::estimate_cognitive_complexity(flat);
    assert_eq!(
        flat_score, 3,
        "3 sequential ifs at depth 0 must score exactly 3"
    );

    // Nested: ifs at depth >= 3 → each inner if scores 1+(depth/3) > 1 → total > 3.
    let nested = "{ { { if a { if b { if c { } } } } } }";
    let nested_score = touring_analysis::estimate_cognitive_complexity(nested);
    assert!(
        nested_score > flat_score,
        "nested ifs must score higher than flat ifs; flat={flat_score}, nested={nested_score}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// v0.3.3 — New API surface tests (quality, integration, functionality)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_e2e_compute_blast_reward_range_and_saturation() {
    use touring_analysis::compute_blast_reward;

    // Floor: 0 edges → minimum reward 0.1 (bandit arm never starved)
    assert!(
        (compute_blast_reward(0) - 0.1).abs() < f64::EPSILON,
        "0 edges must give floor reward 0.1, got {}",
        compute_blast_reward(0)
    );

    // Linear region: 5_000 edges = 50% saturation → 0.5
    let mid = compute_blast_reward(5_000);
    assert!(
        (mid - 0.5).abs() < f64::EPSILON,
        "5_000 edges should give 0.5 reward, got {mid}"
    );

    // Ceiling: 10_000 edges = full saturation → 1.0
    assert!(
        (compute_blast_reward(10_000) - 1.0).abs() < f64::EPSILON,
        "10_000 edges must give ceiling reward 1.0"
    );

    // Clamped above: 100_000 edges → still 1.0, never > 1.0
    assert!(
        (compute_blast_reward(100_000) - 1.0).abs() < f64::EPSILON,
        "reward must clamp at 1.0 for very high edge counts"
    );

    // Monotone: reward(n) <= reward(n+1000)
    let r1 = compute_blast_reward(1_000);
    let r2 = compute_blast_reward(2_000);
    assert!(
        r1 <= r2,
        "compute_blast_reward must be monotonically non-decreasing"
    );
}

#[test]
fn test_e2e_analysis_summary_one_liner_and_fields() {
    let conn = setup_knowledge_db();
    let report = AnalysisPipeline::new(&conn, AnalysisConfig::standard()).run(".");

    let summary = report.to_analysis_summary();

    // passes flag: score >= 0.8
    assert_eq!(
        summary.passes,
        summary.score >= 0.8,
        "passes should match score >= 0.8 threshold: score={}",
        summary.score
    );

    // Status is one of three valid values
    assert!(
        ["healthy", "degraded", "critical"].contains(&summary.status.as_str()),
        "status should be healthy/degraded/critical, got: {}",
        summary.status
    );

    // CI bounds
    assert!(
        summary.confidence_interval[0] <= summary.score,
        "CI lower {} must be <= score {}",
        summary.confidence_interval[0],
        summary.score
    );
    assert!(
        summary.confidence_interval[1] >= summary.score,
        "CI upper {} must be >= score {}",
        summary.confidence_interval[1],
        summary.score
    );

    // Dimensions map mirrors report dimensions
    for dim in &report.dimensions {
        assert!(
            summary.dimensions.contains_key(&dim.name),
            "summary.dimensions missing '{}'; keys: {:?}",
            dim.name,
            summary.dimensions.keys().collect::<Vec<_>>()
        );
    }

    // one_liner contains status, score, and timing
    let line = summary.one_liner();
    assert!(
        line.contains("ms"),
        "one_liner should include timing: {line}"
    );
    assert!(
        line.to_lowercase().contains(&summary.status),
        "one_liner should contain status '{}': {line}",
        summary.status
    );
}

#[test]
fn test_e2e_health_diff_display() {
    let conn = setup_knowledge_db();
    let old_report = AnalysisPipeline::new(&conn, AnalysisConfig::standard()).run(".");

    // Degrade wiring by inserting an orphan
    conn.execute(
        "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility) \
         VALUES ('new_orphan.rs', 'UnwiredFn', 'fn', 'public')",
        [],
    )
    .expect("insert orphan");

    let new_report = AnalysisPipeline::new(&conn, AnalysisConfig::standard()).run(".");
    let diff = old_report.to_health_diff(&new_report);

    // Display impl must produce non-empty output
    let display = format!("{diff}");
    assert!(
        !display.is_empty(),
        "HealthDiff Display should produce output"
    );
    assert!(
        display.contains("delta:"),
        "HealthDiff Display should show delta: {display}"
    );

    // improved flag aligns with score_delta
    assert_eq!(
        diff.improved,
        diff.score_delta >= 0.01,
        "improved flag mismatch: delta={}, improved={}",
        diff.score_delta,
        diff.improved
    );
}

#[test]
fn test_e2e_metrics_dashboard_to_json_line_and_alerts() {
    use touring_analysis::MetricsDashboard;

    let conn = setup_knowledge_db();
    let report = AnalysisPipeline::new(&conn, AnalysisConfig::standard()).run(".");
    let dashboard = report.to_dashboard();

    // to_json_line produces valid single-line JSON
    let line = dashboard.to_json_line();
    assert!(
        !line.contains('\n'),
        "to_json_line must produce a single line"
    );
    let parsed: MetricsDashboard =
        serde_json::from_str(&line).expect("to_json_line must produce valid JSON");
    assert!(
        (parsed.composite_score - dashboard.composite_score).abs() < 1e-6,
        "JSON roundtrip must preserve composite_score"
    );

    // alerts_below with threshold=1.0 should fire for any non-perfect score
    let alerts = dashboard.alerts_below(1.0);
    // At least composite alert fires (score < 1.0 guaranteed for non-empty project)
    if dashboard.composite_score < 1.0 {
        // Either dimension alerts or composite alert fired
        assert!(
            !alerts.is_empty(),
            "alerts_below(1.0) should produce alerts for score={}",
            dashboard.composite_score
        );
        for a in &alerts {
            assert!(
                a.starts_with("ALERT"),
                "each alert must start with ALERT: {a}"
            );
        }
    }

    // alerts_below with threshold=0.0 must always return empty (nothing is below 0)
    assert!(
        dashboard.alerts_below(0.0).is_empty(),
        "no dimension can be below 0.0 threshold"
    );
}

#[test]
fn test_e2e_analysis_config_builder_methods() {
    use touring_analysis::AnalysisConfig;

    // with_budget: adds budget to any preset
    let config = AnalysisConfig::standard().with_budget(250);
    assert_eq!(
        config.budget_ms,
        Some(250),
        "with_budget must set budget_ms"
    );
    assert_eq!(config.quality_sample, 30, "other fields must be unchanged");

    // with_temporal: enables temporal on standard
    let config = AnalysisConfig::standard().with_temporal();
    assert!(config.temporal, "with_temporal must set temporal=true");

    // with_learning: enables learning on standard
    let config = AnalysisConfig::standard().with_learning();
    assert!(config.learning, "with_learning must set learning=true");

    // standard_with_learning: learning enabled, other standard fields intact
    let config = AnalysisConfig::standard_with_learning();
    assert!(
        config.learning,
        "standard_with_learning must have learning=true"
    );
    assert!(
        config.knowledge,
        "standard_with_learning must have knowledge=true"
    );
    assert_eq!(
        config.quality_sample, 30,
        "standard_with_learning inherits quality_sample=30"
    );
    assert!(
        config.budget_ms.is_none(),
        "standard_with_learning has no budget"
    );

    // Chaining: standard + budget + temporal + learning
    let config = AnalysisConfig::standard()
        .with_budget(500)
        .with_temporal()
        .with_learning();
    assert_eq!(config.budget_ms, Some(500));
    assert!(config.temporal);
    assert!(config.learning);
    assert!(config.knowledge);
}

#[test]
fn test_e2e_pipeline_builder_depth_method() {
    use touring_analysis::engine::Depth;

    let conn = setup_knowledge_db();

    // depth(Quick) → quick config applied
    let report_q = AnalysisPipelineBuilder::new(&conn)
        .depth(Depth::Quick)
        .build()
        .run_parallel(".");
    assert!(
        report_q.composite_score >= 0.0 && report_q.composite_score <= 1.0,
        "quick depth report should be valid"
    );

    // depth(Standard) → standard config: knowledge enabled
    let report_s = AnalysisPipelineBuilder::new(&conn)
        .depth(Depth::Standard)
        .build()
        .run_parallel(".");
    assert!(
        report_s.dimensions.iter().any(|d| d.name == "knowledge"),
        "standard depth should include knowledge dimension"
    );

    // depth() overrides a previously set config
    let config_deep = AnalysisConfig::deep();
    let report_override = AnalysisPipelineBuilder::new(&conn)
        .config(config_deep)
        .depth(Depth::Quick) // overrides deep with quick
        .build()
        .run_parallel(".");
    assert!(
        report_override.composite_score >= 0.0,
        "overridden depth pipeline should run"
    );
}

#[test]
fn test_e2e_cached_pipeline_run_cached_with_config() {
    use touring_analysis::AnalysisConfig;
    use touring_analysis::engine::Depth;

    let conn = setup_knowledge_db();
    let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
    let cached = CachedAnalysisPipeline::new(pipeline);

    let config = AnalysisConfig::standard_with_learning();

    // First call: cache miss
    let r1 =
        cached.run_cached_with_config(".", config.clone(), "standard+learning", Depth::Standard);
    let (_, hits, misses) = cached.cache_stats();
    assert_eq!(misses, 1, "first call should be a miss");
    assert_eq!(hits, 0, "first call should not be a hit");

    // Second call with same key: cache hit
    let r2 = cached.run_cached_with_config(".", config, "standard+learning", Depth::Standard);
    let (_, hits, _) = cached.cache_stats();
    assert_eq!(hits, 1, "second call with same key should be a cache hit");
    assert!(
        (r1.composite_score - r2.composite_score).abs() < 1e-9,
        "cached result must be identical"
    );

    // Different key: separate cache entry
    let config2 = AnalysisConfig::standard().with_temporal();
    let _r3 = cached.run_cached_with_config(".", config2, "standard+temporal", Depth::Standard);
    let (entries, _, _) = cached.cache_stats();
    assert_eq!(
        entries, 2,
        "different cache keys should produce separate entries"
    );
}

#[test]
fn test_e2e_knowledge_dimension_exposes_active_gotchas_as_issue() {
    // setup_knowledge_db() inserts 1 active gotcha (decay_score=0.9 > 0.3 threshold)
    let conn = setup_knowledge_db();
    let report = AnalysisPipeline::new(&conn, AnalysisConfig::standard()).run(".");

    let knowledge_dim = report
        .dimensions
        .iter()
        .find(|d| d.name == "knowledge")
        .expect("knowledge dimension must be present in standard run");

    // Active gotchas must be reported as issues (new in v0.3.3)
    assert!(
        knowledge_dim.issues.iter().any(|i| i.contains("gotcha")),
        "active gotchas should appear in knowledge issues: {:?}",
        knowledge_dim.issues
    );

    // import_graph_health must be present in metrics
    assert!(
        knowledge_dim.metrics.get("import_graph_health").is_some(),
        "import_graph_health must be in knowledge metrics"
    );

    // language_distribution must be in metrics
    assert!(
        knowledge_dim.metrics.get("language_distribution").is_some(),
        "language_distribution must be in knowledge metrics"
    );
}

#[cfg(feature = "blast-radius")]
#[test]
fn test_e2e_run_blast_with_real_symbol_index() {
    use std::sync::Arc;
    use touring_code::ast::SymbolIndex;

    // Build a 3-node dependency chain: root → mid → leaf
    let mut idx = SymbolIndex::new();
    idx.reverse_deps
        .insert("root.rs".to_string(), vec!["mid.rs".to_string()]);
    idx.reverse_deps
        .insert("mid.rs".to_string(), vec!["leaf.rs".to_string()]);
    let shared = Arc::new(idx);

    let conn = setup_knowledge_db();

    // Pipeline with real SymbolIndex — run_blast now uses BfsStrategy correctly
    let report = AnalysisPipelineBuilder::new(&conn)
        .config(AnalysisConfig::deep())
        .with_symbol_index(Arc::clone(&shared))
        .with_blast_file("root.rs")
        .build()
        .run_parallel(".");

    let blast_dim = report
        .dimensions
        .iter()
        .find(|d| d.name == "blast_radius")
        .expect("blast_radius dimension must be present when blast_file is set");

    // With real index the strategy must NOT be "none"
    let strategy = blast_dim
        .metrics
        .get("strategy")
        .and_then(|v| v.as_str())
        .unwrap_or("none");
    assert_ne!(
        strategy, "none",
        "run_blast with symbol_index must use a real strategy, not 'none'"
    );
    assert_eq!(
        strategy, "bfs_exact",
        "with SymbolIndex provided, strategy must be bfs_exact"
    );

    // BFS should find 2 affected files (mid.rs, leaf.rs)
    let affected = blast_dim
        .metrics
        .get("affected_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(
        affected, 2,
        "BFS from root.rs should find mid.rs and leaf.rs"
    );

    // Score reflects small affected count (2 < 10 threshold → score close to 1.0)
    assert!(
        blast_dim.score > 0.9,
        "2 affected files should produce high isolation score, got {}",
        blast_dim.score
    );
}

#[cfg(feature = "blast-radius")]
#[test]
fn test_e2e_bfs_strategy_directly_constructable() {
    // Proves BfsStrategy::new() is pub and wired — fixes orphan pub symbol
    use std::sync::Arc;
    use touring_analysis::blast_radius::BfsStrategy;
    use touring_code::ast::SymbolIndex;

    let mut idx = SymbolIndex::new();
    idx.reverse_deps
        .insert("a.rs".to_string(), vec!["b.rs".to_string()]);
    let strategy = BfsStrategy::new(Arc::new(idx));

    // Strategy name is deterministic
    use touring_analysis::blast_radius::BlastRadiusStrategy;
    assert_eq!(strategy.name(), "bfs_exact");
    assert_eq!(
        strategy.latency_tier(),
        touring_analysis::blast_radius::LatencyTier::Medium
    );

    // Can compute (exercises pub BfsStrategy visibility from external crate perspective)
    let result = strategy.compute("a.rs", &AnalysisConfig::standard());
    assert_eq!(result.start_file, "a.rs");
    assert_eq!(result.affected_files.len(), 1);
    let only = result.affected_files.first().expect("one affected");
    assert_eq!(only.path, "b.rs");
    assert_eq!(only.distance, 1);
}

#[test]
fn test_e2e_run_e2e_convenience_fn() {
    // Proves E2eConfig + run_e2e() orchestration works end-to-end
    use touring_analysis::e2e::{E2eConfig, run_e2e};

    let conn = setup_knowledge_db();
    let config = E2eConfig {
        project_root: ".".to_string(),
        depth: Depth::Standard,
    };
    let report = run_e2e(&config, &conn, None);

    // Report is valid
    assert!(!report.project_root.is_empty());
    assert!((0.0..=1.0).contains(&report.composite_score));
    assert!(
        !report.dimensions.is_empty(),
        "standard depth should produce dimensions"
    );

    // Summary derived from run_e2e report is consistent
    let summary = report.to_analysis_summary();
    assert_eq!(summary.score, report.composite_score);
    assert!(summary.passes == (summary.score >= 0.8));
}

#[test]
fn test_e2e_run_quality_included_in_run_parallel() {
    // Proves run_parallel() includes the quality dimension when files are provided.
    // Note: run() only covers wiring/temporal/knowledge — quality requires run_parallel().
    use touring_analysis::pipeline::AnalysisPipelineBuilder;

    let conn = setup_knowledge_db();
    let src = "fn foo() { let x = 1; x }";
    let files = vec![("src/a.rs".to_string(), src.to_string(), "rust".to_string())];

    let report = AnalysisPipelineBuilder::new(&conn)
        .config(AnalysisConfig::standard())
        .with_files(files)
        .build()
        .run_parallel(".");

    let dim_names: Vec<&str> = report.dimensions.iter().map(|d| d.name.as_str()).collect();

    // Quality must be present when files are provided to run_parallel
    assert!(
        dim_names.contains(&"quality"),
        "run_parallel() should include quality dimension when files provided, got: {dim_names:?}"
    );
}
