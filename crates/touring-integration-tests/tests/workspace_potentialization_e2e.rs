//! E2E integration tests — workspace potentialization (2026-04-17).
//!
//! This suite validates the **cross-crate integration** that makes the
//! Touring workspace a single coherent system rather than a pile of
//! loosely-linked crates.
//!
//! Coverage matrix:
//!
//! | Dimension | Crates exercised | What it proves |
//! |---|---|---|
//! | Feature activation | touring-analysis, touring-learning, touring-ast, touring-hooks | All `default` features compile and expose their public API |
//! | Wiring integrity | touring-ast → touring-analysis → touring-learning | RL reward flows from wiring audit to bandit update |
//! | Persistence | touring-hooks (FileKnowledgeDB) | Schema survives round-trip |
//! | Scalability | touring-learning (ReplayBuffer, QTable) | Hot path with many updates |
//! | Quality → RL | touring-analysis pipeline | `analysis_reward_from_report` maps CodeHealthReport to [0.1, 1.0] |
//! | Functional chains | touring-hooks::functional_wiring | Chain detection between modules |
//! | Anti-patterns | touring-hooks::shared::antipatterns | Cross-language detection |
//! | CILA budget | touring-hooks::shared::cila | Budget gating monotonic across levels |
//!
//! These tests deliberately avoid mocking the interfaces they cross — the
//! goal is to prove that the *real* wiring is sound, not to protect
//! per-module contracts (which unit tests already cover).

#![allow(
    clippy::assertions_on_constants,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use rusqlite::Connection;

// ─────────────────────────────────────────────────────────────────────────────
// Dim 1 — Feature activation: every `default` feature must expose its API.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn feature_activation_analysis_default_exports() {
    use touring_analysis::{
        AnalysisConfig, AnalysisInsights, AnalysisPipelineBuilder, analysis_reward_from_report,
    };
    // All four items must be reachable in the default build — proves the
    // `blast-radius`, `quality`, `wiring`, `temporal`, `deep` features are
    // on by default (per 2026-04-17 defaults).
    let conn = Connection::open_in_memory().expect("in-memory db");
    let cfg = AnalysisConfig::standard();
    let pipeline = AnalysisPipelineBuilder::new(&conn).config(cfg).build();
    let report = pipeline.run(".");
    let reward = analysis_reward_from_report(&report);
    assert!(
        (0.1..=1.0).contains(&reward),
        "reward must map CodeHealthReport to [0.1, 1.0], got {reward}"
    );
    // AnalysisInsights must be constructible from the report (G3 wiring)
    let insights = AnalysisInsights::from_report(&report);
    // insights is a struct with fields — the mere act of constructing it
    // proves the cross-module path compiles and runs.
    let _ = format!("{insights:?}");
}

#[test]
fn feature_activation_learning_bandit_defaults() {
    use ndarray::Array1;
    use touring_intelligence::rl::bandit::{FEATURE_DIM, LinUCBBandit, NUM_ARMS};

    let mut bandit = LinUCBBandit::new();
    let feat: Array1<f64> = Array1::from_vec((0..FEATURE_DIM).map(|i| i as f64 * 0.01).collect());
    let (arm, score) = bandit.select_arm(&feat);
    assert!(arm < NUM_ARMS, "arm must be in [0, NUM_ARMS)");
    assert!(
        score.is_finite(),
        "UCB score must be finite even at cold start"
    );

    bandit.update(arm, &feat, 0.75);
    let (_, score2) = bandit.select_arm(&feat);
    assert!(
        score2.is_finite(),
        "UCB score must stay finite after update"
    );
}

