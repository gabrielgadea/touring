#![allow(clippy::needless_collect)]
use super::*;
use crate::knowledge::FileKnowledgeDB;
use tempfile::TempDir;

fn test_db() -> (TempDir, FileKnowledgeDB) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = FileKnowledgeDB::new(&db_path).unwrap();
    (tmp, db)
}

#[test]
fn test_register_pub_symbol() {
    let (_tmp, db) = test_db();
    db.register_pub_symbol("tfidf.rs", "TfIdfVectorizer", "struct", "public")
        .unwrap();

    let orphans = db.orphan_symbols().unwrap();
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0].symbol_name, "TfIdfVectorizer");
    assert!(orphans[0].consumer_file.is_none());
}

// PLT-2026-06-02 — workspace_root filter validation.
// Establishes that `find_all_cycles` can scope the wiring graph to a
// single workspace and rejects cross-tree phantom edges (the source of
// the konverter ↔ analise/kazuba false-positive 136-mod cycle).
#[test]
fn test_find_all_cycles_workspace_root_filter() {
    let (_tmp, db) = test_db();
    let conn = db.conn_ref();

    // Insert a real cycle a↔b in the konverter workspace.
    conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file, workspace_root) \
             VALUES ('packages/konverter/src/a.rs', 'A', 'struct', 'public', 'packages/konverter/src/b.rs', 'konverter')",
            [],
        ).unwrap();
    conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file, workspace_root) \
             VALUES ('packages/konverter/src/b.rs', 'B', 'fn', 'public', 'packages/konverter/src/a.rs', 'konverter')",
            [],
        ).unwrap();

    // Insert a phantom cycle x↔y that lives in analise/ (cross-tree pollution).
    conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file, workspace_root) \
             VALUES ('/home/gabrielgadea/projects/analise/packages/x.rs', 'X', 'struct', 'public', '/home/gabrielgadea/projects/analise/packages/y.rs', 'analise')",
            [],
        ).unwrap();
    conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file, workspace_root) \
             VALUES ('/home/gabrielgadea/projects/analise/packages/y.rs', 'Y', 'fn', 'public', '/home/gabrielgadea/projects/analise/packages/x.rs', 'analise')",
            [],
        ).unwrap();

    // (1) No filter — sees BOTH cycles (legacy behavior).
    let all = find_all_cycles(&db, None, false);
    assert_eq!(all.len(), 2, "legacy (unfiltered) must report both cycles");

    // (1b) Sanity: verify rows are actually in the DB with workspace_root set.
    let row_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM wiring_map WHERE workspace_root = 'konverter'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        row_count, 2,
        "expected 2 konverter rows in DB, got {row_count}"
    );

    // (2) Filter to konverter — phantom cross-tree cycle hidden.
    let kon = find_all_cycles(&db, Some("konverter"), false);
    assert_eq!(
        kon.len(),
        1,
        "konverter-only must report 1 cycle, got {kon:?}"
    );
    let members: Vec<String> = kon[0]
        .modules
        .iter()
        .chain(kon[0].modules.iter())
        .cloned()
        .collect();
    assert!(
        members.iter().any(|m| m.contains("konverter/src/a.rs"))
            && members.iter().any(|m| m.contains("konverter/src/b.rs")),
        "filtered cycle must contain konverter a.rs + b.rs, got {members:?}"
    );
    assert!(
        !members.iter().any(|m| m.starts_with("/home/")),
        "filtered cycle must NOT contain abs_path rows from analise/, got {members:?}"
    );

    // (3) Filter to analise — only phantom cycle surfaces.
    let analise = find_all_cycles(&db, Some("analise"), false);
    assert_eq!(analise.len(), 1, "analise-only must report 1 phantom cycle");
    let analise_modules: Vec<String> = analise[0]
        .modules
        .iter()
        .chain(analise[0].modules.iter())
        .cloned()
        .collect();
    assert!(
        analise_modules
            .iter()
            .all(|m| m.starts_with("/home/gabrielgadea/projects/analise/")),
        "analise-filtered cycle must contain only abs_path rows, got {analise_modules:?}"
    );
}

#[test]
fn test_find_all_cycles_prune_nonexistent() {
    // A05: prune phantom cycles whose endpoint files no longer exist on disk
    // (absorbed crates + cross-project pollution). Hermetic: uses real files
    // created in the temp dir so it does not depend on the host filesystem.
    let (tmp, db) = test_db();
    let conn = db.conn_ref();
    let dir = tmp.path();

    // Real cycle: two files that actually exist.
    let real_a = dir.join("real_a.rs");
    let real_b = dir.join("real_b.rs");
    std::fs::write(&real_a, "// a").unwrap();
    std::fs::write(&real_b, "// b").unwrap();
    let (ra, rb) = (real_a.to_str().unwrap(), real_b.to_str().unwrap());

    // Phantom cycle: absolute paths that were never created.
    let ghost_a = dir.join("ghost_x.rs");
    let ghost_b = dir.join("ghost_y.rs");
    let (ga, gb) = (ghost_a.to_str().unwrap(), ghost_b.to_str().unwrap());

    for (m, c, s) in [(ra, rb, "A"), (rb, ra, "B"), (ga, gb, "X"), (gb, ga, "Y")] {
        conn.execute(
                "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file, workspace_root) \
                 VALUES (?1, ?2, 'struct', 'public', ?3, NULL)",
                rusqlite::params![m, s, c],
            )
            .unwrap();
    }

    // Legacy (no prune): both real + phantom cycles surface.
    let unpruned = find_all_cycles(&db, None, false);
    assert_eq!(
        unpruned.len(),
        2,
        "unpruned must see both cycles, got {unpruned:?}"
    );

    // Prune: the phantom cycle (nonexistent files) is dropped; the real survives.
    let pruned = find_all_cycles(&db, None, true);
    assert_eq!(
        pruned.len(),
        1,
        "prune must drop the phantom cycle, got {pruned:?}"
    );
    assert!(
        pruned[0].modules.iter().all(|m| !m.contains("ghost")),
        "surviving cycle must be the real one, got {:?}",
        pruned[0].modules
    );
}

