//! Neural Hook subcommands (pre-read, post-read, pre-bash, post-bash,
//! pre-edit, post-edit, pre-write, post-write, post-tool-rl, session-start, session-stop).
//!
//! Uses `HookRuntime` for file-knowledge DB and project-root detection.

use crate::hooks::runtime::HookRuntime;

/// Every lifecycle hook `run` accepts.
///
/// Lives in production code, not in the test module, because the test used to
/// keep its own copy: two lists of the same vocabulary that nothing forced to
/// agree (the same drift that let four DAG readers disagree about "completed").
pub const NEURAL_SUBCOMMANDS: &[&str] = &[
    "pre-read",
    "post-read",
    "pre-bash",
    "post-bash",
    "pre-edit",
    "post-edit",
    "pre-write",
    "post-write",
    "post-tool-rl",
    "session-start",
    "session-stop",
];

/// Entry point for the neural hook CLI handler — builds a [`HookRuntime`]
/// rooted at the detected project root, reads the hook payload from stdin, and
/// routes `subcommand` to the matching lifecycle hook
/// (`pre-read`/`post-read`/`pre-bash`/`post-bash`/`pre-edit`/`post-edit`/
/// `pre-write`/`post-write`/`session-start`/`session-stop`/`post-tool-rl`).
/// Returns an error for any unrecognized subcommand.
pub fn run(subcommand: &str) -> anyhow::Result<()> {
    // Reject an unknown name BEFORE touching the disk. Building the runtime
    // first meant a typo needed a healthy knowledge DB to be reported as a
    // typo: on 08/08/2026 a transient `disk I/O error` surfaced instead of
    // `Unknown neural hook`, and the test that asserts the message failed for
    // a reason that had nothing to do with what it tests. Validation that
    // needs no I/O must not be sequenced behind I/O.
    if !NEURAL_SUBCOMMANDS.contains(&subcommand) {
        return Err(anyhow::anyhow!("Unknown neural hook: {subcommand}"));
    }

    let project_root = HookRuntime::detect_project_root();
    let mut runtime = HookRuntime::new(&project_root)
        .map_err(|e| anyhow::anyhow!("HookRuntime init failed: {e}"))?;

    let input = HookRuntime::read_stdin().unwrap_or_else(|_| serde_json::json!({}));

    match subcommand {
        "pre-read" => {
            crate::hooks::pre_read::run(&runtime, &input).map_err(|e| anyhow::anyhow!("{e}"))
        }
        "post-read" => {
            crate::hooks::post_read::run(&runtime, &input).map_err(|e| anyhow::anyhow!("{e}"))
        }
        "pre-bash" => {
            crate::hooks::pre_bash::run(&runtime, &input).map_err(|e| anyhow::anyhow!("{e}"))
        }
        "post-bash" => {
            crate::hooks::post_bash::run(&mut runtime, &input).map_err(|e| anyhow::anyhow!("{e}"))
        }
        "pre-edit" => {
            crate::hooks::pre_edit::run(&runtime, &input).map_err(|e| anyhow::anyhow!("{e}"))
        }
        "post-edit" => {
            crate::hooks::post_edit::run(&mut runtime, &input).map_err(|e| anyhow::anyhow!("{e}"))
        }
        "pre-write" => {
            crate::hooks::pre_write::run(&mut runtime, &input).map_err(|e| anyhow::anyhow!("{e}"))
        }
        "post-write" => {
            crate::hooks::post_write::run(&runtime, &input).map_err(|e| anyhow::anyhow!("{e}"))
        }
        "session-start" => crate::hooks::session_hooks::run_session_start(&mut runtime, &input)
            .map_err(|e| anyhow::anyhow!("{e}")),
        "session-stop" => crate::hooks::session_hooks::run_session_stop(&mut runtime, &input)
            .map_err(|e| anyhow::anyhow!("{e}")),
        "post-tool-rl" => crate::hooks::post_tool_rl::run(&mut runtime, &input)
            .map_err(|e| anyhow::anyhow!("{e}")),
        _ => Err(anyhow::anyhow!("Unknown neural hook: {subcommand}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The production list — no second copy to drift away from it.
    const VALID_SUBCOMMANDS: &[&str] = NEURAL_SUBCOMMANDS;

    fn is_valid_subcommand(subcommand: &str) -> bool {
        VALID_SUBCOMMANDS.contains(&subcommand)
    }

    /// Anti-drift: the guard's list and the dispatch `match` must name the same
    /// hooks. A name in the list without an arm would pass validation and then
    /// fall through to the defensive `_` arm — reported as "unknown" *after*
    /// paying for a runtime, i.e. the bug this file just fixed, reintroduced
    /// one layer down.
    #[test]
    fn every_listed_subcommand_has_a_dispatch_arm() {
        let src = include_str!("neural.rs");
        let dispatch = src
            .split("match subcommand {")
            .nth(1)
            .expect("the dispatch match must exist");
        for cmd in NEURAL_SUBCOMMANDS {
            assert!(
                dispatch.contains(&format!("\"{cmd}\" =>")),
                "{cmd} is accepted by the guard but has no dispatch arm"
            );
        }
    }

    #[test]
    fn an_unknown_subcommand_is_rejected_without_touching_the_disk() {
        // The regression itself: this must hold even when the knowledge DB is
        // unopenable, which is why the guard runs before `HookRuntime::new`.
        // Pointing the runtime at an unwritable root would previously turn the
        // assertion below into a `disk I/O error`.
        let err = run("totally-unknown-hook")
            .expect_err("an unknown hook must be an error")
            .to_string();
        assert!(err.contains("Unknown neural hook"), "got: {err}");
        assert!(!err.contains("HookRuntime"), "no runtime may be built: {err}");
    }

    #[test]
    fn valid_subcommands_list_has_eleven_entries() {
        assert_eq!(VALID_SUBCOMMANDS.len(), 11);
    }

    #[test]
    fn is_valid_subcommand_accepts_all_known() {
        for cmd in VALID_SUBCOMMANDS {
            assert!(is_valid_subcommand(cmd), "expected '{cmd}' to be valid");
        }
    }

    #[test]
    fn is_valid_subcommand_rejects_unknown() {
        assert!(!is_valid_subcommand("unknown"));
        assert!(!is_valid_subcommand("pre-writ")); // partial prefix should still be rejected
        assert!(!is_valid_subcommand(""));
        assert!(!is_valid_subcommand("PRE-READ"));
    }

    #[test]
    fn unknown_subcommand_returns_error() {
        let result = run("totally-unknown-hook");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Unknown neural hook"),
            "expected 'Unknown neural hook' in error, got: {err_msg}"
        );
        assert!(err_msg.contains("totally-unknown-hook"));
    }
}
