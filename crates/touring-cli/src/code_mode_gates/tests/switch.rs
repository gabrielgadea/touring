//! The human kill switch (`TOURING_CODE_GATES_DISABLED=1`) as a property of the WHOLE table.
//!
//! The old suite proved the switch on one sequence (an inspection burst); the gates check it
//! at their own entry, so a gate that forgot to ask would pass every one of those tests. Here
//! every row of the table is replayed with the switch on, and no step may speak — advisories
//! included. Until 04/11/2026 the G5 telemetry line and the E3 counterfactual note survived the
//! switch (an exemption kept as measured when the gates were extracted); Gabriel decided the
//! human switch silences everything (aprofundamento #6). Their counters keep observing.

use super::replay::play_isolated;
use super::rows::scenarios;
use super::scenarios::{Cfg, observe};

/// Rows where G5 or E3 speak with the switch off — so the silence below is not vacuous for them.
fn rows_where(marker: &str) -> usize {
    scenarios()
        .into_iter()
        .filter(|s| !s.cfg.kill_switch)
        .filter(|s| {
            play_isolated(s).steps.iter().any(|step| {
                step.out
                    .as_ref()
                    .is_some_and(|o| observe(&Some(o.clone())).text.contains(marker))
            })
        })
        .count()
}

#[test]
#[serial_test::serial(gate_metrics)]
fn the_kill_switch_silences_every_row_of_the_table() {
    let mut speaking = Vec::new();
    let mut replayed = 0usize;
    for s in scenarios().into_iter().filter(|s| !s.cfg.kill_switch) {
        let switched = s.clone().cfg(Cfg {
            kill_switch: true,
            ..s.cfg.clone()
        });
        replayed += 1;
        for (i, step) in play_isolated(&switched).steps.iter().enumerate() {
            let Some(out) = &step.out else { continue };
            let head: String = out.chars().take(90).collect();
            speaking.push(format!("{} step {i}: {head}", s.name));
        }
    }
    assert!(replayed > 100, "the table shrank: only {replayed} rows replayed");
    assert!(
        speaking.is_empty(),
        "the kill switch is on and a gate still spoke: {speaking:#?}"
    );
}

/// The two advisories the switch now governs do speak with it off, in at least one row each —
/// otherwise the test above would prove nothing about them.
#[test]
#[serial_test::serial(gate_metrics)]
fn g5_and_e3_speak_with_the_switch_off() {
    assert!(rows_where("[G5 telemetria]") > 0, "no row exercises G5");
    assert!(rows_where("[E3 contrafactual") > 0, "no row exercises E3");
}
