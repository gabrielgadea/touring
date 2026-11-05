//! Telemetry the verdict table cannot see: the `gate_metrics` counters a gate bumps next to its
//! decision. They are process-global, so these tests are serial with every other test that
//! reads or drives them (`gate_metrics`), and they assert DELTAS around their own calls.

use crate::code_mode_gates::{CodeModeEnv, GateConfig, GateState, ToolEvent, evaluate};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::atomic::Ordering;
use touring_foundation::gate_metrics::{GateEvent, GateId, global};

fn config(portfolio: &Path) -> GateConfig {
    GateConfig {
        g2_rewrite_disabled: false,
        g1_rewrite_disabled: false,
        portfolio_dir: Some(portfolio.to_path_buf()),
        code_mode_env: CodeModeEnv::default(),
        code_gates_disabled: false,
        g1_precision: Some(None),
        r1_skeleton: Some(None),
    }
}

fn g1_bypassed() -> u64 {
    global().gate_events[GateId::G1 as usize][GateEvent::Bypassed as usize].load(Ordering::Relaxed)
}

fn bash(state: &GateState, root: &Path, session: &str, cmd: &str, cfg: &GateConfig) {
    let input: Value = json!({ "command": cmd });
    let event = ToolEvent {
        project_root: root,
        session,
        tool_name: "Bash",
        tool_input: &input,
    };
    let _ = evaluate(state, &event, cfg);
}

/// A conscious bypass resets the burst window of its class; it is COUNTED as a G1 bypass only
/// when the burst really was one call from denying — a bypass of a harmless first call is not
/// evidence that the gate was in the way.
#[test]
#[serial_test::serial(gate_metrics)]
fn a_bypass_counts_against_g1_only_when_the_burst_was_one_call_from_denying() {
    let root = tempfile::tempdir().expect("project root");
    let port = tempfile::tempdir().expect("portfolio dir");
    let cfg = config(port.path());
    let state = GateState::new();
    let bypass = "TOURING_GATE_OK=1 rg -n 'x' src/x.rs";

    // control: one prior inspection — the burst is two calls from denying
    bash(&state, root.path(), "tel-ctl", "rg -n 'a' src/a.rs", &cfg);
    let before = g1_bypassed();
    bash(&state, root.path(), "tel-ctl", bypass, &cfg);
    assert_eq!(g1_bypassed(), before, "a bypass of a first call is not a G1 bypass");

    // live: two prior inspections — the next one would be rewritten
    bash(&state, root.path(), "tel-live", "rg -n 'a' src/a.rs", &cfg);
    bash(&state, root.path(), "tel-live", "rg -n 'b' src/b.rs", &cfg);
    let before = g1_bypassed();
    bash(&state, root.path(), "tel-live", bypass, &cfg);
    assert_eq!(
        g1_bypassed(),
        before + 1,
        "a bypass of the call the gate would have rewritten is counted once"
    );
}
