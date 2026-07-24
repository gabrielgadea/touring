//! CLI saga handlers (`cli_saga_*`) — extracted from cli_handlers.rs (A-W2.P3).

use crate::runtime::HookRuntime;

/// Registers a saga participant agent with its step count, returning the registration outcome as JSON.
pub fn cli_saga_register(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let agent_id = payload["agent_id"].as_str().unwrap_or_default();
    let step_count = payload["step_count"].as_u64().unwrap_or(0) as u32;
    match rt
        .ctx
        .distributed_saga
        .register(agent_id.to_string(), step_count)
    {
        Ok(()) => serde_json::json!({ "registered" : true, "agent_id" : agent_id }).to_string(),
        Err(e) => serde_json::json!({ "registered" : false, "error" : e.to_string() }).to_string(),
    }
}
/// Runs the prepare phase for a saga step, returning whether the agent votes to commit as JSON.
pub fn cli_saga_prepare(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let agent_id = payload["agent_id"].as_str().unwrap_or_default();
    let tx_id = payload["transaction_id"].as_str().unwrap_or_default();
    let step_id = payload["step_id"].as_str().unwrap_or_default();
    let action = payload["action"].as_str().unwrap_or_default();
    let agent = match rt.ctx.distributed_saga.get_agent(agent_id) {
        Some(a) => a,
        None => {
            return serde_json::json!(
                { "commit" : false, "tx_id" : tx_id, "step_id" : step_id, "error" :
                format!("agent '{}' not registered", agent_id), }
            )
            .to_string();
        }
    };
    let can_commit = agent.prepare(tx_id, step_id, action);
    serde_json::json!({ "commit" : can_commit, "tx_id" : tx_id, "step_id" : step_id, }).to_string()
}
/// Applies a commit-or-compensate decision to a saga step, returning the resulting state as JSON.
pub fn cli_saga_decide(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let agent_id = payload["agent_id"].as_str().unwrap_or_default();
    let tx_id = payload["transaction_id"].as_str().unwrap_or_default();
    let step_id = payload["step_id"].as_str().unwrap_or_default();
    let decision = payload["decision"].as_str().unwrap_or_default();
    let agent = match rt.ctx.distributed_saga.get_agent(agent_id) {
        Some(a) => a,
        None => {
            return serde_json::json!(
                { "tx_id" : tx_id, "step_id" : step_id, "error" :
                format!("agent '{}' not registered", agent_id), }
            )
            .to_string();
        }
    };
    let result = match decision {
        "commit" => agent.execute(tx_id, step_id),
        _ => agent.compensate(tx_id, step_id),
    };
    serde_json::json!(
        { "tx_id" : tx_id, "step_id" : step_id, "result" : format!("{:?}", result), }
    )
    .to_string()
}
/// Stores a base64-encoded compensation delta for a saga step, returning whether it was persisted as JSON.
pub fn cli_saga_delta(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let tx_id = payload["transaction_id"].as_str().unwrap_or_default();
    let step_id = payload["step_id"].as_str().unwrap_or_default();
    let delta_b64 = payload["delta_bytes"].as_str().unwrap_or_default();
    let delta_bytes =
        match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, delta_b64) {
            Ok(bytes) => bytes,
            Err(e) => {
                return serde_json::json!({ "stored" : false, "error" : e.to_string() })
                    .to_string();
            }
        };
    match rt
        .ctx
        .distributed_saga
        .store_delta_sync(tx_id, step_id, delta_bytes)
    {
        Ok(()) => serde_json::json!({ "stored" : true }).to_string(),
        Err(e) => serde_json::json!({ "stored" : false, "error" : e.to_string() }).to_string(),
    }
}
/// Begins a new saga transaction over the given steps, returning the transaction id and commit outcome as JSON.
pub fn cli_saga_begin(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let steps = payload["steps"].as_array().cloned().unwrap_or_default();
    let steps_vec: Vec<(String, String)> = steps
        .iter()
        .filter_map(|s| {
            let step_id = s["step_id"].as_str().unwrap_or_default().to_string();
            let action = s["action"].as_str().unwrap_or_default().to_string();
            if step_id.is_empty() {
                None
            } else {
                Some((step_id, action))
            }
        })
        .collect();
    let handle = match tokio::runtime::Handle::try_current() {
        Ok(h) => h,
        Err(_) => {
            return serde_json::json!(
                { "committed" : false, "error" : "no tokio runtime" }
            )
            .to_string();
        }
    };
    let result = tokio::task::block_in_place(|| {
        handle.block_on(rt.ctx.distributed_saga.begin_transaction(steps_vec))
    });
    match result {
        Ok(tx_id) => serde_json::json!({ "tx_id" : tx_id, "committed" : true }).to_string(),
        Err(e) => serde_json::json!({ "committed" : false, "error" : e.to_string() }).to_string(),
    }
}
/// Reports the current phase of a saga transaction (or lists all transactions when none is given) as JSON.
pub fn cli_saga_status(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let tx_id = payload["transaction_id"].as_str().unwrap_or_default();
    if tx_id.is_empty() {
        serde_json::json!({ "transactions" : Vec::< serde_json::Value >::new() }).to_string()
    } else {
        let phase = rt.ctx.distributed_saga.get_phase(tx_id);
        match phase {
            Some(p) => {
                serde_json::json!({ "tx_id" : tx_id, "phase" : format!("{:?}", p) }).to_string()
            }
            None => serde_json::json!({ "tx_id" : tx_id, "error" : "transaction not found" })
                .to_string(),
        }
    }
}
/// Aborts a saga transaction by rolling back its steps, returning whether the abort succeeded as JSON.
pub fn cli_saga_abort(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let tx_id = payload["transaction_id"].as_str().unwrap_or_default();
    if tx_id.is_empty() {
        return serde_json::json!(
            { "aborted" : false, "error" : "transaction_id required" }
        )
        .to_string();
    }
    let handle = match tokio::runtime::Handle::try_current() {
        Ok(h) => h,
        Err(_) => {
            return serde_json::json!({ "aborted" : false, "error" : "no tokio runtime" })
                .to_string();
        }
    };
    let result = tokio::task::block_in_place(|| {
        handle.block_on(rt.ctx.distributed_saga.handle_decision(tx_id, "rollback"))
    });
    match result {
        Ok(_) => serde_json::json!({ "aborted" : true, "tx_id" : tx_id }).to_string(),
        Err(e) => serde_json::json!({ "aborted" : false, "error" : e.to_string() }).to_string(),
    }
}
