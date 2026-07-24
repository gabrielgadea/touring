//! Saga feature-gated integration tests.
//!
//! These tests validate that the saga feature correctly gates the SagaAgent
//! trait implementation. They only compile when the `saga` feature is enabled.

pub mod saga_feature_gated_tests;
