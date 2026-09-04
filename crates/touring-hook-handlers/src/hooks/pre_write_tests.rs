use super::*;
use crate::knowledge::{BashOutcome, FileKnowledge, FileKnowledgeDB, FileRelation};
use crate::shared::signals::{assemble_scored_context, score_cmp};
use tempfile::TempDir;

fn setup() -> (TempDir, FileKnowledgeDB) {
    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("test.db")).unwrap();
    (tmp, db)
}

// ── Signal G (Wave 5, 2026-04-18) — pre_write rust workflow advisory ──

#[test]
fn test_ast_content_signals_signal_g_none_for_non_rust() {
    // Non-Rust files must not produce a Signal G hint even when
    // other signals fire — the wave5_workflow helper is Rust-only.
    let signals = ast_content_signals("def foo():\n    pass\n", "foo.py", "foo.py");
    assert!(
        !signals
            .iter()
            .any(|(_, s)| s.starts_with("⚙ rust-workflow:")),
        "non-Rust source must not emit rust-workflow signal; got: {signals:?}"
    );
}

#[test]
fn test_ast_content_signals_signal_g_fires_for_rust_pub_api() {
    // Writing a new .rs file with pub surface must surface the
    // Wave 5 workflow advisory in the signal bundle.
    let content = "pub fn new_api() -> u32 { 42 }\n";
    let signals = ast_content_signals(content, "src/new.rs", "src/new.rs");
    let rust_signal = signals
        .iter()
        .find(|(_, s)| s.starts_with("⚙ rust-workflow:"));
    assert!(
        rust_signal.is_some(),
        "pub fn must trigger rust-workflow signal; got: {signals:?}"
    );
    // Weight must be 1.3 as documented.
    assert!(
        (rust_signal.expect("signal present").0 - 1.3).abs() < f32::EPSILON,
        "Signal G weight must be 1.3, got {}",
        rust_signal.expect("signal present").0
    );
}

#[test]
fn test_ast_content_signals_signal_g_reports_unsafe() {
    let content = "pub unsafe fn raw() -> *const u8 { std::ptr::null() }\n";
    let signals = ast_content_signals(content, "src/raw.rs", "src/raw.rs");
    assert!(
        signals
            .iter()
            .any(|(_, s)| s.starts_with("⚙ rust-workflow:") && s.contains("unsafe=1")),
        "unsafe content must show unsafe=1 in Signal G; got: {signals:?}"
    );
}

#[test]
fn test_ast_content_signals_signal_g_skips_trivial_rust() {
    // Private-only trivial Rust produces no Signal G — the helper
    // filters out noise-level files.
    let signals = ast_content_signals("fn _p() {}", "src/p.rs", "src/p.rs");
    assert!(
        !signals
            .iter()
            .any(|(_, s)| s.starts_with("⚙ rust-workflow:")),
        "trivial private fn must not emit Signal G; got: {signals:?}"
    );
}

/// Create a temp project root with `.claude/data/` and an initialized HookRuntime.
fn setup_runtime() -> (TempDir, HookRuntime) {
    let tmp = TempDir::new().expect("create tempdir");
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".claude/data")).expect("create data dir");
    let mut rt = HookRuntime::new(&root).expect("init runtime");
    // Enable enrichment pipeline so full signal collection runs in tests.
    rt.trigger_enrichment();
    (tmp, rt)
}

#[test]
fn test_pre_write_silent_for_empty_content() {
    let signals = ast_content_signals("", "src/main.rs", "src/main.rs");
    assert!(
        signals.is_empty(),
        "empty content should produce no signals"
    );
}

#[test]
fn test_pre_write_silent_for_empty_path() {
    let signals = antipattern_signals("fn main() {}", "");
    assert!(
        signals.is_empty(),
        "empty path should produce no antipattern signals"
    );
}

#[test]
fn test_pre_write_valid_rust_code() {
    let content = "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\npub fn subtract(a: i32, b: i32) -> i32 {\n    a - b\n}\n";
    let signals = ast_content_signals(content, "src/math.rs", "src/math.rs");
    // Valid code should not panic. speculate_v2 behavior depends on tree-sitter.
    let _ = signals;
}

#[test]
fn test_pre_write_unwrap_antipattern_detected() {
    let content = "pub fn risky() -> String {\n    let data = std::fs::read_to_string(\"file.txt\").unwrap();\n    data\n}\n";
    let signals = antipattern_signals(content, "src/main.rs");
    assert!(
        signals.iter().any(|(_, text)| text.contains("unwrap")),
        "should detect .unwrap() antipattern: {signals:?}"
    );
}

#[test]
fn test_pre_write_todo_antipattern_detected() {
    let content = "pub fn placeholder() {\n    todo!()\n}\n";
    let signals = antipattern_signals(content, "src/main.rs");
    assert!(
        signals.iter().any(|(_, text)| text.contains("todo")),
        "should detect todo!() antipattern: {signals:?}"
    );
}

#[test]
fn test_pre_write_unwrap_allowed_in_tests() {
    let content = "fn test_something() {\n    let result = compute().unwrap();\n    assert_eq!(result, 42);\n}\n";
    let signals = antipattern_signals(content, "src/tests/test_main.rs");
    assert!(
        signals.is_empty(),
        "unwrap in test files should not trigger antipattern: {signals:?}"
    );
}

#[test]
fn test_pre_write_valid_python_code() {
    let content = "def add(a: int, b: int) -> int:\n    return a + b\n\nclass Calculator:\n    def multiply(self, a, b):\n        return a * b\n";
    let signals = antipattern_signals(content, "src/calc.py");
    assert!(
        signals.is_empty(),
        "valid Python code should produce no antipattern signals: {signals:?}"
    );
}

