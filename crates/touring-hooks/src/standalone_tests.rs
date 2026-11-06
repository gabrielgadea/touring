//! Guards for the standalone route's declarations (see the module docs of `standalone`).
//!
//! The route is derived from the daemon's dispatch table plus three explicit sets. What can
//! still go wrong is a set that disagrees with the table (an override for a name the table does
//! not know; a "standalone-only" name the table has since gained), a table key that falls
//! between the sets, and a hand-written arm list creeping back into the sources. Each is a test.

use super::*;
use std::collections::HashSet;
use touring_hooks::hook_registry::{all_daemon_hook_names, is_daemon_hook};

/// The production part of a source file of this binary: everything before its first
/// `#[cfg(test)]`.
fn production(full: &str) -> &str {
    &full[..full.find("#[cfg(test)]").unwrap_or(full.len())]
}

/// Every override is a key of the dispatch table: an override for a name the table does not
/// hold is a standalone arm for a hook the daemon never heard of — the drift this module exists
/// to prevent. Feature-aware: an override's `#[cfg]` mirrors its table entry's.
#[test]
fn every_override_is_a_table_key() {
    for entry in OVERRIDES {
        assert!(
            is_daemon_hook(entry.name),
            "override '{}' is not a key of the dispatch table — an override differs FROM a table \
             handler, so it needs one (or it belongs in STANDALONE_ONLY)",
            entry.name
        );
    }
}

/// The 13 overrides measured on 02/11/2026 were converged on 04/11/2026 (aprofundamento #4,
/// approved by Gabriel): every table key runs the daemon's handler standalone too. A new
/// override is a deliberate divergence — add it, state its `why`, and update this pin.
#[test]
fn the_overrides_are_converged() {
    let names: Vec<&str> = OVERRIDES.iter().map(|e| e.name).collect();
    assert!(
        names.is_empty(),
        "standalone overrides came back: {names:?} — each one is a table key whose standalone \
         answer differs from the daemon's; converge it or update this pin with the reason"
    );
}

/// The standalone-only names are NOT table keys. A name the table gains must leave the list, so
/// its handler serves it in both modes instead of the standalone body shadowing it silently.
#[test]
fn standalone_only_and_stateless_names_are_not_table_keys() {
    assert!(!STANDALONE_ONLY.is_empty() && !STATELESS.is_empty());
    let names = STANDALONE_ONLY
        .iter()
        .map(|e| e.name)
        .chain(STATELESS.iter().map(|s| s.name));
    for name in names {
        assert!(
            !is_daemon_hook(name),
            "'{name}' is declared standalone-only but is now a key of the dispatch table — \
             delete its standalone declaration (or move it to OVERRIDES if its standalone \
             behavior must differ)"
        );
    }
}

/// A name is declared once across the three sets — `route` takes the first match, so a second
/// declaration would be dead code that reads as live.
#[test]
fn declared_names_are_unique_across_the_sets() {
    let all: Vec<&str> = OVERRIDES
        .iter()
        .map(|e| e.name)
        .chain(STANDALONE_ONLY.iter().map(|e| e.name))
        .chain(STATELESS.iter().map(|s| s.name))
        .collect();
    let mut seen = HashSet::new();
    for name in &all {
        assert!(seen.insert(*name), "'{name}' is declared twice");
    }
}

/// Every declaration carries its reason, in one line — the reason is what makes an override a
/// decision someone can revisit, rather than an arm nobody dares remove.
#[test]
fn every_declaration_states_a_one_line_reason() {
    let reasons = OVERRIDES
        .iter()
        .map(|e| (e.name, e.why))
        .chain(STANDALONE_ONLY.iter().map(|e| (e.name, e.why)))
        .chain(STATELESS.iter().map(|s| (s.name, s.why)));
    for (name, why) in reasons {
        assert!(
            why.trim().len() > 10 && !why.contains('\n'),
            "'{name}' needs a one-line reason, got {why:?}"
        );
    }
}

