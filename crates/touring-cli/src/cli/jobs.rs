//! CLI async-jobs handlers (`cli_jobs_*`) — extracted from cli_handlers.rs (A-W2.P3).

use crate::runtime::HookRuntime;

/// Spawn a background worker executing a program with arguments.
///
/// Payload: `{"tool_name": "...", "program": "...", "args": ["..."]}`. Returns
/// the generated `job_id` which the caller uses with `cli_jobs_poll` later.
///
/// Uses `execve` (no shell) so arguments are passed literally — safe from
/// command injection even with untrusted argument strings.
pub fn cli_jobs_spawn(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let tool_name = payload
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("anon");
    let program = payload
        .get("program")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let args: Vec<String> = payload
        .get("args")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|a| a.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    if program.is_empty() {
        return serde_json::json!({ "error" : "missing 'program' field" }).to_string();
    }
    if tokio::runtime::Handle::try_current().is_err() {
        return serde_json::json!({ "error" : "no tokio runtime available for spawn" }).to_string();
    }
    let job_id = crate::shared::job_registry::spawn_worker(tool_name, program, &args);
    serde_json::json!(
        { "job_id" : job_id, "status" : "spawned", "tool_name" : tool_name, "program" :
        program, }
    )
    .to_string()
}
/// Poll a spawned worker by job_id.
///
/// Payload: `{"job_id": "..."}`. Returns the job status JSON from
/// `job_registry::poll_worker`.
pub fn cli_jobs_poll(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let job_id = payload.get("job_id").and_then(|v| v.as_str()).unwrap_or("");
    if job_id.is_empty() {
        return serde_json::json!({ "error" : "missing 'job_id' field" }).to_string();
    }
    crate::shared::job_registry::poll_worker(job_id).to_string()
}
/// List all jobs in the registry with their current status.
pub fn cli_jobs_list(_rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    crate::shared::job_registry::list_jobs().to_string()
}
/// Drop a job from the registry. If the job is still running, its `JoinHandle`
/// is aborted. Useful for explicit cleanup after polling a terminal state.
///
/// Payload: `{"job_id": "..."}`. Returns `{"dropped": bool, "job_id": "..."}`.
/// `dropped=true` means the job was found and removed; `dropped=false` means
/// the job_id did not exist.
pub fn cli_jobs_drop(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let job_id = payload.get("job_id").and_then(|v| v.as_str()).unwrap_or("");
    if job_id.is_empty() {
        return serde_json::json!({ "error" : "missing 'job_id' field" }).to_string();
    }
    let dropped = crate::shared::job_registry::drop_job(job_id);
    serde_json::json!({ "dropped" : dropped, "job_id" : job_id, }).to_string()
}
