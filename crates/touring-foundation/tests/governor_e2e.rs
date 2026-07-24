//! Integration tests for the resource governor.
//!
//! Tests the governor in a realistic scenario: timeout enforcement,
//! chunk limit enforcement, and RAII guard behavior.

use std::time::Duration;
use touring_foundation::governor::{PerformanceSettings, ResourceGovernor};

/// Helper to create a tight governor for testing.
fn tight_governor() -> ResourceGovernor {
    ResourceGovernor::new(PerformanceSettings {
        timeout: Duration::from_millis(200),
        max_chunks: 3,
        max_memory_mb: None,
    })
}

#[test]
fn test_integration_timeout_enforcement() {
    // Long-running query simulation: should be aborted at timeout.
    let gov = tight_governor();
    let _guard = gov.enter();

    // Before timeout: OK.
    assert!(gov.check_timeout().is_ok());

    // Simulate work that takes longer than timeout.
    std::thread::sleep(Duration::from_millis(300));

    // After timeout: should error.
    let err = gov.check_timeout().expect_err("timeout should have fired");
    assert!(err.elapsed >= Duration::from_millis(300));
    assert_eq!(err.limit, Duration::from_millis(200));
}

#[test]
fn test_integration_chunk_limit_enforcement() {
    // Chunker that would produce 100k+ chunks: should abort at limit.
    let gov = tight_governor();
    let _guard = gov.enter();

    // Register up to the limit.
    for _ in 0..3 {
        assert!(gov.register_chunk().is_ok());
    }

    // At the limit (3): OK.
    assert_eq!(gov.chunk_count(), 3);
    assert_eq!(gov.max_chunks(), 3);

    // One more: exceeds limit.
    let err = gov.register_chunk().expect_err("should exceed limit");
    assert_eq!(err.limit, 3);
    assert_eq!(err.count, 4);
}

#[test]
fn test_integration_guard_drops_on_scope_exit() {
    // RAII: guard cleans up start_time when dropped.
    let gov = tight_governor();

    {
        let _guard = gov.enter();
        assert!(gov.check_timeout().is_ok());
    } // guard drops here

    // start_time should be cleared now.
    let err = gov.check_timeout().expect_err("start_time should be None");
    assert_eq!(err.elapsed, Duration::ZERO);
}

#[test]
fn test_integration_default_governor_settings() {
    // Default governor: 30s timeout, 100k chunks, no memory cap.
    let gov = ResourceGovernor::default();

    let _guard = gov.enter();
    assert!(gov.check_timeout().is_ok()); // 30s is plenty
    assert_eq!(gov.max_chunks(), 100_000);

    // Register a million chunks — only 100k allowed.
    for _ in 0..100_000 {
        assert!(gov.register_chunk().is_ok());
    }
    let err = gov.register_chunk().expect_err("limit is 100k");
    assert_eq!(err.limit, 100_000);
}
