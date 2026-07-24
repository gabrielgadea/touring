//! Always-success inferlet — for testing and bootstrapping.

/// Raw evaluate — always returns success (1).
pub(crate) fn evaluate_raw(_input: &str) -> i32 {
    1
}
