//! D2.4 — MCP tool: touring_ctx_execute sandboxed multi-language execution.
//!
//! Wraps `ctx_execute_tools::ctx_execute_impl` as an rmcp #[tool] method.

use super::*;
use crate::server::params::CtxExecuteParams;
use crate::tools::ctx_execute_tools::ctx_execute_impl;

#[tool_router(router = router_ctx_execute, vis = "pub(crate)")]
impl TouringServer {
    /// Execute arbitrary code in a sandboxed environment (JS, Python, TS, Shell, etc.).
    ///
    /// Use for: counting, filtering, aggregating, transforming data — anything that would
    /// be more efficient to compute than to mentally process from context.
    ///
    /// Supports: js/node, python, ts/bun, ruby, go, rust, perl, r, elixir, php, shell/bash, sh.
    ///
    /// Forbidden calls are blocked (fs.write*, subprocess.run, eval, etc.) and returned
    /// in `forbidden_calls` array.
    ///
    /// Output is truncated at 1MB. Timeout defaults to 30s, max 120s.
    #[tool(
        annotations(read_only_hint = false, title = "Run code in sandbox"),
        name = "touring_ctx_execute",
        description = "Execute sandboxed code (js/node/bun, python, ts, ruby, go, rust, perl, r, elixir, php, bash/sh) — the 200× compression alternative to multi-MCP-call workflows (see code_mode_recipes.md). Forbidden primitives (fs.write*, subprocess.run, eval) blocked. 1MB output cap; max 120s timeout."
    )]
    async fn ctx_execute(
        &self,
        params: Parameters<CtxExecuteParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let out = ctx_execute_impl(
            p.language,
            p.code,
            p.args,
            p.timeout_ms,
            p.cwd,
            p.allow_forbidden,
        )
        .await
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let mut output = serde_json::json!({
            "stdout": out.stdout,
            "stderr": out.stderr,
            "exit_code": out.exit_code,
            "duration_ms": out.duration_ms,
            "forbidden_calls": out.forbidden_calls,
            "stdout_truncated": out.stdout_truncated,
            "stderr_truncated": out.stderr_truncated,
        });
        let gctx = self.graph_svc.resolve_ctx(None).await;
        self.graph_svc.inject(&mut output, &gctx);
        crate::tools::suggestions::append_to_response(&mut output, "touring_ctx_execute", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}
