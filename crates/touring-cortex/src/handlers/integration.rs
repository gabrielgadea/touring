//! H83: Integration Completeness Handler
//!
//! Runs on SessionEnd and PostCompact to audit wiring completeness.
//! Produces orphan reports and integration suggestions.
//! Persists orphan patterns as gotchas for future sessions.

use std::collections::HashSet;

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{HandlerResult, HookEvent};

/// H83: Audits integration completeness at session boundaries.
///
/// On SessionEnd/PostCompact, queries the wiring_map for orphan pub symbols
/// (exported but never imported). Produces a summary report as context and
/// persists orphan patterns as gotchas so future sessions get warned.
pub(crate) struct IntegrationCompletenessHandler;

impl Handler for IntegrationCompletenessHandler {
    fn name(&self) -> &str {
        "H83_integration_completeness"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::SessionEnd, HookEvent::PostCompact]
    }

    fn priority(&self) -> u8 {
        200 // Low priority — runs after most handlers
    }

    fn dependency_tier(&self) -> u8 {
        1 // T1: requires knowledge DB
    }

    fn timeout_ms(&self) -> u64 {
        100 // Orphan queries may scan wiring_map
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Query orphan symbols from knowledge DB
        let orphans = match ctx.knowledge.orphan_symbols() {
            Ok(entries) if !entries.is_empty() => entries,
            Ok(_) => return HandlerResult::skip(self.name()),
            Err(_) => return HandlerResult::skip(self.name()),
        };

        // Build orphan report grouped by module
        let mut report_parts = Vec::new();
        let mut seen_modules: HashSet<String> = HashSet::new();

        for entry in &orphans {
            if seen_modules.insert(entry.module_file.clone()) {
                let module_orphans: Vec<&str> = orphans
                    .iter()
                    .filter(|e| e.module_file == entry.module_file)
                    .map(|e| e.symbol_name.as_str())
                    .collect();
                report_parts.push(format!(
                    "{}: {} orphan(s) [{}]",
                    entry.module_file,
                    module_orphans.len(),
                    module_orphans.join(", ")
                ));
            }
        }

        let context_line = format!(
            "wiring audit: {} orphan module(s) \u{2014} {}",
            seen_modules.len(),
            report_parts.join("; ")
        );

        // Persist orphan patterns as gotchas for next session
        for module in &seen_modules {
            let module_orphans: Vec<&str> = orphans
                .iter()
                .filter(|e| &e.module_file == module)
                .map(|e| e.symbol_name.as_str())
                .collect();
            let gotcha_text = format!(
                "Orphan pub symbols: [{}] \u{2014} wire into consumers or reduce visibility",
                module_orphans.join(", ")
            );
            let _ = ctx
                .knowledge
                .add_gotcha(module, &gotcha_text, "warning", None);
        }

        HandlerResult::allow(self.name(), Some(context_line))
    }
}

/// Register integration handlers.
pub fn register(pipeline: &mut Pipeline) {
    pipeline.register(Box::new(IntegrationCompletenessHandler));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handler_name() {
        let h = IntegrationCompletenessHandler;
        assert_eq!(h.name(), "H83_integration_completeness");
    }

    #[test]
    fn test_handler_events() {
        let h = IntegrationCompletenessHandler;
        let events = h.events();
        assert!(events.contains(&HookEvent::SessionEnd));
        assert!(events.contains(&HookEvent::PostCompact));
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_handler_priority_is_low() {
        let h = IntegrationCompletenessHandler;
        assert!(
            h.priority() >= 192,
            "should be low priority (audit handler)"
        );
    }

    #[test]
    fn test_handler_dependency_tier() {
        let h = IntegrationCompletenessHandler;
        assert_eq!(h.dependency_tier(), 1, "requires knowledge DB (T1)");
    }

    #[test]
    fn test_handler_timeout() {
        let h = IntegrationCompletenessHandler;
        assert_eq!(h.timeout_ms(), 100);
    }

    #[test]
    fn test_handler_is_not_critical() {
        let h = IntegrationCompletenessHandler;
        assert!(!h.is_critical(), "audit handler should not be critical");
    }

    #[test]
    fn test_handler_is_not_async() {
        let h = IntegrationCompletenessHandler;
        assert!(!h.is_async());
    }

    #[test]
    fn test_handler_no_cache_invalidation() {
        let h = IntegrationCompletenessHandler;
        assert!(!h.requires_cache_invalidation());
    }
}
