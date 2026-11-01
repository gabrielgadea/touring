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

#[path = "../build_git.rs"]
mod build_git;

const BUILD_RS: &str = include_str!("../build.rs");

/// The `.git` entry of this checkout, resolved the way `build.rs` resolves it.
fn dot_git() -> std::path::PathBuf {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_dir
        .ancestors()
        .nth(2)
        .expect("crate dir has a grandparent")
        .join(".git")
}

#[test]
fn build_script_declares_the_resolved_git_inputs() {
    assert!(
        BUILD_RS.contains("build_git::git_inputs")
            && BUILD_RS.contains("cargo:rerun-if-changed={}"),
        "build.rs must declare the git inputs as cargo inputs, or VERGEN_GIT_SHA stays frozen at \
         the SHA of whatever commit last touched a working-tree file — and `--version` then \
         reports a commit that is not the one in the binary"
    );
}

#[test]
fn build_script_reruns_when_head_or_the_branch_ref_moves() {
    let inputs = build_git::git_inputs(&dot_git());
    assert!(
        inputs.iter().any(|p| p.ends_with("HEAD")),
        "HEAD is an input: {inputs:?}"
    );
    assert!(
        inputs
            .iter()
            .any(|p| p.to_string_lossy().contains("refs/heads/")),
        "the branch ref is an input too: a fast-forward advances the ref rather than HEAD's \
         content, and the SHA must follow it: {inputs:?}"
    );
}

/// If the repository moves — or the checkout is a `git worktree`, whose `.git` is a FILE — a
/// literal path silently points at nothing and cargo accepts the missing file without
/// complaint: the staleness returns (measured 28/10/2026 in a worktree). Hence this test reads
/// the resolved HEAD back off disk.
#[test]
fn the_declared_git_inputs_resolve_to_a_real_repository() {
    let inputs = build_git::git_inputs(&dot_git());
    let head = inputs
        .iter()
        .find(|p| p.ends_with("HEAD"))
        .expect("HEAD declared");
    assert!(
        head.exists(),
        "{} does not exist — cargo treats a missing rerun-if-changed path as \"nothing \
         changed\", which is exactly the stale-SHA bug",
        head.display()
    );
}

/// The worktree form, built in a temp dir: `.git` is a file naming the per-worktree dir, whose
/// `commondir` names the shared one — HEAD comes from the first, the refs from the second.
#[test]
fn a_worktree_dot_git_file_is_followed_to_its_head_and_common_refs() {
    let base = std::env::temp_dir().join(format!("touring-build-git-{}", std::process::id()));
    let comum = base.join("repo.git");
    let wt_dir = comum.join("worktrees").join("wt");
    std::fs::create_dir_all(&wt_dir).expect("mkdir");
    std::fs::write(wt_dir.join("HEAD"), "ref: refs/heads/feat/x\n").expect("HEAD");
    std::fs::write(wt_dir.join("commondir"), "../..\n").expect("commondir");
    let checkout = base.join("checkout");
    std::fs::create_dir_all(&checkout).expect("mkdir");
    std::fs::write(
        checkout.join(".git"),
        format!("gitdir: {}\n", wt_dir.display()),
    )
    .expect(".git");

    let inputs = build_git::git_inputs(&checkout.join(".git"));
    assert_eq!(inputs[0], wt_dir.join("HEAD"));
    assert!(
        inputs.contains(&wt_dir.join("../..").join("refs/heads/main")),
        "{inputs:?}"
    );
    assert!(
        inputs.contains(&wt_dir.join("../..").join("refs/heads/feat/x")),
        "{inputs:?}"
    );
    let _ = std::fs::remove_dir_all(&base);
}