#[test]
fn test_pre_write_bare_except_detected() {
    let content = "def risky():\n    try:\n        do_something()\n    except:\n        pass\n";
    let signals = antipattern_signals(content, "src/handler.py");
    assert!(
        signals.iter().any(|(_, text)| text.contains("except")),
        "should detect bare except: {signals:?}"
    );
}

#[test]
fn test_pre_write_large_file_warning() {
    let content: String = (0..550).map(|i| format!("let x_{i} = {i};\n")).collect();
    let signals = ast_content_signals(&content, "src/large.rs", "src/large.rs");
    assert!(
        signals.iter().any(|(_, text)| text.contains("large_file")),
        "should warn about large file: {signals:?}"
    );
}

// ── S-2.3: E2E tests for blast_radius signal in pre_write ─────────────────

/// E2E: ast_content_signals returns non-empty signals for Rust content with pub symbols.
/// Verifies Signal C (wiring_predict) fires correctly. Blast_radius (Signal F) returns
/// None in pre_write context because SymbolIndex is not available - this is the
/// expected design (pre_write cannot do symbol lookups, only content analysis).
#[test]
fn test_pre_write_blast_radius_signal_rust() {
    let content = "pub fn test_func() {}";
    let signals = ast_content_signals(content, "src/lib.rs", "src/lib.rs");
    // wiring_predict fires because there's 1 pub symbol
    assert!(
        signals
            .iter()
            .any(|(_, text)| text.contains("wiring_predict")),
        "ast_content_signals should include wiring_predict signal for pub symbols, got: {signals:?}"
    );
    // Verify score is correct (1.2 for wiring_predict)
    let wiring_sig = signals.iter().find(|(_, t)| t.contains("wiring_predict"));
    assert!(wiring_sig.is_some(), "wiring_predict signal should exist");
    let (score, _) = wiring_sig.unwrap();
    assert_eq!(*score, 1.2, "wiring_predict score should be 1.2");
}

/// E2E: ast_content_signals works for Python files with pub symbols.
/// Tests that language detection and pub symbol counting work for .py files.
#[test]
fn test_pre_write_blast_radius_signal_python() {
    let content = "def public_fn():\n    pass\n\nexport def another_pub():\n    pass\n";
    let signals = ast_content_signals(content, "src/handler.py", "src/handler.py");
    // wiring_predict fires because there are 2 pub symbols in Python
    assert!(
        signals
            .iter()
            .any(|(_, text)| text.contains("wiring_predict")),
        "ast_content_signals should include wiring_predict for Python pub symbols, got: {signals:?}"
    );
}

/// E2E: ast_content_signals works for TypeScript files with pub symbols.
/// Tests that language detection and pub symbol counting work for .ts files.
#[test]
fn test_pre_write_blast_radius_signal_typescript() {
    let content = "export function add() {}\nexport class Calculator {}\n";
    let signals = ast_content_signals(content, "src/app.ts", "src/app.ts");
    // wiring_predict fires because there are 2 pub symbols in TypeScript
    assert!(
        signals
            .iter()
            .any(|(_, text)| text.contains("wiring_predict")),
        "ast_content_signals should include wiring_predict for TypeScript pub symbols, got: {signals:?}"
    );
}

/// E2E: ast_content_signals with large file (>500 lines) triggers file size awareness.
/// This test verifies Signal D (file size) fires correctly.
#[test]
fn test_pre_write_blast_radius_signal_large_file() {
    let content: String = (0..550).map(|i| format!("let x_{i} = {i};\n")).collect();
    let signals = ast_content_signals(&content, "src/large.rs", "src/large.rs");
    assert!(
        signals.iter().any(|(_, text)| text.contains("large_file")),
        "ast_content_signals should include large_file signal for >500 lines, got: {signals:?}"
    );
}

/// E2E: blast_radius_signal returns None when SymbolIndex is unavailable (pre_write context).
/// This is the expected behavior - pre_write does not have access to the symbol index.
/// The signal is added in ast_content_signals at line 545 but idx_opt=None so it never fires.
#[test]
fn test_pre_write_blast_radius_returns_none_when_no_index() {
    // Verify blast_radius_signal itself returns None when idx_opt is None
    use crate::shared::signals::blast_radius_signal;
    let result = blast_radius_signal(None, "src/lib.rs", false);
    assert!(
        result.is_none(),
        "blast_radius_signal should return None when idx_opt is None, got: {result:?}"
    );
}

/// E2E: pre_write::run_returning produces context without panicking.
/// This is an integration test that exercises the full hook pipeline.
/// S1 (2026-09-01): the risk layer must see the PROPOSED content of a file
/// that does not exist yet — `[risk]` reaches the pre_write context.
#[test]
fn test_pre_write_flags_unwrap_in_proposed_content_of_a_new_file() {
    let (_tmp, mut rt) = setup_runtime();
    let risky = "pub fn parse(v: Option<u8>) -> u8 {\n    v.unwrap()\n}\n";
    let input = serde_json::json!({
        "tool_input": {
            "file_path": rt.project_root.join("src/brand_new_s1.rs").display().to_string(),
            "new_file": true,
            "content": risky
        },
        "tool_name": "Write"
    });
    match run_returning(&mut rt, &input) {
        HookResponse::Context { context, .. } => {
            assert!(
                context.contains("[risk]") && context.contains("unwrap=1"),
                "pre_write context must carry the ast-grep risk of the PROPOSED content, got: {context:?}"
            );
        }
        other => unreachable!("expected Context with [risk], got {other:?}"),
    }
}