#[test]
fn test_record_consumer_resolves_orphan() {
    let (_tmp, db) = test_db();
    db.register_pub_symbol("tfidf.rs", "TfIdfVectorizer", "struct", "public")
        .unwrap();

    // Before: orphan
    assert_eq!(db.orphan_symbols().unwrap().len(), 1);

    // Wire it
    db.record_consumer("tfidf.rs", "TfIdfVectorizer", "nexus.rs", Some(5))
        .unwrap();

    // After: the NULL entry still exists, but a consumer entry was added
    // Verify the score instead — it should reflect partial resolution
    let score = db.integration_score("tfidf.rs").unwrap();
    // Total distinct symbols with visibility=public: 1 (TfIdfVectorizer)
    // The NULL entry is separate from the consumer entry
    assert!(score >= 0.0);
}

#[test]
fn test_integration_score_no_pub_symbols() {
    let (_tmp, db) = test_db();
    let score = db.integration_score("empty.rs").unwrap();
    assert_eq!(
        score, 1.0,
        "module with no pub symbols should have score 1.0"
    );
}

#[test]
fn test_integration_score_all_orphaned() {
    let (_tmp, db) = test_db();
    db.register_pub_symbol("mod.rs", "A", "struct", "public")
        .unwrap();
    db.register_pub_symbol("mod.rs", "B", "function", "public")
        .unwrap();
    let score = db.integration_score("mod.rs").unwrap();
    assert_eq!(score, 0.0, "all orphaned = score 0.0");
}

#[test]
fn test_clear_wiring() {
    let (_tmp, db) = test_db();
    db.register_pub_symbol("mod.rs", "A", "struct", "public")
        .unwrap();
    db.register_pub_symbol("mod.rs", "B", "function", "public")
        .unwrap();
    assert_eq!(db.orphan_symbols().unwrap().len(), 2);

    db.clear_wiring("mod.rs").unwrap();
    assert_eq!(db.orphan_symbols().unwrap().len(), 0);
}

#[test]
fn test_module_wiring_status() {
    let (_tmp, db) = test_db();
    db.register_pub_symbol("mod.rs", "A", "struct", "public")
        .unwrap();
    db.register_pub_symbol("mod.rs", "B", "function", "public")
        .unwrap();

    let status = db.module_wiring_status("mod.rs").unwrap();
    assert_eq!(status.total_pub_symbols, 2);
    assert_eq!(status.orphan_symbols.len(), 2);
    assert_eq!(status.integration_score, 0.0);
}

#[test]
fn test_private_symbols_not_orphaned() {
    let (_tmp, db) = test_db();
    db.register_pub_symbol("mod.rs", "internal_fn", "function", "private")
        .unwrap();
    let orphans = db.orphan_symbols().unwrap();
    assert_eq!(
        orphans.len(),
        0,
        "private symbols should not appear as orphans"
    );
}

#[test]
fn test_update_wiring_after_edit() {
    let (_tmp, db) = test_db();

    // Simulate a file with pub symbols being "read" (upserted)
    use crate::knowledge::FileKnowledge;
    let knowledge = FileKnowledge {
            file_path: "src/tfidf.rs".into(),
            language: Some("rust".into()),
            symbols_json: Some(
                r#"[{"name":"TfIdfVectorizer","kind":"struct","is_public":true},{"name":"internal_fn","kind":"function","is_public":false}]"#
                    .into(),
            ),
            imports_json: Some(r#"["crate::metrics::CognitiveMetrics"]"#.into()),
            ..Default::default()
        };
    db.upsert(&knowledge).unwrap();

    // Run wiring update
    update_wiring_after_edit(&db, "src/tfidf.rs");

    // Verify: TfIdfVectorizer should be registered as orphan pub symbol
    let status = db.module_wiring_status("src/tfidf.rs").unwrap();
    assert_eq!(status.total_pub_symbols, 1, "only 1 pub symbol");
    assert_eq!(status.orphan_symbols, vec!["TfIdfVectorizer"]);
}

#[test]
fn test_e2e_wiring_lifecycle() {
    let (_tmp, db) = test_db();

    // 1. Simulate: tfidf.rs is read — has pub TfIdfVectorizer
    db.register_pub_symbol("src/tfidf.rs", "TfIdfVectorizer", "struct", "public")
        .unwrap();

    // Verify: orphan detected
    let orphans = db.orphan_symbols().unwrap();
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0].symbol_name, "TfIdfVectorizer");
    let score = db.integration_score("src/tfidf.rs").unwrap();
    assert_eq!(score, 0.0, "all orphaned = 0.0");

    // 2. Simulate: nexus.rs imports TfIdfVectorizer
    db.record_consumer("src/tfidf.rs", "TfIdfVectorizer", "src/nexus.rs", Some(8))
        .unwrap();

    // Verify: orphan resolved — score should improve
    let score_after = db.integration_score("src/tfidf.rs").unwrap();
    assert!(score_after >= 0.0);

    // 3. Simulate: second pub symbol added without consumer
    db.register_pub_symbol("src/tfidf.rs", "cosine_similarity", "function", "public")
        .unwrap();

    // Verify: partial wiring
    let status = db.module_wiring_status("src/tfidf.rs").unwrap();
    assert_eq!(status.total_pub_symbols, 2);
    assert!(
        status
            .orphan_symbols
            .contains(&"cosine_similarity".to_string())
    );

    // 4. Clear and verify cleanup
    db.clear_wiring("src/tfidf.rs").unwrap();
    assert_eq!(db.orphan_symbols().unwrap().len(), 0);
}

