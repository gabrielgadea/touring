//! S13 guard (25/09/2026): no entry of the DAEMON dispatch table may reach a function that
//! calls `process::exit` — it would kill the shared daemon for every session.
//!
//! Why a source scan and not a behavioural test: a `process::exit(0)` inside a test ends
//! the test binary with status 0, which the harness can read as success — the failure mode
//! hides itself. So the guard reads the table's source and refuses the known emitting
//! entry points, with a positive control proving the scan sees a real offending closure.

const TABLE: &str = include_str!("../src/hook_registry.rs");

/// Calls that emit (and therefore exit) — legal only in one-shot binaries.
const EMITTING: &[&str] = &[
    ".emit()",
    "emit_context_for_event(",
    "emit_allow(",
    "session_hooks::run_session_start(",
    "post_bash::run(",
    "post_write::run(",
];

/// Bodies of every `m.insert("<name>", |rt, v| …)` closure, with the entry name.
fn closure_bodies(src: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(start) = rest.find("m.insert(\"") {
        let after = &rest[start + 10..];
        let name_end = after.find('"').unwrap_or(0);
        let name = after[..name_end].to_string();
        let body_start = start + 10 + name_end;
        let mut depth = 0i32;
        let mut end = rest.len();
        for (i, ch) in rest[body_start..].char_indices() {
            match ch {
                '(' => depth += 1,
                ')' if depth == 0 => {
                    end = body_start + i;
                    break;
                }
                ')' => depth -= 1,
                _ => {}
            }
        }
        out.push((name, rest[body_start..end].to_string()));
        rest = &rest[end..];
    }
    out
}

fn offenders(src: &str) -> Vec<String> {
    closure_bodies(src)
        .into_iter()
        .filter(|(_, body)| {
            let code: String = body
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect();
            EMITTING.iter().any(|call| code.contains(call))
        })
        .map(|(name, _)| name)
        .collect()
}

#[test]
fn daemon_table_never_calls_an_emitting_handler() {
    let bad = offenders(TABLE);
    assert!(
        bad.is_empty(),
        "daemon dispatch entries that would process::exit: {bad:?}"
    );
}

#[test]
fn positive_control_scan_sees_an_emitting_closure() {
    let fake = r#"m.insert("session-start", |rt, v| {
            let _ = crate::session_hooks::run_session_start(rt, v);
            String::new()
        });
        m.insert("ok", |rt, v| crate::x::run_returning(rt, v).to_json());"#;
    assert_eq!(offenders(fake), vec!["session-start".to_string()]);
}

#[test]
fn scan_covers_the_whole_table() {
    // Guard against a parser that silently sees nothing (absence is not zero).
    assert!(
        closure_bodies(TABLE).len() > 50,
        "scanned only {} entries",
        closure_bodies(TABLE).len()
    );
}
