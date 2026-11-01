//! S29 (plan 2026-09-25) — a statusMessage never promises what its handler cannot do (D8).
//!
//! `touring-hook teammate-idle` showed "TACO: quality gate before idle..." while its daemon
//! handler returns `String::new()` unconditionally (38/38 live invocations were
//! `daemon-empty` in the 25/09 baseline): the text promised a gate the executor never
//! applies. This guard generalises the check: every handler of the daemon table that is
//! OBSERVATION-ONLY (its closure body ends in `String::new()` with no other output) must not
//! carry a statusMessage that claims to gate, block, validate or enforce.

use std::path::Path;

/// Registry names whose closure can only return the empty string.
fn observation_only_handlers(registry: &str) -> Vec<String> {
    let lines: Vec<&str> = registry.lines().collect();
    let mut names = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        let Some(rest) = t.strip_prefix("m.insert(\"") else {
            continue;
        };
        let Some(name) = rest.split('"').next() else {
            continue;
        };
        // one-line closures (`|rt, v| crate::…(rt, v));`) return the callee's value
        if !t.ends_with('{') {
            continue;
        }
        let indent = line.len() - t.len();
        let close = format!("{}}});", " ".repeat(indent));
        let Some(end) = lines[i + 1..].iter().position(|l| *l == close) else {
            continue;
        };
        let body = lines[i + 1..i + 1 + end].join("\n");
        let silent = body.trim_end().ends_with("String::new()")
            && !body.contains("to_json")
            && !body.contains("return ")
            && !body.contains("serde_json::to_string");
        if silent {
            names.push(name.to_string());
        }
    }
    names
}

/// The handler name of a `touring-hook <handler>` command line.
fn touring_hook_name(cmd: &str) -> Option<&str> {
    cmd.split("touring-hook ").nth(1)?.split_whitespace().next()
}

/// Every hook object of every matcher group of the settings (`hooks` key or the root).
fn hook_objects(settings: &serde_json::Value) -> Vec<&serde_json::Value> {
    let events = settings
        .get("hooks")
        .and_then(|h| h.as_object())
        .or_else(|| settings.as_object());
    events
        .into_iter()
        .flat_map(|e| e.values())
        .filter_map(|groups| groups.as_array())
        .flatten()
        .filter_map(|group| group.get("hooks").and_then(|h| h.as_array()))
        .flatten()
        .collect()
}

/// `(handler, statusMessage)` for every `touring-hook <handler>` command in the template.
fn status_messages(settings: &serde_json::Value) -> Vec<(String, String)> {
    hook_objects(settings)
        .into_iter()
        .filter_map(|hook| {
            let cmd = hook.get("command")?.as_str()?;
            let msg = hook.get("statusMessage")?.as_str()?;
            Some((touring_hook_name(cmd)?.to_string(), msg.to_string()))
        })
        .collect()
}

#[test]
fn teammate_idle_status_message_matches_behavior() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir");
    let registry = std::fs::read_to_string(crates.join("touring-dispatch/src/hook_registry.rs"))
        .expect("registry");
    let silent = observation_only_handlers(&registry);
    // controls: the parser sees the known observation-only handler and not a real gate
    assert!(
        silent.iter().any(|n| n == "teammate-idle"),
        "control lost: {silent:?}"
    );
    assert!(
        !silent.iter().any(|n| n == "teammate-idle-gate"),
        "a gate classified as silent: {silent:?}"
    );

    let template = crates.join("../client/claude/hooks.settings.json");
    let settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&template).expect("hooks template"))
            .expect("json");
    let messages = status_messages(&settings);
    assert!(
        messages.iter().any(|(n, _)| n == "teammate-idle"),
        "control lost: teammate-idle not in the template"
    );
    let promises = ["gate", "block", "validat", "enforc", "bloque", "valida"];
    let lies: Vec<&(String, String)> = messages
        .iter()
        .filter(|(name, msg)| {
            silent.contains(name) && promises.iter().any(|p| msg.to_lowercase().contains(p))
        })
        .collect();
    assert!(
        lies.is_empty(),
        "observation-only handlers whose statusMessage promises a gate (D8): {lies:?}"
    );
}
