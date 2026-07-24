//! Built-in 17-gate implementations (Rust-native).
//!
//! `default_gates()` returns the canonical 17-gate set; the 3 "real" gates
//! (Architecture, Security, Modularization) parse the workspace's
//! deny.toml / cycle graph / file sizes; the remaining 14 are
//! stubs that report `GateStatus::Advisory` with a clear message
//! pointing to the underlying CI step. They become real as touring
//! upstream integrations ship.

use std::path::Path;

use crate::gate::{Gate, GateId, GateOutcome, GateSeverity};

pub mod architecture;
pub mod best_practices;
pub mod ci_cd_devops;
pub mod craftsmanship;
pub mod dependencies;
pub mod documentation;
pub mod extensibility;
pub mod modularization;
pub mod naming;
pub mod navigability;
pub mod performance;
pub mod product_docs;
pub mod scalability;
pub mod security;
pub mod stub;
pub mod testing;
pub mod ux;

/// Canonical 17-gate set. Use this for `run_harness(change, &default_gates(), cfg)`.
#[must_use]
pub fn default_gates() -> Vec<Box<dyn Gate>> {
    vec![
        Box::new(stub::Stub::code_quality()),
        Box::new(architecture::ArchitectureGate),
        Box::new(security::SecurityGate::new()),
        Box::new(performance::PerformanceGate),
        Box::new(testing::TestingGate),
        Box::new(documentation::DocumentationGate),
        Box::new(best_practices::BestPracticesGate),
        Box::new(ci_cd_devops::CiCdDevopsGate),
        Box::new(modularization::ModularizationGate),
        Box::new(scalability::ScalabilityGate),
        Box::new(extensibility::ExtensibilityGate),
        Box::new(naming::NamingGate),
        Box::new(navigability::NavigabilityGate),
        Box::new(craftsmanship::CraftsmanshipGate),
        Box::new(dependencies::DependenciesGate),
        Box::new(ux::UxGate),
        Box::new(product_docs::ProductDocsGate),
    ]
}

/// Re-export of `default_gates()` for ergonomic access as
/// `touring_harness::builtin_default_gates()`.
#[must_use]
pub fn builtin_default_gates() -> Vec<Box<dyn Gate>> {
    default_gates()
}

/// Heuristic cycle count for the given workspace root.
/// Returns 0 if `.fingerprint` directory exists (build has run clean).
/// 1 indicates a likely build error or no recent build.
#[must_use]
pub fn builtin_cycles_pass(ws_root: &Path) -> u32 {
    architecture::count_cycles_heuristic(ws_root)
}

/// Parse `deny.toml` and count advisories. Returns `Some(n)` if the file
/// exists, `None` otherwise.
#[must_use]
pub fn builtin_deny_advisories(ws_root: &Path) -> Option<u64> {
    read_deny_advisories(ws_root)
}

/// Helper for stub gates: build an Advisory outcome with a standard
/// "external CI step" message.
pub fn external_advisory(id: GateId, severity: GateSeverity, msg: &str) -> GateOutcome {
    let mut o = GateOutcome::advisory(id, severity, msg);
    o.status = crate::gate::GateStatus::External;
    o.score = 1.0;
    o
}

/// Read the deny.toml advisories count for the workspace.
pub fn read_deny_advisories(workspace_root: &Path) -> Option<u64> {
    let p = workspace_root.join("deny.toml");
    let text = std::fs::read_to_string(p).ok()?;
    let mut count = 0_u64;
    let mut in_advisories = false;
    for line in text.lines() {
        if line.trim_start().starts_with('[') {
            in_advisories = line.contains("advisories");
            continue;
        }
        if in_advisories && line.trim_start().starts_with("{ id =") {
            count += 1;
        }
    }
    Some(count)
}
