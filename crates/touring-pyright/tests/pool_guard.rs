//! A process that never opted in to layer 2 must not start (or even look for) a language server.
//!
//! Lives in its own test binary on purpose: the opt-in is a process-wide flag, so this is the only
//! place where "the default state of a fresh process" can be observed without another test flipping it.
//! Hermetic: a fake `.venv/bin/pyright-langserver` leaves a witness file if it is ever executed.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use touring_pyright::pool;
use touring_pyright::pyright::Verdict;

/// A unique scratch project removed on drop.
struct Project {
    root: PathBuf,
}

impl Project {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("touring_pyright_guard_{}", std::process::id()));
        let bin = root.join(".venv/bin");
        fs::create_dir_all(&bin).expect("create the scratch project");
        let server = bin.join("pyright-langserver");
        let witness = root.join("WITNESS");
        fs::write(
            &server,
            format!("#!/bin/sh\ntouch '{}'\nexit 0\n", witness.display()),
        )
        .expect("write the fake server");
        fs::set_permissions(&server, fs::Permissions::from_mode(0o755)).expect("chmod +x");
        Self { root }
    }

    fn witness(&self) -> PathBuf {
        self.root.join("WITNESS")
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn a_process_that_never_opted_in_answers_unknown_and_executes_nothing() {
    let project = Project::new();
    for i in 0..3 {
        let r = pool::checar(&project.root, "<string>", "print(fantasma)\n", None, &[]);
        assert_eq!(r.verdict, Verdict::Unknown, "call #{i}: {r:?}");
        assert_eq!(
            r.motivo.as_deref(),
            Some("camada 2 desligada neste processo (só o daemon mantém pyright)"),
            "call #{i}"
        );
    }
    assert!(
        !project.witness().exists(),
        "the language server of the project was executed by a process that never opted in"
    );
}
