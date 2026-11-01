use super::*;

// ── env isolation for tests that build a production GeneratorContext ──────

/// Runs `f` with `TOURING_PROJECT_ROOT` pinned to a private tempdir, holding the
/// crate-wide [`crate::cli::ENV_LOCK`] for the whole call.
///
/// **Why every `schema_registry_*` test needs it**: `schema_registry_info/check`
/// call `make_context()`, which resolves `$TOURING_PROJECT_ROOT/.claude/touring/
/// knowledge.db` and OPENS it (`build_knowledge_upsert_fn` → `FileKnowledgeDB::new`
/// → `ensure_schema`, plus the wiring gate). Without the lock these tests read the
/// process-global variable while `cli::migrate` / `cli::backup` tests have it
/// pointed at THEIR tempdir, so up to five of them opened — and created, with
/// `ensure_schema`'s older `bash_outcomes`/`gotchas` DDL — the very `knowledge.db`
/// that `cmd_run` was migrating. `cmd_run`'s `CREATE INDEX ... (command_hash)`
/// then hit the window between that CREATE TABLE and its `ALTER TABLE ADD COLUMN`
/// (`no such column: command_hash`; also `database is locked`, `no such column:
/// language`, "knowledge.db must not exist after rollback"). With the var unset
/// the same tests fell back to `./.claude/touring` under the crate dir (a
/// leaked, gitignored database); pinning a tempdir removes that too.
fn with_isolated_project_root<T>(f: impl FnOnce() -> T) -> T {
    struct RestoreEnv(Option<std::ffi::OsString>);
    impl Drop for RestoreEnv {
        fn drop(&mut self) {
            // SAFETY: still under `ENV_LOCK` (the guard below outlives this value).
            match self.0.take() {
                Some(v) => unsafe { std::env::set_var("TOURING_PROJECT_ROOT", v) },
                None => unsafe { std::env::remove_var("TOURING_PROJECT_ROOT") },
            }
        }
    }

    let _env = crate::cli::ENV_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _restore = RestoreEnv(std::env::var_os("TOURING_PROJECT_ROOT"));
    // SAFETY: the caller-side lock above is the single lock every mutation of this
    // variable is taken under, crate-wide.
    unsafe { std::env::set_var("TOURING_PROJECT_ROOT", dir.path()) };
    f()
}

// ── schema_registry_info ─────────────────────────────────────────────────

#[test]
fn test_schema_registry_info_returns_ok() {
    let result = with_isolated_project_root(schema_registry_info);
    assert_eq!(
        result["ok"], true,
        "schema_registry_info must return ok=true"
    );
}

#[test]
fn test_schema_registry_info_has_engine_version() {
    let result = with_isolated_project_root(schema_registry_info);
    assert!(
        result["engine_version"].is_string(),
        "engine_version must be a string, got: {:?}",
        result["engine_version"]
    );
    assert!(
        !result["engine_version"].as_str().unwrap_or("").is_empty(),
        "engine_version must not be empty"
    );
}

#[test]
fn test_schema_registry_info_migration_count_is_number() {
    let result = with_isolated_project_root(schema_registry_info);
    assert!(
        result["migration_count"].is_number(),
        "migration_count must be a number"
    );
}

/// Proves the isolation the helper promises: a `schema_registry_*` call resolves
/// its knowledge DB from `TOURING_PROJECT_ROOT`, so under the helper it must land
/// in the pinned tempdir and nowhere else.
#[test]
fn schema_registry_calls_stay_inside_the_pinned_project_root() {
    let root = with_isolated_project_root(|| {
        let root = std::path::PathBuf::from(
            std::env::var_os("TOURING_PROJECT_ROOT").expect("helper pins the variable"),
        );
        // Same layout `cli::migrate` tests give their root: the DB can only be
        // opened (and thereby created) if the directory is already there.
        std::fs::create_dir_all(root.join(".claude").join("touring")).expect("touring dir");
        let _ = schema_registry_info();
        assert!(
            touring_foundation::TouringConfig::knowledge_db_canonical(&root).exists(),
            "make_context must open the knowledge DB under the pinned root"
        );
        root
    });
    // The pin is restored for the next lock holder: it must not outlive the helper.
    let _env = crate::cli::ENV_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    assert_ne!(
        std::env::var_os("TOURING_PROJECT_ROOT").map(std::path::PathBuf::from),
        Some(root),
        "the pinned root must not leak past the helper"
    );
}

// ── schema_registry_check ────────────────────────────────────────────────

#[test]
fn test_schema_registry_check_current_version_compatible() {
    // The current engine version must always report compatible=true.
    let engine_version = with_isolated_project_root(schema_registry_info)["engine_version"]
        .as_str()
        .expect("engine_version is a string")
        .to_owned();
    let result = with_isolated_project_root(|| schema_registry_check(&engine_version));
    assert_eq!(result["ok"], true);
    assert_eq!(
        result["compatible"], true,
        "current engine version must be compatible with itself"
    );
    assert_eq!(result["requested_version"], engine_version.as_str());
}