/// Every key of the dispatch table has a route here: served from the table, a deliberate
/// override, or an RPC endpoint with its explicit refusal. None is unknown, none is claimed by a
/// standalone-only set (that is the previous test), and the verdicts agree with the predicate.
#[test]
fn every_table_key_has_a_route() {
    let names = all_daemon_hook_names();
    assert!(!names.is_empty());
    for name in names {
        match route(name) {
            Route::Override(_) => {}
            Route::Table => assert!(
                !touring_hooks::hook_registry::is_rpc_endpoint(name),
                "'{name}' is an RPC endpoint but would run from the table"
            ),
            Route::RpcEndpoint => assert!(
                touring_hooks::hook_registry::is_rpc_endpoint(name),
                "'{name}' would be refused as an RPC endpoint but is not one"
            ),
            Route::Stateless(_) | Route::StandaloneOnly(_) | Route::Unknown => {
                panic!("table key '{name}' has no table-derived route")
            }
        }
    }
}

/// THE property: adding a hook to the dispatch table reaches the standalone route with no edit
/// to it. Proven on names the real table does not have, through the same rule `route` applies.
#[test]
fn a_hook_added_to_the_table_is_served_standalone_without_an_edit_here() {
    assert!(matches!(
        route_with("a-hook-added-tomorrow", true),
        Route::Table
    ));
    assert!(matches!(
        route_with("a-hook-added-tomorrow", false),
        Route::Unknown
    ));
    assert!(matches!(
        route_with("cli-a-query-added-tomorrow", true),
        Route::RpcEndpoint
    ));
    // The explicit sets win over the table, which is what the previous tests keep honest.
    // (OVERRIDES is empty since the 04/11/2026 convergence; the rule still holds for any entry.)
    for entry in OVERRIDES {
        assert!(matches!(route_with(entry.name, true), Route::Override(_)));
    }
    let standalone_only = STANDALONE_ONLY[0].name;
    assert!(matches!(
        route_with(standalone_only, false),
        Route::StandaloneOnly(_)
    ));
}

/// No hand-written list of hook names survives in the sources of this binary: no `match` on the
/// subcommand, no string-literal match arm. The declarations are data (`name: "…"`), checked by
/// the tests above against the table; an arm is code nothing checks.
#[test]
fn no_hand_written_arm_list_survives_in_the_sources() {
    for (file, full) in [
        ("main.rs", include_str!("main.rs")),
        ("standalone.rs", include_str!("standalone.rs")),
    ] {
        let prod = production(full);
        assert!(
            !prod.contains("match subcommand"),
            "{file}: a `match subcommand` is a second list of which hooks exist"
        );
        for forbidden in ["ALL_DAEMON_HOOK_NAMES", "task-completed\" =>"] {
            assert!(!prod.contains(forbidden), "{file}: `{forbidden}` is back");
        }
        for (n, line) in prod.lines().enumerate() {
            let t = line.trim_start();
            let is_arm = t.starts_with('"') && t.contains("\" =>") || t.starts_with("| \"");
            assert!(
                !is_arm,
                "{file}:{}: a string-literal match arm ({t}) — declare it in standalone.rs data",
                n + 1
            );
        }
    }
}

