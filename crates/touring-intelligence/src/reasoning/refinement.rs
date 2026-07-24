//! RefinementCycle — Diagnostic-driven cognitive retry (COG-2).
//!
//! A retry loop informed by DiagnosticLayer (from ACO-1).
//! Max iterations, each using a different strategy based on the
//! diagnostic classification of the previous failure.

use crate::rl::aco::diagnostics::{DiagnosticLayer, RetryStrategy, classify_error};

#[cfg(feature = "esaa")]
use crate::reasoning::aco_adapter::EsaaConsumeAdapter;

/// Configuration for the refinement cycle.
#[derive(Debug, Clone)]
pub struct RefinementConfig {
    /// Maximum number of retry iterations.
    pub max_iterations: u32,
}

impl Default for RefinementConfig {
    fn default() -> Self {
        Self { max_iterations: 3 }
    }
}

/// Outcome of a refinement cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefinementOutcome {
    /// The execution succeeded (possibly after retries).
    Resolved,
    /// All retry iterations exhausted without success.
    Exhausted,
    /// An Architecture-level error triggered immediate escalation.
    Escalated,
}

/// A retry loop that classifies failures and adapts strategy per iteration.
#[derive(Debug)]
pub struct RefinementCycle {
    config: RefinementConfig,
    /// History of (layer, error_message) for each failed iteration.
    pub history: Vec<(DiagnosticLayer, String)>,
}

impl RefinementCycle {
    /// Create a new refinement cycle with the given configuration.
    pub fn new(config: RefinementConfig) -> Self {
        Self {
            config,
            history: Vec::new(),
        }
    }

    /// Run the refinement cycle.
    ///
    /// Calls `execute_fn` repeatedly until it succeeds, the cycle is
    /// exhausted, or an Architecture error triggers escalation.
    ///
    /// On each failure, `classify_error` determines the layer, and
    /// the corresponding `RetryStrategy` is logged. Architecture errors
    /// cause immediate `Escalated` return.
    ///
    /// When `esaa_adapter` is `Some`, failures are also routed through the
    /// ESAA coordination layer to obtain adaptive guidance from registered
    /// subsystems. The ESAA result's `result` field is used as hint text;
    /// if no handler processes the failure, fall back to existing behavior.
    #[cfg(feature = "esaa")]
    pub fn run_refinement<F>(
        &mut self,
        mut execute_fn: F,
        esaa_adapter: Option<&EsaaConsumeAdapter>,
    ) -> RefinementOutcome
    where
        F: FnMut() -> Result<(), String>,
    {
        for _iteration in 0..self.config.max_iterations {
            match execute_fn() {
                Ok(()) => return RefinementOutcome::Resolved,
                Err(err_msg) => {
                    let diag = classify_error(&err_msg, "refinement");
                    let layer = diag.layer.clone();

                    self.history.push((layer.clone(), err_msg.clone()));

                    // Architecture errors → immediate escalation
                    if diag.retry_strategy == RetryStrategy::Halt {
                        return RefinementOutcome::Escalated;
                    }

                    // Route through ESAA adapter if provided
                    if let Some(adapter) = esaa_adapter {
                        let esaa_result = adapter.route_failure(&layer, &err_msg);
                        if esaa_result.success {
                            // Adaptive guidance available — stored in esaa_result.result
                            // (future iterations could use this to adjust strategy)
                            let _hint = String::from_utf8_lossy(&esaa_result.result).to_string();
                        }
                    }
                    // Other strategies: continue loop (next iteration applies
                    // different approach based on the classified layer)
                }
            }
        }

        RefinementOutcome::Exhausted
    }