/// S7 (2026-09-01): a proposed Python file that does not parse is flagged in
/// the pre_write context before it is written.
#[test]
fn test_pre_write_flags_python_syntax_error_in_proposed_file() {
    let (_tmp, mut rt) = setup_runtime();
    let input = serde_json::json!({
        "tool_input": {
            "file_path": rt.project_root.join("scripts/broken_s7.py").display().to_string(),
            "new_file": true,
            "content": "def foo(:\n    return 1\n"
        },
        "tool_name": "Write"
    });
    match run_returning(&mut rt, &input) {
        HookResponse::Context { context, .. } => {
            assert!(
                context.contains("[py-syntax]"),
                "pre_write context must flag the syntax error of the PROPOSED .py, got: {context:?}"
            );
        }
        other => unreachable!("expected Context with [py-syntax], got {other:?}"),
    }
}

/// S5 (2026-09-01): the pre-write antipattern signal must be ACTIONABLE — it
/// names the line, so Claude can fix the proposed content before writing it.
#[test]
fn test_pre_write_antipattern_signal_carries_line_numbers() {
    let content = "fn main() {\n    let a = 1;\n    let x: Option<i32> = None;\n    let _ = x.unwrap();\n}\n";
    let signals = antipattern_signals(content, "src/main.rs");
    assert_eq!(signals.len(), 1, "{signals:?}");
    let text = &signals[0].1;
    assert!(text.contains("unwrap"), "{text}");
    assert!(
        text.contains("L4:"),
        "the signal must point at the offending line (L4), got: {text}"
    );
}

/// S2 (2026-09-01): a hardcoded secret in the PROPOSED content of a new file
/// reaches the pre_write context as a P0 — without echoing the value.
#[test]
fn test_pre_write_flags_hardcoded_secret_in_proposed_new_file() {
    let (_tmp, mut rt) = setup_runtime();
    // Assembled at runtime so this source file never carries the pattern.
    let dsn = format!(
        "postgres://admin:{}@db.example.com:5432/app",
        ["s3cr3t", "P4ssw0rd"].concat()
    );
    let content = format!("pub fn dsn() -> &'static str {{\n    \"{dsn}\"\n}}\n");
    let input = serde_json::json!({
        "tool_input": {
            "file_path": rt.project_root.join("src/config_s2.rs").display().to_string(),
            "new_file": true,
            "content": content
        },
        "tool_name": "Write"
    });
    match run_returning(&mut rt, &input) {
        HookResponse::Context { context, .. } => {
            assert!(
                context.contains("[secret] P0 F2.4"),
                "pre_write context must carry the F2.4 verdict on the PROPOSED content, got: {context:?}"
            );
            assert!(!context.contains("P4ssw0rd"), "never echo the value: {context:?}");
        }
        other => unreachable!("expected Context with [secret], got {other:?}"),
    }
}

/// S3 (2026-09-02): a NEW .rs that uses a known pub type without importing it
/// gets the `use` path BEFORE the write — imports read from the PROPOSED
/// content (no DB row exists for a file that does not exist), known types from
/// the per-name wiring lookup.
#[test]
fn test_pre_write_suggests_the_use_path_for_a_known_pub_type_in_a_new_file() {
    let (_tmp, mut rt) = setup_runtime();
    rt.ctx
        .knowledge
        .register_pub_symbol("src/tfidf.rs", "TfIdfVectorizer", "struct", "public")
        .expect("register producer");
    let content = "pub fn build() -> TfIdfVectorizer {\n    TfIdfVectorizer::default()\n}\n";
    let input = serde_json::json!({
        "tool_input": {
            "file_path": rt.project_root.join("src/search_s3.rs").display().to_string(),
            "new_file": true,
            "content": content
        },
        "tool_name": "Write"
    });
    match run_returning(&mut rt, &input) {
        HookResponse::Context { context, .. } => {
            assert!(
                context.contains("[import] `TfIdfVectorizer`"),
                "pre_write must predict the missing `use` from the PROPOSED content, got: {context:?}"
            );
            assert!(
                context.contains("use crate::tfidf::TfIdfVectorizer;"),
                "the suggestion carries the concrete path: {context:?}"
            );
        }
        other => unreachable!("expected Context with [import], got {other:?}"),
    }
}

/// A3 (2026-09-02): a NEW file that declares a name the symbol index already
/// defines in another file gets the homonym BEFORE the write, with the
/// file:line to reuse.
#[test]
fn test_pre_write_flags_a_homonym_of_a_name_the_new_file_declares() {
    let (_tmp, mut rt) = setup_runtime();
    rt.symbol_store()
        .expect("runtime symbol store")
        .upsert_symbol(&touring_code::ast::SymbolLocation {
            symbol_name: "TfIdfVectorizer".to_string(),
            file_path: "src/tfidf.rs".to_string(),
            line: 12,
            column: 0,
            is_definition: true,
            kind: Some("struct".to_string()),
        })
        .expect("upsert definition");
    let content = "pub struct TfIdfVectorizer {\n    pub dims: usize,\n}\n";
    let input = serde_json::json!({
        "tool_input": {
            "file_path": rt.project_root.join("src/vectorizer_a3.rs").display().to_string(),
            "new_file": true,
            "content": content
        },
        "tool_name": "Write"
    });
    match run_returning(&mut rt, &input) {
        HookResponse::Context { context, .. } => {
            assert!(
                context.contains("[related] `TfIdfVectorizer`"),
                "pre_write must surface the homonym from the symbol index, got: {context:?}"
            );
            assert!(context.contains("src/tfidf.rs:12"), "{context:?}");
        }
        other => unreachable!("expected Context with [related], got {other:?}"),
    }
}

