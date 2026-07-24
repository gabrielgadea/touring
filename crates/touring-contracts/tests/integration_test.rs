//! Integration test: an *external* type implements [`LearnRuntime`], proving the
//! contract trait is usable from outside the crate — the whole point of the seam
//! (a downstream host satisfies the contract; a generic consumer drives it).

use serde_json::{Value, json};
use touring_contracts::LearnRuntime;

/// A downstream host that satisfies the contract by counting dispatched calls.
#[derive(Default)]
struct CountingHost {
    calls: usize,
}

impl LearnRuntime for CountingHost {
    fn learning_reward(&mut self, _payload: &Value) -> String {
        self.calls += 1;
        json!({ "ok": true }).to_string()
    }
    fn gotcha_add(&mut self, _payload: &Value) -> String {
        self.calls += 1;
        json!({ "ok": true }).to_string()
    }
    fn memory_store(&mut self, _payload: &Value) -> String {
        self.calls += 1;
        json!({ "ok": true }).to_string()
    }
}

/// A generic consumer that only knows the trait — mirrors how the CEG's learn
/// stage is generic over the contract (`fn …(rt: &mut impl LearnRuntime)`).
fn run_learn_cycle<R: LearnRuntime>(rt: &mut R) {
    let _ = rt.learning_reward(&json!({ "tool_name": "t", "reward": 1.0 }));
    let _ = rt.gotcha_add(&json!({ "pattern": "p" }));
    let _ = rt.memory_store(&json!({ "key": "k", "value": "v" }));
}

#[test]
fn external_host_satisfies_learn_runtime_contract() {
    let mut host = CountingHost::default();
    run_learn_cycle(&mut host);
    assert_eq!(
        host.calls, 3,
        "all three X9 LEARN ops dispatched through the trait"
    );
}