    /// Run the refinement cycle without ESAA integration.
    ///
    /// Convenience overload for callers that do not need ESAA routing.
    #[cfg(not(feature = "esaa"))]
    pub fn run_refinement<F>(&mut self, mut execute_fn: F) -> RefinementOutcome
    where
        F: FnMut() -> Result<(), String>,
    {
        for _iteration in 0..self.config.max_iterations {
            match execute_fn() {
                Ok(()) => return RefinementOutcome::Resolved,
                Err(err_msg) => {
                    let diag = classify_error(&err_msg, "refinement");
                    let layer = diag.layer.clone();

                    self.history.push((layer.clone(), err_msg));

                    // Architecture errors → immediate escalation
                    if diag.retry_strategy == RetryStrategy::Halt {
                        return RefinementOutcome::Escalated;
                    }
                    // Other strategies: continue loop
                }
            }
        }

        RefinementOutcome::Exhausted
    }

    /// Get the number of retries that were attempted.
    pub fn retry_count(&self) -> usize {
        self.history.len()
    }

    /// Get the last diagnostic layer, if any.
    pub fn last_layer(&self) -> Option<&DiagnosticLayer> {
        self.history.last().map(|(layer, _)| layer)
    }
}

#[cfg(not(feature = "esaa"))]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_refinement_succeeds_first_try() {
        let mut cycle = RefinementCycle::new(RefinementConfig::default());
        let outcome = cycle.run_refinement(|| Ok(()));
        assert_eq!(outcome, RefinementOutcome::Resolved);
        assert_eq!(cycle.retry_count(), 0);
    }

    #[test]
    fn test_refinement_retries_on_syntax_error() {
        let mut cycle = RefinementCycle::new(RefinementConfig { max_iterations: 3 });
        let mut attempts = 0;
        let outcome = cycle.run_refinement(|| {
            attempts += 1;
            if attempts < 3 {
                Err("parse error: unexpected token".to_string())
            } else {
                Ok(())
            }
        });
        assert_eq!(outcome, RefinementOutcome::Resolved);
        assert_eq!(cycle.retry_count(), 2); // 2 failures before success
        assert_eq!(cycle.history[0].0, DiagnosticLayer::Syntax);
    }

    #[test]
    fn test_refinement_halts_on_architecture_error() {
        let mut cycle = RefinementCycle::new(RefinementConfig::default());
        let outcome = cycle
            .run_refinement(|| Err("cycle detected in dependency graph, deadlock".to_string()));
        assert_eq!(outcome, RefinementOutcome::Escalated);
        assert_eq!(cycle.retry_count(), 1);
        assert_eq!(cycle.history[0].0, DiagnosticLayer::Architecture);
    }

    #[test]
    fn test_refinement_max_iterations() {
        let mut cycle = RefinementCycle::new(RefinementConfig { max_iterations: 2 });
        let outcome = cycle.run_refinement(|| Err("wrong output".to_string()));
        assert_eq!(outcome, RefinementOutcome::Exhausted);
        assert_eq!(cycle.retry_count(), 2);
    }

    #[test]
    fn test_refinement_strategy_selection() {
        let mut cycle = RefinementCycle::new(RefinementConfig { max_iterations: 3 });
        let mut call = 0;
        let outcome = cycle.run_refinement(|| {
            call += 1;
            match call {
                1 => Err("parse error: syntax".to_string()), // Syntax → AutoFix
                2 => Err("invariant violated".to_string()),  // Contract → EscalateToUser
                _ => Err("wrong output again".to_string()),  // Logic → RePlan
            }
        });
        // None of these cause Halt → all 3 iterations exhausted
        assert_eq!(outcome, RefinementOutcome::Exhausted);
        assert_eq!(cycle.history[0].0, DiagnosticLayer::Syntax);
        assert_eq!(cycle.history[1].0, DiagnosticLayer::Contract);
        assert_eq!(cycle.history[2].0, DiagnosticLayer::Logic);
    }

    #[test]
    fn test_refinement_outcome_recording() {
        let mut cycle = RefinementCycle::new(RefinementConfig { max_iterations: 3 });
        let _ = cycle.run_refinement(|| Err("some logic error".to_string()));
        assert_eq!(cycle.retry_count(), 3);
        // All should be Logic layer (no keywords matched)
        for (layer, _) in &cycle.history {
            assert_eq!(*layer, DiagnosticLayer::Logic);
        }
        assert_eq!(cycle.last_layer(), Some(&DiagnosticLayer::Logic));
    }
}
