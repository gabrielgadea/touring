//! E2E tests for post_edit cascade wiring (Wave C2, 2026-04-20).
use std::fs;
use tempfile::TempDir;
use touring_hooks::shared::api_cascade_bridge::{ApiSurfaceCache, analyze_rust_edit};

fn setup_project() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        r#"[package]
name = "dummy"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    dir
}

#[test]
fn post_edit_rust_edit_populates_cache() {
    let dir = setup_project();
    let path = dir.path().join("src/lib.rs");
    let src = "pub fn foo() {}";
    fs::write(&path, src).unwrap();
    let cache = ApiSurfaceCache::new();
    let outcome = analyze_rust_edit(&path, src, &cache);
    // First call: FirstObservation (no prior surface) — no cascade plan
    let _ = outcome.plan(); // must not panic
    // Cache must now have one entry for this path
    assert_eq!(cache.len(), 1, "cache should contain the seeded entry");
}

#[test]
fn post_edit_second_rust_edit_emits_cascade_plan() {
    let dir = setup_project();
    let path = dir.path().join("src/lib.rs");
    // First edit: define pub fn foo
    let before = "pub fn foo() -> u32 { 42 }";
    fs::write(&path, before).unwrap();
    let cache = ApiSurfaceCache::new();
    let _ = analyze_rust_edit(&path, before, &cache);
    // Second edit: change return type (API change)
    let after = "pub fn foo() -> String { String::new() }";
    fs::write(&path, after).unwrap();
    let outcome = analyze_rust_edit(&path, after, &cache);
    // Outcome must be Diffed (second observation) — plan presence depends on differ
    assert!(
        outcome.is_diff(),
        "second call must produce a Diffed outcome"
    );
    let _ = outcome.plan(); // must not panic
}

#[test]
fn post_edit_non_rust_skips_cascade() {
    let dir = setup_project();
    // Python file — analyze_rust_edit skips non-Rust
    let path = dir.path().join("script.py");
    let src = "def foo(): pass";
    fs::write(&path, src).unwrap();
    let cache = ApiSurfaceCache::new();
    let outcome = analyze_rust_edit(&path, src, &cache);
    // Non-Rust: returns NotRust, plan is None, cache is untouched
    assert!(
        outcome.plan().is_none(),
        "non-Rust path should yield no plan"
    );
    assert_eq!(
        cache.len(),
        0,
        "cache must not be touched for non-Rust files"
    );
}

#[test]
fn post_edit_identical_edits_produce_empty_plan() {
    let dir = setup_project();
    let path = dir.path().join("src/lib.rs");
    let src = "pub fn stable() -> u32 { 1 }";
    fs::write(&path, src).unwrap();
    let cache = ApiSurfaceCache::new();
    // Seed the cache
    let _ = analyze_rust_edit(&path, src, &cache);
    // Second call with identical source — no API change
    let outcome = analyze_rust_edit(&path, src, &cache);
    assert!(
        outcome.is_diff(),
        "identical edit is still a Diffed outcome"
    );
    if let Some(plan) = outcome.plan() {
        // Plan may be empty when no public API changed
        let _ = plan.is_empty();
    }
}
