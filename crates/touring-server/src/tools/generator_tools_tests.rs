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
