//! Tool Implementations — 94 MCP tools across 16 modules.
//!
//! Each module contains the business logic for a group of related tools.
//! The `#[tool]` annotations live in `server/mod.rs` inside the `#[tool_router]`
//! block, which delegates to functions in these modules.
//!
//! ## Architecture (v10 modular)
//!
//! ```text
//! server/
//!   mod.rs       — TouringServer struct + 33 #[tool] methods + ServerHandler
//!   params.rs    — 33 parameter structs (JsonSchema for rmcp auto-schema)
//! tools/
//!   ast_tools    — touring_ast_overview, touring_ast_find, touring_ast_edit
//!   memory_tools — touring_memory_store, touring_memory_recall, touring_scan_pii
//!   file_tools   — touring_file_ops, touring_checkpoint, touring_index_status
//!   project_tools — touring_project, touring_graph, touring_decompose
//!   utility_tools — touring_classify_intent, touring_suggest, touring_session
//!   drift        — touring_evolution_drift
//!   wiring_audit — touring_wiring, touring_wiring_audit, touring_gotcha
//!   cluster_tools — touring_memory_clusters (list/stats/members/similar, gated by async-memory feature)
//!   context_tools — touring_minimal_context (token-efficient entry point)
//!   suggestions  — next-tool suggestion middleware (inline in every response)
//!   risk_scoring — touring_detect_changes (risk-scored impact analysis)
//! cortex/
//!   metrics.rs   — Per-handler execution metrics + aggregation
//!   pipeline.rs  — Handler pipeline with metrics instrumentation
//! ```
//!
//! ## Note on removed modules
//!
//! `hook_tools.rs` and `learning_tools.rs` were removed — their functionality
//! was superseded by cortex handlers (rmcp 1.2).

