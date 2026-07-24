//! CLI entity-disambiguation handlers.
//!
//! Provides `cli-entity-resolve` — context-aware symbol disambiguation using canonical
//! entity codes from the entity_registry module.

use crate::runtime::HookRuntime;

/// `cli-entity-resolve` — disambiguate a symbol using context-aware resolution.
///
/// # Arguments
/// - `symbol`: Symbol name to disambiguate (e.g., "Index", "Manager", "Handler")
/// - `context_module`: Optional module path hint for context-aware resolution
/// - `crate_filter`: Optional crate name hint (e.g., "touring-index")
/// - `limit`: Maximum candidates to return (default: 10)
///
/// # Example
/// ```ignore
/// touring entity-resolve "Index" --context-module "touring-index"
/// ```
pub fn cli_entity_resolve(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let symbol = payload.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
    let context_module = payload.get("context_module").and_then(|v| v.as_str());
    let crate_filter = payload.get("crate_filter").and_then(|v| v.as_str());
    let limit = payload.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    if symbol.is_empty() {
        return serde_json::json!({
            "error": "symbol is required",
            "example": {
                "symbol": "Index",
                "context_module": "touring-index",
                "limit": 10
            }
        })
        .to_string();
    }

    // P3.3: Entity registry integration — use actual registry from InfraRuntime
    let entity_registry = rt.infra.entity_registry.borrow();
    match entity_registry.as_ref() {
        Some(registry) => {
            match registry.resolve(symbol, context_module, crate_filter, limit) {
                Ok(result) => {
                    let response = serde_json::json!({
                        "symbol": symbol,
                        "is_generic": result.disambiguated_count > 1,
                        "total_candidates": result.total_candidates,
                        "disambiguated_count": result.disambiguated_count,
                        "candidates": result.candidates.iter().map(|c| {
                            serde_json::json!({
                                "entity_code": c.entity_code,
                                "module_path": c.module_path,
                                "file_path": c.file_path,
                                "line": c.line,
                                "confidence": c.confidence
                            })
                        }).collect::<Vec<_>>(),
                        "status": "resolved",
                        "context_hint": context_module.or(crate_filter).map(|s| s.to_string())
                    });
                    return response.to_string();
                }
                Err(e) => {
                    // Fall through to design_only on error
                    tracing::warn!("entity_registry.resolve failed: {}", e);
                }
            }
        }
        None => {
            // Registry not initialized yet — call init and retry
            drop(entity_registry);
            rt.init_entity_registry();
            let entity_registry = rt.infra.entity_registry.borrow();
            if let Some(registry) = entity_registry.as_ref() {
                match registry.resolve(symbol, context_module, crate_filter, limit) {
                    Ok(result) => {
                        let response = serde_json::json!({
                            "symbol": symbol,
                            "is_generic": result.disambiguated_count > 1,
                            "total_candidates": result.total_candidates,
                            "disambiguated_count": result.disambiguated_count,
                            "candidates": result.candidates.iter().map(|c| {
                                serde_json::json!({
                                    "entity_code": c.entity_code,
                                    "module_path": c.module_path,
                                    "file_path": c.file_path,
                                    "line": c.line,
                                    "confidence": c.confidence
                                })
                            }).collect::<Vec<_>>(),
                            "status": "resolved",
                            "context_hint": context_module.or(crate_filter).map(|s| s.to_string())
                        });
                        return response.to_string();
                    }
                    Err(e) => {
                        tracing::warn!("entity_registry.resolve failed after init: {}", e);
                    }
                }
            }
        }
    }

    // P3.3: Fallback to design_only if registry unavailable or error
    serde_json::json!({
        "symbol": symbol,
        "is_generic": true,
        "total_candidates": 0,
        "disambiguated_count": 0,
        "candidates": [],
        "status": "design_only",
        "message": "Entity registry not available. Initialize with touring session-start.",
        "context_hint": context_module.or(crate_filter).map(|s| s.to_string())
    })
    .to_string()
}

/// `cli-entity-list` — list all canonical entity codes.
///
/// # Arguments
/// - `domain`: Optional domain filter (e.g., "crate", "person", "concept")
pub fn cli_entity_list(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let domain = payload.get("domain").and_then(|v| v.as_str());

    serde_json::json!({
        "domain": domain.unwrap_or("all"),
        "entities": [
            {"code": "ALCA", "canonical_name": "Alice Cao", "domain": "person", "primary_module": "touring-cortex"},
            {"code": "JORI", "canonical_name": "Jordan Smith", "domain": "person", "primary_module": "touring-learning"},
            {"code": "GABR", "canonical_name": "Gabriel Gadea", "domain": "person"},
            {"code": "IDX", "canonical_name": "Touring Index", "domain": "crate", "primary_module": "touring-index"},
            {"code": "AST", "canonical_name": "Touring AST", "domain": "crate", "primary_module": "touring-ast"},
            {"code": "HOK", "canonical_name": "Touring Hooks", "domain": "crate", "primary_module": "touring-hooks"},
            {"code": "SVR", "canonical_name": "Touring Server", "domain": "crate", "primary_module": "touring-server"},
            {"code": "CORT", "canonical_name": "Touring Cortex", "domain": "crate", "primary_module": "touring-cortex"},
            {"code": "CORE", "canonical_name": "Touring Core", "domain": "crate", "primary_module": "touring-core"},
            {"code": "LRN", "canonical_name": "Touring Learning", "domain": "crate", "primary_module": "touring-learning"},
            {"code": "SIMD", "canonical_name": "Touring SIMD", "domain": "crate", "primary_module": "touring-simd"}
        ],
        "count": 11,
        "status": "design_only"
    })
    .to_string()
}

/// `cli-entity-generic-patterns` — list generic name patterns needing disambiguation.
pub fn cli_entity_generic_patterns(_rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    serde_json::json!({
        "patterns": [
            {"pattern": "Index", "disambiguation_hint": "touring-index"},
            {"pattern": "Manager", "disambiguation_hint": "internal"},
            {"pattern": "Handler", "disambiguation_hint": "hook"},
            {"pattern": "Loop", "disambiguation_hint": "learning"},
            {"pattern": "Engine", "disambiguation_hint": "simulation"},
            {"pattern": "Bridge", "disambiguation_hint": "aco"},
            {"pattern": "Cache", "disambiguation_hint": "caching"},
            {"pattern": "Pipeline", "disambiguation_hint": "flow"}
        ],
        "count": 8,
        "status": "design_only"
    })
    .to_string()
}
