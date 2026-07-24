//! End-to-end proof for the polyglot wiring PoC (keystone P-A).
//!
//! Proves that under `TOURING_POLYGLOT_WIRING=1` a first-party Python source
//! file populates the wiring graph (producer + consumer rows) and participates
//! in orphan detection / integration scoring — the model the Rust-only `.rs`
//! gate blocked (`docs/2026-07-03-polyglot-parity-plan.md` §5). Also proves the
//! 258-FP defense still holds under the flag (docs/*.py, venv rejected).
//!
//! Gated on the `knowledge` feature (the layer these functions live in) so the
//! default `cargo test -p touring-storage` build is unaffected. Run with:
//! `cargo test -p touring-storage --features knowledge --test polyglot_wiring_poc`.
#![cfg(feature = "knowledge")]

use tempfile::TempDir;
use touring_storage::knowledge::FileKnowledgeDB;

/// One comprehensive test: the opt-in flag is process-global (read once into a
/// `OnceLock`), so every assertion lives in a single function that sets it
/// before the first wiring call.
#[test]
fn python_populates_wiring_graph_under_flag() {
    // SAFETY: set before any wiring call in this dedicated single-test binary;
    // the value is read exactly once into a OnceLock. No other thread reads the
    // environment concurrently here.
    unsafe {
        std::env::set_var("TOURING_POLYGLOT_WIRING", "1");
    }

    let tmp = TempDir::new().expect("tempdir");
    let db = FileKnowledgeDB::new(&tmp.path().join("poc.db")).expect("open db");

    // Producer: a first-party Python module with two public classes.
    db.register_pub_symbol("pkg/models.py", "User", "class", "public")
        .expect("register User");
    db.register_pub_symbol("pkg/models.py", "Order", "class", "public")
        .expect("register Order");
    // A Rust producer alongside — proves the graph stays mixed-language.
    db.register_pub_symbol("crates/a/src/lib.rs", "Widget", "struct", "public")
        .expect("register Widget");

    // Consumer: another Python module imports User (only). Order stays orphan.
    db.record_consumer("pkg/models.py", "User", "app/main.py", Some(3))
        .expect("wire User");

    // 258-FP defense under the flag: a docs/*.py and a vendored venv file are
    // rejected at the write gate (the calls are no-ops, not errors).
    db.register_pub_symbol("docs/example.py", "Demo", "class", "public")
        .expect("gate docs .py");
    db.register_pub_symbol(
        "apps/svc/venv/lib/site-packages/dep/mod.py",
        "Vendor",
        "class",
        "public",
    )
    .expect("gate venv .py");

    // 1. Python rows landed — non_rust_rows > 0 (the whole point of P-A).
    let diag = db.wiring_db_diagnostic().expect("diagnostic");
    assert!(
        diag.non_rust_rows >= 2,
        "expected Python producer/consumer rows; got non_rust_rows={}",
        diag.non_rust_rows
    );

    // 2. Order (Python, unwired) is an orphan; User (Python, wired) is not.
    let orphans = db.orphan_symbols().expect("orphans");
    let orphan_names: Vec<&str> = orphans.iter().map(|e| e.symbol_name.as_str()).collect();
    assert!(
        orphan_names.contains(&"Order"),
        "unwired Python symbol must appear as orphan; got {orphan_names:?}"
    );
    assert!(
        !orphan_names.contains(&"User"),
        "wired Python symbol must NOT be an orphan; got {orphan_names:?}"
    );

    // 3. The FP-defense files never entered the graph → never orphan.
    assert!(
        !orphan_names.contains(&"Demo"),
        "docs/*.py must stay blocked under the flag"
    );
    assert!(
        !orphan_names.contains(&"Vendor"),
        "vendored site-packages must stay blocked under the flag"
    );

    // 4. integration_score for the Python module = 1 wired / 2 total = 0.5.
    let score = db
        .integration_score("pkg/models.py")
        .expect("integration score");
    assert!(
        (score - 0.5).abs() < 1e-9,
        "expected 0.5 integration score for pkg/models.py; got {score}"
    );

    // 5. Per-module orphan view agrees: Order only.
    let mod_orphans = db
        .orphan_symbols_for_module("pkg/models.py")
        .expect("per-module orphans");
    let mod_names: Vec<&str> = mod_orphans.iter().map(|e| e.symbol_name.as_str()).collect();
    assert_eq!(mod_names, vec!["Order"], "per-module orphan mismatch");
}