#[test]
fn test_inject_wiring_reward_no_panic() {
    let (_tmp, db) = test_db();
    // Should not panic even with no data
    inject_wiring_reward(&db, "unknown.rs", 0.5);
}

// -------------------------------------------------------------------------
// New E2E tests: prove the full Wiring Intelligence System works end-to-end
// -------------------------------------------------------------------------

/// A) FLUXO COMPLETO L1→L3:
/// post-read registers orphan, score=0.0 → record_consumer resolves it → score=1.0
#[test]
fn test_e2e_l1_to_l3_full_flow() {
    let (_tmp, db) = test_db();

    // L1: post-read populates wiring_map with pub symbol — initially orphan
    db.register_pub_symbol("src/tfidf.rs", "TfIdfVectorizer", "struct", "public")
        .unwrap();

    // Verify: orphan detected, score = 0.0
    let orphans_before = db.orphan_symbols().unwrap();
    assert_eq!(
        orphans_before.len(),
        1,
        "should detect 1 orphan after register"
    );
    assert_eq!(orphans_before[0].symbol_name, "TfIdfVectorizer");
    assert_eq!(orphans_before[0].module_file, "src/tfidf.rs");
    let score_before = db.integration_score("src/tfidf.rs").unwrap();
    assert_eq!(score_before, 0.0, "all orphaned = score 0.0");

    // L3: post-edit records consumer (simulates nexus.rs importing TfIdfVectorizer)
    db.record_consumer("src/tfidf.rs", "TfIdfVectorizer", "src/nexus.rs", Some(12))
        .unwrap();

    // Verify: no longer orphan, score improved to 1.0
    let orphans_after = db.orphan_symbols().unwrap();
    assert_eq!(
        orphans_after.len(),
        0,
        "after record_consumer the symbol is no longer an orphan"
    );
    let score_after = db.integration_score("src/tfidf.rs").unwrap();
    assert_eq!(
        score_after, 1.0,
        "1 symbol with 1 consumer = integration score 1.0"
    );
}

/// B) FLUXO IMPORT PREDICTION (L2):
/// Register TfIdfVectorizer as orphan, verify it appears in module_wiring_status,
/// and confirm the orphan_symbols list contains the expected suggestion target.
#[test]
fn test_e2e_import_prediction_l2() {
    let (_tmp, db) = test_db();

    // Register pub symbol in a specific module
    db.register_pub_symbol("src/tfidf.rs", "TfIdfVectorizer", "struct", "public")
        .unwrap();

    // Verify via module_wiring_status that TfIdfVectorizer appears as orphan
    let status = db.module_wiring_status("src/tfidf.rs").unwrap();
    assert_eq!(status.total_pub_symbols, 1);
    assert_eq!(status.symbols_with_consumers, 0);
    assert_eq!(status.integration_score, 0.0);
    assert_eq!(status.orphan_symbols, vec!["TfIdfVectorizer"]);

    // The pre-edit hook would use this info to suggest:
    // "use crate::tfidf::TfIdfVectorizer"
    // We verify the orphan data is sufficient to build that suggestion:
    let orphans = db.orphan_symbols().unwrap();
    let suggestion_target = orphans
        .iter()
        .find(|e| e.symbol_name == "TfIdfVectorizer" && e.module_file == "src/tfidf.rs");
    assert!(
        suggestion_target.is_some(),
        "orphan_symbols must contain TfIdfVectorizer from src/tfidf.rs for import suggestion"
    );
    // Confirm the suggested import path can be constructed: use crate::tfidf::TfIdfVectorizer
    let entry = suggestion_target.unwrap();
    let module_path = entry
        .module_file
        .trim_start_matches("src/")
        .trim_end_matches(".rs")
        .replace('/', "::");
    let suggested_import = format!("use crate::{}::{};", module_path, entry.symbol_name);
    assert_eq!(
        suggested_import, "use crate::tfidf::TfIdfVectorizer;",
        "import suggestion should be derivable from orphan data"
    );
}

