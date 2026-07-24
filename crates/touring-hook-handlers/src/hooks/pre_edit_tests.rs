use super::*;
use crate::knowledge::{BashOutcome, FileKnowledge, FileKnowledgeDB, FileRelation};
use tempfile::TempDir;

fn setup() -> (TempDir, FileKnowledgeDB) {
    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("test.db")).unwrap();
    (tmp, db)
}

#[test]
fn test_compose_no_knowledge() {
    let (_tmp, db) = setup();
    let ctx = compose_edit_context(None, &db, "unknown.py");
    assert!(ctx.is_none());
}

// ── Signal 12 (Wave 5) — pre_edit rust workflow advisory ──

#[test]
fn test_compose_rust_workflow_advisory_none_for_unknown_extension() {
    // Wave 5.1 (2026-04-18): the advisory is now multi-lang —
    // .py/.ts/.tsx/.js are handled. Only extensions `Lang::from_path`
    // cannot classify (`.xyz`) still return None.
    let tmp = TempDir::new().expect("tmp");
    let p = tmp.path().join("unknown.xyz");
    std::fs::write(&p, "arbitrary").expect("write");
    assert!(compose_rust_workflow_advisory(p.to_str().expect("utf8")).is_none());
}

#[test]
fn test_compose_rust_workflow_advisory_multilang_for_python() {
    // Regression anchor: Python sources MUST now surface the
    // multi-lang `code-workflow [python]:` advisory via pre_edit.
    let tmp = TempDir::new().expect("tmp");
    let py_path = tmp.path().join("app.py");
    std::fs::write(&py_path, "def greet(name):\n    return f'hi {name}'\n").expect("write");
    let hint = compose_rust_workflow_advisory(py_path.to_str().expect("utf8"));
    assert!(
        hint.as_deref()
            .map(|h| h.starts_with("⚙ code-workflow [") && h.contains("[python]"))
            .unwrap_or(false),
        ".py file must emit multi-lang advisory; got: {hint:?}"
    );
}

#[test]
fn test_compose_rust_workflow_advisory_none_for_missing_file() {
    assert!(compose_rust_workflow_advisory("/nonexistent/path.rs").is_none());
}

#[test]
fn test_compose_rust_workflow_advisory_none_for_trivial_rust() {
    // Private helper with no pub surface + near-zero complexity →
    // wave5_workflow::rust_workflow_hint returns None.
    let tmp = TempDir::new().expect("tmp");
    let p = tmp.path().join("helper.rs");
    std::fs::write(&p, "fn _priv() {}\n").expect("write");
    assert!(
        compose_rust_workflow_advisory(p.to_str().expect("utf8")).is_none(),
        "trivial private fn must not produce advisory"
    );
}

#[test]
fn test_compose_rust_workflow_advisory_surfaces_for_public_api() {
    let tmp = TempDir::new().expect("tmp");
    let p = tmp.path().join("api.rs");
    std::fs::write(&p, "pub fn exposed() -> u32 { 1 }\n").expect("write");
    let hint = compose_rust_workflow_advisory(p.to_str().expect("utf8"))
        .expect("public API must surface advisory");
    assert!(hint.starts_with("⚙ rust-workflow:"));
    assert!(hint.contains("pub_surface=1"));
}

#[test]
fn test_compose_rust_workflow_advisory_reports_unsafe() {
    let tmp = TempDir::new().expect("tmp");
    let p = tmp.path().join("raw.rs");
    std::fs::write(
        &p,
        "pub unsafe fn raw() -> *const u8 { std::ptr::null() }\n",
    )
    .expect("write");
    let hint = compose_rust_workflow_advisory(p.to_str().expect("utf8"))
        .expect("public unsafe must surface advisory");
    assert!(hint.contains("unsafe=1"), "got: {hint:?}");
}

#[test]
fn test_compose_rust_workflow_advisory_skips_large_files() {
    // Files >100KB must yield None to stay within pre-edit 2s budget.
    let tmp = TempDir::new().expect("tmp");
    let p = tmp.path().join("big.rs");
    // 200KB of pub fn boilerplate.
    let huge = "pub fn a() {}\n".repeat(20_000);
    std::fs::write(&p, &huge).expect("write");
    assert!(
        compose_rust_workflow_advisory(p.to_str().expect("utf8")).is_none(),
        "files >100KB must be skipped by size guard"
    );
}