#[test]
fn test_schema_registry_check_unknown_version_incompatible() {
    let result = with_isolated_project_root(|| schema_registry_check("99.99.99-nonexistent"));
    assert_eq!(result["ok"], true);
    assert_eq!(
        result["compatible"], false,
        "an unknown version must not be reported as compatible"
    );
}

#[test]
fn test_schema_registry_check_returns_engine_version() {
    let result = with_isolated_project_root(|| schema_registry_check("1.0.0"));
    assert!(
        result["engine_version"].is_string(),
        "engine_version field must be present in schema_registry_check output"
    );
}

// ── Suggestion 4 — check_contracts_in_tantivy functional_signature ────────

#[test]
fn check_contracts_in_tantivy_empty_returns_empty_array() {
    let result = check_contracts_in_tantivy(&[]);
    assert_eq!(result, serde_json::json!([]), "empty input must return []");
}

#[test]
fn check_contracts_in_tantivy_result_has_functional_signature_field() {
    use touring_generator::plan::contracts::SymbolRef;
    let symbols = vec![SymbolRef::named("nonexistent_symbol_xyz_abc_touring")];
    let result = check_contracts_in_tantivy(&symbols);
    let arr = result.as_array().expect("must return array");
    assert_eq!(arr.len(), 1);
    let entry = &arr[0];
    // Suggestion 4: functional_signature key must always be present (null when not found).
    assert!(
        entry.get("functional_signature").is_some(),
        "functional_signature key must be present in contract hint; got: {entry:?}"
    );
    assert_eq!(
        entry["found_in_index"], false,
        "nonexistent symbol must not be found"
    );
    assert_eq!(
        entry["functional_signature"],
        serde_json::Value::Null,
        "functional_signature must be null when symbol not found"
    );
}

// ── S-4 ingest_targets (21/08/2026) ───────────────────────────────────────

#[test]
fn ingest_targets_keeps_files_not_parent_dirs_dedups_in_order_and_drops_blanks() {
    let got = ingest_targets(
        [
            "crates/a/src/x.rs",
            "",
            "crates/a/src/y.rs",
            "crates/a/src/x.rs",
            "  ",
        ]
        .into_iter(),
    );
    assert_eq!(got, vec!["crates/a/src/x.rs", "crates/a/src/y.rs"]);
    // The old S-4 reduced these to their parent `crates/a/src` and rebuilt it.
    assert!(
        got.iter().all(|p| p.ends_with(".rs")),
        "must stay files: {got:?}"
    );
}

#[test]
fn ingest_targets_caps_at_ingest_max_files() {
    let many: Vec<String> = (0..INGEST_MAX_FILES + 17)
        .map(|i| format!("f{i}.rs"))
        .collect();
    let got = ingest_targets(many.iter().map(String::as_str));
    assert_eq!(got.len(), INGEST_MAX_FILES);
    assert_eq!(
        got.first().map(String::as_str),
        Some("f0.rs"),
        "first-seen order kept"
    );
}

#[test]
fn ingest_targets_empty_input_yields_nothing_to_spawn() {
    assert!(ingest_targets(std::iter::empty()).is_empty());
}

// ── suggest/consumer skeletons — a junta suggest↔serde ───────────────────
// Até 2026-08-25 ambos os sítios construíam JSON à mão com QUATRO
// vocabulários de campo que nunca existiram nos structs (merge_strategy,
// run_clippy, write_to_disk, keep_backup) e plan-validate rejeitava o que
// plan-suggest emitia. Estes testes prendem a junta para sempre: o que os
// builders emitem DEVE deserializar como GeneratorPlan.

#[test]
fn suggest_skeleton_deserializes_as_generator_plan() {
    let v = build_skeleton_plan("intent de teste", &GeneratorKind::RustModule);
    let plan: GeneratorPlan =
        serde_json::from_value(v).expect("o skeleton do suggest DEVE parsear como GeneratorPlan");
    assert_eq!(plan.intent, "intent de teste");
    assert_eq!(plan.kind, GeneratorKind::RustModule);
}

#[test]
fn consumer_plan_deserializes_as_generator_plan() {
    let v = build_consumer_plan("crates/x/src/lib.rs", "orphan_sym", "function", 3);
    let plan: GeneratorPlan = serde_json::from_value(v)
        .expect("o plan do consumer-wiring DEVE parsear como GeneratorPlan");
    assert_eq!(plan.kind, GeneratorKind::ConsumerGenerator);
    assert_eq!(plan.contracts.symbols_must_exist.len(), 1);
    assert_eq!(
        plan.contracts.files_must_exist,
        vec!["crates/x/src/lib.rs".to_owned()]
    );
    assert!(plan.metadata.tags.contains(&"orphan_sym".to_owned()));
    assert!(
        !plan.plan_id.is_nil(),
        "consumer plan carrega id único, não o placeholder"
    );
}