/// P-B storage-level proof for TypeScript: `.ts` producer/consumer rows land,
/// orphan detection works, and node_modules / `*.test.ts` stay blocked. The
/// specifier→file resolution is proven separately in `touring-hooks-core`
/// (`ts_js_resolver_tests`); together they cover TS wiring end-to-end.
#[test]
fn typescript_populates_wiring_graph_under_flag() {
    // SAFETY: set before any wiring call; read once into a OnceLock.
    unsafe {
        std::env::set_var("TOURING_POLYGLOT_WIRING", "1");
    }
    let tmp = TempDir::new().expect("tempdir");
    let db = FileKnowledgeDB::new(&tmp.path().join("ts.db")).expect("open db");

    db.register_pub_symbol("web/src/models.ts", "User", "class", "public")
        .expect("register User");
    db.register_pub_symbol("web/src/models.ts", "Order", "class", "public")
        .expect("register Order");
    db.record_consumer("web/src/models.ts", "User", "web/src/app.ts", Some(1))
        .expect("wire User");

    // node_modules + `*.test.ts` are blocked at the write gate.
    db.register_pub_symbol(
        "web/node_modules/react/index.js",
        "React",
        "function",
        "public",
    )
    .expect("gate node_modules");
    db.register_pub_symbol("web/src/models.test.ts", "TestOnly", "function", "public")
        .expect("gate test file");

    let orphans = db.orphan_symbols().expect("orphans");
    let names: Vec<&str> = orphans.iter().map(|e| e.symbol_name.as_str()).collect();
    assert!(
        names.contains(&"Order"),
        "unwired TS symbol must be orphan; got {names:?}"
    );
    assert!(
        !names.contains(&"User"),
        "wired TS symbol must NOT be orphan; got {names:?}"
    );
    assert!(!names.contains(&"React"), "node_modules must be blocked");
    assert!(!names.contains(&"TestOnly"), "*.test.ts must be blocked");

    let score = db
        .integration_score("web/src/models.ts")
        .expect("integration score");
    assert!(
        (score - 0.5).abs() < 1e-9,
        "expected 0.5 integration score for web/src/models.ts; got {score}"
    );
}

/// P-B storage-level proof for Java: `.java` producer/consumer rows land, orphan
/// detection works, `*Test.java` stays blocked, and Go (`.go`) stays deferred
/// (not admitted → no false Go orphans). The FQN→path resolution is proven in
/// `touring-hooks-core` (`java_import_maps_dotted_name_to_source_path`).
#[test]
fn java_wires_and_go_is_deferred_under_flag() {
    // SAFETY: set before any wiring call; read once into a OnceLock.
    unsafe {
        std::env::set_var("TOURING_POLYGLOT_WIRING", "1");
    }
    let tmp = TempDir::new().expect("tempdir");
    let db = FileKnowledgeDB::new(&tmp.path().join("java.db")).expect("open db");

    // Java is file-based: one public class per file.
    db.register_pub_symbol("com/foo/User.java", "User", "class", "public")
        .expect("register User");
    db.register_pub_symbol("com/foo/Order.java", "Order", "class", "public")
        .expect("register Order");
    db.record_consumer("com/foo/User.java", "User", "com/app/Main.java", Some(2))
        .expect("wire User");

    // `*Test.java` blocked at the gate; a Go file is deferred (not admitted).
    db.register_pub_symbol(
        "src/test/java/com/FooTest.java",
        "FooTest",
        "class",
        "public",
    )
    .expect("gate test file");
    db.register_pub_symbol("pkg/service.go", "GoSym", "function", "public")
        .expect("gate .go (deferred)");

    let orphans = db.orphan_symbols().expect("orphans");
    let names: Vec<&str> = orphans.iter().map(|e| e.symbol_name.as_str()).collect();
    assert!(
        names.contains(&"Order"),
        "unwired Java symbol must be orphan; got {names:?}"
    );
    assert!(
        !names.contains(&"User"),
        "wired Java symbol must NOT be orphan; got {names:?}"
    );
    assert!(!names.contains(&"FooTest"), "*Test.java must be blocked");
    assert!(
        !names.contains(&"GoSym"),
        "Go is deferred — .go must not enter the graph (no false orphans)"
    );
}

