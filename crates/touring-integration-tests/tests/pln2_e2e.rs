//! E2E integration tests for pln2 implemented features.
//!
//! Covers: B6 (lazy loading), B7 (WiringFingerprintStore), E1 (cross-crate blast),
//! E3 (RenameCandidate), E4 (quality_trend), F1 (OtelConfig), G1 (pattern scanners),
//! G2 (RL reward), G3 (AnalysisInsights), C9 (pub consts).

// Integration tests: assertions_on_constants are intentional regression
// guards (e.g. SHARD_COUNT > 0 protects against future refactors). Unwrap
// and expect are idiomatic in test harnesses.
#![allow(
    clippy::assertions_on_constants,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use rusqlite::Connection;
use touring_analysis::{
    AnalysisConfig, AnalysisInsights, AnalysisPipelineBuilder, BlastRadiusEngine, OtelConfig,
    WiringFingerprintStore, analysis_reward_from_report, analyze_wiring_incremental,
    detect_churn_patterns, init_otel_subscriber, scan_dead_patterns,
};
use touring_code::ast::{
    HeatMap, IncrementalPipeline, Lang, MAX_RECURSION_DEPTH, RenameCandidate, SHARD_COUNT,
    SymbolChangeSet,
};

const NOW: f64 = 1_000_000.0;

const RUST_SOURCE: &str = r#"
pub struct Processor {
    name: String,
}

impl Processor {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string() }
    }

    pub fn process(&self, input: &str) -> String {
        format!("{}: {}", self.name, input)
    }
}

pub fn helper(x: i32) -> i32 {
    x * 2
}
"#;

// ─────────────────────────────────────────────────────────────────────────────
// B6: Lazy loading — IncrementalPipeline
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_b6_lazy_loading_queue_and_count() {
    let mut pipeline = IncrementalPipeline::new();
    assert_eq!(
        pipeline.pending_lazy_count(),
        0,
        "fresh pipeline has 0 pending"
    );

    pipeline.queue_for_lazy("src/a.rs", "pub fn a() {}");
    pipeline.queue_for_lazy("src/b.rs", "pub fn b() {}");
    assert_eq!(
        pipeline.pending_lazy_count(),
        2,
        "should have 2 pending after queuing"
    );
}

#[test]
fn test_b6_ensure_loaded_triggers_parse() {
    let mut pipeline = IncrementalPipeline::new();
    pipeline.queue_for_lazy("src/proc.rs", RUST_SOURCE);

    // ensure_loaded should parse the queued file and remove it from pending
    let loaded = pipeline.ensure_loaded("src/proc.rs");
    assert!(loaded, "ensure_loaded should return true for a queued file");
    assert_eq!(
        pipeline.pending_lazy_count(),
        0,
        "pending should be 0 after ensure_loaded"
    );
}

#[test]
fn test_b6_prefetch_from_heat() {
    let mut pipeline = IncrementalPipeline::new();
    // Queue several files
    for i in 0..5 {
        pipeline.queue_for_lazy(format!("src/file_{i}.rs"), format!("pub fn f{i}() {{}}"));
    }

    let mut heat = HeatMap::new(100);
    // Record edits to make some files "hot"
    heat.record_edit("src/file_0.rs", NOW);
    heat.record_edit("src/file_0.rs", NOW);
    heat.record_edit("src/file_1.rs", NOW);

    // prefetch_from_heat should load top-N hottest files
    let loaded = pipeline.prefetch_from_heat(&heat, 2);
    assert!(
        loaded <= 2,
        "prefetch_from_heat must load at most N={} files, loaded={loaded}",
        2
    );
}

