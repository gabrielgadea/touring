//! Integration tests — ACO bridge + HookRuntime end-to-end.
//!
//! Validates:
//! - Quality assessment tracks all hook outcomes across a full lifecycle
//! - Result cache improves hit rate on repeated reads
//! - Cache invalidation works on edit events
//! - Final TrackerReport shows PASS status for all-success scenarios
//! - Session lifecycle (start → hooks → stop) works end-to-end

#[cfg(test)]
mod tests {
    use crate::aco_bridge::{HookOutcome, HookQualityAssessment};
    use crate::runtime::HookRuntime;
    use tempfile::TempDir;
    use touring_intelligence::rl::aco::tracker::TrackerStatus;

    /// Helper: create a HookRuntime in a temp directory.
    fn make_runtime() -> (TempDir, HookRuntime) {
        let tmp = TempDir::new().unwrap();
        let rt = HookRuntime::new(tmp.path()).unwrap();
        (tmp, rt)
    }

    /// Helper: build a successful pre-hook outcome.
    fn pre_hook_outcome(name: &str, latency_ms: u64) -> HookOutcome {
        HookOutcome {
            hook_name: name.to_string(),
            success: true,
            latency_ms,
            context_injected: true,
            knowledge_captured: false,
            error: None,
        }
    }

    /// Helper: build a successful post-hook outcome.
    fn post_hook_outcome(name: &str, latency_ms: u64) -> HookOutcome {
        HookOutcome {
            hook_name: name.to_string(),
            success: true,
            latency_ms,
            context_injected: false,
            knowledge_captured: true,
            error: None,
        }
    }

    // ── Test 1: Full lifecycle simulation ─────────────────────────────

    #[test]
    fn test_full_hook_lifecycle_pre_read_post_read_pre_edit_post_edit() {
        let (_tmp, mut rt) = make_runtime();

        // Initialize quality tracking
        rt.reset_quality_tracking("integration-session-1");

        // Step 1: pre_read
        rt.record_hook_outcome(pre_hook_outcome("pre_read", 8));

        // Step 2: post_read
        rt.record_hook_outcome(post_hook_outcome("post_read", 15));

        // Step 3: pre_edit
        rt.record_hook_outcome(pre_hook_outcome("pre_edit", 6));

        // Step 4: post_edit
        rt.record_hook_outcome(post_hook_outcome("post_edit", 20));

        // Verify all 4 outcomes tracked
        let assessment = rt.ctx.quality_assessment.as_ref().unwrap();
        assert_eq!(assessment.total_hooks_fired, 4);
        assert_eq!(assessment.streaming_stats.total(), 4);

        // Verify report has 9 dimensions and PASS status
        let report = rt.quality_report(1).unwrap();
        assert_eq!(report.dims.len(), 9);
        assert_eq!(report.iteration, 1);
        assert_eq!(
            report.status,
            TrackerStatus::Pass,
            "All-success scenario should PASS, composite={}",
            report.composite
        );
        assert!(
            report.composite >= 0.8,
            "Composite should be high for all-pass: {}",
            report.composite
        );
    }

    // ── Test 2: Cache hit rate improves on repeated reads ──────────────

    #[test]
    fn test_cache_hit_rate_improves_on_repeated_reads() {
        let (_tmp, rt) = make_runtime();

        // First read of file — cache miss
        let miss1 = rt.check_cache("pre_read", "src/main.rs");
        assert!(miss1.is_none(), "First read should be a cache miss");

        // Simulate computing and caching the result
        let result = r#"{"symbols":["main","Config"],"count":2}"#.to_string();
        rt.store_cache("pre_read", "src/main.rs", result.clone());

        // Second read of same file — cache hit
        let hit1 = rt.check_cache("pre_read", "src/main.rs");
        assert_eq!(hit1, Some(result.clone()));

        // Third read — still a hit
        let hit2 = rt.check_cache("pre_read", "src/main.rs");
        assert_eq!(hit2, Some(result));

        // Hit rate should be 2/3 (2 hits out of 3 lookups)
        let rate = rt.cache_hit_rate();
        assert!(
            (rate - 2.0 / 3.0).abs() < 0.01,
            "Expected ~66% hit rate, got {:.2}%",
            rate * 100.0
        );
    }

    // ── Test 3: Cache invalidation on edit ─────────────────────────────