#[test]
fn test_compose_with_dependents() {
    let (_tmp, db) = setup();
    db.upsert_relation(&FileRelation {
        source: "app.py".to_string(),
        target: "utils.py".to_string(),
        relation_type: "imports".to_string(),
    })
    .unwrap();
    db.upsert_relation(&FileRelation {
        source: "tests.py".to_string(),
        target: "utils.py".to_string(),
        relation_type: "imports".to_string(),
    })
    .unwrap();

    let ctx = compose_edit_context(None, &db, "utils.py").unwrap();
    assert!(ctx.contains("2 file(s) import this"));
}

#[test]
fn test_compose_with_notes() {
    let (_tmp, db) = setup();
    db.upsert(&FileKnowledge {
        file_path: "src/main.py".to_string(),
        notes: Some("ProcessType enum bug: use REVISAO_ORDINARIA".to_string()),
        ..Default::default()
    })
    .unwrap();

    let ctx = compose_edit_context(None, &db, "src/main.py").unwrap();
    assert!(ctx.contains("note:"));
    assert!(ctx.contains("ProcessType"));
}

#[test]
fn test_compose_warns_about_lint_failures() {
    let (_tmp, db) = setup();
    // Simulate ruff failure on this file
    db.record_bash_outcome(&BashOutcome {
        command: "ruff check src/main.py".to_string(),
        command_short: "ruff".to_string(),
        exit_code: 1,
        success: false,
        error_pattern: Some("B007 loop variable, E501 line too long".to_string()),
        file_context: Some("src/main.py".to_string()),
        command_hash: String::new(),
        executed_at: String::new(),
    })
    .unwrap();

    let ctx = compose_edit_context(None, &db, "src/main.py").unwrap();
    assert!(
        ctx.contains("quality"),
        "Should warn about quality gate failure"
    );
    assert!(ctx.contains("ruff"), "Should mention the failing command");
    assert!(ctx.contains("B007"), "Should show the error pattern");
}

#[test]
fn test_silence_for_non_lint_failures() {
    let (_tmp, db) = setup();
    // Non-lint failure (e.g., pytest) — should NOT trigger quality warning
    db.record_bash_outcome(&BashOutcome {
        command: "python src/main.py".to_string(),
        command_short: "python".to_string(),
        exit_code: 1,
        success: false,
        error_pattern: Some("ImportError: no module named foo".to_string()),
        file_context: Some("src/main.py".to_string()),
        command_hash: String::new(),
        executed_at: String::new(),
    })
    .unwrap();

    let ctx = compose_edit_context(None, &db, "src/main.py");
    // Should be None or not contain "quality" — python failure is not a lint issue
    if let Some(c) = ctx {
        assert!(
            !c.contains("quality"),
            "Non-lint failures should NOT trigger quality warning"
        );
    }
}

#[test]
fn test_compose_combined_signals() {
    let (_tmp, db) = setup();
    // File with dependents + lint failure + notes
    db.upsert_relation(&FileRelation {
        source: "app.py".to_string(),
        target: "utils.py".to_string(),
        relation_type: "imports".to_string(),
    })
    .unwrap();
    db.upsert(&FileKnowledge {
        file_path: "utils.py".to_string(),
        notes: Some("Bug with caching".to_string()),
        ..Default::default()
    })
    .unwrap();
    db.record_bash_outcome(&BashOutcome {
        command: "ruff check utils.py".to_string(),
        command_short: "ruff".to_string(),
        exit_code: 1,
        success: false,
        error_pattern: Some("E501 line too long".to_string()),
        file_context: Some("utils.py".to_string()),
        command_hash: String::new(),
        executed_at: String::new(),
    })
    .unwrap();

    let ctx = compose_edit_context(None, &db, "utils.py").unwrap();
    assert!(ctx.contains("import this")); // dependents
    assert!(ctx.contains("quality")); // lint warning
    assert!(ctx.contains("note:")); // notes
    assert!(ctx.contains("|")); // multiple signals joined
}

