//! Module contract tests for touring-hooks.
//!
//! Ensures that public interfaces between modules remain stable and
//! that changes in shared/ don't break callers.
//!
//! # Usage
//!
//! Run with: `cargo test --package touring-hooks -- module_contracts`

use touring_hooks::circuit_state_machine::{CircuitCheck, CircuitState, OpClass};
use touring_hooks::errors::TouringError;
use touring_hooks::shared::async_runtime::AsyncConfig;

#[test]
fn test_touring_error_from_string() {
    let err: TouringError = "test error".into();
    assert!(err.to_string().contains("Hook error"));
}

#[test]
fn test_touring_error_from_io() {
    use std::io;
    let io_err = io::Error::new(io::ErrorKind::NotFound, "not found");
    let err: TouringError = io_err.into();
    assert!(err.to_string().contains("IO error"));
}

#[test]
fn test_circuit_state_new_is_empty() {
    let state = CircuitState::new();
    let now = 0;
    assert!(!state.is_any_open(now));
}

#[test]
fn test_op_class_from_hook_name() {
    assert_eq!(OpClass::from_hook_name("index-find"), OpClass::Light);
    assert_eq!(OpClass::from_hook_name("session-start"), OpClass::Critical);
    assert_eq!(OpClass::from_hook_name("mcts-search"), OpClass::Heavy);
    assert_eq!(OpClass::from_hook_name("unknown"), OpClass::Medium);
}

#[test]
fn test_async_config_validate() {
    let config = AsyncConfig::default();
    assert!(config.validate().is_ok());

    let bad_config = AsyncConfig {
        tokio_threads: 500,
        rayon_threads: 0,
        track_tasks: true,
    };
    assert!(bad_config.validate().is_err());
}

#[test]
fn test_circuit_check_proceed() {
    let check = CircuitCheck::proceed(OpClass::Medium);
    assert!(!check.should_skip());
    assert_eq!(check.retry_after_secs, 0);
}

#[test]
fn test_circuit_check_skip() {
    let check = CircuitCheck::skip("global", "global", OpClass::Heavy, 30);
    assert!(check.should_skip());
    assert_eq!(check.retry_after_secs, 30);
}

#[test]
fn test_error_context_chaining() {
    let err = TouringError::knowledge("read failed")
        .context()
        .with_context("loading config")
        .build();
    let msg = err.to_string();
    assert!(msg.contains("read failed"));
    assert!(msg.contains("loading config"));
}

#[test]
fn test_touring_error_display() {
    let err = TouringError::Wiring("orphan detected".to_string());
    let display = format!("{}", err);
    assert!(display.contains("Wiring error"));
}

#[test]
fn test_touring_error_knowledge() {
    let err = TouringError::knowledge("file not found");
    assert!(err.to_string().contains("Knowledge error"));
}

#[test]
fn test_touring_error_aco() {
    let err = TouringError::aco("pheromone overflow");
    assert!(err.to_string().contains("ACO error"));
}

#[test]
fn test_circuit_state_is_global_open() {
    let state = CircuitState::new();
    assert!(!state.is_global_open(0));
}

#[test]
fn test_circuit_state_total_weighted_score() {
    let state = CircuitState::new();
    assert_eq!(state.total_weighted_score(), 0.0);
}
