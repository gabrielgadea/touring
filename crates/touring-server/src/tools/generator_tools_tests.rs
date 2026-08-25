use super::*;

// ── schema_registry_info ─────────────────────────────────────────────────

#[test]
fn test_schema_registry_info_returns_ok() {
    let result = schema_registry_info();
    assert_eq!(
        result["ok"], true,
        "schema_registry_info must return ok=true"
    );
}

#[test]
fn test_schema_registry_info_has_engine_version() {
    let result = schema_registry_info();
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
    let result = schema_registry_info();
    assert!(
        result["migration_count"].is_number(),
        "migration_count must be a number"
    );
}

// ── schema_registry_check ────────────────────────────────────────────────

#[test]
fn test_schema_registry_check_current_version_compatible() {
    // The current engine version must always report compatible=true.
    let engine_version = schema_registry_info()["engine_version"]
        .as_str()
        .expect("engine_version is a string")
        .to_owned();
    let result = schema_registry_check(&engine_version);
    assert_eq!(result["ok"], true);
    assert_eq!(
        result["compatible"], true,
        "current engine version must be compatible with itself"
    );
    assert_eq!(result["requested_version"], engine_version.as_str());
}

#[test]
fn test_schema_registry_check_unknown_version_incompatible() {
    let result = schema_registry_check("99.99.99-nonexistent");
    assert_eq!(result["ok"], true);
    assert_eq!(
        result["compatible"], false,
        "an unknown version must not be reported as compatible"
    );
}

#[test]
fn test_schema_registry_check_returns_engine_version() {
    let result = schema_registry_check("1.0.0");
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
    let plan: GeneratorPlan =
        serde_json::from_value(v).expect("o plan do consumer-wiring DEVE parsear como GeneratorPlan");
    assert_eq!(plan.kind, GeneratorKind::ConsumerGenerator);
    assert_eq!(plan.contracts.symbols_must_exist.len(), 1);
    assert_eq!(plan.contracts.files_must_exist, vec!["crates/x/src/lib.rs".to_owned()]);
    assert!(plan.metadata.tags.contains(&"orphan_sym".to_owned()));
    assert!(!plan.plan_id.is_nil(), "consumer plan carrega id único, não o placeholder");
}
