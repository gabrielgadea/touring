//! CLI inferlets handlers (`cli_inferlets_*`) — extracted from cli_handlers.rs (A-W2.P3).

use crate::runtime::HookRuntime;

/// List loaded inferlet pools via the `InferletService`.
///
/// Requires `init_inferlets()` to have run at daemon startup. Returns the list
/// of `InferletKind` names as strings, or an error message if the service is
/// not initialized (e.g., built without `inferlets-wasm` feature).
pub fn cli_inferlets_list(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let service = match rt.ctx.inferlet_service.as_ref() {
        Some(s) => s,
        None => {
            return serde_json::json!(
                { "error" : "inferlet service not initialized", "hint" :
                "rebuild with --features inferlets-wasm and restart daemon" }
            )
            .to_string();
        }
    };
    let kinds = service.loaded_kinds_sync();
    let kind_names: Vec<String> = kinds.iter().map(|k| format!("{k:?}")).collect();
    serde_json::json!({ "count" : kind_names.len(), "loaded_inferlets" : kind_names, }).to_string()
}
/// Execute an inferlet by name with the given input string.
///
/// Payload must contain `name` (one of: always_success, memory, pattern,
/// classifier) and optional `input` (defaults to empty string).
pub fn cli_inferlets_exec(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let name = payload
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    let input = payload.get("input").and_then(|v| v.as_str()).unwrap_or("");
    if name.is_empty() {
        return serde_json::json!(
            { "error" : "missing 'name' field", "valid_names" : ["always_success",
            "memory", "pattern", "classifier"] }
        )
        .to_string();
    }
    let kind = match name.as_str() {
        "always_success" => crate::inferlets::InferletKind::AlwaysSuccess,
        "memory" => crate::inferlets::InferletKind::Memory,
        "pattern" => crate::inferlets::InferletKind::Pattern,
        "classifier" => crate::inferlets::InferletKind::Classifier,
        other => {
            return serde_json::json!(
                { "error" : format!("unknown inferlet kind: {other}"), "valid_names" :
                ["always_success", "memory", "pattern", "classifier"] }
            )
            .to_string();
        }
    };
    let service = match rt.ctx.inferlet_service.as_ref() {
        Some(s) => s.clone(),
        None => {
            return serde_json::json!(
                { "error" : "inferlet service not initialized", "hint" :
                "rebuild with --features inferlets-wasm and restart daemon" }
            )
            .to_string();
        }
    };
    // Evaluate on a dedicated thread to avoid blocking the actor Tokio worker.
    // Using Runtime::new() (single-threaded) avoids the "cannot block from within
    // runtime" panic that would occur with block_on on a Tokio worker thread.
    let input_owned = input.to_string();
    let handle = std::thread::Builder::new()
        .name("inferlets-eval".into())
        .spawn(move || {
            let rt = tokio::runtime::Runtime::new()
                .expect("failed to build tokio runtime for inferlet eval");
            rt.block_on(service.evaluate_async(kind, &input_owned))
        })
        .expect("failed to spawn inferlets-eval thread");
    let result = handle.join().expect("inferlets-eval thread panicked");
    match result {
        Ok(plugin_result) => {
            serde_json::json!({ "status" : "ok", "inferlet" : &name, "result" :
            format!("{plugin_result:?}") })
        }
        Err(e) => {
            serde_json::json!({ "status" : "error", "inferlet" : &name, "error" : e })
        }
    }
    .to_string()
}
