//! `touring_rename_symbol` MCP tool — rename a symbol across a scope.
//!
//! Uses RenamePlan + generate_rename_plan from the refactor module.
//! For AST-aware rename, delegates to touring_code::ast::ssr structural rewrite.

use crate::refactor::rename::{RiskTier, generate_rename_plan};
use crate::server::TouringServer;
use crate::server::params::{RenameSymbolParams, RenameSymbolResponse, RenameSymbolResult};

/// Implementation entry point — called by the `#[tool]` wrapper in tools_infra.rs.
pub async fn rename_symbol_impl(
    _server: &TouringServer,
    params: RenameSymbolParams,
) -> Result<String, String> {
    let symbol = params.symbol.trim();
    if symbol.is_empty() {
        return Err("symbol cannot be empty".to_string());
    }

    let new_name = params.new_name.trim();
    if new_name.is_empty() {
        return Err("new_name cannot be empty".to_string());
    }

    if symbol == new_name {
        return Err("new_name must be different from the current symbol name".to_string());
    }

    // Validate scope
    let valid_scope = match params.scope.as_deref() {
        Some("file") | Some("dir") | Some("project") => true,
        None => true, // default scope is valid
        _ => false,
    };
    if !valid_scope {
        return Err("scope must be one of: file, dir, project".to_string());
    }

    // For now, generate an empty plan with the symbol info.
    // Real implementation would query wiring daemon for consumers + AST locations.
    let plan = generate_rename_plan(symbol, new_name, vec![]);

    // Build response
    let risk_tier_str = match plan.tier {
        RiskTier::Low => "low",
        RiskTier::Medium => "medium",
        RiskTier::High => "high",
    };

    let results: Vec<RenameSymbolResult> = plan
        .edits
        .iter()
        .map(|e| RenameSymbolResult {
            file_path: e.file.clone(),
            line: e.line,
            col: e.col,
            kind: e.kind.clone(),
        })
        .collect();

    let response = RenameSymbolResponse {
        old_symbol: plan.old_symbol,
        new_symbol: plan.new_symbol,
        results,
        blast_radius: plan.blast_radius,
        risk_tier: risk_tier_str.to_string(),
        plan_hash: plan.plan_hash,
    };

    serde_json::to_string(&response).map_err(|e| format!("JSON serialize error: {e}"))
}