/// P-G package-aware model: Go participates via the `"go:<import-path>"` key
/// namespace (NOT file-keyed `.go`). Producers (exported symbols across the
/// package's many files) and consumers (`import` + `pkg.Sym()`) both key by the
/// package import-path, so the file-keyed `wiring_map` JOIN resolves — closing
/// the import-path↔file mismatch that made Go a false-orphan risk. File-keyed
/// `.go` stays rejected; vendored packages stay excluded.
#[test]
fn go_package_key_wires_and_file_keyed_go_stays_rejected() {
    // SAFETY: set before any wiring call; read once into a OnceLock.
    unsafe {
        std::env::set_var("TOURING_POLYGLOT_WIRING", "1");
    }
    let tmp = TempDir::new().expect("tempdir");
    let db = FileKnowledgeDB::new(&tmp.path().join("gopkg.db")).expect("open db");

    // Producers: two exported symbols in the SAME package (`mymod/pkg`), as if
    // declared across two files — keyed by the package import-path, not a file.
    db.register_pub_symbol("go:mymod/pkg", "Handler", "function", "public")
        .expect("register Handler");
    db.register_pub_symbol("go:mymod/pkg", "Config", "struct", "public")
        .expect("register Config");

    // Consumer: another package (`mymod/app`) imports mymod/pkg and uses Handler
    // (only). Config stays an unused export → a genuine orphan.
    db.record_consumer("go:mymod/pkg", "Handler", "go:mymod/app", Some(5))
        .expect("wire Handler");

    // File-keyed `.go` stays REJECTED (the false-orphan class) — a no-op.
    db.register_pub_symbol("mymod/pkg/service.go", "FileScoped", "function", "public")
        .expect("gate .go file");
    // Vendored package excluded.
    db.register_pub_symbol("go:mymod/vendor/dep", "Vendor", "function", "public")
        .expect("gate vendored");

    // 1. Go package rows landed (non_rust > 0).
    let diag = db.wiring_db_diagnostic().expect("diagnostic");
    assert!(
        diag.non_rust_rows >= 2,
        "expected Go package producer/consumer rows; got non_rust_rows={}",
        diag.non_rust_rows
    );

    // 2. Config (unused export) is a genuine orphan; Handler (wired) is not.
    let orphans = db.orphan_symbols().expect("orphans");
    let names: Vec<&str> = orphans.iter().map(|e| e.symbol_name.as_str()).collect();
    assert!(
        names.contains(&"Config"),
        "unused Go export must be an orphan; got {names:?}"
    );
    assert!(
        !names.contains(&"Handler"),
        "wired Go export must NOT be an orphan; got {names:?}"
    );

    // 3. File-keyed `.go` + vendored never entered the graph → never orphan
    //    (the false-orphan class Go was deferred over stays defended).
    assert!(
        !names.contains(&"FileScoped"),
        "file-keyed .go must stay rejected"
    );
    assert!(
        !names.contains(&"Vendor"),
        "vendored Go package must be excluded"
    );

    // 4. integration_score for the package = 1 wired / 2 total = 0.5.
    let score = db
        .integration_score("go:mymod/pkg")
        .expect("integration score");
    assert!(
        (score - 0.5).abs() < 1e-9,
        "expected 0.5 integration score for go:mymod/pkg; got {score}"
    );
}
