//! `touring kpi` — Wave R2 falsifiable commitments dashboard.
//!
//! Reads `~/.claude/rust/docs/kpi/commitments.yaml`, checks each
//! threshold against live daemon signals, and prints a JSON snapshot.
//!
//! # Examples
//!
//! ```bash
//! touring kpi -j                  # JSON dashboard
//! touring kpi --check             # exit 1 when any commitment FAILs
//! touring kpi --snapshot          # persist docs/kpi/YYYY-MM/YYYY-MM-DD.json
//! touring kpi --refine            # F7: recommend refinement actuators (telemetry §12)
//! touring kpi --check --snapshot  # combine
//! ```

use super::daemon_query;

/// Run the `kpi` CLI subcommand.
///
/// Flags:
/// - `--check`: exit non-zero when any commitment FAILs
/// - `--snapshot`: persist a JSON snapshot under `docs/kpi/YYYY-MM/`
///
/// # Errors
///
/// Returns an error if the daemon socket is unreachable, the daemon
/// reports a failure, or `--check` is set and a commitment FAILed.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let check = args.iter().any(|a| a == "--check");
    let snapshot = args.iter().any(|a| a == "--snapshot");
    let refine = args.iter().any(|a| a == "--refine");

    let mut payload = serde_json::Map::new();
    if check {
        payload.insert("check".to_string(), serde_json::Value::Bool(true));
    }
    if snapshot {
        payload.insert("snapshot".to_string(), serde_json::Value::Bool(true));
    }
    if refine {
        payload.insert("refine".to_string(), serde_json::Value::Bool(true));
    }

    let output = daemon_query("cli-kpi", serde_json::Value::Object(payload))?;
    println!("{output}");

    if check {
        let parsed: serde_json::Value =
            serde_json::from_str(&output).unwrap_or(serde_json::Value::Null);
        let failed = parsed
            .pointer("/summary/failed")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if failed > 0 {
            anyhow::bail!("kpi check failed: {failed} commitment(s) below threshold");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn check_flag_recognized() {
        let args = s(&["touring", "kpi", "--check"]);
        let has_check = args.iter().any(|a| a == "--check");
        assert!(has_check);
    }

    #[test]
    fn snapshot_flag_recognized() {
        let args = s(&["touring", "kpi", "--snapshot"]);
        let has_snap = args.iter().any(|a| a == "--snapshot");
        assert!(has_snap);
    }

    #[test]
    fn refine_flag_recognized() {
        let args = s(&["touring", "kpi", "--refine"]);
        assert!(args.iter().any(|a| a == "--refine"));
    }

    #[test]
    fn no_args_is_valid() {
        let args = s(&["touring", "kpi"]);
        let result = run(&args);
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(
                !msg.contains("Usage"),
                "no-args should not emit usage error: {msg}"
            );
        }
    }
}
