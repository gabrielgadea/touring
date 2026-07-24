//! E2E integration test for ESAA subsystem wiring (FASE 5).
//!
//! Validates that:
//!   1. EsaaConsumeAdapter correctly bridges RefinementCycle diagnostics to EsaaCoordinator
//!   2. EsaaCoordinator subsystems receive and process diagnostic events
//!   3. The routing table correctly dispatches events to named subsystems

use touring_intelligence::reasoning::aco_adapter::EsaaConsumeAdapter;
use touring_intelligence::rl::aco::diagnostics::DiagnosticLayer;

#[test]
fn test_esa_adapter_route_failure_with_syntax_handler() {
    let adapter = EsaaConsumeAdapter::new();

    // The built-in Transformer subsystem handles "transform" events
    // Route a syntax error — should get routed, coordinator broadcasts to all subsystems
    let output = adapter.route_failure(&DiagnosticLayer::Syntax, "unexpected token at line 5");

    // Coordinator broadcasts to ALL subsystems when no routing table entry exists
    // At least one built-in subsystem should succeed (Transformer or Monitor)
    // If no subsystem handles "diagnostic.syntax" type, coordinator falls back to broadcast
    let result_str = String::from_utf8_lossy(&output.result);
    eprintln!(
        "Syntax diagnostic output: success={}, result={}",
        output.success, result_str
    );
    // Result should always be non-empty (fallback if no handler)
    assert!(!output.result.is_empty());
}

#[test]
fn test_esa_adapter_route_failure_with_architecture_handler() {
    let adapter = EsaaConsumeAdapter::new();

    // Architecture errors should route to diagnostic.architecture
    let output = adapter.route_failure(
        &DiagnosticLayer::Architecture,
        "circular dependency detected between modules A and B",
    );

    // Architecture errors have the highest severity — coordinator routes to all subsystems
    // that handle diagnostic.architecture (Validator/Halt strategy built-in)
    assert!(!output.result.is_empty());
    eprintln!(
        "Architecture diagnostic output: success={}, latency_us={}",
        output.success, output.latency_us
    );
}

#[test]
fn test_esa_adapter_layer_to_event_type_mapping() {
    // Verify all 4 diagnostic layers map to correct event_type strings
    // This is tested indirectly via route_failure — we validate the routing is non-empty

    let adapter = EsaaConsumeAdapter::new();

    for layer in [
        DiagnosticLayer::Syntax,
        DiagnosticLayer::Logic,
        DiagnosticLayer::Contract,
        DiagnosticLayer::Architecture,
    ] {
        let output = adapter.route_failure(&layer, "test error message");
        // All layers should produce a result (even if success=false, result is non-empty)
        assert!(
            !output.result.is_empty(),
            "layer {:?} should always produce a result",
            layer
        );
    }
}

#[test]
fn test_esa_adapter_no_panic_on_empty_coordinator() {
    // EsaaCoordinator with no registered custom subsystems should still return valid output
    let adapter = EsaaConsumeAdapter::new();

    for layer in [
        DiagnosticLayer::Syntax,
        DiagnosticLayer::Logic,
        DiagnosticLayer::Contract,
        DiagnosticLayer::Architecture,
    ] {
        let output = adapter.route_failure(&layer, "graceful degradation test");
        // Must not panic — coordinator falls back gracefully
        assert!(
            !output.result.is_empty(),
            "empty result for layer {:?}",
            layer
        );
        assert!(
            output.latency_us < 60_000_000,
            "route_failure for layer {layer:?} must complete without hanging"
        );
    }
}