/// S8 (2026-09-02): the CWE scanner runs over the content about to be WRITTEN.
/// `CweScanLayer` existed since H6 and was wired to no pipeline (REGRA #0).
/// The fixture is assembled at runtime so the detector never fires on this
/// test file itself.
#[test]
fn test_pre_write_flags_a_hardcoded_credential_in_the_new_content() {
    let (_tmp, mut rt) = setup_runtime();
    let prefix = ["s", "k", "-"].concat();
    let content = format!(
        "pub fn client() -> String {{\n    let api_key = \"{prefix}live-PLACEHOLDER\";\n    api_key.to_string()\n}}\n"
    );
    let input = serde_json::json!({
        "tool_input": {
            "file_path": rt.project_root.join("src/client_s8.rs").display().to_string(),
            "new_file": true,
            "content": content
        },
        "tool_name": "Write"
    });
    match run_returning(&mut rt, &input) {
        HookResponse::Context { context, .. } => {
            assert!(
                context.contains("CWE-798"),
                "pre_write must name the hardcoded credential before the write, got: {context:?}"
            );
        }
        other => unreachable!("expected Context with CWE-798, got {other:?}"),
    }
}

/// S9 (2026-09-02): a proposed `.rs` carries its semantic badge — syn's
/// `RustSemanticReport` (generics, trait impls, lifetimes) and the public API
/// count, computed on the content BEFORE it is written.
#[test]
fn test_pre_write_shows_the_rust_semantic_badge_for_a_new_rs_file() {
    let (_tmp, mut rt) = setup_runtime();
    let content = "pub struct Holder<T: Clone> {\n    pub value: T,\n}\n\n\
impl<T: Clone> Default for Holder<T> where T: Default {\n    fn default() -> Self {\n        Self { value: T::default() }\n    }\n}\n\n\
pub fn make<T: Clone + Default>() -> Holder<T> {\n    Holder::default()\n}\n";
    let input = serde_json::json!({
        "tool_input": {
            "file_path": rt.project_root.join("src/holder_s9.rs").display().to_string(),
            "new_file": true,
            "content": content
        },
        "tool_name": "Write"
    });
    match run_returning(&mut rt, &input) {
        HookResponse::Context { context, .. } => {
            assert!(
                context.contains("[rust] semantic "),
                "pre_write must carry the semantic badge for a new .rs, got: {context:?}"
            );
            assert!(context.contains("pub API"), "{context:?}");
        }
        other => unreachable!("expected Context with the [rust] badge, got {other:?}"),
    }
}

/// S10 (2026-09-02, ex-B4): the call graph reaches TS/JS content — the library
/// dispatched them already; only the hook's `.rs`/`.py` filter kept them out.
#[test]
fn test_pre_write_callgraph_signal_fires_for_typescript_and_javascript() {
    let src = "function helper() { return 1; }\nfunction main() { helper(); helper(); }\n";
    for path in ["src/app.ts", "src/app.js"] {
        let (score, sig) = callgraph_signal_for_write(src, path)
            .unwrap_or_else(|| panic!("{path}: the callgraph signal must fire for TS/JS"));
        assert!((score - 1.1).abs() < f32::EPSILON);
        assert!(sig.contains("callers: [main]"), "{path}: {sig}");
    }
    assert!(
        callgraph_signal_for_write("# not code", "notes.md").is_none(),
        "languages without a call graph stay silent"
    );
}

#[test]
fn test_pre_write_run_returning_produces_context() {
    let (_tmp, mut rt) = setup_runtime();

    let rust_content = "pub fn new_func() {\n    let x = 42;\n}\n";
    let input = serde_json::json!({
        "tool_input": {
            "file_path": "/tmp/test_project/src/lib.rs",
            "new_file": true,
            "content": rust_content
        },
        "tool_name": "Write"
    });

    let response = run_returning(&mut rt, &input);
    match response {
        HookResponse::Context { context, .. } => {
            // Context should be produced - at minimum it contains the file info
            assert!(
                !context.is_empty(),
                "pre_write context should not be empty, got: {context:?}"
            );
        }
        HookResponse::Allow => {
            // Allow is acceptable for simple content
        }
        other => {
            unreachable!("unexpected response: {other:?}");
        }
    }
}

/// E2E: pre_write::run_returning with pub symbols produces wiring_predict context.
/// This integration test verifies Signal C (wiring_predict) propagates to the final context.
#[test]
fn test_pre_write_run_returning_includes_wiring_predict() {
    let (_tmp, mut rt) = setup_runtime();

    // Content with pub symbols should trigger wiring_predict
    let rust_content = "pub fn add(a: i32, b: i32) -> i32 { a + b }\n";
    let input = serde_json::json!({
        "tool_input": {
            "file_path": "/tmp/test_project/src/math.rs",
            "new_file": true,
            "content": rust_content
        },
        "tool_name": "Write"
    });

    let response = run_returning(&mut rt, &input);
    match response {
        HookResponse::Context { context, .. } => {
            // wiring_predict signal should be in the context
            assert!(
                context.contains("wiring_predict"),
                "pre_write context should include wiring_predict for pub symbols, got: {context:?}"
            );
        }
        HookResponse::Allow => {
            // Allow is acceptable - not all signals may fire depending on CILA level
        }
        other => {
            unreachable!("unexpected response: {other:?}");
        }
    }
}

// ── End S-2.3 E2E tests ───────────────────────────────────────────────────

#[test]
fn test_pre_write_ast_content_signals_includes_rel_path() {
    // Verify ast_content_signals accepts and uses rel_path parameter.
    let content = "pub fn example() {}";
    let file_path = "src/example.rs";
    let rel_path = "src/example.rs";
    let signals = ast_content_signals(content, file_path, rel_path);
    // The function should not panic and should return a result.
    let _ = signals; // Just verify it doesn't panic.
}