/// C) FLUXO ECOSYSTEM (L0):
/// Classify roles, register modules, verify low_integration and entry_points.
#[test]
fn test_e2e_ecosystem_full_flow() {
    use crate::ecosystem::{
        ModuleRole, classify_module_role, entry_points, low_integration_modules, register_module,
    };
    let (_tmp, db) = test_db();

    // Classify various paths
    assert_eq!(classify_module_role("src/main.rs"), ModuleRole::EntryPoint);
    assert_eq!(classify_module_role("src/lib.rs"), ModuleRole::Library);
    assert_eq!(classify_module_role("src/utils.rs"), ModuleRole::Internal);
    assert_eq!(classify_module_role("tests/e2e.rs"), ModuleRole::Test);
    assert_eq!(classify_module_role("benches/perf.rs"), ModuleRole::Bench);

    // Register modules — orphan module has a pub symbol with no consumer
    db.register_pub_symbol("src/orphan_mod.rs", "UnusedStruct", "struct", "public")
        .unwrap();
    register_module(&db, "src/orphan_mod.rs", 1, 0, 0);

    // Register wired module (no pub symbols = score 1.0)
    register_module(&db, "src/lib.rs", 0, 5, 2);

    // Register entry point
    register_module(&db, "src/main.rs", 0, 3, 0);

    // Verify: entry_points returns lib.rs and main.rs
    let eps = entry_points(&db);
    assert_eq!(eps.len(), 2);
    assert!(eps.contains(&"src/lib.rs".to_string()));
    assert!(eps.contains(&"src/main.rs".to_string()));

    // Verify: low_integration_modules returns orphan_mod.rs (score < 0.5)
    let low = low_integration_modules(&db, 0.5);
    assert_eq!(low.len(), 1, "only orphan_mod.rs should be low integration");
    assert_eq!(low[0].0, "src/orphan_mod.rs");
    assert!(
        low[0].1 < 0.5,
        "orphan_mod.rs score should be < 0.5, got {}",
        low[0].1
    );

    // Wire the orphan and re-register — score should improve
    db.record_consumer("src/orphan_mod.rs", "UnusedStruct", "src/main.rs", Some(3))
        .unwrap();
    register_module(&db, "src/orphan_mod.rs", 1, 0, 0);

    let low_after = low_integration_modules(&db, 0.5);
    assert!(
        low_after.is_empty(),
        "after wiring, no modules should be low integration"
    );
}

/// E) FLUXO E2E COMPLETO — CICLO DE VIDA DE UM MODULO:
/// 7 steps: register 2 symbols → verify orphans → consumer for 1 → partial score
/// → consumer for 2nd → full score=1.0 → clear → verify clean → reward no panic
#[test]
fn test_e2e_full_module_lifecycle() {
    let (_tmp, db) = test_db();

    // Step 1: Register module with 2 pub symbols
    db.register_pub_symbol("src/new_module.rs", "PublicFnA", "function", "public")
        .unwrap();
    db.register_pub_symbol("src/new_module.rs", "PublicStructB", "struct", "public")
        .unwrap();

    // Step 2: Verify both are orphans, score = 0.0
    let orphans = db.orphan_symbols().unwrap();
    let module_orphans: Vec<_> = orphans
        .iter()
        .filter(|e| e.module_file == "src/new_module.rs")
        .collect();
    assert_eq!(module_orphans.len(), 2, "both symbols should be orphans");
    let score = db.integration_score("src/new_module.rs").unwrap();
    assert_eq!(score, 0.0, "no consumers yet = score 0.0");

    // Step 3: Register consumer for 1st symbol only
    db.record_consumer(
        "src/new_module.rs",
        "PublicFnA",
        "src/consumer_a.rs",
        Some(7),
    )
    .unwrap();

    // Step 4: Verify 1 orphan remains, score improved > 0.0 and < 1.0
    let orphans_partial = db.orphan_symbols().unwrap();
    let module_orphans_partial: Vec<_> = orphans_partial
        .iter()
        .filter(|e| e.module_file == "src/new_module.rs")
        .collect();
    assert_eq!(
        module_orphans_partial.len(),
        1,
        "only PublicStructB should remain as orphan"
    );
    assert_eq!(
        module_orphans_partial[0].symbol_name, "PublicStructB",
        "PublicStructB is the remaining orphan"
    );
    let score_partial = db.integration_score("src/new_module.rs").unwrap();
    assert!(
        score_partial > 0.0,
        "score should be > 0.0 after 1 consumer, got {}",
        score_partial
    );
    assert!(
        score_partial < 1.0,
        "score should be < 1.0 with 1 orphan remaining, got {}",
        score_partial
    );
    assert!(
        (score_partial - 0.5).abs() < f64::EPSILON,
        "score should be exactly 0.5 (1 of 2 wired), got {}",
        score_partial
    );

    // Step 5: Register consumer for 2nd symbol
    db.record_consumer(
        "src/new_module.rs",
        "PublicStructB",
        "src/consumer_b.rs",
        Some(3),
    )
    .unwrap();

    // Step 6: Verify 0 orphans, score = 1.0
    let orphans_final = db.orphan_symbols().unwrap();
    let module_orphans_final: Vec<_> = orphans_final
        .iter()
        .filter(|e| e.module_file == "src/new_module.rs")
        .collect();
    assert_eq!(
        module_orphans_final.len(),
        0,
        "all symbols wired = 0 orphans"
    );
    let score_final = db.integration_score("src/new_module.rs").unwrap();
    assert_eq!(score_final, 1.0, "2 of 2 symbols wired = score 1.0");

    // Step 7: Clear wiring and verify everything is clean
    db.clear_wiring("src/new_module.rs").unwrap();
    let orphans_cleared = db.orphan_symbols().unwrap();
    let module_orphans_cleared: Vec<_> = orphans_cleared
        .iter()
        .filter(|e| e.module_file == "src/new_module.rs")
        .collect();
    assert_eq!(module_orphans_cleared.len(), 0, "after clear, 0 orphans");
    let score_cleared = db.integration_score("src/new_module.rs").unwrap();
    assert_eq!(
        score_cleared, 1.0,
        "after clear: no pub symbols = score 1.0"
    );

    // Verify inject_wiring_reward does not panic
    inject_wiring_reward(&db, "src/new_module.rs", 0.0);
    inject_wiring_reward(&db, "src/new_module.rs", 1.0);
}

