//! Cross-audit E2E — Session 2026-05-10 (touring-hooks surface).
//!
//! Asserts the behavioural contracts of:
//!
//! 1. **`cli_suggester::run`** — the `cli-suggest` PreToolUse handler returns
//!    valid JSON for every (tool_name, tool_input) shape Claude Code emits.
//!    Coverage spans `Bash` / `Grep` / `Glob` / `Read` / `Edit` / `Write` and
//!    explicitly checks the silent-path contract (non-code Read returns "{}").
//!
//! 2. **`hook_registry` wiring** — `cli-suggest` (added 2026-05-10) and
//!    `cli-index-ingest` (B3 fix) are both present in `ALL_DAEMON_HOOK_NAMES`
//!    and in the dispatch table built by `build_dispatch_table`.
//!
//! 3. **JSON output schema invariant** — every non-empty response is a valid
//!    JSON object with a `hookSpecificOutput.additionalContext: String` field;
//!    every empty response is exactly `"{}"`. Claude Code's strict
//!    hook-schema validator rejects anything else.
//!
//! 4. **Confidence gate** — inputs that should not produce a suggestion
//!    (unknown tool, missing field, non-code file) emit `"{}"` rather than
//!    a low-confidence suggestion.
//!
//! 5. **Anti-pattern detection** — the classifier promotes safer Touring
//!    commands for high-cost shell patterns (`sed -i`, `git`, `cargo build`,
//!    `cat *.rs`, raw `grep PascalCase`).

use serde_json::{Value, json};
use tempfile::TempDir;

use touring_hooks::cli_suggester;
use touring_hooks::runtime::HookRuntime;

/// Build a throw-away `HookRuntime` rooted at a tempdir. The cli_suggester
/// only queries read-only state (FileKnowledge / SymbolStore) — a fresh
/// runtime is empty but functional, and that's exactly what we want for the
/// classifier paths (no enrichment data to leak between tests).
fn make_runtime() -> (TempDir, HookRuntime) {
    let tmp = TempDir::new().expect("tempdir");
    std::fs::create_dir_all(tmp.path().join(".claude/data")).expect("mkdir");
    let rt = HookRuntime::new(tmp.path()).expect("hook runtime init");
    (tmp, rt)
}

/// Parse the raw JSON string returned by `cli_suggester::run`, returning the
/// `additionalContext` text when present (None for the `"{}"` empty case).
fn additional_context(output: &str) -> Option<String> {
    let v: Value = serde_json::from_str(output).expect("output is valid JSON");
    if v.as_object().map(|o| o.is_empty()).unwrap_or(false) {
        return None;
    }
    v.get("hookSpecificOutput")
        .and_then(|h| h.get("additionalContext"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
}

// ── (1) Per-tool classifier coverage ─────────────────────────────────────────

#[test]
fn classifier_grep_pascalcase_emits_symbol_lookup() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Grep",
        "tool_input": { "pattern": "DomainCircuitBreaker" }
    });
    let out = cli_suggester::run(&rt, &payload);
    let ctx = additional_context(&out).expect("non-empty");
    assert!(ctx.contains("symbol-lookup"), "cluster missing: {ctx}");
    assert!(ctx.contains("touring index find DomainCircuitBreaker"));
    assert!(ctx.contains("touring wiring impact"));
}

#[test]
fn classifier_grep_free_text_routes_to_tantivy() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Grep",
        "tool_input": { "pattern": "TODO fix the thing" }
    });
    let out = cli_suggester::run(&rt, &payload);
    let ctx = additional_context(&out).expect("non-empty");
    assert!(ctx.contains("free-text-search"), "cluster wrong: {ctx}");
    assert!(ctx.contains("tantivy search"));
}

#[test]
fn classifier_bash_sed_inplace_promotes_taco_forge_perfect_edit() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Bash",
        "tool_input": { "command": "sed -i 's/old/new/g' foo.rs" }
    });
    let out = cli_suggester::run(&rt, &payload);
    let ctx = additional_context(&out).expect("non-empty");
    assert!(
        ctx.contains("anti-pattern-bash-edit"),
        "cluster wrong: {ctx}"
    );
    assert!(ctx.contains("Edit tool"));
}

