//! Build-time metadata emission for the `touring` binary.
//!
//! Wave 5 (2026-04-18): reactivated after the `vergen-gix` 1.0 API
//! stabilized. The three vergen crates had a transitive `vergen-lib`
//! version skew (0.1.6 vs 9.1.0) that broke the "all builders" pattern;
//! the replacement uses the native `Gix::all_git()` + `Rustc::default()`
//! idiom, which pins vergen-lib 9.1.0 through a single call site.
//!
//! Exposes `VERGEN_*` env vars consumed at compile time via
//! `option_env!()` (graceful absence when the feature is off):
//! - `VERGEN_GIT_SHA` — short git SHA (or absent on shallow/non-git builds)
//! - `VERGEN_GIT_DESCRIBE` — `git describe` output when tags exist
//! - `VERGEN_RUSTC_SEMVER` — rustc version used to build the binary
//! - `VERGEN_CARGO_FEATURES` — comma-separated feature flags active
//! - `VERGEN_BUILD_TIMESTAMP` — RFC-3339 build time (UTC)
//!
//! Graceful degradation: every emission is best-effort. A missing
//! toolchain, shallow clone, or repo-less build must not fail the
//! workspace compilation — failures log a `cargo:warning=` and
//! continue. `main.rs` then falls back to `"unknown"` via
//! `option_env!("VERGEN_GIT_SHA").unwrap_or("unknown")`.

fn main() {
    #[cfg(feature = "build-info")]
    emit_vergen();

    // Always re-run when Cargo.toml / build.rs change (features toggle).
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=build.rs");
}

#[cfg(feature = "build-info")]
fn emit_vergen() {
    use vergen_gix::{Emitter, Gix};

    // vergen-gix 10.x (gix 0.84 → gix-date >=0.12, fixes RUSTSEC-2025-0140):
    // `Gix::all_git()` is now infallible (returns the config builder); git
    // probing + graceful defaulting of absent vars happen at `.emit()` time
    // (no `fail_on_error` flag → best-effort, never fails the build).
    let gix = Gix::all_git();

    if let Err(e) = Emitter::default()
        .add_instructions(&gix)
        .and_then(|em| em.emit())
    {
        println!("cargo:warning=vergen emission failed: {e}");
    }

    // rustc + cargo features + timestamp: emit via pure-std, no vergen
    // crate needed. This side-steps the vergen-lib 0.1.x vs 9.x skew
    // entirely — cargo already surfaces these via env vars at build time.
    if let Ok(rustc) =
        std::process::Command::new(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string()))
            .arg("--version")
            .output()
        && let Ok(s) = String::from_utf8(rustc.stdout)
    {
        println!("cargo:rustc-env=VERGEN_RUSTC_SEMVER={}", s.trim());
    }

    if let Ok(ts) = std::process::Command::new("date")
        .arg("-u")
        .arg("+%Y-%m-%dT%H:%M:%SZ")
        .output()
        && let Ok(s) = String::from_utf8(ts.stdout)
    {
        println!("cargo:rustc-env=VERGEN_BUILD_TIMESTAMP={}", s.trim());
    }

    // Cargo already exposes the active feature list via env vars of the
    // form CARGO_FEATURE_<NAME>=1. Aggregate them into a single comma
    // list so `main.rs` can print one line rather than iterating env.
    let mut features: Vec<String> = std::env::vars()
        .filter_map(|(k, _)| {
            k.strip_prefix("CARGO_FEATURE_")
                .map(|n| n.to_lowercase().replace('_', "-"))
        })
        .collect();
    features.sort();
    println!(
        "cargo:rustc-env=VERGEN_CARGO_FEATURES={}",
        features.join(",")
    );
}