    #[test]
    fn test_cache_invalidation_on_edit() {
        let (_tmp, rt) = make_runtime();

        // Cache results for two files
        rt.store_cache("pre_read", "src/lib.rs", r#"{"s":1}"#.into());
        rt.store_cache("pre_read", "src/main.rs", r#"{"s":2}"#.into());
        rt.store_cache("post_read", "src/lib.rs", r#"{"k":true}"#.into());

        // Both files have cached data
        assert!(rt.check_cache("pre_read", "src/lib.rs").is_some());
        assert!(rt.check_cache("pre_read", "src/main.rs").is_some());
        assert!(rt.check_cache("post_read", "src/lib.rs").is_some());

        // Edit lib.rs — invalidate only lib.rs entries
        let invalidated = rt.invalidate_cache_for_file("src/lib.rs");
        assert_eq!(
            invalidated, 2,
            "Should invalidate pre_read + post_read for lib.rs"
        );

        // lib.rs gone, main.rs still cached
        assert!(rt.check_cache("pre_read", "src/lib.rs").is_none());
        assert!(rt.check_cache("post_read", "src/lib.rs").is_none());
        assert!(rt.check_cache("pre_read", "src/main.rs").is_some());
    }

    // ── Test 4: Quality report PASS status for perfect session ─────────

    #[test]
    fn test_quality_report_pass_status_perfect_session() {
        let (_tmp, mut rt) = make_runtime();
        rt.reset_quality_tracking("perfect-session");

        // Simulate a perfect session: 10 hook pairs, all fast, all successful
        for i in 0..10 {
            let file = format!("file_{i}.py");
            rt.record_hook_outcome(HookOutcome {
                hook_name: format!("pre_read_{}", file),
                success: true,
                latency_ms: 5 + (i as u64 % 3),
                context_injected: true,
                knowledge_captured: false,
                error: None,
            });
            rt.record_hook_outcome(HookOutcome {
                hook_name: format!("post_read_{}", file),
                success: true,
                latency_ms: 10 + (i as u64 % 5),
                context_injected: false,
                knowledge_captured: true,
                error: None,
            });
        }

        let report = rt.quality_report(1).unwrap();
        assert_eq!(report.status, TrackerStatus::Pass);
        assert_eq!(report.dims.len(), 9);

        // All dimensions should be high
        for dim in &report.dims {
            assert!(
                dim.score >= 0.5,
                "Dimension {} ({}) has low score {:.2}",
                dim.dim_id,
                dim.name,
                dim.score
            );
        }
    }

    // ── Test 5: Quality tracking with failures ─────────────────────────

    #[test]
    fn test_quality_report_with_failures() {
        let (_tmp, mut rt) = make_runtime();
        rt.reset_quality_tracking("mixed-session");

        // 3 successful hooks
        rt.record_hook_outcome(pre_hook_outcome("pre_read", 5));
        rt.record_hook_outcome(post_hook_outcome("post_read", 10));
        rt.record_hook_outcome(pre_hook_outcome("pre_edit", 7));

        // 1 failed hook
        rt.record_hook_outcome(HookOutcome {
            hook_name: "post_edit".into(),
            success: false,
            latency_ms: 200,
            context_injected: false,
            knowledge_captured: false,
            error: Some("timeout writing to DB".into()),
        });

        let report = rt.quality_report(1).unwrap();
        assert_eq!(report.dims.len(), 9);

        // D6 (Reliability) should be < 1.0 due to failure
        let d6 = report.dims.iter().find(|d| d.dim_id == "D6").unwrap();
        assert!(
            d6.score < 1.0,
            "Reliability should be < 1.0 with a failure, got {}",
            d6.score
        );

        // D1 (Precision) should also reflect the error
        let d1 = report.dims.iter().find(|d| d.dim_id == "D1").unwrap();
        assert!(d1.score < 1.0);
    }

    // ── Test 6: Reset tracking starts fresh ────────────────────────────

    #[test]
    fn test_reset_quality_tracking_clears_previous() {
        let (_tmp, mut rt) = make_runtime();

        // First session
        rt.reset_quality_tracking("session-1");
        rt.record_hook_outcome(pre_hook_outcome("pre_read", 5));
        rt.record_hook_outcome(post_hook_outcome("post_read", 10));

        let _report1 = rt.quality_report(1).unwrap();
        assert_eq!(
            rt.ctx
                .quality_assessment
                .as_ref()
                .unwrap()
                .total_hooks_fired,
            2
        );

        // Reset for new session
        rt.reset_quality_tracking("session-2");
        assert_eq!(
            rt.ctx
                .quality_assessment
                .as_ref()
                .unwrap()
                .total_hooks_fired,
            0,
            "Reset should clear previous outcomes"
        );
        assert_eq!(
            rt.ctx.quality_assessment.as_ref().unwrap().session_id,
            "session-2"
        );

        // Report after reset should be valid but empty
        let report2 = rt.quality_report(2).unwrap();
        assert_eq!(report2.iteration, 2);
        assert_eq!(report2.dims.len(), 9);
    }

    // ── Test 7: Cache + quality work together ──────────────────────────

    #[test]
    fn test_cache_and_quality_integration() {
        let (_tmp, mut rt) = make_runtime();
        rt.reset_quality_tracking("cache-quality-session");

        let file = "src/utils.py";

        // First read: cache miss → compute → cache store → record outcome
        assert!(rt.check_cache("pre_read", file).is_none());
        let computed = r#"{"context":"utils has 5 symbols"}"#.to_string();
        rt.store_cache("pre_read", file, computed.clone());
        rt.record_hook_outcome(pre_hook_outcome("pre_read", 12));

        // post_read after first read
        rt.record_hook_outcome(post_hook_outcome("post_read", 18));

        // Second read: cache hit → skip compute → still record outcome
        let cached = rt.check_cache("pre_read", file);
        assert_eq!(cached, Some(computed));
        rt.record_hook_outcome(HookOutcome {
            hook_name: "pre_read".into(),
            success: true,
            latency_ms: 1, // Much faster due to cache
            context_injected: true,
            knowledge_captured: false,
            error: None,
        });

        // Edit the file → invalidate cache
        rt.invalidate_cache_for_file(file);
        rt.record_hook_outcome(pre_hook_outcome("pre_edit", 8));
        rt.record_hook_outcome(post_hook_outcome("post_edit", 14));

        // After edit, cache should be empty for this file
        assert!(rt.check_cache("pre_read", file).is_none());

        // Quality report should track all 5 outcomes
        let assessment = rt.ctx.quality_assessment.as_ref().unwrap();
        assert_eq!(assessment.total_hooks_fired, 5);

        let report = rt.quality_report(1).unwrap();
        assert_eq!(report.status, TrackerStatus::Pass);
    }

    // ── Test 8: HookQualityAssessment standalone dimensions ────────────

    #[test]
    fn test_standalone_assessment_dimension_coverage() {
        let mut assessment = HookQualityAssessment::new("dim-test");

        // Only pre-hooks, no post-hooks → integration score should be 0.5
        assessment.record(pre_hook_outcome("pre_read", 5));
        assessment.record(pre_hook_outcome("pre_edit", 7));

        let report = assessment.to_tracker_report(1);

        // D7 (Integration) should be 0.5 (has pre but no post)
        let d7 = report.dims.iter().find(|d| d.dim_id == "D7").unwrap();
        assert!(
            (d7.score - 0.5).abs() < f64::EPSILON,
            "Integration should be 0.5 with only pre-hooks, got {}",
            d7.score
        );

        // D4 (Knowledge) should be 1.0 (no post-hooks to evaluate)
        let d4 = report.dims.iter().find(|d| d.dim_id == "D4").unwrap();
        assert!(
            (d4.score - 1.0).abs() < f64::EPSILON,
            "Knowledge should be 1.0 when no post-hooks exist, got {}",
            d4.score
        );
    }

    // ── Test 9: Multiple files in cache ────────────────────────────────

    #[test]
    fn test_cache_multiple_files_independent() {
        let (_tmp, rt) = make_runtime();

        // Cache 3 different files
        rt.store_cache("pre_read", "a.py", "a".into());
        rt.store_cache("pre_read", "b.py", "b".into());
        rt.store_cache("pre_read", "c.py", "c".into());

        // All should be retrievable
        assert_eq!(rt.check_cache("pre_read", "a.py"), Some("a".into()));
        assert_eq!(rt.check_cache("pre_read", "b.py"), Some("b".into()));
        assert_eq!(rt.check_cache("pre_read", "c.py"), Some("c".into()));

        // Invalidate only b.py
        rt.invalidate_cache_for_file("b.py");
        assert_eq!(rt.check_cache("pre_read", "a.py"), Some("a".into()));
        assert!(rt.check_cache("pre_read", "b.py").is_none());
        assert_eq!(rt.check_cache("pre_read", "c.py"), Some("c".into()));
    }

    // ── Test 10: Session stop generates quality in report ──────────────

    #[test]
    fn test_session_stop_includes_quality_data() {
        let (_tmp, mut rt) = make_runtime();
        rt.reset_quality_tracking("stop-test");

        // Simulate some hooks
        rt.record_hook_outcome(pre_hook_outcome("pre_read", 5));
        rt.record_hook_outcome(post_hook_outcome("post_read", 10));
        rt.record_hook_outcome(pre_hook_outcome("pre_edit", 7));
        rt.record_hook_outcome(post_hook_outcome("post_edit", 12));

        // Simulate what session_stop does: generate final report
        let report = rt.quality_report(1).unwrap();
        assert_eq!(report.status, TrackerStatus::Pass);
        assert!(report.composite >= 0.8);

        // Cache hit rate should be 0.0 (nothing was cached in this test)
        assert!((rt.cache_hit_rate() - 0.0).abs() < f64::EPSILON);
    }

    // ── Test 11: Latency tracking accuracy ─────────────────────────────

    #[test]
    fn test_latency_dimension_tracks_slow_hooks() {
        let (_tmp, mut rt) = make_runtime();
        rt.reset_quality_tracking("latency-test");

        // 5 fast hooks
        for _ in 0..5 {
            rt.record_hook_outcome(pre_hook_outcome("pre_read", 10));
        }
        // 1 slow hook (>100ms)
        rt.record_hook_outcome(HookOutcome {
            hook_name: "pre_read".into(),
            success: true,
            latency_ms: 250,
            context_injected: true,
            knowledge_captured: false,
            error: None,
        });

        let report = rt.quality_report(1).unwrap();
        let d3 = report.dims.iter().find(|d| d.dim_id == "D3").unwrap();
        // 5/6 are fast
        assert!(
            (d3.score - 5.0 / 6.0).abs() < 0.01,
            "D3 score should be ~83%, got {:.2}%",
            d3.score * 100.0
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // E2E: NEW MODULES INTEGRATION TESTS
    // Tests for HookResponse, CircuitStateMachine, TouringError, AsyncConfig, ResultExt
    // ═══════════════════════════════════════════════════════════════════════════

    // ── Test 12: HookResponse all 6 variants emit correctly ──────────────────

    #[test]
    fn test_hook_response_all_variants_emit() {
        use crate::hook_response::HookResponse;

        // Test Context variant - context() takes 1 arg
        let ctx = HookResponse::context("test context");
        match ctx {
            HookResponse::Context {
                context,
                event_name,
            } => {
                assert_eq!(context, "test context");
                assert_eq!(event_name, None);
            }
            _ => panic!("Expected Context variant"),
        }

        // Test Context with event - context_with_event() takes 2 args
        let ctx_with_event = HookResponse::context_with_event("test context", "pre_read");
        match ctx_with_event {
            HookResponse::Context {
                context,
                event_name,
            } => {
                assert_eq!(context, "test context");
                assert_eq!(event_name, Some("pre_read".into()));
            }
            _ => panic!("Expected Context variant"),
        }

        // Test Deny variant - deny() takes 1 arg
        let deny = HookResponse::deny("syntax error");
        match deny {
            HookResponse::Deny {
                reason,
                context,
                event_name,
            } => {
                assert_eq!(reason, "syntax error");
                assert_eq!(context, None);
                assert_eq!(event_name, None);
            }
            _ => panic!("Expected Deny variant"),
        }

        // Test Deny with context - deny_with_context() takes 2 args
        let deny_ctx = HookResponse::deny_with_context("syntax error", "fix the syntax");
        match deny_ctx {
            HookResponse::Deny {
                reason,
                context,
                event_name: _,
            } => {
                assert_eq!(reason, "syntax error");
                assert_eq!(context, Some("fix the syntax".into()));
            }
            _ => panic!("Expected Deny variant"),
        }

        // Test Block variant - block() takes 1 arg
        let block = HookResponse::block("4 antipatterns detected");
        match block {
            HookResponse::Block {
                reason,
                context,
                event_name,
            } => {
                assert_eq!(reason, "4 antipatterns detected");
                assert_eq!(context, None);
                assert_eq!(event_name, None);
            }
            _ => panic!("Expected Block variant"),
        }

        // Test Halt variant - halt() takes 1 arg
        let halt = HookResponse::halt("5 consecutive failures");
        match halt {
            HookResponse::Halt { reason } => {
                assert_eq!(reason, "5 consecutive failures");
            }
            _ => panic!("Expected Halt variant"),
        }

        // Test ContextWithUpdatedInput - takes 2 args: context and updated_input
        let updated = HookResponse::context_with_updated_input(
            "normalizing path",
            serde_json::json!({"path": "/absolute/path/to/file.rs"}),
        );
        match updated {
            HookResponse::ContextWithUpdatedInput {
                context,
                event_name,
                updated_input,
            } => {
                assert_eq!(context, "normalizing path");
                assert_eq!(event_name, None);
                assert_eq!(updated_input["path"], "/absolute/path/to/file.rs");
            }
            _ => panic!("Expected ContextWithUpdatedInput variant"),
        }

        // Test Allow variant - allow() takes 0 args
        let allow = HookResponse::allow();
        match allow {
            HookResponse::Allow => {}
            _ => panic!("Expected Allow variant"),
        }
    }

    // ── Test 13: CircuitStateMachine full integration ───────────────────────

    #[test]
    fn test_circuit_state_machine_full_flow() {
        use crate::circuit_state_machine::{CircuitCheck, CircuitState, OpClass};

        let state = CircuitState::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Initial state: all circuits closed
        assert!(
            !state.is_any_open(now),
            "No circuits should be open initially"
        );
        assert!(!state.is_global_open(now), "Global should be closed");
        assert!(
            !state.is_class_open(now),
            "Class should be closed initially"
        );
        assert!(
            !state.is_project_open(now),
            "Project should be closed initially"
        );
        assert!(
            !state.is_session_open(now),
            "Session should be closed initially"
        );

        // total_weighted_score should be 0 initially
        assert_eq!(state.total_weighted_score(), 0.0);

        // CircuitCheck::proceed should allow
        let check = CircuitCheck::proceed(OpClass::Medium);
        assert!(!check.should_skip(), "Medium ops should not be skipped");
        assert_eq!(check.retry_after_secs, 0);

        // CircuitCheck::skip should set skip=true
        let skip_check = CircuitCheck::skip("proj-x", "global", OpClass::Critical, 30);
        assert!(skip_check.should_skip(), "Critical should be skipped");
        assert_eq!(skip_check.retry_after_secs, 30);
        assert_eq!(skip_check.circuit, "global");
        assert_eq!(skip_check.reason, "proj-x");
    }

    // ── Test 14: TouringError context chaining end-to-end ───────────────────

    #[test]
    fn test_touring_error_context_chaining_e2e() {
        use crate::errors::TouringError;

        // Simulate a knowledge DB error cascading through layers
        let err = TouringError::knowledge("file not found")
            .context()
            .with_context("loading project knowledge")
            .with_context("initializing knowledge DB")
            .with_context("startup phase")
            .build();

        let msg = err.to_string();
        assert!(
            msg.contains("file not found"),
            "Should contain original error"
        );
        assert!(
            msg.contains("loading project knowledge"),
            "Should have layer 1 context"
        );
        assert!(
            msg.contains("initializing knowledge DB"),
            "Should have layer 2 context"
        );
        assert!(msg.contains("startup phase"), "Should have layer 3 context");

        // Verify error type is preserved through chaining
        let err2 = TouringError::wiring("orphan symbol detected")
            .context()
            .with_context("wiring audit")
            .build();
        let msg2 = err2.to_string();
        assert!(msg2.contains("Wiring error"), "Type should be preserved");
        assert!(
            msg2.contains("orphan symbol detected"),
            "Should contain original"
        );
    }

    // ── Test 15: AsyncConfig validation integration ─────────────────────────

    #[test]
    fn test_async_config_validation_e2e() {
        use crate::shared::async_runtime::AsyncConfig;

        // Valid config
        let config = AsyncConfig::default();
        assert!(config.validate().is_ok(), "Default config should be valid");

        // Custom valid config
        let config = AsyncConfig {
            tokio_threads: 8,
            rayon_threads: 4,
            track_tasks: true,
        };
        assert!(config.validate().is_ok(), "Normal config should be valid");

        // Invalid: too many Tokio threads
        let bad_tokio = AsyncConfig {
            tokio_threads: 300,
            rayon_threads: 0,
            track_tasks: true,
        };
        assert!(
            bad_tokio.validate().is_err(),
            "Should reject >256 tokio threads"
        );
        assert!(
            bad_tokio
                .validate()
                .unwrap_err()
                .to_string()
                .contains("Tokio")
        );

        // Invalid: too many Rayon threads
        let bad_rayon = AsyncConfig {
            tokio_threads: 0,
            rayon_threads: 200,
            track_tasks: true,
        };
        assert!(
            bad_rayon.validate().is_err(),
            "Should reject >128 rayon threads"
        );
        assert!(
            bad_rayon
                .validate()
                .unwrap_err()
                .to_string()
                .contains("Rayon")
        );
    }

    // ── Test 16: ResultExt and OptionExt in chained operations ──────────────

    #[test]
    fn test_result_option_ext_e2e() {
        use crate::shared::result_ext::{OptionExt, ResultExt};

        // EC56: Updated to use unwrap_or_debug (only method retained after POTENCIALIZAR trim).
        // ResultExt: unwrap_or_debug with Ok
        let result: Result<i32, &str> = Ok(42);
        assert_eq!(result.unwrap_or_debug(0, "should not log"), 42);

        // ResultExt: unwrap_or_debug with Err (returns default)
        let result: Result<i32, &str> = Err("test error");
        assert_eq!(result.unwrap_or_debug(0, "logged error"), 0);

        // OptionExt: unwrap_or_debug with Some
        let option: Option<i32> = Some(100);
        assert_eq!(option.unwrap_or_debug(0, "should not log"), 100);

        // OptionExt: unwrap_or_debug with None (returns default)
        let option: Option<i32> = None;
        assert_eq!(option.unwrap_or_debug(0, "option was none"), 0);

        // Verify ResultExt trait is accessible via internal module use
        let result_ok: Result<i32, &str> = Ok(42);
        let unwrapped = result_ok.unwrap_or_debug(0, "test");
        assert_eq!(unwrapped, 42);

        let result_err: Result<i32, &str> = Err("error");
        let unwrapped_err = result_err.unwrap_or_debug(0, "logged");
        assert_eq!(unwrapped_err, 0); // Returns default
    }

    // ── Test 17: OpClass classification from hook names ─────────────────────

    #[test]
    fn test_opclass_from_hook_name_e2e() {
        use crate::circuit_state_machine::OpClass;

        // Light operations (< 10ms typical) - exact patterns from circuit_state_machine
        assert_eq!(OpClass::from_hook_name("index-find"), OpClass::Light);
        assert_eq!(OpClass::from_hook_name("ast-find"), OpClass::Light);
        assert_eq!(OpClass::from_hook_name("wiring"), OpClass::Light);
        assert_eq!(OpClass::from_hook_name("memory-recall"), OpClass::Light);
        assert_eq!(OpClass::from_hook_name("suggest"), OpClass::Light);

        // Critical operations (session-level)
        assert_eq!(OpClass::from_hook_name("session-start"), OpClass::Critical);
        assert_eq!(OpClass::from_hook_name("daemon-health"), OpClass::Critical);

        // Heavy operations (async/parallel)
        assert_eq!(OpClass::from_hook_name("mcts-search"), OpClass::Heavy);
        assert_eq!(OpClass::from_hook_name("index-rebuild"), OpClass::Heavy);
        assert_eq!(OpClass::from_hook_name("blast"), OpClass::Heavy);

        // Medium operations (default - or unknown hooks)
        assert_eq!(OpClass::from_hook_name("ann-search"), OpClass::Medium);
        assert_eq!(OpClass::from_hook_name("pre-read"), OpClass::Medium);
        assert_eq!(OpClass::from_hook_name("post-edit"), OpClass::Medium);
    }

    // ── Test 18: Global state + class breakers interaction ──────────────────

    #[test]
    fn test_global_and_class_breaker_interaction() {
        use crate::circuit_state_machine::{CircuitState, OpClass};

        let state = CircuitState::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // CircuitState is initialized with no failures, so all circuits closed
        assert!(
            !state.is_class_open(now),
            "Class should be closed initially"
        );
        assert!(
            !state.is_global_open(now),
            "Global should be closed initially"
        );
        assert!(
            !state.is_project_open(now),
            "Project should be closed initially"
        );

        // Verify OpClass thresholds (from circuit_state_machine.rs)
        assert_eq!(
            OpClass::Critical.threshold(),
            10,
            "Critical threshold should be 10"
        );
        assert_eq!(OpClass::Heavy.threshold(), 8, "Heavy threshold should be 8");
        assert_eq!(
            OpClass::Medium.threshold(),
            6,
            "Medium threshold should be 6"
        );
        assert_eq!(
            OpClass::Light.threshold(),
            10,
            "Light threshold should be 10"
        );
    }

    // ── Test 19: Error From implementations ─────────────────────────────────

    #[test]
    fn test_error_from_implementations_e2e() {
        use crate::errors::TouringError;
        use std::io;

        // From io::Error
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file missing");
        let touring_err: TouringError = io_err.into();
        assert!(touring_err.to_string().contains("IO error"));
        assert!(touring_err.to_string().contains("file missing"));

        // From String
        let touring_err: TouringError = "direct string error".into();
        assert!(touring_err.to_string().contains("Hook error"));

        // From &str
        let touring_err: TouringError = "str error".into();
        assert!(touring_err.to_string().contains("Hook error"));

        // TouringError::knowledge shorthand
        let err = TouringError::knowledge("db connection failed");
        assert!(err.to_string().contains("Knowledge error"));

        // TouringError::aco shorthand
        let err = TouringError::aco("pheromone overflow");
        assert!(err.to_string().contains("ACO error"));
    }

    // ── Test 20: CircuitCheck skip vs proceed behavior ─────────────────────

    #[test]
    fn test_circuit_check_skip_vs_proceed() {
        use crate::circuit_state_machine::{CircuitCheck, OpClass};

        // Proceed: default for all operations when circuit is healthy
        let check = CircuitCheck::proceed(OpClass::Light);
        assert!(!check.should_skip(), "Should not skip light op");
        assert_eq!(check.retry_after_secs, 0);
        assert_eq!(check.circuit, "none"); // proceed sets circuit to "none"

        // Skip with specific retry delay
        let check = CircuitCheck::skip("proj-1", "sess-1", OpClass::Critical, 45);
        assert!(check.should_skip(), "Should skip critical");
        assert_eq!(check.retry_after_secs, 45);
        assert!(check.reason.contains("proj-1"));
        assert_eq!(check.circuit, "sess-1");

        // Skip preserves class information
        let check = CircuitCheck::skip("proj-x", "sess-2", OpClass::Heavy, 60);
        assert!(check.should_skip());
        // retry_after_secs is from the skip call, not derived from OpClass
        assert_eq!(check.retry_after_secs, 60);
    }

    // ── Test 21: End-to-end error handling flow ─────────────────────────────

    #[test]
    fn test_end_to_end_error_handling_flow() {
        use crate::errors::TouringError;

        // Simulate: Option content → parse JSON → upsert knowledge
        fn simulate_file_processing(content: Option<&str>) -> Result<usize, TouringError> {
            // Step 1: Read file (might fail)
            let content = content.ok_or_else(|| TouringError::io("empty content"))?;

            // Step 2: Parse JSON (might fail)
            let _parsed: serde_json::Value =
                serde_json::from_str(content).map_err(|e| TouringError::json(e.to_string()))?;

            // Step 3: Process records
            Ok(42) // 42 records processed
        }

        // Happy path
        let json = r#"{"key": "value"}"#;
        let result = simulate_file_processing(Some(json));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);

        // Error path: None content
        let result = simulate_file_processing(None);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("IO error") || err_msg.contains("empty content"));

        // Error path: Invalid JSON
        let result = simulate_file_processing(Some("not json"));
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("JSON error"));
    }

    // ── Test 22: AsyncRuntime task tracking ─────────────────────────────────

    #[test]
    fn test_async_runtime_task_tracking() {
        use crate::shared::async_runtime::{AsyncConfig, AsyncRuntimeCheck, TokioRuntime};

        // Record spawn/complete cycle
        let initial = TokioRuntime::active_tasks();
        TokioRuntime::record_spawn();
        assert_eq!(TokioRuntime::active_tasks(), initial + 1);
        TokioRuntime::record_complete();
        assert_eq!(TokioRuntime::active_tasks(), initial);

        // Verify no leaked tasks at clean state
        let result = crate::shared::async_runtime::assert_no_leaked_tasks();
        assert!(result.is_ok(), "Should have no leaked tasks");

        // AsyncConfig validation
        let config = AsyncConfig::default();
        assert!(config.validate().is_ok());
    }
}
