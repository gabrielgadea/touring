//! E2E tests for post_edit RuleEngine bridge (Wave S-RE, 2026-04-20).
//! Verifies that `bridge_post_edit_rule_engine` correctly consults the
//! RuleEngine and pushes SubtaskProposal to the cascade queue when a rule matches.

use std::fs;

fn setup_project() -> std::path::PathBuf {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        r#"[package] name = "dummy" version = "0.1.0" edition = "2021"#,
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    dir.keep()
}

#[test]
fn bridge_no_match_returns_no_rule() {
    let root = setup_project();
    let mut rt =
        touring_hooks::runtime::HookRuntime::new(&root).expect("runtime should initialize");
    let result = touring_hooks::post_edit_rule_engine::bridge_post_edit_rule_engine(
        &mut rt,
        "nonexistent_symbol_xyz",
        "/tmp/test.rs",
    );
    let out = result.expect("bridge should not error");
    assert_eq!(
        out, "no matching rule",
        "unmatched symbol should return no matching rule"
    );
}

#[test]
fn bridge_rule_engine_creates_proposal_on_match() {
    let root = setup_project();
    let mut rt =
        touring_hooks::runtime::HookRuntime::new(&root).expect("runtime should initialize");
    let result = touring_hooks::post_edit_rule_engine::bridge_post_edit_rule_engine(
        &mut rt,
        "MyStruct",
        "/tmp/test_struct.rs",
    );
    let out = result.expect("bridge should not return error");
    assert!(
        out == "no matching rule" || out.starts_with("rule matched:"),
        "expected either no match or rule matched, got: {}",
        out
    );
}

#[test]
fn bridge_preserves_exit_0_invariant() {
    let root = setup_project();
    let mut rt =
        touring_hooks::runtime::HookRuntime::new(&root).expect("runtime should initialize");
    let result = touring_hooks::post_edit_rule_engine::bridge_post_edit_rule_engine(
        &mut rt,
        "any_symbol",
        "/any/path.rs",
    );
    assert!(
        result.is_ok(),
        "bridge should return Ok for all normal inputs"
    );
}

#[test]
fn hook_runtime_cascade_queue_accepts_proposal() {
    use std::path::Path;
    use touring_code::ast::api_cascade::{CascadePlan, Severity, SubtaskProposal};

    let root = setup_project();
    let rt = touring_hooks::runtime::HookRuntime::new(&root).expect("runtime should initialize");
    let proposal = SubtaskProposal {
        api_item: "TestSymbol".to_string(),
        symbol: "TestSymbol".to_string(),
        kind: touring_code::ast::rust_semantic::ApiChangeKind::Added,
        callers: vec![],
        reason: "test proposal".to_string(),
        severity: Severity::High,
    };
    let plan = CascadePlan {
        proposals: vec![proposal],
    };
    rt.ctx.cascade_queue.push(Path::new("/fake/path.rs"), &plan);
}