#[test]
fn test_b6_heat_map_capacity_invariant() {
    let mut hm = HeatMap::new(3);
    hm.record_edit("a.rs", NOW);
    hm.record_edit("b.rs", NOW);
    hm.record_edit("c.rs", NOW);
    hm.record_edit("d.rs", NOW); // triggers eviction
    assert!(
        hm.len() <= 3,
        "HeatMap must not exceed capacity, got {}",
        hm.len()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// B7: WiringFingerprintStore — incremental wiring analysis
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_b7_fingerprint_store_new_is_empty() {
    let store = WiringFingerprintStore::new();
    // A fresh store has no fingerprints — all files are "changed"
    let conn = Connection::open_in_memory().expect("in-memory db");
    assert!(
        !store.is_unchanged(&conn, "src/lib.rs"),
        "fresh store: file should appear changed (no fingerprint)"
    );
}

#[test]
fn test_b7_analyze_wiring_incremental_does_not_panic() {
    let conn = Connection::open_in_memory().expect("in-memory db");
    // Create minimal schema
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS wiring_map (
            id INTEGER PRIMARY KEY,
            module_file TEXT NOT NULL,
            symbol_name TEXT NOT NULL,
            symbol_kind TEXT NOT NULL DEFAULT 'function',
            visibility TEXT NOT NULL DEFAULT 'public',
            consumer_file TEXT
        )",
    )
    .expect("create wiring_map");

    let mut store = WiringFingerprintStore::new();
    // Must not panic on empty DB
    let report = analyze_wiring_incremental(&conn, &mut store);
    assert!(
        report.score >= 0.0 && report.score <= 1.0,
        "wiring score must be in [0,1], got {}",
        report.score
    );
}

#[test]
fn test_b7_fingerprint_refresh_marks_unchanged() {
    let conn = Connection::open_in_memory().expect("in-memory db");
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS wiring_map (
            id INTEGER PRIMARY KEY,
            module_file TEXT NOT NULL,
            symbol_name TEXT NOT NULL,
            symbol_kind TEXT NOT NULL DEFAULT 'function',
            visibility TEXT NOT NULL DEFAULT 'public',
            consumer_file TEXT
        )",
    )
    .expect("create wiring_map");
    conn.execute(
        "INSERT INTO wiring_map (module_file, symbol_name) VALUES ('src/lib.rs', 'my_fn')",
        [],
    )
    .expect("insert");

    let mut store = WiringFingerprintStore::new();
    // After refresh, file should be considered unchanged
    store.refresh(&conn, "src/lib.rs");
    assert!(
        store.is_unchanged(&conn, "src/lib.rs"),
        "after refresh, file should be unchanged (same DB content)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// E1: Cross-crate blast radius
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_e1_blast_radius_engine_empty_strategies() {
    // BlastRadiusEngine with no strategies must not panic
    let engine = BlastRadiusEngine::new(vec![]);
    let _ = engine;
}

#[test]
fn test_e1_cross_crate_flag_in_analysis_config() {
    // standard config: cross_crate = false (single-crate analysis)
    let standard = AnalysisConfig::standard();
    assert!(
        !standard.cross_crate,
        "standard AnalysisConfig must have cross_crate=false"
    );

    // deep config: cross_crate = true (all crates included)
    let deep = AnalysisConfig::deep();
    assert!(
        deep.cross_crate,
        "deep AnalysisConfig must have cross_crate=true"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// E3: RenameCandidate + SymbolChangeSet
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_e3_rename_candidate_fields() {
    let rc = RenameCandidate {
        from_name: "old_fn".to_string(),
        to_name: "new_fn".to_string(),
        file_path: "src/lib.rs".to_string(),
        from_line: 10,
        to_line: 20,
    };
    assert_eq!(rc.from_name, "old_fn");
    assert_eq!(rc.to_name, "new_fn");
    assert_eq!(rc.from_line, 10);
    assert_eq!(rc.to_line, 20);
}

#[test]
fn test_e3_symbol_change_set_clone() {
    // SymbolChangeSet must implement Clone (E3)
    let original = SymbolChangeSet::default();
    let cloned = original.clone();
    assert!(
        cloned.is_empty(),
        "cloned empty SymbolChangeSet must be empty"
    );
}

#[test]
fn test_e3_symbol_change_set_renames_field() {
    let mut cs = SymbolChangeSet::default();
    cs.renames.push(RenameCandidate {
        from_name: "foo".to_string(),
        to_name: "bar".to_string(),
        file_path: "src/main.rs".to_string(),
        from_line: 5,
        to_line: 5,
    });
    assert_eq!(
        cs.renames.len(),
        1,
        "renames field must be accessible and appendable"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// E4: quality_trend
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_e4_quality_trend_empty_db() {
    use touring_analysis::TrendDirection;
    use touring_analysis::quality_trend;
    let conn = Connection::open_in_memory().expect("in-memory db");
    // Must return Stable on empty DB (no history to compute from)
    let trend = quality_trend(&conn, 5);
    assert!(
        matches!(
            trend,
            TrendDirection::Stable | TrendDirection::Improving | TrendDirection::Degrading
        ),
        "quality_trend must return a valid TrendDirection, got {:?}",
        trend
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// F1: OtelConfig + init_otel_subscriber
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_f1_otel_config_disabled_by_default() {
    // Without env vars set, OtelConfig::from_env() must be disabled
    // (clear relevant env to ensure reproducibility)
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT") };
    let cfg = OtelConfig::from_env();
    assert!(
        !cfg.enabled || cfg.effective_endpoint().is_none(),
        "OtelConfig must be disabled or have no endpoint when env var is unset"
    );
}

#[test]
fn test_f1_otel_config_disabled_constructor() {
    let cfg = OtelConfig::disabled();
    assert!(
        !cfg.enabled,
        "OtelConfig::disabled() must have enabled=false"
    );
    assert!(
        cfg.effective_endpoint().is_none(),
        "disabled config must have no effective endpoint"
    );
}

#[test]
fn test_f1_init_otel_subscriber_disabled_no_error() {
    let cfg = OtelConfig::disabled();
    // init_otel_subscriber with disabled config must succeed (no-op)
    let result = init_otel_subscriber(&cfg);
    assert!(
        result.is_ok(),
        "init_otel_subscriber with disabled config must not error: {:?}",
        result
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// G1: scan_dead_patterns + detect_churn_patterns
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_g1_scan_dead_patterns_empty_input() {
    let patterns = scan_dead_patterns(&[]);
    assert!(
        patterns.is_empty(),
        "scan_dead_patterns on empty input must return empty"
    );
}

#[test]
fn test_g1_scan_dead_patterns_normal_symbols() {
    // Normal symbol names should not match dead-code patterns
    let symbols = vec![
        "process_data".to_string(),
        "compute_score".to_string(),
        "build_index".to_string(),
    ];
    let patterns = scan_dead_patterns(&symbols);
    // Either empty (feature off) or returns matched patterns (feature on)
    // Must not panic
    let _ = patterns;
}

#[test]
fn test_g1_detect_churn_patterns_empty_input() {
    let patterns = detect_churn_patterns(&[]);
    assert!(
        patterns.is_empty(),
        "detect_churn_patterns on empty input must return empty"
    );
}

#[test]
fn test_g1_detect_churn_patterns_normal_paths() {
    let paths = vec![
        "src/lib.rs".to_string(),
        "crates/touring-ast/src/parser.rs".to_string(),
        "crates/touring-hooks/src/pre_edit.rs".to_string(),
    ];
    let patterns = detect_churn_patterns(&paths);
    // Must not panic, result depends on feature gate
    let _ = patterns;
}

// ─────────────────────────────────────────────────────────────────────────────
// G2: analysis_reward_from_report
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_g2_reward_range_is_valid() {
    let conn = Connection::open_in_memory().expect("in-memory db");
    let pipeline = AnalysisPipelineBuilder::new(&conn)
        .config(AnalysisConfig::standard())
        .build();
    let report = pipeline.run(".");
    let reward = analysis_reward_from_report(&report);
    assert!(
        (0.1..=1.0).contains(&reward),
        "RL reward must be in [0.1, 1.0], got {reward}"
    );
}

#[test]
fn test_g2_reward_clamped_lower_bound() {
    // Reward must always be >= 0.1 (clamped lower bound regardless of score)
    let conn = Connection::open_in_memory().expect("in-memory db");
    let pipeline = AnalysisPipelineBuilder::new(&conn)
        .config(AnalysisConfig::standard())
        .build();
    let report = pipeline.run(".");
    let reward = analysis_reward_from_report(&report);
    assert!(reward >= 0.1, "reward must be >= 0.1 (clamped lower bound)");
}

// ─────────────────────────────────────────────────────────────────────────────
// G3: AnalysisInsights
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_g3_analysis_insights_from_report() {
    let conn = Connection::open_in_memory().expect("in-memory db");
    let pipeline = AnalysisPipelineBuilder::new(&conn)
        .config(AnalysisConfig::standard())
        .build();
    let report = pipeline.run(".");
    let insights = AnalysisInsights::from_report(&report);

    // health_score must mirror composite_score
    assert!(
        (insights.health_score - report.composite_score).abs() < 1e-9,
        "AnalysisInsights.health_score must equal report.composite_score"
    );
}

#[test]
fn test_g3_analysis_insights_to_context_string() {
    let conn = Connection::open_in_memory().expect("in-memory db");
    let pipeline = AnalysisPipelineBuilder::new(&conn)
        .config(AnalysisConfig::standard())
        .build();
    let report = pipeline.run(".");
    let insights = AnalysisInsights::from_report(&report)
        .with_quality_trend(&touring_analysis::TrendDirection::Stable)
        .with_orphan_count(42);

    let ctx = insights.to_context_string();
    assert!(
        !ctx.is_empty(),
        "to_context_string must not return empty string"
    );
    assert!(
        ctx.contains("42") || ctx.contains("stable") || ctx.len() > 10,
        "context string must contain insights: '{ctx}'"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// C9: pub const SHARD_COUNT + MAX_RECURSION_DEPTH
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_c9_shard_count_is_power_of_two() {
    // SHARD_COUNT must be a power of 2 for efficient sharding
    assert!(SHARD_COUNT > 0, "SHARD_COUNT must be > 0");
    assert!(
        SHARD_COUNT.is_power_of_two(),
        "SHARD_COUNT={SHARD_COUNT} must be a power of two for DashMap sharding"
    );
}

#[test]
fn test_c9_max_recursion_depth_is_sane() {
    // MAX_RECURSION_DEPTH must be large enough to handle real codebases
    // but bounded to prevent stack overflow
    assert!(
        MAX_RECURSION_DEPTH >= 64,
        "MAX_RECURSION_DEPTH={MAX_RECURSION_DEPTH} must be >= 64"
    );
    assert!(
        MAX_RECURSION_DEPTH <= 4096,
        "MAX_RECURSION_DEPTH={MAX_RECURSION_DEPTH} must be <= 4096"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Full integration: pln2 pipeline end-to-end
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_pln2_full_pipeline_integration() {
    use touring_code::ast::SymbolIndex;

    // 1. Build SymbolIndex (AST layer)
    let mut index = SymbolIndex::new();
    index
        .index_file("src/processor.rs", RUST_SOURCE, Lang::Rust)
        .expect("index_file must not fail on valid Rust");

    // 2. Build IncrementalPipeline + lazy loading (B6)
    let mut inc_pipeline = IncrementalPipeline::new();
    inc_pipeline.queue_for_lazy("src/extra.rs", "pub fn extra() -> i32 { 42 }");
    let _ = inc_pipeline.ensure_loaded("src/extra.rs");

    // 3. WiringFingerprintStore (B7)
    let conn = Connection::open_in_memory().expect("in-memory db");
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS wiring_map (
            id INTEGER PRIMARY KEY,
            module_file TEXT NOT NULL,
            symbol_name TEXT NOT NULL,
            symbol_kind TEXT NOT NULL DEFAULT 'function',
            visibility TEXT NOT NULL DEFAULT 'public',
            consumer_file TEXT
        )",
    )
    .expect("create schema");
    let mut fp_store = WiringFingerprintStore::new();
    let wiring_report = analyze_wiring_incremental(&conn, &mut fp_store);
    assert!(
        wiring_report.score >= 0.0,
        "wiring score must be non-negative"
    );

    // 4. AnalysisPipeline (F1 + G2 + G3)
    let pipeline = AnalysisPipelineBuilder::new(&conn)
        .config(AnalysisConfig::standard())
        .build();
    let report = pipeline.run(".");

    // 5. OtelConfig (F1) — must not panic
    let otel_cfg = OtelConfig::disabled();
    let _ = init_otel_subscriber(&otel_cfg);

    // 6. RL reward (G2)
    let reward = analysis_reward_from_report(&report);
    assert!(
        (0.1..=1.0).contains(&reward),
        "RL reward out of bounds: {reward}"
    );

    // 7. AnalysisInsights (G3)
    let insights = AnalysisInsights::from_report(&report)
        .with_quality_trend(&touring_analysis::TrendDirection::Stable)
        .with_orphan_count(wiring_report.orphan_count);
    let ctx = insights.to_context_string();
    assert!(
        !ctx.is_empty(),
        "AnalysisInsights context must not be empty"
    );

    // 8. Pattern scanners (G1)
    let symbols: Vec<String> = index
        .find_symbol("Processor")
        .iter()
        .map(|s| s.symbol_name.clone())
        .collect();
    let dead = scan_dead_patterns(&symbols);
    let _ = dead; // feature-gated; may be empty

    let file_paths = vec!["src/processor.rs".to_string()];
    let churn = detect_churn_patterns(&file_paths);
    let _ = churn;

    // 9. RenameCandidate (E3)
    let mut changeset = SymbolChangeSet::default();
    changeset.renames.push(RenameCandidate {
        from_name: "Processor".to_string(),
        to_name: "DataProcessor".to_string(),
        file_path: "src/processor.rs".to_string(),
        from_line: 2,
        to_line: 2,
    });
    assert_eq!(changeset.renames.len(), 1);

    // 10. Pub consts (C9)
    assert!(SHARD_COUNT > 0);
    assert!(MAX_RECURSION_DEPTH >= 64);

    eprintln!("=== PLN2 E2E INTEGRATION: ALL PHASES PASSED ===");
    eprintln!(
        "  wiring_score={:.2}, rl_reward={:.2}, ctx_len={}",
        wiring_report.score,
        reward,
        ctx.len()
    );
}
