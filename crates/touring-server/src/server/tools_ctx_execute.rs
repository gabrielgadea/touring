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
    /// Forbidden calls (fs.write*, subprocess.run, eval, etc.) are detected and returned
    /// in the `forbidden_calls` array. By default they WARN (a `[CEG WARNING]` stderr
    /// banner) and the run proceeds; `TOURING_CEG_FORBIDDEN_ENFORCE=1` upgrades the
    /// policy to Block. Saying "blocked" here while the executor warned was the D8
    /// text-vs-executor drift the analise-a2 field report caught (30/08/2026).
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
            None, // tunables por chamada são superfície do `touring run` (CLI)
        )
        .await
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Cross-audit 04/09/2026 — este adaptador montava o proprio envelope com
        // apenas os 7 campos base, enquanto `format_output` (o serializador que a
        // propria doc dele diz servir "MCP consumers") ficava sem NENHUM chamador.
        // O custo era do consumidor MCP: sem `success`, sem taxonomia de falha, sem
        // `stored_path`/`retrieval_hint` — o localizador do spill nunca chegava a
        // quem precisava dele para ler a saida elidida.
        let mut output = crate::tools::ctx_execute_tools::format_output(Ok(&out));
        let gctx = self.graph_svc.resolve_ctx(None).await;
        self.graph_svc.inject(&mut output, &gctx);
        crate::tools::suggestions::append_to_response(&mut output, "touring_ctx_execute", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}