pub mod ast_tools;
pub mod clone_tools;
pub mod cluster_tools;
pub mod context_tools;
pub mod ctx_execute_tools;
pub mod diff_parser;
pub mod drift;
pub mod file_tools;
pub mod generator_tools;
pub(crate) mod generator_tools_introspect;
pub mod hybrid_search;
pub mod memory_tools;
pub mod project_tools;
pub mod refactor_preview;
pub mod refactor_tools;
pub mod risk_scoring;
pub mod search_tools;
pub mod session_hints;
pub mod suggestions;
pub mod summary_cache;
pub mod utility_tools;
pub mod wiring_audit;

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// Read and concatenate all server tool files (mod.rs + tools_*.rs submodules).
    /// Used by tests that verify tool registrations spread across submodules.
    fn read_all_server_tool_files(server_dir: &Path) -> String {
        let files = [
            "mod.rs",
            "tools_core.rs",
            "tools_analysis.rs",
            "tools_infra.rs",
            "tools_generator.rs",
            "tools_metadata.rs",
            "tools_infra_ext.rs",
            "tools_analysis_evolution.rs",
        ];
        files
            .iter()
            .map(|f| {
                let p = server_dir.join(f);
                std::fs::read_to_string(&p).unwrap_or_else(|_| String::new())
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// All 5 tool module source files must exist on disk.
    #[test]
    fn tool_module_files_exist() {
        let tool_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tools");
        let modules = [
            "ast_tools.rs",
            "context_tools.rs",
            "diff_parser.rs",
            "generator_tools.rs",
            "hybrid_search.rs",
            "refactor_preview.rs",
            "risk_scoring.rs",
            "session_hints.rs",
            "suggestions.rs",
            "summary_cache.rs",
            "memory_tools.rs",
            "file_tools.rs",
            "project_tools.rs",
            "utility_tools.rs",
            "drift.rs",
            "wiring_audit.rs",
        ];
        for module in &modules {
            assert!(
                tool_dir.join(module).exists(),
                "{module} missing from {tool_dir}",
                tool_dir = tool_dir.display(),
            );
        }
    }

    /// Removed modules must NOT exist on disk (prevent resurrection).
    #[test]
    fn removed_modules_absent() {
        let tool_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tools");
        let removed = ["hook_tools.rs", "learning_tools.rs"];
        for module in &removed {
            assert!(
                !tool_dir.join(module).exists(),
                "{module} should have been removed (superseded by cortex handlers)"
            );
        }
    }

    /// All server tool files combined must contain exactly 83 `#[tool(` annotations.
    /// Tools are split across server/mod.rs and server/tools_*.rs submodules.
    /// If this number changes, update the doc comment in this file.
    /// 46 = 42 (v31) + 4 (v32: jobs spawn/poll/list/drop).
    /// 66 = 46 (v32) + 20 (v33: generator tools).
    /// 69 = 66 (v33) + 3 (PLN2: schema_registry_info + bundle + schema_check already wired).
    /// 70 = 69 + 1 (EA6: touring_wiring_suggest — completes CLI→handler→registry→MCP chain).
    /// 71 = 70 + 1 (EB3: touring_generator_registry_status — PlanRegistry snapshot via MCP).
    /// 80 = 71 + 9 (Pln2 P9: metadata MCP tools in tools_metadata.rs).
    /// 81 = 80 + 1 (Wave cross-audit: touring_generator_consumer_wiring).
    /// 83 = 81 + 2 (S-4/S-5/S-6: touring_profile_heap_dump + touring_profile_flamegraph).
    /// 84 = 83 + 1 (W16: touring_health_delta_status — health_delta MCP sync, reset already counted above).
    /// 85 = 84 + 1 (pre-existing delta before A.2.5 skip CLI).
    /// 86 = 85 + 1 (Wave B B.1: touring_ssr_apply MCP tool).
    /// 87 = 86 + 1 (Wave B B.5.8: touring_source_change MCP tool).
    /// 91 = 87 + 4 (D.3.5: touring_fix_apply + A.2.5 skip + profile_query + W15 health_delta MCP).
    /// 92 = 91 + 1 (D7: touring_rename_symbol MCP tool).
    /// 93 = 92 + 1 (D9: touring_detect_clones MCP tool).
    /// 94 = 93 + 1 (D9: touring_rename_symbol MCP tool).
    /// 95 = 94 + 2 (D9: touring_detect_clones + update existing count).
    /// 97 = 95 + 2 (MCP tools total: was 94, now 96 after D9 added + 1 error in prior count).
    #[test]
    fn server_has_42_tools() {
        let server_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/server");
        // Scan mod.rs and all tools_*.rs submodules
        let files: Vec<_> = [
            "mod.rs",
            "tools_core.rs",
            "tools_analysis.rs",
            "tools_infra.rs",
            "tools_generator.rs",
            "tools_metadata.rs",
            "tools_infra_ext.rs",
            "tools_analysis_evolution.rs",
        ]
        .iter()
        .map(|f| server_dir.join(f))
        .collect();
        let tool_count: usize = files
            .iter()
            .map(|p| {
                std::fs::read_to_string(p)
                    .unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
                    .matches("#[tool(")
                    .count()
            })
            .sum();
        // 103 = 102 + 1 doc-reference to `#[tool(...)]` added in the
        // tool_router merge comment block at mod.rs (FIX 2026-05-23 for
        // gotcha:mcp-tools-empty-tool-router-split-impl). The literal text
        // appears in a comment, not an actual macro invocation.
        //
        // NOTE: this textual count is INTENTIONALLY brittle as a "did anything
        // change" signal — the actual runtime registration is now guarded by
        // `every_tools_submodule_has_tool_router_macro` and
        // `server_mod_merges_every_sub_router` below, which catch the deeper
        // regression where #[tool(...)] exist but #[tool_router] does not.
        assert_eq!(
            tool_count, 103,
            "Expected 103 #[tool()] annotations across server tool files, found {tool_count}. \
             Update the doc comment in tools/mod.rs if tools were added or removed."
        );
    }

    /// server/params.rs must exist and contain parameter structs.
    #[test]
    fn params_module_exists() {
        let params_rs = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/server/params.rs");
        assert!(
            params_rs.exists(),
            "server/params.rs missing — parameter structs must be in params module"
        );
        let content = std::fs::read_to_string(&params_rs)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", params_rs.display()));
        // At least 20 param structs expected
        let struct_count = content.matches("pub(crate) struct").count();
        assert!(
            struct_count >= 20,
            "Expected ≥20 param structs in params.rs, found {struct_count}"
        );
    }

    /// Each tool module must expose at least one `pub` item (proving delegation).
    #[test]
    fn tool_modules_have_public_api() {
        let tool_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tools");
        let modules = [
            "ast_tools",
            "memory_tools",
            "file_tools",
            "project_tools",
            "utility_tools",
        ];
        for module in &modules {
            let path = tool_dir.join(format!("{module}.rs"));
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}.rs: {e}", module));
            assert!(
                content.contains("pub "),
                "{module}.rs has no public items — logic delegation is broken"
            );
        }
    }

    /// Every `tools_*.rs` submodule that holds `#[tool(...)]` annotations
    /// MUST also carry a `#[tool_router(router = ..., vis = "pub(crate)")]`
    /// decorator on its `impl TouringServer { ... }` block. Without that
    /// decorator, the rmcp macro never sees the tools and they are silently
    /// dropped from `tools/list` at runtime — the exact regression captured
    /// by `gotcha:mcp-tools-empty-tool-router-split-impl:2026-05-23`.
    ///
    /// This is a STRUCTURAL guard: it catches the bug at `cargo test` time,
    /// before any deployment, and is cheap (~ms).
    #[test]
    fn every_tools_submodule_has_tool_router_macro() {
        let server_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/server");
        let mut offenders = Vec::new();
        for entry in std::fs::read_dir(&server_dir)
            .unwrap_or_else(|e| panic!("cannot list {}: {e}", server_dir.display()))
        {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !name.starts_with("tools_") || !name.ends_with(".rs") {
                continue;
            }
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {name}: {e}"));
            let tool_count = content.matches("#[tool(").count();
            if tool_count == 0 {
                continue;
            }
            let has_router = content.contains("#[tool_router(router =");
            if !has_router {
                offenders.push(format!(
                    "{name} has {tool_count} #[tool(...)] but NO #[tool_router(router = ..., vis = \"pub(crate)\")] decorator on impl TouringServer — the tools will be invisible to MCP runtime"
                ));
            }
        }
        assert!(
            offenders.is_empty(),
            "rmcp tool_router macro missing on submodule(s):\n  - {}",
            offenders.join("\n  - ")
        );
    }

    /// `server/mod.rs::TouringServer::new` MUST merge every sibling sub-router
    /// into the master ToolRouter. If a new `tools_<foo>.rs` is added with its
    /// own `#[tool_router(router = router_foo, ...)]` but `mod.rs` does not
    /// call `tr.merge(Self::router_foo())`, that foo's tools never reach the
    /// runtime registry. This guard scans both files and asserts a 1:1 mapping.
    #[test]
    fn server_mod_merges_every_sub_router() {
        let server_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/server");
        let mod_rs = std::fs::read_to_string(server_dir.join("mod.rs"))
            .unwrap_or_else(|e| panic!("cannot read server/mod.rs: {e}"));
        let mut declared_routers: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(&server_dir).expect("read_dir") {
            let entry = entry.expect("entry");
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !name.starts_with("tools_") || !name.ends_with(".rs") {
                continue;
            }
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            // Extract router name from `#[tool_router(router = router_X, ...`
            if let Some(idx) = content.find("#[tool_router(router = ") {
                let after = &content[idx + "#[tool_router(router = ".len()..];
                let router_name: String = after
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !router_name.is_empty() {
                    declared_routers.push(router_name);
                }
            }
        }
        let mut missing = Vec::new();
        for router in &declared_routers {
            let merge_call = format!("Self::{router}()");
            if !mod_rs.contains(&merge_call) {
                missing.push(format!(
                    "tools_*.rs declares {router} but mod.rs does NOT call tr.merge({merge_call})"
                ));
            }
        }
        assert!(
            missing.is_empty(),
            "server/mod.rs TouringServer::new is missing tr.merge() for {} sub-router(s):\n  - {}",
            missing.len(),
            missing.join("\n  - ")
        );
    }

    /// touring-hooks Cargo.toml must NOT list touring-server as a dependency.
    /// This ensures the hooks crate remains independently compilable.
    #[test]
    fn hooks_crate_independent_of_server() {
        let hooks_cargo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("touring-hooks/Cargo.toml");
        let content = std::fs::read_to_string(&hooks_cargo)
            .unwrap_or_else(|e| panic!("cannot read touring-hooks/Cargo.toml: {e}"));
        assert!(
            !content.contains("touring-server"),
            "touring-hooks must NOT depend on touring-server (circular dependency risk)"
        );
    }

    /// touring_wiring tool must be registered in a server tool file with all 3 actions documented.
    #[test]
    fn wiring_tool_registered() {
        let server_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/server");
        // Tools may live in mod.rs or any tools_*.rs submodule
        let all_content = read_all_server_tool_files(&server_dir);
        assert!(
            all_content.contains("touring_wiring"),
            "touring_wiring tool must be registered in a server tool file"
        );
        // Verify all 3 documented actions are handled
        for action in &["\"status\"", "\"orphans\"", "\"modules\""] {
            assert!(
                all_content.contains(action),
                "touring_wiring must handle action {action}"
            );
        }
    }

    /// touring_gotcha tool must be registered in a server tool file with list/stats actions.
    #[test]
    fn gotcha_tool_registered() {
        let server_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/server");
        let all_content = read_all_server_tool_files(&server_dir);
        assert!(
            all_content.contains("touring_gotcha"),
            "touring_gotcha tool must be registered in a server tool file"
        );
        // Verify WiringParams and GotchaParams structs exist in params.rs
        let params_rs = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/server/params.rs");
        let params_content = std::fs::read_to_string(&params_rs)
            .unwrap_or_else(|e| panic!("cannot read server/params.rs: {e}"));
        assert!(
            params_content.contains("WiringParams"),
            "WiringParams struct must exist in params.rs"
        );
        assert!(
            params_content.contains("GotchaParams"),
            "GotchaParams struct must exist in params.rs"
        );
    }
}
