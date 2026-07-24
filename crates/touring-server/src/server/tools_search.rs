//! `touring_search` MCP tool (C3) — intent-ranked tool discovery.
//!
//! A second always-on `#[tool_router]` block (router_search), merged into the
//! server router set in `mod.rs`. Lets an MCP client discover the right Touring
//! tool from a natural-language intent (progressive disclosure) instead of
//! loading every tool schema upfront. Backed by [`crate::tool_catalog`].

use super::*;

#[tool_router(router = router_search, vis = "pub(crate)")]
impl TouringServer {
    #[tool(
        annotations(read_only_hint = true, title = "Discover tools by intent"),
        name = "touring_search",
        description = "Discover the right Touring tool/command for a natural-language intent \
                       (e.g. 'find who calls a function', 'check system health', 'aggregate \
                       across files'). Returns up to top_k ranked tools — each with name, kind, \
                       summary and when_to_use — so you load only the schemas you need instead \
                       of every tool upfront (progressive disclosure)."
    )]
    async fn touring_search(
        &self,
        params: Parameters<crate::server::params::SearchToolsParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let intent = p.intent.trim();
        if intent.is_empty() {
            return Err(McpError::invalid_params(
                "intent must not be empty".to_string(),
                None,
            ));
        }
        let top_k = p.top_k.unwrap_or(8).clamp(1, 50);
        let json = crate::tool_catalog::search_as_json(intent, top_k);
        let text = serde_json::to_string_pretty(&json)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}