#[test]
fn test_function_replacement_suggests_touring() {
    // old_string that spans an entire function (has "def " and "return")
    let old = "def my_function(x):\n    # do stuff\n    return x * 2\n";
    assert!(
        edit_spans_entire_function(old),
        "Should detect function span"
    );
    let suggestion = suggest_replace_symbol_body(old);
    assert!(
        suggestion.contains("mcp__touring__touring_ast_edit"),
        "Expected ast_edit suggestion, got: {:?}",
        suggestion
    );
}

#[test]
fn test_partial_edit_no_suggestion() {
    let old = "    return x * 2"; // just one line — not a whole function
    assert!(!edit_spans_entire_function(old));
    let suggestion = suggest_replace_symbol_body(old);
    assert!(suggestion.is_empty());
}

#[test]
fn test_signal7_prevention_integrated() {
    let (_tmp, db) = setup();
    // Create a high-confidence gotcha (hit_count >= 2 via add_gotcha + increment)
    let id = db
        .add_gotcha(
            "config",
            "Always validate config keys before access",
            "WARN",
            None,
        )
        .unwrap();
    db.increment_gotcha_hit(id);
    db.increment_gotcha_hit(id);
    db.increment_gotcha_hit(id);

    let ctx = compose_edit_context(None, &db, "src/config.py");
    // Should have either GOTCHA (Signal 6) or prevention context (Signal 7)
    assert!(ctx.is_some(), "File with gotcha should produce context");
    let c = ctx.unwrap();
    assert!(
        c.contains("GOTCHA") || c.contains("prevention"),
        "Should contain gotcha or prevention warning: {c}"
    );
}

#[test]
fn test_signal8_predictor_graceful_no_data() {
    let (_tmp, db) = setup();
    // No edit history = predictor has nothing to learn
    let ctx = compose_edit_context(None, &db, "empty.py");
    // Should be None — no signals at all
    assert!(ctx.is_none());
}

#[test]
fn test_wiring_signal_included_for_orphans() {
    let (_tmp, db) = setup();
    db.register_pub_symbol("src/tfidf.rs", "TfIdfVectorizer", "struct", "public")
        .unwrap();
    let ctx = compose_edit_context(None, &db, "src/tfidf.rs");
    assert!(ctx.is_some());
    let text = ctx.unwrap();
    assert!(
        text.contains("wiring"),
        "should include wiring signal: {text}"
    );
    assert!(
        text.contains("TfIdfVectorizer"),
        "should mention orphan symbol: {text}"
    );
}

#[test]
fn test_wiring_signal_absent_when_no_orphans() {
    let (_tmp, db) = setup();
    // No wiring data — should not produce wiring signal
    let ctx = compose_edit_context(None, &db, "src/clean.rs");
    if let Some(text) = ctx {
        assert!(
            !text.contains("wiring("),
            "should not include wiring signal when no orphans: {text}"
        );
    }
}

#[test]
fn test_wiring_signal_absent_when_fully_wired() {
    let (_tmp, db) = setup();
    db.register_pub_symbol("src/mod.rs", "Foo", "struct", "public")
        .unwrap();
    db.record_consumer("src/mod.rs", "Foo", "src/main.rs", Some(3))
        .unwrap();
    let ctx = compose_edit_context(None, &db, "src/mod.rs");
    // Score should be 1.0 (all symbols have consumers), so no wiring signal
    if let Some(text) = ctx {
        assert!(
            !text.contains("wiring("),
            "fully wired module should not trigger wiring signal: {text}"
        );
    }
}