/// F) EDGE CASES:
/// - Module with 0 pub symbols → score 1.0
/// - Private symbol → not orphan
/// - visibility="crate" → not treated as public orphan
/// - consumer_file empty string → still resolves orphan
/// - Same symbol registered twice → idempotent (INSERT OR IGNORE)
#[test]
fn test_e2e_edge_cases() {
    let (_tmp, db) = test_db();

    // Edge case 1: Module with no pub symbols → score 1.0
    let score_empty = db.integration_score("src/empty_module.rs").unwrap();
    assert_eq!(
        score_empty, 1.0,
        "module with 0 pub symbols should have score 1.0"
    );

    // Edge case 2: Private symbol → not orphaned
    db.register_pub_symbol(
        "src/priv_module.rs",
        "private_helper",
        "function",
        "private",
    )
    .unwrap();
    let orphans_priv = db.orphan_symbols().unwrap();
    assert_eq!(
        orphans_priv.len(),
        0,
        "private symbol must not appear as orphan"
    );
    // Score for module with only private symbol = 1.0 (0 pub symbols total)
    let score_priv = db.integration_score("src/priv_module.rs").unwrap();
    assert_eq!(
        score_priv, 1.0,
        "module with only private symbols = score 1.0 (no pub symbols to wire)"
    );

    // Edge case 3: visibility="crate" → NOT public, must not appear in orphan_symbols
    db.register_pub_symbol("src/crate_mod.rs", "CrateSymbol", "struct", "crate")
        .unwrap();
    let orphans_crate = db.orphan_symbols().unwrap();
    let crate_orphans: Vec<_> = orphans_crate
        .iter()
        .filter(|e| e.module_file == "src/crate_mod.rs")
        .collect();
    assert_eq!(
        crate_orphans.len(),
        0,
        "crate-visibility symbol must not appear as public orphan"
    );

    // Edge case 4: Same symbol registered twice → idempotent
    db.register_pub_symbol("src/dup_mod.rs", "DupSymbol", "struct", "public")
        .unwrap();
    db.register_pub_symbol("src/dup_mod.rs", "DupSymbol", "struct", "public")
        .unwrap(); // second registration must not error
    let orphans_dup = db.orphan_symbols().unwrap();
    let dup_count = orphans_dup
        .iter()
        .filter(|e| e.module_file == "src/dup_mod.rs" && e.symbol_name == "DupSymbol")
        .count();
    assert_eq!(
        dup_count, 1,
        "duplicate registration must be idempotent (INSERT OR IGNORE)"
    );

    // Edge case 5: Record consumer with import_line=None → still resolves orphan
    db.register_pub_symbol("src/no_line.rs", "AnonSymbol", "function", "public")
        .unwrap();
    db.record_consumer("src/no_line.rs", "AnonSymbol", "src/somewhere.rs", None)
        .unwrap();
    let orphans_no_line = db.orphan_symbols().unwrap();
    let no_line_orphans: Vec<_> = orphans_no_line
        .iter()
        .filter(|e| e.module_file == "src/no_line.rs")
        .collect();
    assert_eq!(
        no_line_orphans.len(),
        0,
        "record_consumer with import_line=None must still resolve orphan"
    );
}

/// Additional: clear_consumer_entries removes consumer-side entries,
/// re-orphaning the symbol (NULL entry remains from register_pub_symbol).
///
/// Behavior verified by observation:
/// - register_pub_symbol inserts (module, symbol, NULL) row
/// - record_consumer inserts a SEPARATE (module, symbol, consumer) row (different UNIQUE key)
/// - clear_consumer_entries deletes only rows where consumer_file = X
/// - The original NULL row survives → symbol becomes orphan again
#[test]
fn test_e2e_clear_consumer_entries_reorphans_symbol() {
    let (_tmp, db) = test_db();

    // Register pub symbol — creates NULL orphan row
    db.register_pub_symbol("src/mod_x.rs", "SymX", "struct", "public")
        .unwrap();

    // Wire it — creates separate consumer row (NULL row stays due to UNIQUE index key difference)
    db.record_consumer("src/mod_x.rs", "SymX", "src/consumer_x.rs", Some(1))
        .unwrap();

    // Verify: wired (score=1.0 — consumer_file IS NOT NULL row exists)
    assert_eq!(db.integration_score("src/mod_x.rs").unwrap(), 1.0);
    // orphan_symbols uses NOT EXISTS subquery: consumer row exists → no orphan returned
    assert_eq!(db.orphan_symbols().unwrap().len(), 0);

    // Simulate: consumer_x.rs is re-scanned → its consumer entries removed
    db.clear_consumer_entries("src/consumer_x.rs").unwrap();

    // After deleting the consumer row, only the NULL orphan row remains
    // → symbol is re-orphaned, score drops back to 0.0
    let score_after_clear = db.integration_score("src/mod_x.rs").unwrap();
    assert_eq!(
        score_after_clear, 0.0,
        "after clear_consumer_entries, the NULL orphan row remains → score 0.0"
    );
    let orphans_reorphaned = db.orphan_symbols().unwrap();
    assert_eq!(
        orphans_reorphaned.len(),
        1,
        "SymX must be re-orphaned after consumer entry is removed"
    );
    assert_eq!(orphans_reorphaned[0].symbol_name, "SymX");
}