#[test]
fn test_pre_write_with_dependents() {
    let (_tmp, db) = setup();
    db.upsert_relation(&FileRelation {
        source: "app.rs".to_string(),
        target: "utils.rs".to_string(),
        relation_type: "imports".to_string(),
    })
    .unwrap();
    db.upsert_relation(&FileRelation {
        source: "tests.rs".to_string(),
        target: "utils.rs".to_string(),
        relation_type: "imports".to_string(),
    })
    .unwrap();

    let signals = knowledge_signals(&db, "utils.rs", "");
    assert!(
        signals
            .iter()
            .any(|(_, text)| text.contains("2 file(s) import this")),
        "should report dependents: {signals:?}"
    );
}

#[test]
fn test_pre_write_cila_budget_levels() {
    assert_eq!(cila_budget_write(0), 1200);
    assert_eq!(cila_budget_write(1), 1200);
    assert_eq!(cila_budget_write(2), 3000);
    assert_eq!(cila_budget_write(3), 3000);
    assert_eq!(cila_budget_write(4), 6000);
    assert_eq!(cila_budget_write(5), 6000);
    assert_eq!(cila_budget_write(6), 6000);
}

#[test]
fn test_pre_write_scored_signals_sorted() {
    let mut signals: Vec<(f32, String)> = vec![
        (0.5, "low".to_string()),
        (2.5, "high".to_string()),
        (1.0, "mid".to_string()),
    ];
    signals.sort_by(score_cmp);
    assert_eq!(signals[0].1, "high");
    assert_eq!(signals[1].1, "mid");
    assert_eq!(signals[2].1, "low");
}

#[test]
fn test_pre_write_budget_truncation() {
    let signals: Vec<(f32, String)> = vec![
        (2.0, "first signal here".to_string()),
        (
            1.0,
            "second signal that is rather long and should be truncated by budget".to_string(),
        ),
    ];

    let (result_parts, _used) = assemble_scored_context(&signals, 20);

    assert_eq!(
        result_parts.len(),
        1,
        "should only fit first signal within budget"
    );
    assert_eq!(result_parts[0], "first signal here");
}

#[test]
fn test_detect_language() {
    assert_eq!(detect_language("src/main.rs"), "rust");
    assert_eq!(detect_language("src/handler.py"), "python");
    assert_eq!(detect_language("src/app.ts"), "typescript");
    assert_eq!(detect_language("src/app.tsx"), "typescript");
    assert_eq!(detect_language("src/index.js"), "javascript");
    assert_eq!(detect_language("src/index.jsx"), "javascript");
    assert_eq!(detect_language("src/main.go"), "go");
    assert_eq!(detect_language("src/main.c"), "c");
    assert_eq!(detect_language("src/main.cpp"), "cpp");
    assert_eq!(detect_language("src/Main.java"), "java");
    assert_eq!(detect_language("README.md"), "markdown");
    assert_eq!(detect_language("Makefile"), "unknown");
    assert_eq!(detect_language(""), "unknown");
}

#[test]
fn test_pre_write_combined_signals() {
    let (_tmp, db) = setup();
    db.upsert_relation(&FileRelation {
        source: "app.rs".to_string(),
        target: "utils.rs".to_string(),
        relation_type: "imports".to_string(),
    })
    .unwrap();
    db.upsert(&FileKnowledge {
        file_path: "utils.rs".to_string(),
        notes: Some("Bug with caching layer".to_string()),
        ..Default::default()
    })
    .unwrap();
    db.record_bash_outcome(&BashOutcome {
        command: "cargo clippy utils.rs".to_string(),
        command_short: "clippy".to_string(),
        exit_code: 1,
        success: false,
        error_pattern: Some("needless_borrow warning".to_string()),
        file_context: Some("utils.rs".to_string()),
        command_hash: String::new(),
        executed_at: String::new(),
    })
    .unwrap();

    let signals = knowledge_signals(&db, "utils.rs", "");
    let has_impact = signals.iter().any(|(_, t)| t.contains("import this"));
    let has_quality = signals.iter().any(|(_, t)| t.contains("quality"));
    let has_note = signals.iter().any(|(_, t)| t.contains("note:"));

    assert!(has_impact, "should have impact signal: {signals:?}");
    assert!(has_quality, "should have quality signal: {signals:?}");
    assert!(has_note, "should have note signal: {signals:?}");
}

#[test]
fn test_count_pub_symbols_rust() {
    let content = "pub fn add(a: i32, b: i32) -> i32 { a + b }\npub struct Point { x: f64, y: f64 }\npub(crate) fn internal() {}\nfn private() {}\npub enum Color { Red, Green, Blue }\npub async fn fetch() {}\n";
    assert_eq!(count_pub_symbols(content, "rust"), 4);
}

#[test]
fn test_count_pub_symbols_python() {
    let content =
        "def public_fn():\n    pass\n\ndef _private_fn():\n    pass\n\nclass MyClass:\n    pass\n";
    assert_eq!(count_pub_symbols(content, "python"), 2);
}

#[test]
fn test_count_pub_symbols_typescript() {
    let content = "export function add() {}\nexport class Calc {}\nfunction internal() {}\n";
    assert_eq!(count_pub_symbols(content, "typescript"), 2);
}

#[test]
fn test_knowledge_signals_empty_db() {
    let (_tmp, db) = setup();
    let signals = knowledge_signals(&db, "unknown.rs", "");
    assert!(
        signals.is_empty(),
        "empty DB should produce no knowledge signals: {signals:?}"
    );
}

// ── Direct unit tests for pub-symbol detection helpers ───────────────