#[test]
fn classifier_bash_git_routes_to_regra11_prohibition() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Bash",
        "tool_input": { "command": "git log --oneline" }
    });
    let out = cli_suggester::run(&rt, &payload);
    let ctx = additional_context(&out).expect("non-empty");
    assert!(
        ctx.contains("regra-11-git-prohibited"),
        "cluster wrong: {ctx}"
    );
    assert!(ctx.contains("touring memory recall"));
}

#[test]
fn classifier_bash_cargo_routes_to_doctor() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Bash",
        "tool_input": { "command": "cargo build -p touring-core --release" }
    });
    let out = cli_suggester::run(&rt, &payload);
    let ctx = additional_context(&out).expect("non-empty");
    assert!(
        ctx.contains("system-health-precheck"),
        "cluster wrong: {ctx}"
    );
    assert!(ctx.contains("touring doctor"));
}

#[test]
fn classifier_read_rust_emits_rust_semantic_and_tdg() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Read",
        "tool_input": { "file_path": "crates/foo/src/lib.rs" }
    });
    let out = cli_suggester::run(&rt, &payload);
    let ctx = additional_context(&out).expect("non-empty");
    assert!(ctx.contains("read-rust-comprehend"));
    assert!(ctx.contains("rust-semantic"));
    assert!(ctx.contains("touring ast tdg"));
}

#[test]
fn classifier_write_tsx_routes_to_perfect_create_tsx() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Write",
        "tool_input": { "file_path": "src/components/Button.tsx" }
    });
    let out = cli_suggester::run(&rt, &payload);
    let ctx = additional_context(&out).expect("non-empty");
    assert!(ctx.contains("new-reactcomponent"), "cluster wrong: {ctx}");
    assert!(ctx.contains("Write tool"));
}

#[test]
fn classifier_edit_rust_emits_pre_edit_triage_with_tdg_gate() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Edit",
        "tool_input": { "file_path": "crates/foo/src/lib.rs" }
    });
    let out = cli_suggester::run(&rt, &payload);
    let ctx = additional_context(&out).expect("non-empty");
    assert!(ctx.contains("pre-edit-triage-rust"));
    assert!(ctx.contains("touring ast tdg"));
    assert!(ctx.contains("STOP at grade D/F") || ctx.contains("STOP at TDG D/F"));
}

#[test]
fn classifier_glob_rust_pattern_suggests_workspace_info() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Glob",
        "tool_input": { "pattern": "**/*.rs" }
    });
    let out = cli_suggester::run(&rt, &payload);
    let ctx = additional_context(&out).expect("non-empty");
    assert!(ctx.contains("file-enumeration"));
    assert!(ctx.contains("touring index files"));
}

// ── (2) Silent path — empty response when no high-confidence suggestion ──────

#[test]
fn classifier_silent_for_non_code_read() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Read",
        "tool_input": { "file_path": "README.md" }
    });
    let out = cli_suggester::run(&rt, &payload);
    assert_eq!(out, "{}", "non-code Read must emit empty {{}}");
}

#[test]
fn classifier_silent_for_unknown_tool() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "FuturisticToolFromMars",
        "tool_input": { "some": "value" }
    });
    let out = cli_suggester::run(&rt, &payload);
    assert_eq!(out, "{}");
}

#[test]
fn classifier_silent_for_missing_tool_name() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({ "tool_input": { "x": 1 } });
    let out = cli_suggester::run(&rt, &payload);
    assert_eq!(out, "{}");
}

// ── (3) Output schema invariant ──────────────────────────────────────────────