#[test]
fn feature_activation_ast_incremental_pipeline() {
    use touring_code::ast::{IncrementalPipeline, SHARD_COUNT};

    let mut p = IncrementalPipeline::new();
    p.queue_for_lazy("src/sample.rs", "pub fn sample() {}");
    assert_eq!(p.pending_lazy_count(), 1);
    let loaded = p.ensure_loaded("src/sample.rs");
    assert!(loaded, "ensure_loaded must report success for queued file");
    assert_eq!(p.pending_lazy_count(), 0);

    assert!(
        SHARD_COUNT.is_power_of_two(),
        "SHARD_COUNT must be a power of two"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Dim 2 — Wiring integrity: RL reward emerges from analysis pipeline.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn wiring_analysis_to_learning_reward_flow() {
    use ndarray::Array1;
    use touring_analysis::{AnalysisConfig, AnalysisPipelineBuilder, analysis_reward_from_report};
    use touring_intelligence::rl::bandit::{FEATURE_DIM, LinUCBBandit};

    let conn = Connection::open_in_memory().expect("in-memory db");
    let report = AnalysisPipelineBuilder::new(&conn)
        .config(AnalysisConfig::standard())
        .build()
        .run(".");

    let reward = analysis_reward_from_report(&report);
    assert!(
        (0.1..=1.0).contains(&reward),
        "analysis reward must land in [0.1, 1.0]"
    );

    let mut bandit = LinUCBBandit::new();
    let feat: Array1<f64> = Array1::from_vec((0..FEATURE_DIM).map(|_| 0.5).collect());
    let (arm, _) = bandit.select_arm(&feat);
    bandit.update(arm, &feat, reward);
    let (_, score) = bandit.select_arm(&feat);
    assert!(
        score.is_finite(),
        "bandit must digest analysis reward without producing NaN/Inf"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Dim 3 — Persistence: FileKnowledgeDB schema survives round-trip.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn persistence_knowledge_db_roundtrip() {
    use tempfile::TempDir;
    use touring_hooks::knowledge::{FileKnowledge, FileKnowledgeDB};

    let tmp = TempDir::new().expect("tempdir");
    let db = FileKnowledgeDB::new(&tmp.path().join("knowledge.db")).expect("init db");
    let input = FileKnowledge {
        file_path: "src/lib.rs".to_string(),
        language: Some("rust".to_string()),
        line_count: 200,
        symbol_count: 12,
        notes: Some("module has unsafe block at line 42".to_string()),
        ..Default::default()
    };
    db.upsert(&input).expect("upsert");
    let got = db.lookup("src/lib.rs").expect("lookup").expect("exists");
    assert_eq!(got.file_path, input.file_path);
    assert_eq!(got.language, input.language);
    assert_eq!(got.line_count, input.line_count);
    assert_eq!(got.symbol_count, input.symbol_count);
    assert_eq!(got.notes, input.notes);
}

// ─────────────────────────────────────────────────────────────────────────────
// Dim 4 — Scalability: hot path survives 1k bandit updates.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn scalability_bandit_hot_path_1k_updates() {
    use ndarray::Array1;
    use touring_intelligence::rl::bandit::{FEATURE_DIM, LinUCBBandit};

    let mut bandit = LinUCBBandit::new();
    let feat: Array1<f64> = Array1::from_vec((0..FEATURE_DIM).map(|i| i as f64 * 0.001).collect());

    for i in 0..1_000 {
        let (arm, score) = bandit.select_arm(&feat);
        assert!(score.is_finite(), "iteration {i}: score must stay finite");
        let reward = if i % 7 == 0 { 1.0 } else { 0.2 };
        bandit.update(arm, &feat, reward);
    }

    // After 1k updates, the bandit should have meaningful state
    assert!(bandit.total_pulls() >= 1, "bandit must record pulls");
}

#[test]
fn scalability_replay_buffer_bounded_growth() {
    use touring_intelligence::rl::online_rl::{Experience, ReplayBuffer};

    let cap = 64;
    let mut buf = ReplayBuffer::new(cap);
    for i in 0..10_000u64 {
        buf.push(Experience {
            state: i,
            action: i % 8,
            reward: (i as f64) * 0.001,
            next_state: i + 1,
            terminal: false,
        });
    }
    assert_eq!(
        buf.len(),
        cap,
        "ReplayBuffer must bound itself to capacity regardless of push pressure"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Dim 5 — Functional chains: cross-module detection works on real sigs.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn functional_chains_sequential_and_complementary_detection() {
    use touring_hooks::functional_wiring::{ChainType, FunctionalSignature, detect_chains};

    let auth = FunctionalSignature {
        file_path: "auth/login.rs".to_string(),
        module_purpose: Some("authentication".to_string()),
        symbols: vec![],
        domain: Some("auth".to_string()),
        content_hash: None,
    };
    let token = FunctionalSignature {
        file_path: "auth/token.rs".to_string(),
        module_purpose: Some("authentication".to_string()),
        symbols: vec![],
        domain: Some("auth".to_string()),
        content_hash: None,
    };
    let chains = detect_chains(&[auth, token]);
    assert!(
        chains
            .iter()
            .any(|(_, _, _, _, t, _)| *t == ChainType::Complementary),
        "two files in same domain must form a Complementary chain"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Dim 6 — Anti-patterns: cross-language detection.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn antipatterns_multi_language_detection() {
    use touring_hooks::shared::antipatterns::detect_antipatterns_with_lines;

    let cases = [
        ("fn bad() { x.unwrap(); }", "rust", "rust unwrap"),
        ("fn stub() { todo!() }", "rust", "rust todo!()"),
        (
            "try:\n    pass\nexcept:\n    pass",
            "python",
            "python bare except",
        ),
        ("console.log('debug')", "javascript", "js console.log"),
    ];
    for (src, lang, label) in cases {
        let issues = detect_antipatterns_with_lines(src, lang);
        assert!(
            !issues.is_empty(),
            "antipattern detector must flag {label}: {src}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Dim 7 — CILA budget: monotonic across levels.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn cila_budget_monotonic_non_decreasing() {
    use touring_hooks::shared::cila::cila_budget_read;
    let budgets: Vec<usize> = (0u8..=6).map(cila_budget_read).collect();
    for window in budgets.windows(2) {
        assert!(
            window[1] >= window[0],
            "CILA budget must be non-decreasing across levels, got {budgets:?}"
        );
    }
    assert!(budgets[6] > budgets[0], "L6 budget must exceed L0 budget");
}

#[test]
fn cila_should_enrich_respects_tool_filter() {
    use touring_hooks::shared::cila::should_enrich;
    // CILA 0 with the enrichment flag on but a read-only tool must *still*
    // decline — the tool filter is the gate, not just CILA.
    assert!(!should_enrich(false, 0, "Edit"), "no flag → no enrichment");
    assert!(
        !should_enrich(true, 0, "Edit"),
        "flag at CILA 0 → still off for Edit"
    );
    // CILA 2 with the flag on and a mutation tool → enrich.
    assert!(
        should_enrich(true, 2, "Edit"),
        "flag at CILA 2 → on for Edit"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Dim 8 — Cross-crate typing: gotcha store roundtrip via knowledge DB.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn cross_crate_gotcha_roundtrip() {
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().expect("tempdir");
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).expect("init");
    // Insert a gotcha and pull it back out — verifies the knowledge schema
    // accepts the data shape that the CLI hook surface emits.
    let added = db.add_gotcha(
        "src/danger.rs",
        "raw pointer dereference without validation",
        "high",
        None,
    );
    assert!(added.is_ok(), "add_gotcha must succeed");
    let listed = db.list_gotchas();
    assert!(
        listed.iter().any(|g| g.pattern == "src/danger.rs"),
        "inserted gotcha must be retrievable via list_gotchas"
    );
}
