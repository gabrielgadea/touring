//! The human kill switch (`TOURING_CODE_GATES_DISABLED=1`) as a property of the WHOLE table.
//!
//! The old suite proved the switch on one sequence (an inspection burst); the gates check it
//! at their own entry, so a gate that forgot to ask would pass every one of those tests. Here
//! every row of the table is replayed with the switch on, and no step may speak — except the
//! two telemetry advisories the switch has never governed.

use super::replay::play_isolated;
use super::rows::scenarios;
use super::scenarios::{Cfg, K, observe};

/// Advisories that survive the switch in the reference implementation (measured when the
/// gates were extracted, and kept as they were — a behavior change is not part of a refactor):
/// the G5 telemetry line and the E3 counterfactual-comment note. Both are `additionalContext`
/// only; neither can deny, rewrite or block a call.
const NOT_GOVERNED_BY_THE_SWITCH: &[&str] = &["[G5 telemetria]", "[E3 contrafactual"];

#[test]
#[serial_test::serial(gate_metrics)]
fn the_kill_switch_silences_every_row_of_the_table() {
    let mut speaking = Vec::new();
    let mut survivors = 0usize;
    let mut replayed = 0usize;
    for s in scenarios().into_iter().filter(|s| !s.cfg.kill_switch) {
        let switched = s.clone().cfg(Cfg {
            kill_switch: true,
            ..s.cfg.clone()
        });
        replayed += 1;
        for (i, step) in play_isolated(&switched).steps.iter().enumerate() {
            let Some(out) = &step.out else { continue };
            let seen = observe(&Some(out.clone()));
            let exempt = seen.k == K::Advisory
                && NOT_GOVERNED_BY_THE_SWITCH
                    .iter()
                    .any(|m| seen.text.contains(m));
            if exempt {
                survivors += 1;
            } else {
                let head: String = out.chars().take(90).collect();
                speaking.push(format!("{} step {i}: {head}", s.name));
            }
        }
    }
    assert!(replayed > 100, "the table shrank: only {replayed} rows replayed");
    assert!(
        speaking.is_empty(),
        "the kill switch is on and a gate still spoke: {speaking:#?}"
    );
    assert!(
        survivors > 0,
        "the exemption list is dead weight: no telemetry advisory survived the switch"
    );
}
