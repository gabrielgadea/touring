//! `touring change-contract` — S-09/R8 formal self-mutation gate.
//!
//! Evaluates a [`ChangeContract`](touring_hooks::gateway::change_contract::ChangeContract):
//! given `--pre` and `--post` `HarnessQuality` JSON snapshots (and optional
//! `--invariants`), reports whether a self-mutation may commit — the
//! no-regression proof. The `Edit tool` gate captures the
//! before/after `touring harness-metric` around an edit and calls this to decide
//! whether the change preserves harness quality.
//!
//! Exit status is `0` whether committed or blocked — the JSON `committed` field
//! carries the decision; callers gate on it. A malformed payload is an error.

use anyhow::{Result, anyhow};

use super::daemon_query;

/// Run the `change-contract` CLI subcommand.
///
/// Accepts `--pre <json>`, `--post <json>`, and optional `--invariants <json>`,
/// forwards them to the daemon `cli-change-contract` handler, and prints the
/// `ContractVerdict` JSON.
///
/// # Errors
///
/// Returns an error on an unknown flag, a missing flag value, invalid JSON, or
/// an unreachable daemon socket.
pub fn run(args: &[String]) -> Result<()> {
    let mut payload = serde_json::Map::new();
    // Convention (matches `health-delta`/`jobs`/`memory`): args[0]=binary,
    // args[1]=subcommand, args[2..]=the actual flags. Start parsing at 2.
    let mut i = 2;
    while i < args.len() {
        let flag = args[i].as_str();
        match flag {
            "--pre" | "--post" | "--invariants" => {
                let key = flag.trim_start_matches("--").to_string();
                let raw = args
                    .get(i + 1)
                    .ok_or_else(|| anyhow!("{flag} requires a JSON value"))?;
                let parsed: serde_json::Value = serde_json::from_str(raw)
                    .map_err(|e| anyhow!("invalid JSON for {flag}: {e}"))?;
                payload.insert(key, parsed);
                i += 2;
            }
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }
    if !payload.contains_key("pre") || !payload.contains_key("post") {
        return Err(anyhow!(
            "change-contract requires --pre and --post HarnessQuality JSON snapshots"
        ));
    }
    let output = daemon_query("cli-change-contract", serde_json::Value::Object(payload))?;
    println!("{output}");
    Ok(())
}
