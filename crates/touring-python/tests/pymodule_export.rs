//! Regression guard for the PyInit export (24/09/2026).
//!
//! The defect this pins: for a month the cdylib shipped ZERO `PyInit`
//! symbols — the build was green and every `import claude_learning_kernel`
//! died with "dynamic module does not define module export function". A
//! green build proves nothing about the export table; only looking at it
//! does. Two layers, like the repo's other guards: a content invariant that
//! runs anywhere, and the dynamic-table proof that builds and looks.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

/// Content invariant: the `#[pymodule]` invocation lives in THIS crate (the
/// cdylib) and the registration contract lives in `register_all` — one owner
/// of each side, never both in the rlib again.
#[test]
fn the_pymodule_lives_in_the_cdylib_crate() {
    let shim = include_str!("../src/lib.rs");
    assert!(
        shim.contains("#[pymodule]"),
        "the invocation left the cdylib — the shim is hollow again"
    );
    assert!(
        shim.contains("touring_bindings::python::register_all"),
        "the shim must delegate to the single registration contract"
    );
    let bindings = include_str!("../../touring-bindings/src/python/mod.rs");
    assert!(
        !bindings.contains("#[pymodule]\nfn claude_learning_kernel"),
        "a second invocation back in the rlib splits the ownership again"
    );
    assert!(
        bindings.contains("pub fn register_all"),
        "register_all is the delegation target"
    );
}

/// The export-table proof: build the cdylib and read its dynamic symbols —
/// the exact check the month-long defect would have failed.
#[test]
fn the_cdylib_exports_pyinit_claude_learning_kernel() {
    let root = workspace_root();
    let build = Command::new("cargo")
        .args(["build", "-p", "touring-python"])
        .current_dir(&root)
        .output()
        .expect("spawn cargo build");
    assert!(
        build.status.success(),
        "cargo build -p touring-python failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let so = root.join("target/debug/libclaude_learning_kernel.so");
    let nm = Command::new("nm")
        .args(["-D", so.to_str().expect("utf8 path")])
        .output()
        .expect("spawn nm");
    let table = String::from_utf8_lossy(&nm.stdout);
    assert!(
        table.contains("PyInit_claude_learning_kernel"),
        "PyInit vanished from the dynamic table again — imports would die \
         with a green build, exactly like 23/08-24/09"
    );
}
