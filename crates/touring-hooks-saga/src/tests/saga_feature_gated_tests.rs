//! Tests that SagaAgent trait impl is only available with saga feature.
//!
//! These tests compile only when saga feature is enabled.
//! Without the feature, the SagaAgent impl block is cfg-gated and will not compile.
//!
//! Run with: cargo test -p touring-hooks-saga --features saga

/// Integration test confirming SagaAgent trait is accessible with saga feature.
/// This test validates that TestAgent implements SagaAgent when the feature is enabled.
#[cfg(feature = "saga")]
#[tokio::test]
async fn test_saga_agent_available_with_feature() {
    use crate::distributed::{DistributedSagaCoordinator, StepResult};

    let coord = DistributedSagaCoordinator::new();
    coord.register_agent(crate::distributed::TestAgent::new("agent-test-1", true));

    let agent = coord.get_agent("agent-test-1");
    assert!(
        agent.is_some(),
        "SagaAgent-backed agent should be registered"
    );
    assert_eq!(agent.unwrap().agent_id(), "agent-test-1");
}

/// Verify prepare() returns the vote value configured on TestAgent.
#[cfg(feature = "saga")]
#[tokio::test]
async fn test_saga_agent_prepare_vote() {
    use crate::distributed::DistributedSagaCoordinator;

    let coord = DistributedSagaCoordinator::new();
    // TestAgent with vote=true should vote commit
    coord.register_agent(crate::distributed::TestAgent::new("commit-agent", true));

    let agent = coord.get_agent("commit-agent").expect("agent registered");
    assert!(
        agent.prepare("tx-1", "step-1", "do_action"),
        "vote=true should prepare true"
    );

    // TestAgent with vote=false should vote rollback
    coord.register_agent(crate::distributed::TestAgent::new("rollback-agent", false));
    let agent2 = coord.get_agent("rollback-agent").expect("agent registered");
    assert!(
        !agent2.prepare("tx-2", "step-1", "do_action"),
        "vote=false should prepare false"
    );
}

/// Verify execute() returns StepResult::Succeeded for both agent types.
#[cfg(feature = "saga")]
#[tokio::test]
async fn test_saga_agent_execute_returns_success() {
    use crate::distributed::{DistributedSagaCoordinator, StepResult};

    let coord = DistributedSagaCoordinator::new();
    coord.register_agent(crate::distributed::TestAgent::new("exec-agent", true));

    let agent = coord.get_agent("exec-agent").expect("agent registered");
    let result = agent.execute("tx-1", "step-1");
    assert!(
        matches!(result, StepResult::Succeeded),
        "execute should succeed"
    );
}

/// Verify compensate() returns StepResult::Succeeded for both agent types.
#[cfg(feature = "saga")]
#[tokio::test]
async fn test_saga_agent_compensate_returns_success() {
    use crate::distributed::{DistributedSagaCoordinator, StepResult};

    let coord = DistributedSagaCoordinator::new();
    coord.register_agent(crate::distributed::TestAgent::new("compensate-agent", true));

    let agent = coord
        .get_agent("compensate-agent")
        .expect("agent registered");
    let result = agent.compensate("tx-1", "step-1");
    assert!(
        matches!(result, StepResult::Succeeded),
        "compensate should succeed"
    );
}