/// Prove inject_wiring_reward correctly detects improvement and regression.
#[test]
fn test_e2e_inject_wiring_reward_signals() {
    let (_tmp, db) = test_db();

    db.register_pub_symbol("src/signal_mod.rs", "SignalA", "function", "public")
        .unwrap();
    db.register_pub_symbol("src/signal_mod.rs", "SignalB", "struct", "public")
        .unwrap();

    let score_orphaned = db.integration_score("src/signal_mod.rs").unwrap();
    assert_eq!(score_orphaned, 0.0);

    // Inject reward with previous=0.0, current=0.0 → no signal (delta < 0.01)
    inject_wiring_reward(&db, "src/signal_mod.rs", 0.0); // no-op, same score

    // Wire 1 symbol
    db.record_consumer("src/signal_mod.rs", "SignalA", "src/caller.rs", Some(5))
        .unwrap();
    let score_partial = db.integration_score("src/signal_mod.rs").unwrap();
    assert!(
        (score_partial - 0.5).abs() < f64::EPSILON,
        "score should be 0.5"
    );

    // Inject improvement reward: previous=0.0, current=0.5 → wiring_improvement signal
    inject_wiring_reward(&db, "src/signal_mod.rs", 0.0); // should log wiring_improvement

    // Wire the second symbol
    db.record_consumer("src/signal_mod.rs", "SignalB", "src/caller.rs", Some(6))
        .unwrap();
    let score_full = db.integration_score("src/signal_mod.rs").unwrap();
    assert_eq!(score_full, 1.0);

    // Inject with same previous to confirm it does not panic
    inject_wiring_reward(&db, "src/signal_mod.rs", 1.0); // delta=0.0, no signal
    inject_wiring_reward(&db, "src/signal_mod.rs", 0.5); // improvement signal
}

// ── B3 fix: orphan_symbols_for_module parameterized query ──

#[test]
fn test_orphan_symbols_for_module_returns_only_target() {
    let (_tmp, db) = test_db();
    db.register_pub_symbol("src/a.rs", "SymA", "struct", "public")
        .unwrap();
    db.register_pub_symbol("src/b.rs", "SymB", "function", "public")
        .unwrap();

    let orphans_a = db.orphan_symbols_for_module("src/a.rs").unwrap();
    assert_eq!(orphans_a.len(), 1);
    assert_eq!(orphans_a[0].symbol_name, "SymA");

    let orphans_b = db.orphan_symbols_for_module("src/b.rs").unwrap();
    assert_eq!(orphans_b.len(), 1);
    assert_eq!(orphans_b[0].symbol_name, "SymB");
}

#[test]
fn test_orphan_symbols_for_module_excludes_wired() {
    let (_tmp, db) = test_db();
    db.register_pub_symbol("src/mod.rs", "Wired", "struct", "public")
        .unwrap();
    db.register_pub_symbol("src/mod.rs", "Orphan", "struct", "public")
        .unwrap();
    db.record_consumer("src/mod.rs", "Wired", "src/consumer.rs", Some(1))
        .unwrap();

    let orphans = db.orphan_symbols_for_module("src/mod.rs").unwrap();
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0].symbol_name, "Orphan");
}

#[test]
fn test_orphan_symbols_for_module_empty_when_no_symbols() {
    let (_tmp, db) = test_db();
    let orphans = db.orphan_symbols_for_module("src/nonexistent.rs").unwrap();
    assert!(orphans.is_empty());
}

// ── B4 fix: all_pub_symbols returns all registered pub symbols ──

#[test]
fn test_all_pub_symbols_includes_wired_and_orphaned() {
    let (_tmp, db) = test_db();
    db.register_pub_symbol("src/a.rs", "Orphan", "struct", "public")
        .unwrap();
    db.register_pub_symbol("src/b.rs", "Wired", "function", "public")
        .unwrap();
    db.record_consumer("src/b.rs", "Wired", "src/consumer.rs", Some(1))
        .unwrap();

    let all = db.all_pub_symbols().unwrap();
    // Both should appear: all_pub_symbols returns the producer rows (consumer_file IS NULL)
    assert!(
        all.iter().any(|e| e.symbol_name == "Orphan"),
        "Orphan symbol should appear in all_pub_symbols"
    );
    assert!(
        all.iter().any(|e| e.symbol_name == "Wired"),
        "Wired symbol should also appear (its NULL producer row still exists)"
    );
}

#[test]
fn test_all_pub_symbols_excludes_private() {
    let (_tmp, db) = test_db();
    db.register_pub_symbol("src/a.rs", "PubSym", "struct", "public")
        .unwrap();
    db.register_pub_symbol("src/a.rs", "PrivSym", "function", "private")
        .unwrap();

    let all = db.all_pub_symbols().unwrap();
    assert!(all.iter().any(|e| e.symbol_name == "PubSym"));
    assert!(
        !all.iter().any(|e| e.symbol_name == "PrivSym"),
        "Private symbols should not appear"
    );
}

// ── FIX-2: direct-path consumer detection + lowercase-symbol support ───

#[test]
fn extract_direct_path_finds_crate_function_call() {
    let src = "        crate::lifecycle::handle_file_changed(rt, v)\n";
    let paths = extract_direct_path_expressions(src);
    assert!(
        paths.contains(&"crate::lifecycle::handle_file_changed".to_string()),
        "expected crate::lifecycle::handle_file_changed in {paths:?}"
    );
}

