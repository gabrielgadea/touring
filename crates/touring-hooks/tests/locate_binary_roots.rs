//! Which `touring-daemon` an E2E test spawns.
//!
//! Cross-audit R2 (14/09/2026): in a plain `cargo test`, `locate_binary` picked
//! `target/llvm-cov-target/debug/touring-daemon`, the leftover of a coverage run
//! nine hours older than the code under test, because that root came before
//! `target/`. The judge's cargo clause went green on that stale daemon. The
//! fixture below is that exact disk state.

#[path = "common/private_daemon.rs"]
#[allow(dead_code)]
mod private_daemon;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use private_daemon::{pick_binary, target_roots};

const WS: &str = "/ws/target";

fn on_disk(paths: &[&str]) -> impl Fn(&Path) -> bool {
    let set: HashSet<PathBuf> = paths.iter().map(PathBuf::from).collect();
    move |p: &Path| set.contains(p)
}

#[test]
fn a_plain_run_never_picks_a_stale_coverage_binary() {
    let exists = on_disk(&[
        "/ws/target/llvm-cov-target/debug/touring-daemon",
        "/ws/target/release/touring-daemon",
    ]);
    let roots = target_roots(
        None,
        Some(Path::new("/ws/target/debug/deps/binary_e2e-1a2b")),
        Path::new(WS),
    );
    assert_eq!(
        pick_binary("touring-daemon", &roots, exists),
        Some(PathBuf::from("/ws/target/release/touring-daemon"))
    );
}

#[test]
fn a_coverage_run_uses_the_coverage_build() {
    let exists = on_disk(&[
        "/ws/target/llvm-cov-target/debug/touring-daemon",
        "/ws/target/release/touring-daemon",
    ]);
    let roots = target_roots(
        None,
        Some(Path::new(
            "/ws/target/llvm-cov-target/debug/deps/binary_e2e-1a2b",
        )),
        Path::new(WS),
    );
    assert_eq!(
        pick_binary("touring-daemon", &roots, exists),
        Some(PathBuf::from(
            "/ws/target/llvm-cov-target/debug/touring-daemon"
        ))
    );
}

#[test]
fn an_explicit_cargo_target_dir_wins() {
    let exists = on_disk(&["/elsewhere/debug/touring", "/ws/target/release/touring"]);
    let roots = target_roots(
        Some(PathBuf::from("/elsewhere")),
        Some(Path::new("/ws/target/debug/deps/t-1")),
        Path::new(WS),
    );
    assert_eq!(
        pick_binary("touring", &roots, exists),
        Some(PathBuf::from("/elsewhere/debug/touring"))
    );
}

#[test]
fn coverage_is_still_found_when_it_is_the_only_build() {
    // The 02/08/2026 case: nothing under target/, the binary only in the coverage root.
    let exists = on_disk(&["/ws/target/llvm-cov-target/release/touring-hook"]);
    let roots = target_roots(None, Some(Path::new("/opt/runner/test-bin")), Path::new(WS));
    assert_eq!(
        pick_binary("touring-hook", &roots, exists),
        Some(PathBuf::from(
            "/ws/target/llvm-cov-target/release/touring-hook"
        ))
    );
}

#[test]
fn an_executable_outside_a_deps_dir_adds_no_root_and_roots_do_not_repeat() {
    let roots = target_roots(
        None,
        Some(Path::new("/usr/bin/cargo-nextest")),
        Path::new(WS),
    );
    assert_eq!(
        roots,
        vec![
            PathBuf::from(WS),
            PathBuf::from("/ws/target/llvm-cov-target")
        ]
    );
    let own = target_roots(
        Some(PathBuf::from(WS)),
        Some(Path::new("/ws/target/debug/deps/t-1")),
        Path::new(WS),
    );
    assert_eq!(
        own,
        vec![
            PathBuf::from(WS),
            PathBuf::from("/ws/target/llvm-cov-target")
        ]
    );
}

#[test]
fn nothing_built_is_none() {
    let roots = target_roots(None, None, Path::new(WS));
    assert_eq!(pick_binary("touring", &roots, |_| false), None);
}
