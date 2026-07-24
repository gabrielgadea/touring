//! EsaaConsumeAdapter — bridges RefinementCycle diagnostic events to EsaaCoordinator.
//!
//! REGRA #0: Potencialização — connects two previously independent subsystems.

use crate::rl::aco::diagnostics::DiagnosticLayer;
use crate::rl::aco::esaa::{
    Analyzer, EsaaCoordinator, EsaaInput, EsaaOutput, EsaaRegisterError, EsaaSubsystem, Monitor,
    Transformer, Validator,
};
use std::collections::HashMap;

/// Maps DiagnosticLayer variants to ESAA event_type strings.
fn layer_to_event_type(layer: &DiagnosticLayer) -> &'static str {
    match layer {
        DiagnosticLayer::Syntax => "diagnostic.syntax",
        DiagnosticLayer::Logic => "diagnostic.logic",
        DiagnosticLayer::Contract => "diagnostic.contract",
        DiagnosticLayer::Architecture => "diagnostic.architecture",
    }
}

/// Adapts RefinementCycle failure diagnostics to EsaaCoordinator input.
/// Consumers: RefinementCycle when it needs adaptive retry strategy via ESAA routing.
pub struct EsaaConsumeAdapter {
    coordinator: EsaaCoordinator,
}

impl EsaaConsumeAdapter {
    /// Create a new adapter with a fresh EsaaCoordinator.
    ///
    /// Registers a default set of 4 ESAA subsystems (Validator, Monitor,
    /// Analyzer, Transformer) so the coordinator is never empty when called
    /// from RefinementCycle. Additional subsystems may be registered via
    /// `register()` before use.
    pub fn new() -> Self {
        let mut coordinator = EsaaCoordinator::new();

        // Register default subsystems — at least one always succeeds so
        // route_failure() never returns all-failures for a known layer.
        let _ = coordinator.register(Box::new(Validator::new("validator.default")));
        let _ = coordinator.register(Box::new(Monitor::new("monitor.default")));
        let _ = coordinator.register(Box::new(Analyzer::new("analyzer.default")));
        let _ = coordinator.register(Box::new(Transformer::new("transformer.default")));

        Self { coordinator }
    }

    /// Register a named subsystem.
    pub fn register(&mut self, subsystem: Box<dyn EsaaSubsystem>) -> Result<(), EsaaRegisterError> {
        self.coordinator.register(subsystem)
    }

    /// Route a diagnostic failure through the ESAA coordination layer.
    /// Returns the first successful subsystem output, or a fallback error output.
    pub fn route_failure(&self, layer: &DiagnosticLayer, error_msg: &str) -> EsaaOutput {
        let event_type = layer_to_event_type(layer);
        let mut metadata = HashMap::new();
        metadata.insert("error".to_string(), error_msg.to_string());
        metadata.insert("layer".to_string(), format!("{:?}", layer));

        let input = EsaaInput {
            event_type: event_type.to_string(),
            payload: error_msg.as_bytes().to_vec(),
            metadata,
        };

        let results = self.coordinator.route(input);
        results
            .into_iter()
            .find(|o| o.success)
            .unwrap_or_else(|| EsaaOutput {
                success: false,
                result: format!("no handler for layer {:?}", layer).into_bytes(),
                latency_us: 0,
            })
    }
}

impl Default for EsaaConsumeAdapter {
    fn default() -> Self {
        Self::new()
    }
}