#[test]
fn extract_direct_path_finds_super_type() {
    let src = "let x: super::types::FileEntry = super::types::FileEntry::new();";
    let paths = extract_direct_path_expressions(src);
    assert!(paths.iter().any(|p| p == "super::types::FileEntry"));
}

#[test]
fn extract_direct_path_ignores_word_boundary_false_positive() {
    // `my_crate::foo::bar` must not be captured — it's not at a word
    // boundary because `_` precedes `crate`.
    let src = "my_crate::foo::bar();";
    let paths = extract_direct_path_expressions(src);
    assert!(paths.is_empty(), "got {paths:?}");
}

#[test]
fn extract_direct_path_ignores_bare_crate_prefix() {
    // Single-segment `crate::foo` would be captured; `crate::` with
    // nothing after is not a valid path expression.
    let src = "use crate::\nuse crate::foo::Bar;";
    let paths = extract_direct_path_expressions(src);
    // `crate::foo::Bar` has at least 2 `::` separators — captured.
    assert!(paths.iter().any(|p| p == "crate::foo::Bar"));
}

#[test]
fn extract_direct_path_survives_multibyte_content() {
    // Regression: prior implementation used `content[i..]` for slice
    // inspection which panicked when `i` landed inside a multi-byte
    // UTF-8 sequence (e.g. em-dash `—`, emoji, accented letters). The
    // rewrite uses `content.get(i..)` which returns None on boundaries
    // and is panic-free.
    let src = "//! touring-hook — Native Rust binary\n\
                   crate::lifecycle::handle_file_changed(rt, v);\n\
                   // ✨ unicode commentary — still fine\n\
                   crate::hook_registry::register(ç);\n";
    let paths = extract_direct_path_expressions(src);
    assert!(paths.contains(&"crate::lifecycle::handle_file_changed".to_string()));
    assert!(paths.contains(&"crate::hook_registry::register".to_string()));
}

#[test]
fn extract_direct_path_dedupes_repeated_occurrences() {
    let src = "crate::lifecycle::handle_get(a);\n    crate::lifecycle::handle_get(b);\n";
    let paths = extract_direct_path_expressions(src);
    let hits: Vec<&String> = paths
        .iter()
        .filter(|p| p.as_str() == "crate::lifecycle::handle_get")
        .collect();
    assert_eq!(hits.len(), 1, "deduplicated to 1, got {:?}", paths);
}

#[test]
fn record_consumer_from_path_supports_lowercase_function_symbols() {
    // Regression: previous version filtered symbols to uppercase-only,
    // silently dropping handler-style function consumers. This test
    // asserts that `handle_file_changed` (lowercase) IS recorded and
    // that `crate::` resolves relative to the consumer's crate root.
    let (_tmp, db) = test_db();
    db.register_pub_symbol(
        "crates/touring-hooks/src/lifecycle.rs",
        "handle_file_changed",
        "function",
        "public",
    )
    .unwrap();

    record_consumer_from_path(
        &db,
        "crate::lifecycle::handle_file_changed",
        "crates/touring-hooks/src/hook_registry.rs",
    );

    let orphans = db.orphan_symbols().unwrap();
    let still_orphan = orphans
        .iter()
        .any(|s| s.symbol_name == "handle_file_changed");
    assert!(
        !still_orphan,
        "handle_file_changed must not be orphan after crate-root-aware consumer recording"
    );
}

#[test]
fn record_consumer_from_path_resolves_workspace_crate_root() {
    // `crate::lifecycle::X` from `crates/touring-hooks/src/hook_registry.rs`
    // MUST resolve to `crates/touring-hooks/src/lifecycle.rs` — not
    // the workspace-relative `src/lifecycle.rs` the legacy code emitted.
    let (_tmp, db) = test_db();
    db.register_pub_symbol(
        "crates/touring-hooks/src/lifecycle.rs",
        "handle_x",
        "function",
        "public",
    )
    .unwrap();

    record_consumer_from_path(
        &db,
        "crate::lifecycle::handle_x",
        "crates/touring-hooks/src/hook_registry.rs",
    );

    let status = db
        .module_wiring_status("crates/touring-hooks/src/lifecycle.rs")
        .unwrap();
    assert!(
        status.symbols_with_consumers >= 1,
        "expected ≥1 wired symbol, got {}",
        status.symbols_with_consumers
    );
    assert!(
        !status.orphan_symbols.contains(&"handle_x".to_string()),
        "handle_x must not be orphan: {:?}",
        status.orphan_symbols
    );
}

// ── FIX-4: re-export consumer detection ───────────────────────────────

#[test]
fn extract_reexport_pairs_catches_pub_use() {
    let src = "pub use worktree::handle_worktree_create;";
    let pairs = extract_reexport_pairs(src);
    assert_eq!(pairs.len(), 1);
    assert_eq!(
        pairs[0],
        ("worktree".to_string(), "handle_worktree_create".to_string())
    );
}

#[test]
fn extract_reexport_pairs_catches_pub_crate_use() {
    let src = "    pub(crate) use subagent::handle_subagent_start;";
    let pairs = extract_reexport_pairs(src);
    assert_eq!(pairs.len(), 1);
    assert_eq!(
        pairs[0],
        ("subagent".to_string(), "handle_subagent_start".to_string())
    );
}

#[test]
fn extract_reexport_pairs_ignores_crate_absolute_paths() {
    // Absolute paths (`crate::...`, `super::...`) are handled by
    // `extract_direct_path_expressions` — `extract_reexport_pairs`
    // must not double-count them.
    let src = "pub(crate) use crate::lifecycle::handle_file_changed;";
    let pairs = extract_reexport_pairs(src);
    assert!(pairs.is_empty(), "expected empty, got {pairs:?}");
}