#[test]
fn test_is_pub_rust_symbol_positive_cases() {
    assert!(is_pub_rust_symbol("pub fn foo()"));
    assert!(is_pub_rust_symbol("pub struct Foo {"));
    assert!(is_pub_rust_symbol("pub enum Bar {"));
    assert!(is_pub_rust_symbol("pub trait Baz {"));
    assert!(is_pub_rust_symbol("pub type Alias = String;"));
    assert!(is_pub_rust_symbol("pub const MAX: usize = 100;"));
    assert!(is_pub_rust_symbol("pub static TABLE: [u8; 4] = [0; 4];"));
    assert!(is_pub_rust_symbol("pub async fn handler()"));
}

#[test]
fn test_is_pub_rust_symbol_negative_cases() {
    assert!(!is_pub_rust_symbol("fn internal()"));
    assert!(!is_pub_rust_symbol("pub(crate) fn semi_pub()"));
    assert!(!is_pub_rust_symbol("pub(super) struct Foo"));
    assert!(!is_pub_rust_symbol("// pub fn commented()"));
    assert!(!is_pub_rust_symbol(""));
}

#[test]
fn test_is_pub_python_symbol_positive_cases() {
    assert!(is_pub_python_symbol("def public_fn():"));
    assert!(is_pub_python_symbol("class MyClass:"));
}

#[test]
fn test_is_pub_python_symbol_negative_cases() {
    assert!(!is_pub_python_symbol("def _private():"));
    assert!(!is_pub_python_symbol("class _Internal:"));
    // indented = method, not top-level
    assert!(!is_pub_python_symbol("    def method(self):"));
}

#[test]
fn test_is_export_js_symbol_positive_cases() {
    assert!(is_export_js_symbol("export function foo() {}"));
    assert!(is_export_js_symbol("export default class Bar {}"));
    assert!(is_export_js_symbol("export const MAX = 100;"));
}

#[test]
fn test_is_export_js_symbol_negative_cases() {
    assert!(!is_export_js_symbol("function internal() {}"));
    assert!(!is_export_js_symbol("const x = 1;"));
    assert!(!is_export_js_symbol("// export function commented() {}"));
}

// ── BM25 gotcha ranking in knowledge_signals ─────────────────────────

#[test]
fn test_knowledge_signals_gotcha_bm25_produces_signal() {
    let (_tmp, db) = setup();
    // Add a gotcha that matches via content regex
    db.add_gotcha(
        "blocking_call",
        "do not use blocking calls inside async fn",
        "warning",
        None,
    )
    .expect("add gotcha");

    let signals = knowledge_signals(
        &db,
        "async_handler.rs",
        "async fn handler() { blocking_call() }",
    );
    let has_gotcha = signals.iter().any(|(_, t)| t.contains("GOTCHA"));
    assert!(
        has_gotcha,
        "content-matched gotcha should appear in signals: {signals:?}"
    );
}

#[test]
fn test_knowledge_signals_gotcha_bm25_top2_cap() {
    let (_tmp, db) = setup();
    // Add 4 gotchas — only top 2 should surface via BM25 ranking
    for i in 0..4 {
        db.add_gotcha(
            "target_module",
            &format!("gotcha message {i}"),
            "warning",
            None,
        )
        .expect("add gotcha");
    }

    let signals = knowledge_signals(&db, "src/target_module.rs", "");
    let gotcha_count = signals.iter().filter(|(_, t)| t.contains("GOTCHA")).count();
    assert!(
        gotcha_count <= 2,
        "knowledge_signals should emit at most 2 gotcha signals, got {gotcha_count}: {signals:?}"
    );
}

#[test]
fn test_knowledge_signals_no_gotchas_no_signal() {
    let (_tmp, db) = setup();
    // No gotchas registered — no GOTCHA signal should appear
    let signals = knowledge_signals(&db, "src/clean_module.rs", "");
    let has_gotcha = signals.iter().any(|(_, t)| t.contains("GOTCHA"));
    assert!(
        !has_gotcha,
        "no gotchas should produce no GOTCHA signal: {signals:?}"
    );
}

// ── is_pub_rust_symbol edge cases ────────────────────────────────────

#[test]
fn test_is_pub_rust_symbol_pub_use_and_mod() {
    // `pub use` and `pub mod` are not fn/struct/etc — should be false.
    assert!(!is_pub_rust_symbol("pub use std::collections::HashMap;"));
    assert!(!is_pub_rust_symbol("pub mod utils;"));
}

#[test]
fn test_is_pub_rust_symbol_indented_pub_fn() {
    // Indented lines (methods inside impl) — trim() still makes them match.
    // This is intentional: count_pub_symbols counts all pub fn, including impl methods.
    assert!(is_pub_rust_symbol("    pub fn method(&self)"));
    assert!(is_pub_rust_symbol("\tpub fn tabbed()"));
}

#[test]
fn test_is_pub_rust_symbol_pub_crate_variants() {
    // pub(crate), pub(super), pub(in path) — all restricted, must be false.
    assert!(!is_pub_rust_symbol("pub(crate) fn internal()"));
    assert!(!is_pub_rust_symbol("pub(super) struct Inner {}"));
    assert!(!is_pub_rust_symbol("pub(in crate::foo) fn scoped()"));
}

#[test]
fn test_is_pub_rust_symbol_empty_and_whitespace() {
    assert!(!is_pub_rust_symbol(""));
    assert!(!is_pub_rust_symbol("   "));
    assert!(!is_pub_rust_symbol("// pub fn commented_out()"));
}

// ── is_pub_python_symbol edge cases ──────────────────────────────────

#[test]
fn test_is_pub_python_symbol_tab_indented_method() {
    // Tab-indented = method body, not top-level.
    assert!(!is_pub_python_symbol("\tdef method(self):"));
}

