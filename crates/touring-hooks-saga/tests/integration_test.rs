//! Integration tests for `touring_hooks_saga`.
//!
//! Run via `cargo test -p touring-hooks-saga`. Exercises the public saga API
//! end-to-end without the `saga` feature (the feature gates the TestAgent helper;
//! the coordinator itself is always available).

use touring_hooks_saga::DistributedSagaCoordinator;

#[test]
fn integration_coordinator_starts_empty() {
    let coord = DistributedSagaCoordinator::new();
    assert!(
        coord.get_agent("nonexistent").is_none(),
        "a fresh coordinator must have no registered agents"
    );
}
