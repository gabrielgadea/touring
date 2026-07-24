//! Pre-Glob Hook — symbol enrichment for `Glob` tool calls (D43, master plan W2).
//!
//! Glob patterns are usually file globs (e.g. `**/*.rs`). When the pattern is
//! actually a bare identifier (e.g. `HookRuntime`), share `pre_grep`'s symbol
//! lookup so users still get the enrichment. All other patterns pass through
//! silently, including the common `**` and `*.ext` cases — `is_symbol_like`
//! rejects them by design (whitespace, slashes, `*`, dots all disqualify).

use crate::runtime::{HookResponse, HookRuntime};

/// Run the pre-glob hook (diverging — used by the `touring-hook pre-glob` CLI path).
#[tracing::instrument(skip(runtime, input), fields(hook = "pre_glob"))]
pub fn run(
    runtime: &HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    run_returning(runtime, input).emit()
}

/// Daemon-callable variant returning [`HookResponse`] instead of diverging.
///
/// Delegates to [`crate::pre_grep::run_returning`] — both Grep and Glob payloads
/// place the pattern under `tool_input.pattern`, so the same algorithm works.
pub fn run_returning(runtime: &HookRuntime, input: &serde_json::Value) -> HookResponse {
    crate::pre_grep::run_returning(runtime, input)
}