#[test]
fn test_is_pub_python_symbol_async_def() {
    // `async def` is NOT matched because the line starts with "async", not "def".
    // count_pub_symbols is a heuristic — this is documented behavior.
    assert!(!is_pub_python_symbol("async def fetch():"));
}

#[test]
fn test_is_pub_python_symbol_dunder() {
    // Double-underscore methods start with _ so they are excluded.
    assert!(!is_pub_python_symbol("def __init__(self):"));
    assert!(!is_pub_python_symbol("class __Meta:"));
}

// ── count_pub_symbols unknown language ───────────────────────────────

#[test]
fn test_count_pub_symbols_unknown_language() {
    let content = "pub fn foo() {}\nexport function bar() {}\ndef baz(): pass\n";
    // Unknown language — heuristic has no rule, returns 0.
    assert_eq!(count_pub_symbols(content, "go"), 0);
    assert_eq!(count_pub_symbols(content, ""), 0);
    assert_eq!(count_pub_symbols(content, "unknown"), 0);
}

#[test]
fn test_count_pub_symbols_empty_content() {
    assert_eq!(count_pub_symbols("", "rust"), 0);
    assert_eq!(count_pub_symbols("", "python"), 0);
    assert_eq!(count_pub_symbols("", "typescript"), 0);
}

// ── antipattern_signals test-file detection variants ─────────────────

#[test]
fn test_antipattern_signals_test_dir_skipped() {
    // Files under a `tests/` directory are considered test files.
    let content = "pub fn risky() { data.unwrap() }\n";
    let signals = antipattern_signals(content, "tests/integration.rs");
    assert!(
        signals.is_empty(),
        "tests/ directory files should be skipped: {signals:?}"
    );
}

#[test]
fn test_antipattern_signals_spec_file_skipped() {
    let content = "describe('foo', () => { expect(x).unwrap(); });\n";
    let signals = antipattern_signals(content, "src/foo.spec.ts");
    // spec files are treated as test files — no antipatterns emitted.
    assert!(
        signals.is_empty(),
        "spec files should be skipped: {signals:?}"
    );
}

#[test]
fn test_antipattern_signals_non_test_file_not_skipped() {
    // A file with `test` in the name but not matching test-file pattern
    // should still be scanned.
    let content = "pub fn contest_winner() { result.unwrap() }\n";
    let signals = antipattern_signals(content, "src/contest.rs");
    assert!(
        signals.iter().any(|(_, t)| t.contains("unwrap")),
        "non-test file with unwrap should trigger antipattern: {signals:?}"
    );
}

// ── quality_depth_signals ─────────────────────────────────────────────

#[test]
fn test_quality_depth_test_file_skipped() {
    // Test files should produce no quality_depth signals (avoid noise in test code).
    let content = "fn deeply_nested() { if a { if b { if c { if d { if e { x } } } } } }\n";
    let signals = quality_depth_signals(content, "tests/integration.rs");
    assert!(
        signals.is_empty(),
        "quality_depth_signals should be empty for test files: {signals:?}"
    );
}

#[test]
fn test_quality_depth_high_cc_triggers_signal() {
    // Build content with deeply nested control flow to exceed CC > 15 threshold.
    let content = r#"fn complex(a: bool, b: bool, c: bool, d: bool) {
    if a { if b { if c { if d {
        while a { for _ in 0..10 { if b { match c { true => {} false => {} } } } }
    } } } }
    if a && b { } else if c { } else if d { }
    loop { if a { break; } }
}"#;
    let signals = quality_depth_signals(content, "src/lib.rs");
    assert!(
            signals
                .iter()
                .any(|(score, msg)| *score >= 1.2
                    && (msg.contains("CC") || msg.contains("complexity"))),
            "high complexity content should produce a CC signal: {signals:?}"
        );
}

#[test]
fn test_quality_depth_unwrap_density_triggers_signal() {
    // 5 .unwrap() calls in a short file → risk_score > 0.3 threshold.
    let content = r#"fn risky() -> String {
    let a = get_a().unwrap();
    let b = get_b().unwrap();
    let c = get_c().unwrap();
    let d = get_d().unwrap();
    let e = get_e().unwrap();
    format!("{a}{b}{c}{d}{e}")
}"#;
    let signals = quality_depth_signals(content, "src/risky.rs");
    assert!(
        signals
            .iter()
            .any(|(score, msg)| *score >= 1.4 && msg.contains("unwrap")),
        "high unwrap density should produce an unwrap signal with score ≥ 1.4: {signals:?}"
    );
}

// ── is_export_js_symbol edge cases ───────────────────────────────────

#[test]
fn test_is_export_js_symbol_export_type() {
    // TypeScript `export type` and `export interface` are valid exports.
    assert!(is_export_js_symbol("export type Foo = string;"));
    assert!(is_export_js_symbol("export interface Bar {}"));
}

#[test]
fn test_is_export_js_symbol_indented_export() {
    // Indented export (e.g., inside a namespace block) — trim() makes it match.
    assert!(is_export_js_symbol("  export function inner() {}"));
}

// ── bench_required_features_signal tests ────────────────────────────

#[test]
fn test_classify_bench_or_test_target() {
    assert_eq!(
        classify_bench_or_test_target("benches/my_bench.rs"),
        Some("bench")
    );
    assert_eq!(
        classify_bench_or_test_target("/project/benches/perf.rs"),
        Some("bench")
    );
    assert_eq!(
        classify_bench_or_test_target("tests/integration.rs"),
        Some("test")
    );
    assert_eq!(
        classify_bench_or_test_target("/project/tests/e2e.rs"),
        Some("test")
    );
    assert_eq!(classify_bench_or_test_target("src/main.rs"), None);
    assert_eq!(classify_bench_or_test_target("src/benches.rs"), None);
}

