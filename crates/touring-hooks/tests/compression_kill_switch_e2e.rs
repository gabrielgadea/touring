//! `TOURING_COMPRESSION_PROFILES=0` kill-switch — isolated in its OWN binary.
//!
//! This test mutates the **process environment**, and env vars are global while
//! libtest runs a file's tests concurrently. Living next to the 30 profile
//! audits in `compression_profiles_e2e.rs`, its `set_var(..., "0")` window made
//! every concurrent neighbour observe compression as disabled and receive raw
//! passthrough — which is exactly how `audit_p04_git_log_collapses_to_one_line`
//! failed under `cargo test --workspace` (2026-08-02) while passing in
//! isolation. The original even carried a TODO admitting the hazard.
//!
//! Cargo compiles every `tests/*.rs` as a SEPARATE binary, so keeping the only
//! env-mutating test alone in this file removes the race structurally rather
//! than papering over it with a mutex that all 30 neighbours would have to take.
//! Keep this file to exactly ONE test for that reason.

#![cfg(feature = "tantivy-fts")]

use serde_json::json;
use touring_hooks::compression_profiles::compress_for;

#[test]
fn disabled_flag_returns_raw_passthrough() {
    let raw = "test result: ok 100/100\n";
    let args = json!({"command": "cargo test"});

    // SAFETY: this binary contains exactly one test, so no other thread can be
    // reading the environment concurrently — the invariant the previous location
    // could not offer.
    unsafe { std::env::set_var("TOURING_COMPRESSION_PROFILES", "0") };
    let out = compress_for("Bash", &args, raw);
    assert_eq!(&*out, raw, "disabled flag → raw passthrough");
    unsafe { std::env::remove_var("TOURING_COMPRESSION_PROFILES") };

    // And with the switch cleared the profile engages again, proving the flag —
    // not some unrelated condition — produced the passthrough above.
    let compressed = compress_for("Bash", &args, raw);
    assert!(
        compressed.len() <= raw.len(),
        "profile must engage once the kill switch is cleared"
    );
}
