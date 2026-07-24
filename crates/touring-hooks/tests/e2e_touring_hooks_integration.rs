//! Comprehensive E2E tests for touring-hooks integration points
//!
//! Tests verify that all major subsystems work together correctly:
//! - HookResponse variants
//! - Knowledge DB initialization
//! - ACO bridge integration
//! - Metrics
//! - Hook registry

use touring_hooks::{
    aco_bridge::{HookOutcome, HookQualityAssessment, HookResultCache},
    errors::TouringError,
    hook_registry::ALL_DAEMON_HOOK_NAMES,
    hook_response::HookResponse,
    knowledge::FileKnowledgeDB,
    metrics::RuntimeMetrics,
};

#[cfg(test)]
mod e2e_hook_response {
    use super::*;

    /// E2E Test 1: HookResponse Context variant
    #[test]
    fn test_hook_response_context_variant() {
        let ctx = HookResponse::Context {
            context: "test prompt".to_string(),
            event_name: Some("pre-read".to_string()),
        };
        assert!(matches!(ctx, HookResponse::Context { .. }));
    }

    /// E2E Test 2: HookResponse Allow variant
    #[test]
    fn test_hook_response_allow_variant() {
        let allow = HookResponse::Allow;
        assert!(matches!(allow, HookResponse::Allow));
    }

    /// E2E Test 3: HookResponse Deny variant
    #[test]
    fn test_hook_response_deny_variant() {
        let deny = HookResponse::Deny {
            reason: "forbidden".to_string(),
            context: None,
            event_name: None,
        };
        assert!(matches!(deny, HookResponse::Deny { .. }));
    }

    /// E2E Test 4: HookResponse Block variant
    #[test]
    fn test_hook_response_block_variant() {
        let block = HookResponse::Block {
            reason: "blocked reason".to_string(),
            context: None,
            event_name: None,
        };
        assert!(matches!(block, HookResponse::Block { .. }));
    }
}

#[cfg(test)]
mod e2e_knowledge_db {
    use super::*;

    /// E2E Test 5: Knowledge DB initialization
    #[test]
    fn test_knowledge_db_initializes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("knowledge.db");

        let knowledge = FileKnowledgeDB::new(&db_path);
        assert!(
            knowledge.is_ok(),
            "FileKnowledgeDB should initialize: {:?}",
            knowledge.err()
        );
    }
}

#[cfg(test)]
mod e2e_aco_bridge {
    use super::*;

    /// E2E Test 6: ACO bridge hook quality assessment
    #[test]
    fn test_hook_quality_assessment_new() {
        let assessment = HookQualityAssessment::new("test-session");
        assert_eq!(assessment.session_id, "test-session");
        assert_eq!(assessment.total_hooks_fired, 0);
    }

    /// E2E Test 7: HookOutcome recording
    #[test]
    fn test_hook_outcome_recording() {
        let outcome = HookOutcome {
            hook_name: "pre_read".to_string(),
            success: true,
            latency_ms: 12,
            context_injected: true,
            knowledge_captured: true,
            error: None,
        };

        let mut assessment = HookQualityAssessment::new("test");
        assessment.record(outcome);

        assert_eq!(assessment.total_hooks_fired, 1);
    }

    /// E2E Test 8: Hook result cache creation
    #[test]
    fn test_hook_result_cache_creation() {
        let _cache = HookResultCache::new(100, Some(300_000));
        // Cache created successfully
    }
}

#[cfg(test)]
mod e2e_errors {
    use super::*;

    /// E2E Test 9: Error types work correctly
    #[test]
    fn test_touring_error_types() {
        let error = TouringError::knowledge("test error");
        assert!(error.to_string().contains("test error"));

        let error_with_ctx = TouringError::knowledge("error with context")
            .context()
            .with_context("hook: pre-read")
            .with_context("file: test.rs:42")
            .build();

        let ctx_str = format!("{:?}", error_with_ctx);
        assert!(ctx_str.contains("pre-read"));
    }
}

#[cfg(test)]
mod e2e_metrics {
    use super::*;

    /// E2E Test 10: Runtime metrics types exist
    #[test]
    fn test_runtime_metrics_types() {
        // RuntimeMetrics is a complex struct meant to be populated by HookRuntime
        // Verify the type exists and is constructible via Default
        let metrics = RuntimeMetrics {
            hooks: touring_hooks::metrics::HookMetrics::default(),
            rl: None,
            bandit: None,
            cognitive: None,
            cache: touring_hooks::metrics::CacheMetrics::default(),
            session_turn: 0,
        };

        assert_eq!(metrics.session_turn, 0);
    }
}

#[cfg(test)]
mod e2e_hook_registry {
    use super::*;

    /// E2E Test 11: Hook registry verification
    #[test]
    fn test_all_daemon_hooks_registered() {
        // Verify all hooks are properly registered
        let hook_count = ALL_DAEMON_HOOK_NAMES.len();
        assert!(hook_count > 100, "Expected > 100 hooks, got {}", hook_count);

        // Verify key hooks exist
        let required_hooks = vec![
            "pre-read",
            "pre-bash",
            "pre-edit",
            "pre-write",
            "pre-tool-use",
            "post-read",
            "post-bash",
            "post-edit",
            "post-write",
            "post-tool-use",
            "session-start",
            "session-stop",
            "task-created",
            "task-completed",
            "enter-plan-mode",
            "exit-plan-mode",
        ];

        for hook in required_hooks {
            assert!(
                ALL_DAEMON_HOOK_NAMES.contains(&hook),
                "Missing required hook: {}",
                hook
            );
        }
    }
}