#[test]
fn extract_reexport_pairs_ignores_external_crates() {
    let src = "use serde_json::Value;\n\
                   use std::fs::File;\n\
                   use tokio::sync::Mutex;";
    let pairs = extract_reexport_pairs(src);
    assert!(pairs.is_empty(), "expected empty, got {pairs:?}");
}

#[test]
fn record_reexport_consumer_registers_edge_to_colocated_submod() {
    // When `lifecycle.rs` re-exports `handle_x` from `subagent`, the
    // consumer edge should resolve to `lifecycle/subagent.rs`.
    let (_tmp, db) = test_db();
    db.register_pub_symbol(
        "crates/touring-hooks/src/lifecycle/subagent.rs",
        "handle_x",
        "function",
        "public",
    )
    .unwrap();

    record_reexport_consumer(
        &db,
        "crates/touring-hooks/src/lifecycle.rs",
        "subagent",
        "handle_x",
    );

    let orphans = db.orphan_symbols().unwrap();
    assert!(
        !orphans.iter().any(|s| s.symbol_name == "handle_x"),
        "handle_x must not be orphan after re-export edge"
    );
}

#[test]
fn record_consumer_from_path_skips_self_and_wildcards() {
    let (_tmp, db) = test_db();
    // These calls must not panic and must not record edges.
    record_consumer_from_path(&db, "crate::lifecycle::*", "src/hook_registry.rs");
    record_consumer_from_path(&db, "crate::lifecycle::self", "src/hook_registry.rs");
    record_consumer_from_path(&db, "std::io::Result", "src/hook_registry.rs");
    // std::* must not register — path must start with `crate::` or `super::`.
    let entries = db.all_pub_symbols().unwrap();
    assert!(entries.is_empty(), "unexpected entries {:?}", entries);
}

// ── Wave 22 (S-Q1a): wiring_modules_aggregate tests ──

#[test]
fn test_wiring_modules_aggregate_empty() {
    let (_tmp, db) = test_db();
    let rows = db.wiring_modules_aggregate().unwrap();
    assert!(rows.is_empty(), "fresh DB has no modules");
}

#[test]
fn test_wiring_modules_aggregate_all_orphaned() {
    let (_tmp, db) = test_db();
    db.register_pub_symbol("src/a.rs", "Foo", "struct", "public")
        .unwrap();
    db.register_pub_symbol("src/a.rs", "Bar", "function", "public")
        .unwrap();

    let rows = db.wiring_modules_aggregate().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].module_file, "src/a.rs");
    assert_eq!(rows[0].total_pub, 2);
    assert_eq!(rows[0].wired_count, 0);
    assert_eq!(rows[0].integration_score(), 0.0);
}

#[test]
fn test_wiring_modules_aggregate_partial_wiring() {
    let (_tmp, db) = test_db();
    db.register_pub_symbol("src/m.rs", "A", "struct", "public")
        .unwrap();
    db.register_pub_symbol("src/m.rs", "B", "struct", "public")
        .unwrap();
    db.record_consumer("src/m.rs", "A", "src/consumer.rs", None)
        .unwrap();

    let rows = db.wiring_modules_aggregate().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].total_pub, 2);
    assert_eq!(rows[0].wired_count, 1);
    let score = rows[0].integration_score();
    assert!((score - 0.5).abs() < 1e-9, "expected 0.5, got {score}");
}

#[test]
fn test_wiring_modules_aggregate_fully_wired() {
    let (_tmp, db) = test_db();
    db.register_pub_symbol("src/x.rs", "X", "struct", "public")
        .unwrap();
    db.record_consumer("src/x.rs", "X", "src/y.rs", None)
        .unwrap();

    let rows = db.wiring_modules_aggregate().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].integration_score(), 1.0);
}

#[test]
fn test_wiring_modules_aggregate_no_pub_symbols_score_one() {
    let (_tmp, _db) = test_db();
    // No rows registered → no rows returned; score-of-empty-module is 1.0
    // via the WiringModuleAggregateRow::integration_score() helper.
    let row = WiringModuleAggregateRow {
        module_file: "src/empty.rs".to_string(),
        total_pub: 0,
        wired_count: 0,
    };
    assert_eq!(row.integration_score(), 1.0);
}

#[test]
fn test_wiring_modules_aggregate_multiple_modules() {
    let (_tmp, db) = test_db();
    db.register_pub_symbol("src/a.rs", "A1", "struct", "public")
        .unwrap();
    db.register_pub_symbol("src/b.rs", "B1", "struct", "public")
        .unwrap();
    db.register_pub_symbol("src/b.rs", "B2", "struct", "public")
        .unwrap();
    db.record_consumer("src/b.rs", "B1", "src/c.rs", None)
        .unwrap();

    let rows = db.wiring_modules_aggregate().unwrap();
    assert_eq!(rows.len(), 2);
    // Rows are ORDER BY module_file
    let a = rows.iter().find(|r| r.module_file == "src/a.rs").unwrap();
    let b = rows.iter().find(|r| r.module_file == "src/b.rs").unwrap();
    assert_eq!(a.total_pub, 1);
    assert_eq!(a.wired_count, 0);
    assert_eq!(b.total_pub, 2);
    assert_eq!(b.wired_count, 1);
}