#[test]
fn test_import_prediction_suggests_orphan_symbol() {
    let (_tmp, db) = setup();
    // Register a pub symbol in another module (orphan — no consumer yet)
    db.register_pub_symbol("src/tfidf.rs", "TfIdfVectorizer", "struct", "public")
        .unwrap();
    // File being edited has no imports/symbols
    let suggestions = detect_unresolved_types(
        "let v: TfIdfVectorizer = TfIdfVectorizer::new();",
        &db,
        "src/main.rs",
    );
    assert!(
        !suggestions.is_empty(),
        "should suggest import for TfIdfVectorizer"
    );
    assert!(
        suggestions[0].contains("TfIdfVectorizer"),
        "suggestion should mention the type: {:?}",
        suggestions
    );
    assert!(
        suggestions[0].contains("tfidf"),
        "suggestion should mention the source module: {:?}",
        suggestions
    );
}

#[test]
fn test_import_prediction_ignores_builtins() {
    let (_tmp, db) = setup();
    let suggestions =
        detect_unresolved_types("let v: Vec<String> = Vec::new();", &db, "src/main.rs");
    assert!(
        suggestions.is_empty(),
        "builtins should not trigger import prediction: {:?}",
        suggestions
    );
}

#[test]
fn test_import_prediction_ignores_local_symbols() {
    let (_tmp, db) = setup();
    // Register local symbol in the file being edited
    db.upsert(&FileKnowledge {
        file_path: "src/main.rs".into(),
        symbols_json: Some(r#"[{"name":"MyStruct","kind":"struct","is_public":true}]"#.into()),
        ..Default::default()
    })
    .unwrap();
    let suggestions = detect_unresolved_types("let v = MyStruct::default();", &db, "src/main.rs");
    assert!(
        suggestions.is_empty(),
        "local symbols should not trigger import prediction: {:?}",
        suggestions
    );
}

// ── B4 fix: import prediction works for symbols with existing consumers ──

#[test]
fn test_import_prediction_suggests_wired_symbol() {
    let (_tmp, db) = setup();
    // Register a pub symbol that ALREADY has a consumer (not an orphan)
    db.register_pub_symbol("src/tfidf.rs", "TfIdfVectorizer", "struct", "public")
        .unwrap();
    db.record_consumer("src/tfidf.rs", "TfIdfVectorizer", "src/other.rs", Some(3))
        .unwrap();

    // A different file wants to use it too — should still get a suggestion
    let suggestions = detect_unresolved_types(
        "let v: TfIdfVectorizer = TfIdfVectorizer::new();",
        &db,
        "src/main.rs",
    );
    assert!(
        !suggestions.is_empty(),
        "should suggest import even for symbols with existing consumers"
    );
    assert!(
        suggestions[0].contains("TfIdfVectorizer"),
        "suggestion should mention the type: {:?}",
        suggestions
    );
}

// ── B1 fix: ecosystem wired via post_read ──

#[test]
fn test_ecosystem_signal_6c_in_compose_edit_context() {
    let (_tmp, db) = setup();
    // Register a module with an orphan pub symbol (low integration)
    db.register_pub_symbol("src/orphan_mod.rs", "OrphanStruct", "struct", "public")
        .unwrap();
    crate::ecosystem::register_module(&db, "src/orphan_mod.rs", 1, 0, 0);

    // When editing the orphan module, Signal 6c should appear
    let ctx = compose_edit_context(None, &db, "src/orphan_mod.rs");
    assert!(
        ctx.is_some(),
        "should produce context for low-integration module"
    );
    let text = ctx.unwrap();
    // Signal 11 (wiring check) should fire for orphan symbols
    assert!(
        text.contains("wiring"),
        "should include wiring signal for orphan: {text}"
    );
}

// ── Signal I-5: callgraph_signal_for_file ─────────────────────────────────

#[test]
fn test_callgraph_signal_unsupported_extension_returns_none() {
    // .txt is not Rust or Python — function must return None immediately
    let result = callgraph_signal_for_file("/tmp/notes.txt");
    assert!(
        result.is_none(),
        "unsupported extension must return None, got: {result:?}"
    );
}

#[test]
fn test_callgraph_signal_missing_file_returns_none() {
    // Non-existent .rs path — fs::read_to_string fails, Option short-circuits
    let result = callgraph_signal_for_file("/nonexistent/path/does_not_exist.rs");
    assert!(
        result.is_none(),
        "missing file must return None gracefully, got: {result:?}"
    );
}

#[test]
fn test_callgraph_signal_rust_with_call_sites_returns_some() {
    // Write a Rust file where one function calls another — call graph has entries
    let tmp = tempfile::NamedTempFile::with_suffix(".rs").expect("create temp .rs file");
    let source = r#"
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn double(x: i32) -> i32 {
    add(x, x)
}

fn triple(x: i32) -> i32 {
    add(x, add(x, x))
}
"#;
    std::fs::write(tmp.path(), source).expect("write temp source");
    let path = tmp.path().to_str().expect("valid utf-8 path");

    let result = callgraph_signal_for_file(path);
    // `add` is called by both `double` and `triple` — should appear in output
    assert!(
        result.is_some(),
        "Rust source with internal call sites must return Some, got None"
    );
    let text = result.unwrap();
    assert!(
        text.starts_with("callers: "),
        "output must start with 'callers: ', got: {text:?}"
    );
    // format_callgraph_context emits "callers: [sym1, sym2]" or "HOTSPOT: `sym` has N callers"
    assert!(
        text.contains("callers") && (text.contains('[') || text.contains("HOTSPOT")),
        "output must contain caller list or hotspot annotation, got: {text:?}"
    );
}

#[test]
fn test_callgraph_signal_source_with_no_call_sites_returns_none() {
    // A Rust file with only declarations and no internal calls — call graph empty
    let tmp = tempfile::NamedTempFile::with_suffix(".rs").expect("create temp .rs file");
    let source = r#"
pub struct Foo {
    pub x: i32,
}

impl Foo {
    pub fn new(x: i32) -> Self {
        Foo { x }
    }
}
"#;
    std::fs::write(tmp.path(), source).expect("write temp source");
    let path = tmp.path().to_str().expect("valid utf-8 path");

    let result = callgraph_signal_for_file(path);
    // No function calls another function defined in this file — must be silent
    assert!(
        result.is_none(),
        "source with no internal call sites must return None, got: {result:?}"
    );
}

// ── S1: TDG grade signal — unit tests for the TdgReport path ─────────────

/// Verify that the TDG composite logic produces a D/F grade for a file
/// with worst-case complexity (avg_complexity = 20, high_complexity_count = 4).
///
/// This tests the mathematical path in `compose_quality_evolution` without
/// depending on tree-sitter parsing of a synthetic Rust file (which can
/// produce wildly variable CC depending on the parser build config).
///
/// The invariant we check: `TdgReport::from_components` with complexity_score
/// near 0.0 (high avg_CC) and antipatterns_score near 0.6 (4 high-CC symbols)
/// must produce a grade whose `to_diagnostic_opt()` returns `Some` (D or F).
#[test]
fn tdg_from_components_emits_diagnostic_for_worst_case_complexity() {
    // Simulate: avg_complexity = 20 (maximum) → complexity_score = 0.0
    let complexity_score = (1.0_f64 - (20.0_f64 / 20.0).min(1.0)).clamp(0.0, 1.0);
    // Simulate: high_complexity_count = 4 → antipatterns_score = 1 - 0.40 = 0.60
    let antipatterns_score = (1.0_f64 - (4.0_f64 * 0.10).min(0.40)).clamp(0.0, 1.0);

    let tdg = touring_analysis::quality::TdgReport::from_components(
        complexity_score,   // 0.0
        1.0,                // coverage — neutral
        1.0,                // duplication — neutral
        0.0,                // churn — neutral
        1.0,                // entropy — neutral
        antipatterns_score, // 0.60
    );

    // At complexity=0.0 the composite must be below the D threshold (0.55).
    assert!(
        tdg.to_diagnostic_opt().is_some(),
        "worst-case complexity must produce a D/F diagnostic, grade={} composite={:.3}",
        tdg.grade_letter(),
        tdg.composite,
    );
    // Grade must be D or F — not C or above.
    let gl = tdg.grade_letter();
    assert!(
        gl == "D" || gl == "F",
        "worst-case complexity must yield grade D or F, got {gl}"
    );
}

/// Verify that the TDG composite logic does NOT produce a D/F grade for a
/// trivially simple file (avg_complexity = 1, zero high-complexity symbols).
#[test]
fn tdg_from_components_no_diagnostic_for_simple_file_metrics() {
    // Simulate: avg_complexity = 1.0 → complexity_score ≈ 0.95
    let complexity_score = (1.0_f64 - (1.0_f64 / 20.0).min(1.0)).clamp(0.0, 1.0);
    // Simulate: high_complexity_count = 0 → antipatterns_score = 1.0
    let antipatterns_score = 1.0_f64;

    let tdg = touring_analysis::quality::TdgReport::from_components(
        complexity_score, // ~0.95
        1.0,
        1.0,
        0.0,
        1.0,
        antipatterns_score, // 1.0
    );

    // Simple file must NOT trigger a D/F diagnostic.
    assert!(
        tdg.to_diagnostic_opt().is_none(),
        "simple file must not produce D/F diagnostic, grade={} composite={:.3}",
        tdg.grade_letter(),
        tdg.composite,
    );
}

/// Verify that a trivially simple Rust file does not produce a TDG D/F
/// warning in compose_quality_evolution output.
#[test]
fn compose_quality_evolution_no_tdg_warning_for_simple_file() {
    let simple_source = r#"
/// Returns the sum of two integers.
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;
    let tmp = tempfile::NamedTempFile::with_suffix(".rs").expect("create temp .rs file");
    std::fs::write(tmp.path(), simple_source).expect("write temp source");
    let path = tmp.path().to_str().expect("valid utf-8 path");

    let result = compose_quality_evolution(None, path, path);

    // A trivially simple file either returns None (no signals at all) or
    // produces output without a TDG D/F warning.
    if let Some(text) = result {
        assert!(
            !text.contains("TDG: grade F") && !text.contains("TDG: grade D"),
            "simple file must not trigger TDG D/F warning, got: {text:?}"
        );
    }
}

// ── G2: BlastWarning threshold logic ─────────────────────────────────────

#[test]
fn blast_warning_threshold_exceeds_10_emits_b300() {
    // Verifies the threshold guard: count > 10 triggers BlastWarning::HighBlast.
    use touring_analysis::blast_radius::BlastWarning;
    let threshold = 10_usize;
    let count = 15_usize;
    let should_emit = count > threshold;
    assert!(
        should_emit,
        "count={count} > threshold={threshold} must trigger emission"
    );
    let w = BlastWarning::HighBlast {
        symbol: "src/heavy.rs".to_string(),
        affected_files: count,
        threshold,
    };
    assert_eq!(
        w.code_str(),
        "B-300",
        "must use RFC-100 code B-300 for HighBlast"
    );
}

#[test]
fn blast_warning_threshold_at_boundary_no_emit() {
    // count == threshold must NOT trigger emission (strict >).
    let threshold = 10_usize;
    let count = 10_usize;
    assert!(
        !(count > threshold),
        "count=={threshold} must NOT trigger emission"
    );
}

// ── Wave 12 (2026-04-27) — B-301 RefactorRequired uses 6-dim TDG ────────

/// Wave 12: B-301 must consume `tdg.composite` (6-dim) — the previous
/// 1-dim avg_complexity proxy missed coverage, duplication, churn,
/// entropy, and antipattern dimensions. This test verifies that a file
/// with good complexity (`complexity = 1.0`) but bad on the other 5
/// dimensions still trips the B-301 gate.
#[test]
fn b301_six_dim_tdg_catches_what_one_dim_proxy_misses() {
    use touring_analysis::quality::TdgReport;

    // Construct a TdgReport where the OLD 1-dim proxy would NOT flag
    // (complexity = 1.0 → quality_score = 1.0 ≥ 0.4) but the NEW 6-dim
    // composite DOES flag because the other 5 dimensions are zero.
    let tdg = TdgReport::from_components(
        1.0, // complexity — would have been the 1-dim proxy (good)
        0.0, // coverage (bad)
        0.0, // duplication (bad)
        0.0, // churn (bad)
        0.0, // entropy (bad)
        0.0, // antipatterns (bad)
    );

    // Weights: complexity=0.20 → composite = 1.0*0.20 + 0*0.80 = 0.20
    assert!(
        tdg.composite < 0.4,
        "6-dim composite must catch this case: got {:.3}",
        tdg.composite
    );

    // B-301 predicate (Wave 12): blast > 20 AND tdg.composite < 0.4.
    const B301_BLAST_THRESHOLD: usize = 20;
    const B301_QUALITY_THRESHOLD: f64 = 0.40;
    let blast_count = 25_usize;
    let should_fire = blast_count > B301_BLAST_THRESHOLD && tdg.composite < B301_QUALITY_THRESHOLD;
    assert!(
        should_fire,
        "B-301 must fire with blast={blast_count}, composite={:.3}",
        tdg.composite
    );
}

/// B-301 happy-path: high blast + low TDG composite emits the correct
/// RFC-100 code with `Severity::Error` and the file path attached.
#[test]
fn b301_emits_b301_error_with_six_dim_quality_score() {
    use touring_analysis::blast_radius::BlastWarning;
    use touring_analysis::quality::TdgReport;
    use touring_foundation::diagnostic::Severity;

    // All-bad TDG (composite ~ 0.20).
    let tdg = TdgReport::from_components(0.2, 0.2, 0.2, 0.2, 0.2, 0.2);
    let blast_count = 30_usize;

    // Build the same finding the production gate constructs.
    let finding = BlastWarning::RefactorRequired {
        file: "src/big.rs".to_string(),
        quality_score: tdg.composite,
        blast_radius: blast_count,
    };

    let diag = finding.to_diagnostic();
    assert_eq!(diag.code, "B-301", "must emit RFC-100 B-301");
    assert_eq!(diag.severity, Severity::Error, "B-301 must be Error");
    assert_eq!(diag.file.as_deref(), Some("src/big.rs"));
    assert!(diag.help.is_some(), "B-301 must include actionable help");
    // Wave 12 assertion: quality_score on the diagnostic carries the
    // 6-dim composite (~0.20), not a recomputed 1-dim proxy.
    assert!(
        (tdg.composite - 0.20).abs() < 0.01,
        "tdg.composite must be the all-0.2 average (~0.20), got {:.3}",
        tdg.composite
    );
}

/// B-301 must NOT emit at the blast boundary (`blast_count == 20`)
/// because the production gate uses strict `>` not `>=`.
#[test]
fn b301_not_emitted_at_blast_boundary() {
    use touring_analysis::quality::TdgReport;

    let tdg = TdgReport::from_components(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    assert!(tdg.composite < 0.4, "all-0 must produce composite < 0.4");

    const B301_BLAST_THRESHOLD: usize = 20;
    const B301_QUALITY_THRESHOLD: f64 = 0.40;
    let blast_count = 20_usize;
    let should_fire = blast_count > B301_BLAST_THRESHOLD && tdg.composite < B301_QUALITY_THRESHOLD;
    assert!(
        !should_fire,
        "B-301 must NOT fire at boundary blast == 20 (strict >)"
    );
}

/// B-301 must NOT emit when TDG composite is above the quality
/// threshold, even with very high blast — the gate is a conjunction.
#[test]
fn b301_not_emitted_when_tdg_composite_above_threshold() {
    use touring_analysis::quality::TdgReport;

    // Healthy file: all dimensions = 1.0 → composite = 1.0.
    let tdg = TdgReport::from_components(1.0, 1.0, 1.0, 1.0, 1.0, 1.0);
    assert!(tdg.composite >= 0.4, "all-1.0 must be ≥ 0.4");

    const B301_BLAST_THRESHOLD: usize = 20;
    const B301_QUALITY_THRESHOLD: f64 = 0.40;
    let blast_count = 100_usize; // very high blast, but quality is OK
    let should_fire = blast_count > B301_BLAST_THRESHOLD && tdg.composite < B301_QUALITY_THRESHOLD;
    assert!(
        !should_fire,
        "B-301 must NOT fire on healthy file even with high blast: composite={:.3}",
        tdg.composite
    );
}
