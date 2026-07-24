//! F3 (W12.3) E2E — `touring update` + `touring component` (Pln2 productization F3).
//!
//! RED (2026-07-24): neither `touring update` nor `touring component` exists in
//! the command_table — every invocation below exits non-zero with "unknown
//! command". GREEN: the propagation core lands — `update` re-links
//! `.touring/bin/` to the target toolchain and records the resolved version in
//! `.touring/toolchain.lock` (deterministic `--rollback`); `component`
//! manages optional per-project binaries on top of the same resolution.
//!
//! Isolation: every test pins HOME + TOURING_HOME to TempDirs so the host's
//! real `~/.touring` / `~/.local/bin` are never consulted; `--no-restart`
//! keeps the daemon lifecycle out of scope (covered by w12_5 E2E).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

fn touring_bin() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace = Path::new(manifest_dir).parent().unwrap().parent().unwrap();
    workspace.join("target/release/touring")
}

/// Scaffold a fake toolchain `<home>/toolchains/<version>/bin/` with stub
/// executables for the core binaries plus any `extras`.
fn make_fake_toolchain(touring_home: &Path, version: &str, extras: &[&str]) {
    let bin = touring_home.join("toolchains").join(version).join("bin");
    std::fs::create_dir_all(&bin).expect("mkdir toolchain bin");
    let mut names = vec!["touring", "touring-hook", "touring-daemon"];
    names.extend_from_slice(extras);
    for name in names {
        let p = bin.join(name);
        std::fs::write(&p, format!("#!/bin/sh\necho fake-{name}-{version}\n"))
            .expect("write stub bin");
        let mut perms = std::fs::metadata(&p).expect("stat").permissions();
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
        std::fs::set_permissions(&p, perms).expect("chmod");
    }
}

/// Scaffold a project dir with `.touring/{touring.toml,bin/}` pinned to `channel`.
fn make_project(root: &Path, channel: &str) {
    let dot = root.join(".touring");
    std::fs::create_dir_all(dot.join("bin")).expect("mkdir .touring/bin");
    std::fs::create_dir_all(dot.join("data")).expect("mkdir .touring/data");
    std::fs::write(
        dot.join("touring.toml"),
        format!("[toolchain]\nchannel = \"{channel}\"\n"),
    )
    .expect("write touring.toml");
}

/// Run `touring <args>` with HOME/TOURING_HOME pinned to the sandbox.
fn run_touring(home: &Path, touring_home: &Path, args: &[&str]) -> Output {
    Command::new(touring_bin())
        .args(args)
        .env("HOME", home)
        .env("TOURING_HOME", touring_home)
        // Never let the sandboxed CLI talk to the live session daemon.
        .env_remove("TOURING_DAEMON_SOCKET")
        .env_remove("TOURING_DAEMON_SOCK")
        .output()
        .expect("spawn touring")
}