/// What a table handler's answer becomes: nothing, `emit`'s funnel, or the daemon-forward funnel.
#[test]
fn table_answers_are_classified_by_what_may_leave() {
    use touring_hook_runtime::hook_response::HookResponse;

    for silent in ["", "   ", "{}", " {} \n"] {
        assert_eq!(
            classify_table_answer(silent),
            TableAnswer::Silent,
            "{silent:?}"
        );
    }
    // What `run_returning(..).to_json()` produces conforms — these are the answers of the 16
    // names served from the table.
    for response in [
        HookResponse::context_with_event("ctx", "PreToolUse"),
        HookResponse::deny("no"),
        HookResponse::halt("stop"),
        HookResponse::context_with_updated_input("ctx", serde_json::json!({"file_path": "a"})),
    ] {
        let json = response.to_json();
        assert_eq!(
            classify_table_answer(&json),
            TableAnswer::Conforming,
            "{json}"
        );
    }
    // The `task-completed` receipt carries keys the contract does not admit; prose is not JSON;
    // a JSON array is not an object. All three leave through the daemon-forward funnel.
    for odd in [
        r#"{"event":"task-completed","RL":"reward_signal","rl_reward":1.0}"#,
        "a plain-text hint",
        "[1,2]",
    ] {
        assert_eq!(
            classify_table_answer(odd),
            TableAnswer::NeedsContractFunnel,
            "{odd}"
        );
    }
}

/// `touring-hook help` must not advertise a name that answers `unknown subcommand`. It did:
/// `pre-compact` was listed and had no standalone arm. Read from the source of the help text
/// (`print_help` prints straight to stdout), checked against the route.
#[test]
fn help_lists_only_names_that_answer() {
    let main_rs = include_str!("main.rs");
    let start = main_rs
        .find("SUBCOMMANDS:")
        .expect("print_help has a SUBCOMMANDS section");
    let end = main_rs[start..]
        .find("ENVIRONMENT:")
        .map(|e| start + e)
        .expect("print_help has an ENVIRONMENT section");
    let mut checked = 0;
    for line in main_rs[start..end].lines() {
        // A subcommand line is indented by exactly 8 spaces: `        name   description`.
        let Some(rest) = line.strip_prefix("        ") else {
            continue;
        };
        if rest.starts_with(' ') || rest.trim().is_empty() {
            continue;
        }
        let name = rest.split_whitespace().next().unwrap_or_default();
        // Answered before routing, in `main` (no runtime, no table).
        if matches!(name, "help" | "dedup-stdin") {
            continue;
        }
        checked += 1;
        assert!(
            !matches!(route(name), Route::Unknown),
            "`touring-hook help` lists '{name}' but the standalone route does not know it"
        );
    }
    assert!(
        checked > 20,
        "the help parse found only {checked} names — the scan went blind"
    );
}

/// A panicking handler must not become a failure Claude Code shows: `fail_open_on_panic` makes the
/// process exit 0 and say so on stderr — in this unwinding build and in the release one
/// (`panic = "abort"`) alike, because the hook runs before either. The probe re-runs this test
/// binary on this very test: the child panics under the hook, the parent reads how it ended.
#[test]
fn a_panicking_handler_exits_zero_and_says_so_on_stderr() {
    const PROBE: &str = "TOURING_STANDALONE_PANIC_PROBE";
    if std::env::var_os(PROBE).is_some() {
        fail_open_on_panic();
        panic!("probe: a handler blew up");
    }
    let out = std::process::Command::new(std::env::current_exe().expect("test binary path"))
        .args([
            "--exact",
            "standalone::tests::a_panicking_handler_exits_zero_and_says_so_on_stderr",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(PROBE, "1")
        .output()
        .expect("re-run the test binary as the probe");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "a panic under fail_open_on_panic must exit 0 (stderr: {stderr})"
    );
    assert!(
        stderr.contains("a standalone handler panicked") && stderr.contains("probe: a handler"),
        "the panic must be named on stderr: {stderr}"
    );
}

/// The refusal an RPC endpoint gets: it is routed, as itself, and `cli-suggest` — the one hook
/// in that namespace — is not refused. Since the overrides converged (#4, 04/11/2026) it is
/// served by the daemon's own table handler, like every other table hook.
#[test]
fn rpc_endpoints_are_refused_and_the_cli_hook_is_not() {
    assert!(matches!(route("cli-doctor"), Route::RpcEndpoint));
    if cfg!(feature = "pre-hooks") {
        assert!(matches!(route("cli-suggest"), Route::Table));
    }
}