#[test]
fn test_has_non_std_imports_positive() {
    assert!(has_non_std_imports("use my_crate::BlockQuantizer;\n"));
    assert!(has_non_std_imports("use touring_simd::quantization;\n"));
}

#[test]
fn test_has_non_std_imports_negative() {
    assert!(!has_non_std_imports("use std::collections::HashMap;\n"));
    assert!(!has_non_std_imports("use core::mem;\n"));
    assert!(!has_non_std_imports("use alloc::vec::Vec;\n"));
    assert!(!has_non_std_imports("fn main() {}\n"));
}

#[test]
fn test_has_target_without_required_features_missing() {
    let cargo = "\
[[bench]]
name = \"quantize_bench\"
harness = false
";
    assert!(has_target_without_required_features(
        cargo,
        "[[bench]]",
        "quantize_bench"
    ));
}

#[test]
fn test_has_target_without_required_features_present() {
    let cargo = "\
[[bench]]
name = \"quantize_bench\"
harness = false
required-features = [\"quantization\"]
";
    assert!(!has_target_without_required_features(
        cargo,
        "[[bench]]",
        "quantize_bench"
    ));
}

#[test]
fn test_has_target_without_required_features_no_match() {
    let cargo = "\
[[bench]]
name = \"other_bench\"
harness = false
";
    assert!(!has_target_without_required_features(
        cargo,
        "[[bench]]",
        "quantize_bench"
    ));
}

#[test]
fn test_has_target_without_required_features_multiple_sections() {
    let cargo = "\
[[bench]]
name = \"safe_bench\"
required-features = [\"fast\"]

[[bench]]
name = \"risky_bench\"
harness = false
";
    // safe_bench has features, risky_bench does not
    assert!(!has_target_without_required_features(
        cargo,
        "[[bench]]",
        "safe_bench"
    ));
    assert!(has_target_without_required_features(
        cargo,
        "[[bench]]",
        "risky_bench"
    ));
}

#[test]
fn test_scan_toml_section_basic() {
    let lines = vec!["[[bench]]", "name = \"foo\"", "harness = false", ""];
    let mut pos = 0;
    assert!(scan_toml_section(&lines, &mut pos, "foo"));
    assert_eq!(pos, 4); // scanned past all lines
}

#[test]
fn test_scan_toml_section_with_required_features() {
    let lines = vec![
        "[[bench]]",
        "name = \"foo\"",
        "required-features = [\"bar\"]",
        "",
    ];
    let mut pos = 0;
    assert!(!scan_toml_section(&lines, &mut pos, "foo"));
}

// ── Wave 12 (2026-04-27) — B-302 PatchExpansion gate ────────────────────

/// Helper: build a `PatchPreview` for testing the B-302 gate.
#[cfg(feature = "mpatch-fuzzy")]
fn build_patch_preview(
    new_content: &str,
    confidence: f32,
    method: crate::shared::mpatch_preview::PatchMethod,
) -> crate::shared::mpatch_preview::PatchPreview {
    crate::shared::mpatch_preview::PatchPreview {
        matched: true,
        method,
        confidence,
        preview: new_content.to_string(),
    }
}

/// B-302 fires when the patch EXPANDS code (new > old) AND confidence
/// is below the 0.7 threshold. Helper returns `Some(delta)`.
#[cfg(feature = "mpatch-fuzzy")]
#[test]
fn b302_emits_on_low_confidence_expansion() {
    let old_source = "fn foo() {}\n";
    let new_source = "fn foo() {\n    let x = 1;\n    let y = 2;\n}\n"; // bigger
    let preview = build_patch_preview(
        new_source,
        0.55, // low confidence
        crate::shared::mpatch_preview::PatchMethod::Fuzzy,
    );

    let result = super::emit_b302_if_low_confidence_expansion("src/foo.rs", old_source, &preview);

    assert!(
        result.is_some(),
        "B-302 must fire on expansion + low confidence"
    );
    let delta = result.expect("delta must be Some");
    assert!(delta.is_expansion(), "delta must reflect expansion");
    assert!(
        !delta.is_confident(),
        "low-confidence delta must not be is_confident"
    );
}

/// B-302 silent when the patch expands but confidence is high (≥ 0.7).
/// The fuzzy match worked well; no need to alarm the operator.
#[cfg(feature = "mpatch-fuzzy")]
#[test]
fn b302_silent_on_high_confidence_expansion() {
    let old_source = "fn foo() {}\n";
    let new_source = "fn foo() {\n    let x = 1;\n}\n";
    let preview = build_patch_preview(
        new_source,
        0.95, // high confidence
        crate::shared::mpatch_preview::PatchMethod::Exact,
    );

    let result = super::emit_b302_if_low_confidence_expansion("src/foo.rs", old_source, &preview);

    assert!(
        result.is_none(),
        "B-302 must NOT fire when confidence is high"
    );
}

/// B-302 silent when the patch CONTRACTS code (delta < 0), regardless
/// of confidence — only expansions trip the gate.
#[cfg(feature = "mpatch-fuzzy")]
#[test]
fn b302_silent_on_contraction_even_with_low_confidence() {
    let old_source = "fn foo() {\n    let x = 1;\n    let y = 2;\n}\n";
    let new_source = "fn foo() {}\n"; // smaller
    let preview = build_patch_preview(
        new_source,
        0.30, // very low confidence
        crate::shared::mpatch_preview::PatchMethod::Fuzzy,
    );

    let result = super::emit_b302_if_low_confidence_expansion("src/foo.rs", old_source, &preview);

    assert!(
        result.is_none(),
        "B-302 must NOT fire on contraction (only expansions)"
    );
}