#[test]
fn every_non_empty_output_is_a_valid_json_object_with_additional_context() {
    let (_tmp, rt) = make_runtime();
    let payloads = vec![
        json!({"tool_name": "Grep", "tool_input": {"pattern": "FooBar"}}),
        json!({"tool_name": "Bash", "tool_input": {"command": "sed -i 's/a/b/' x.rs"}}),
        json!({"tool_name": "Bash", "tool_input": {"command": "cargo test"}}),
        json!({"tool_name": "Read", "tool_input": {"file_path": "x.py"}}),
        json!({"tool_name": "Edit", "tool_input": {"file_path": "x.ts"}}),
        json!({"tool_name": "Write", "tool_input": {"file_path": "x.rs"}}),
    ];
    for p in payloads {
        let out = cli_suggester::run(&rt, &p);
        let v: Value = serde_json::from_str(&out)
            .unwrap_or_else(|e| panic!("invalid JSON for {p}: {e}\nout={out}"));
        assert!(v.is_object(), "must be object: {out}");
        if !v.as_object().expect("obj").is_empty() {
            let h = v
                .get("hookSpecificOutput")
                .unwrap_or_else(|| panic!("missing hookSpecificOutput in {out}"));
            assert_eq!(
                h.get("hookEventName").and_then(|x| x.as_str()),
                Some("PreToolUse")
            );
            assert!(
                h.get("additionalContext")
                    .and_then(|x| x.as_str())
                    .is_some()
            );
        }
    }
}

// ── (4) Hook registry wiring — cli-suggest + cli-index-ingest both present ──

#[test]
fn hook_registry_contains_session_handlers() {
    use touring_hooks::hook_registry::ALL_DAEMON_HOOK_NAMES;
    assert!(
        ALL_DAEMON_HOOK_NAMES.contains(&"cli-suggest"),
        "cli-suggest missing from ALL_DAEMON_HOOK_NAMES"
    );
    assert!(
        ALL_DAEMON_HOOK_NAMES.contains(&"cli-index-ingest"),
        "cli-index-ingest missing from ALL_DAEMON_HOOK_NAMES"
    );
}

#[test]
fn hook_registry_dispatch_table_routes_session_handlers() {
    let dispatch = touring_hooks::hook_registry::build_dispatch_table();
    assert!(
        dispatch.contains_key("cli-suggest"),
        "cli-suggest missing from dispatch table"
    );
    assert!(
        dispatch.contains_key("cli-index-ingest"),
        "cli-index-ingest missing from dispatch table"
    );
}

// ── (5) Disable-via-env contract ─────────────────────────────────────────────

#[test]
fn classifier_respects_disable_env_var() {
    let (_tmp, rt) = make_runtime();

    // Set the env var, then call — output must be "{}" even on a clean
    // symbol-lookup input that would otherwise emit C03.
    // SAFETY: test serial via env var requires single-threaded execution
    // (cargo test runs each test fn on its own task — set_var here is OK
    // because we unset before returning).
    unsafe {
        std::env::set_var("TOURING_SUGGESTER_DISABLED", "1");
    }
    let out = cli_suggester::run(
        &rt,
        &json!({"tool_name": "Grep", "tool_input": {"pattern": "DomainCircuitBreaker"}}),
    );
    unsafe {
        std::env::remove_var("TOURING_SUGGESTER_DISABLED");
    }
    assert_eq!(out, "{}", "DISABLED env var must silence the classifier");
}

// ── (6) TTL cache de-duplicates identical inputs in the same process ─────────

#[test]
fn classifier_ttl_cache_suppresses_duplicate_input_in_same_process() {
    let (_tmp, rt) = make_runtime();
    let payload = json!({
        "tool_name": "Grep",
        "tool_input": { "pattern": "UniqueAuditSymbolName_E2E" }
    });

    let first = cli_suggester::run(&rt, &payload);
    assert_ne!(first, "{}", "first call must emit");

    let second = cli_suggester::run(&rt, &payload);
    assert_eq!(
        second, "{}",
        "second call with identical input must hit the TTL cache"
    );

    // A different pattern bypasses the cache.
    let different = cli_suggester::run(
        &rt,
        &json!({"tool_name": "Grep", "tool_input": {"pattern": "AnotherSymbol_E2E"}}),
    );
    assert_ne!(different, "{}", "different input must NOT hit the cache");
}
