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

/// Every handler a hook command can run: each `touring-hook <handler>` in it, with the program
/// path bare or double-quoted. The parser used to read only the FIRST bare occurrence, so the
/// registered TeammateIdle command (an `if`/`else` that execs `teammate-idle` under the kill
/// switch and `teammate-idle-gate` otherwise, with `"$HOME/.claude/hooks/touring-hook" <handler>`
/// quoted) yielded no handler at all, and the control below lost the very hook it guards.
fn touring_hook_names(cmd: &str) -> Vec<String> {
    cmd.match_indices("touring-hook")
        .filter_map(|(at, program)| {
            let rest = &cmd[at + program.len()..];
            // `touring-hook-foo.sh` is another program; only the program itself is followed by
            // an optional closing quote and then a space.
            let rest = rest.strip_prefix('"').unwrap_or(rest);
            let rest = rest.strip_prefix(' ')?;
            let name = rest.split_whitespace().next()?;
            Some(name.trim_matches(|c| c == ';' || c == '"').to_string())
        })
        .collect()
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

/// `(handlers the command can run, statusMessage)` for every `touring-hook <handler>` command in
/// the template.
fn status_messages(settings: &serde_json::Value) -> Vec<(Vec<String>, String)> {
    hook_objects(settings)
        .into_iter()
        .filter_map(|hook| {
            let cmd = hook.get("command")?.as_str()?;
            let msg = hook.get("statusMessage")?.as_str()?;
            let names = touring_hook_names(cmd);
            (!names.is_empty()).then(|| (names, msg.to_string()))
        })
        .collect()
}

/// The messages of `messages` that promise a gate the handlers they run cannot keep.
///
/// A message that promises a gate is a lie when the handler it runs is observation-only. A command
/// that can run SEVERAL handlers (an `if` on a kill switch) is judged on all of them: when every
/// one is silent the promise is a lie, and when only some are, the promise is true only on the
/// other path, so the message has to SAY so — it must name the kill switch.
fn lying_messages<'a>(
    messages: &'a [(Vec<String>, String)],
    silent: &[String],
) -> Vec<&'a (Vec<String>, String)> {
    let promises = ["gate", "block", "validat", "enforc", "bloque", "valida"];
    messages
        .iter()
        .filter(|(names, msg)| {
            let lower = msg.to_lowercase();
            if !promises.iter().any(|p| lower.contains(p)) {
                return false;
            }
            let silent_names = names.iter().filter(|n| silent.contains(*n)).count();
            silent_names == names.len() || (silent_names > 0 && !lower.contains("kill switch"))
        })
        .collect()
}

/// The rule keeps its teeth on the shapes the template can take. Without this, adapting the
/// parser to the conditional command could have quietly stopped catching anything.
#[test]
fn the_rule_still_flags_a_promise_no_handler_keeps() {
    let silent = vec!["teammate-idle".to_string()];
    let judge = |command: &str, message: &str| {
        let settings = serde_json::json!({"hooks": {"TeammateIdle": [{"hooks": [
            {"type": "command", "command": command, "statusMessage": message}
        ]}]}});
        let messages = status_messages(&settings);
        let lies = lying_messages(&messages, &silent).len();
        (messages.len(), lies)
    };
    let bare = "$HOME/.claude/hooks/touring-hook teammate-idle";
    let quoted = "\"$HOME/.claude/hooks/touring-hook\" teammate-idle";
    let either = "if [ \"${OFF:-}\" = 1 ]; then exec \"$HOME/.claude/hooks/touring-hook\" teammate-idle; else exec \"$HOME/.claude/hooks/touring-hook\" teammate-idle-gate; fi";

    // every handler silent: a promise is a lie, bare or quoted program path
    assert_eq!(judge(bare, "TACO: quality gate before idle"), (1, 1));
    assert_eq!(judge(quoted, "TACO: quality gate before idle"), (1, 1));
    // an observation message is honest for a silent handler
    assert_eq!(
        judge(bare, "TACO: noting idle teammate (observation)"),
        (1, 0)
    );
    // a conditional command is a lie when its message hides the silent branch...
    assert_eq!(judge(either, "TACO: teammate idle gate"), (1, 1));
    // ...and honest when it names the switch that turns the gate off
    assert_eq!(
        judge(either, "TACO: teammate idle gate (kill switch OFF=1)"),
        (1, 0)
    );
    // a real gate may say so; a script that merely has the program name as a prefix is no handler
    assert_eq!(
        judge(
            "$HOME/.claude/hooks/touring-hook teammate-idle-gate",
            "TACO: gate"
        ),
        (1, 0)
    );
    assert_eq!(
        judge("$HOME/.claude/hooks/touring-hook-guard.sh", "TACO: gate"),
        (0, 0)
    );
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
        messages
            .iter()
            .any(|(names, _)| names.iter().any(|n| n == "teammate-idle")),
        "control lost: teammate-idle not in the template"
    );
    // The registered command is conditional: both branches must be seen, or the parser lost the
    // gate again and the rule below would judge the silent branch alone.
    assert!(
        messages.iter().any(|(names, _)| {
            names.iter().any(|n| n == "teammate-idle")
                && names.iter().any(|n| n == "teammate-idle-gate")
        }),
        "control lost: the teammate-idle hook no longer parses to both of its handlers"
    );
    let lies = lying_messages(&messages, &silent);
    assert!(
        lies.is_empty(),
        "observation-only handlers whose statusMessage promises a gate (D8): {lies:?}"
    );
}