fn stdout_str(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr_str(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// readlink of a `.touring/bin/<name>` entry, as a String path.
fn link_target(project: &Path, name: &str) -> String {
    std::fs::read_link(project.join(".touring/bin").join(name))
        .unwrap_or_else(|e| panic!("read_link .touring/bin/{name}: {e}"))
        .display()
        .to_string()
}

#[test]
fn test_update_switches_channel_writes_lock_and_rolls_back() {
    let home = TempDir::new().expect("home");
    let th = TempDir::new().expect("touring_home");
    make_fake_toolchain(th.path(), "vA", &[]);
    make_fake_toolchain(th.path(), "vB", &[]);
    let proj = TempDir::new().expect("project");
    make_project(proj.path(), "vA");
    let proj_arg = proj.path().display().to_string();

    // 1. Plain `update` resolves the pin (vA) and links the bins.
    let out = run_touring(
        home.path(),
        th.path(),
        &["update", "--project", &proj_arg, "--no-restart"],
    );
    assert!(
        out.status.success(),
        "update (pin) failed: {}\n{}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        link_target(proj.path(), "touring").contains("/vA/"),
        "expected vA link, got {}",
        link_target(proj.path(), "touring")
    );

    // 2. `update --channel vB` switches the active channel and records it.
    let out = run_touring(
        home.path(),
        th.path(),
        &[
            "update",
            "--channel",
            "vB",
            "--project",
            &proj_arg,
            "--no-restart",
        ],
    );
    assert!(
        out.status.success(),
        "update --channel vB failed: {}\n{}",
        stdout_str(&out),
        stderr_str(&out)
    );
    for bin in ["touring", "touring-hook", "touring-daemon"] {
        assert!(
            link_target(proj.path(), bin).contains("/vB/"),
            "bin {bin} should point at vB, got {}",
            link_target(proj.path(), bin)
        );
    }
    let lock = std::fs::read_to_string(proj.path().join(".touring/toolchain.lock"))
        .expect("toolchain.lock must exist after update");
    assert!(lock.contains("active = \"vB\""), "lock: {lock}");
    assert!(lock.contains("previous = \"vA\""), "lock: {lock}");

    // 3. `update --rollback` restores vA deterministically from the lock.
    let out = run_touring(
        home.path(),
        th.path(),
        &[
            "update",
            "--rollback",
            "--project",
            &proj_arg,
            "--no-restart",
        ],
    );
    assert!(
        out.status.success(),
        "update --rollback failed: {}\n{}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        link_target(proj.path(), "touring").contains("/vA/"),
        "rollback should restore vA, got {}",
        link_target(proj.path(), "touring")
    );
    let lock = std::fs::read_to_string(proj.path().join(".touring/toolchain.lock"))
        .expect("lock after rollback");
    assert!(lock.contains("active = \"vA\""), "lock: {lock}");
}

#[test]
fn test_update_refuses_unknown_toolchain_loud() {
    let home = TempDir::new().expect("home");
    let th = TempDir::new().expect("touring_home");
    make_fake_toolchain(th.path(), "vA", &[]);
    let proj = TempDir::new().expect("project");
    make_project(proj.path(), "vA");
    let proj_arg = proj.path().display().to_string();

    let out = run_touring(
        home.path(),
        th.path(),
        &[
            "update",
            "--channel",
            "v-missing",
            "--project",
            &proj_arg,
            "--no-restart",
        ],
    );
    assert!(
        !out.status.success(),
        "update to a missing toolchain must fail loud"
    );
    let all = format!("{}{}", stdout_str(&out), stderr_str(&out));
    assert!(
        all.contains("not installed") || all.contains("v-missing"),
        "error should name the missing toolchain: {all}"
    );
}

#[test]
fn test_component_list_add_remove_lifecycle() {
    let home = TempDir::new().expect("home");
    let th = TempDir::new().expect("touring_home");
    // vA offers one optional component beyond the core binaries.
    make_fake_toolchain(th.path(), "vA", &["touring-quality"]);
    let proj = TempDir::new().expect("project");
    make_project(proj.path(), "vA");
    let proj_arg = proj.path().display().to_string();

    // Link core bins first so `list` sees a live project.
    let out = run_touring(
        home.path(),
        th.path(),
        &["update", "--project", &proj_arg, "--no-restart"],
    );
    assert!(out.status.success(), "update failed: {}", stderr_str(&out));

    // list: core linked, optional available (not yet linked).
    let out = run_touring(
        home.path(),
        th.path(),
        &["component", "list", "--project", &proj_arg],
    );
    assert!(out.status.success(), "list failed: {}", stderr_str(&out));
    let listed = stdout_str(&out);
    assert!(listed.contains("touring-quality"), "list: {listed}");
    assert!(listed.contains("touring-hook"), "list: {listed}");

    // add: links the optional component from the active toolchain.
    let out = run_touring(
        home.path(),
        th.path(),
        &[
            "component",
            "add",
            "touring-quality",
            "--project",
            &proj_arg,
        ],
    );
    assert!(out.status.success(), "add failed: {}", stderr_str(&out));
    assert!(
        link_target(proj.path(), "touring-quality").contains("/vA/"),
        "component should link from vA, got {}",
        link_target(proj.path(), "touring-quality")
    );

    // remove: unlinks the optional component…
    let out = run_touring(
        home.path(),
        th.path(),
        &[
            "component",
            "remove",
            "touring-quality",
            "--project",
            &proj_arg,
        ],
    );
    assert!(out.status.success(), "remove failed: {}", stderr_str(&out));
    assert!(
        !proj.path().join(".touring/bin/touring-quality").exists(),
        "component link must be gone after remove"
    );

    // …but refuses to remove a core binary (potentialize, never reduce).
    let out = run_touring(
        home.path(),
        th.path(),
        &[
            "component",
            "remove",
            "touring-hook",
            "--project",
            &proj_arg,
        ],
    );
    assert!(
        !out.status.success(),
        "removing a core binary must be refused"
    );
}

#[test]
fn test_update_and_component_help_exit_zero() {
    let home = TempDir::new().expect("home");
    let th = TempDir::new().expect("touring_home");
    for args in [&["update", "--help"][..], &["component", "--help"][..]] {
        let out = run_touring(home.path(), th.path(), args);
        assert!(
            out.status.success(),
            "`touring {}` must exit 0 (command_table registration proof), got: {}\n{}",
            args.join(" "),
            stdout_str(&out),
            stderr_str(&out)
        );
        assert!(
            stdout_str(&out).contains("USAGE"),
            "help should print USAGE: {}",
            stdout_str(&out)
        );
    }
}

#[test]
fn test_toolchain_install_from_source() {
    let home = TempDir::new().expect("home");
    let th = TempDir::new().expect("touring_home");
    std::fs::create_dir_all(th.path().join("toolchains")).expect("toolchains dir");

    // Fake canonical-source workspace: target/release/ with the core binaries.
    let src = TempDir::new().expect("source");
    let release = src.path().join("target/release");
    std::fs::create_dir_all(&release).expect("mkdir release");
    for name in ["touring", "touring-hook", "touring-daemon"] {
        let p = release.join(name);
        std::fs::write(&p, format!("#!/bin/sh\necho src-{name}\n")).expect("stub");
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&p).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&p, perms).unwrap();
    }

    let src_arg = src.path().display().to_string();
    let out = run_touring(
        home.path(),
        th.path(),
        &["toolchain", "install", "--from-source", &src_arg, "vdev"],
    );
    assert!(
        out.status.success(),
        "install --from-source failed: {}\n{}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let installed = th.path().join("toolchains/vdev/bin");
    for name in ["touring", "touring-hook", "touring-daemon"] {
        assert!(
            installed.join(name).is_file(),
            "missing installed bin {name}"
        );
    }
    let meta =
        std::fs::read_to_string(th.path().join("toolchains/vdev/meta.toml")).expect("meta.toml");
    assert!(meta.contains("local-source"), "meta: {meta}");
}
