//! The `--version` git label must not go stale (17/10/2026).
//!
//! `VERGEN_GIT_SHA` reaches the binary through `option_env!`, so it is frozen when the crate
//! COMPILES, not when the binary runs. A commit that changes no working-tree file therefore
//! leaves the previous SHA baked into a freshly-linked binary: measured here, a binary built
//! at 21:26 printed `git 4455e8c2` while HEAD was `a97c4c9d` — four commits later.
//!
//! This is not cosmetic. `--version` is the label the release/propagation gate reads to decide
//! whether a binary is fresh, so a frozen SHA lets a **stale binary pass a freshness check** —
//! the "the version label does not prove the build" failure, with the label itself as the
//! false evidence.
//!
//! The tests read `build.rs` rather than running a build: they are fast, need no git, and fail
//! the moment the `rerun-if-changed` declaration is removed.

use std::path::Path;

const BUILD_RS: &str = include_str!("../build.rs");

#[test]
fn build_script_reruns_when_head_moves() {
    assert!(
        BUILD_RS.contains("rerun-if-changed=../../.git/HEAD"),
        "build.rs must declare ../../.git/HEAD as a cargo input, or VERGEN_GIT_SHA stays frozen at \
         the SHA of whatever commit last touched a working-tree file — and `--version` then \
         reports a commit that is not the one in the binary"
    );
}

#[test]
fn build_script_reruns_when_the_branch_ref_moves() {
    assert!(
        BUILD_RS.contains("refs/heads/"),
        "build.rs must declare the branch ref as an input too: a fast-forward advances the ref \
         rather than HEAD's content, and the SHA must follow it"
    );
}

/// The declared paths are relative to the crate directory (`crates/touring-server`), so
/// `../../.git/HEAD` is the workspace checkout. If the repository moves, these silently point at
/// nothing and cargo accepts the missing file without complaint — the staleness returns.
///
/// Hence this test reads them back off disk.
#[test]
fn the_declared_git_inputs_resolve_to_a_real_repository() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for relative in ["../../.git/HEAD"] {
        let resolved = crate_dir.join(relative);
        assert!(
            resolved.exists(),
            "{relative} does not resolve from {} — cargo treats a missing rerun-if-changed \
             path as \"nothing changed\", which is exactly the stale-SHA bug",
            crate_dir.display()
        );
    }
}