#!/usr/bin/env bash
# AUTO-GENERATED — W4.8 migration
# Generated: 2026-05-12T00:17:06.851837+00:00
set -euo pipefail

echo 'Phase 1: Rewrite use statements...'

touring ast grep 'crates/touring-analysis/src/lib.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-analysis/src/pipeline.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-analysis/src/blast_radius/hnsw.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-analysis/src/blast_radius/bfs.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-analysis/src/blast_radius/mod.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-analysis/src/quality/rust_semantic.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-assists/src/handlers/format_rust_preserve.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-cognitive/src/predictive_focus_cache.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-cortex/src/handlers/neural.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-cortex/src/handlers/incremental_indexing.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-generator/src/executor/typestate.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-generator/src/core/context.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/knowledge_symbol_bridge.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/post_edit.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/pre_read.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/post_write.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/pre_edit_prevention.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/cli_handlers_index.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/pre_write.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/callgraph_enrichment.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/hook_runtime.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/precomputed_signals.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/cli_handlers_decompose.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/health_delta.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/cli_handlers.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/post_read.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/pre_edit.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/qa_syntax.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/post_edit_rule_engine.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/ast_bridge.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/dependency_cache.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/shadow_v2.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/idempotency.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/wave5_workflow.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/cli_handlers_semantics.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/shared/signal_pipeline.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/shared/cascade_queue.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/shared/reindex.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/shared/api_cascade_bridge.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/shared/cursor_pool.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/shared/signals.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/shared/parser_cache.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/runtime/traits.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-hooks/src/runtime/impls_symbols.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-index/src/similarity.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-index/src/watcher.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-index/src/incremental.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-learning/src/bandit/ast_features.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-python/src/rust_semantic_bindings.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-python/src/ast_rl_bridge.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-python/src/ast_bindings.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-semantics/src/lib.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-semantics/src/source_to_def.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-semantics/src/semantics.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-semantics/src/multi_lang.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-server/src/graph_service.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-server/src/server/tools_core.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-server/src/server/tools_infra.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-server/src/server/tools_analysis.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-server/src/server/mod.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-server/src/tools/ast_tools.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-server/src/tools/refactor_tools.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-server/src/tools/clone_tools.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-server/src/output/json.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-server/src/output/mod.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-server/src/output/toon.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-server/src/cli/ast.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-server/src/cli/ssr.rs' 'touring_ast' --rewrite 'touring_code::ast' --lang rust || true
touring ast grep 'crates/touring-ast/src/ssr/mod.rs' 'touring_ast_polyglot' --rewrite 'touring_code::polyglot' --lang rust || true
touring ast grep 'crates/touring-generator/src/validate/polyglot.rs' 'touring_ast_polyglot' --rewrite 'touring_code::polyglot' --lang rust || true
touring ast grep 'crates/touring-hooks/src/cli_handlers_polyglot.rs' 'touring_ast_polyglot' --rewrite 'touring_code::polyglot' --lang rust || true
touring ast grep 'crates/touring-hooks/src/shared/ast_grep_signal.rs' 'touring_ast_polyglot' --rewrite 'touring_code::polyglot' --lang rust || true
touring ast grep 'crates/touring-hooks/src/shared/risk_patterns.rs' 'touring_ast_polyglot' --rewrite 'touring_code::polyglot' --lang rust || true
touring ast grep 'crates/touring-server/src/server/tools_metadata.rs' 'touring_ast_polyglot' --rewrite 'touring_code::polyglot' --lang rust || true
touring ast grep 'crates/touring-server/src/cli/language.rs' 'touring_language' --rewrite 'touring_code::language' --lang rust || true
touring ast grep 'crates/touring-hooks/src/cli_handlers_semantics.rs' 'touring_semantics' --rewrite 'touring_code::semantics' --lang rust || true
touring ast grep 'crates/touring-index/src/lib.rs' 'touring_semantics' --rewrite 'touring_code::semantics' --lang rust || true

echo 'Phase 2: Update Cargo.toml dependencies...'

sed -i 's/touring-ast *=/touring-code =/g' crates/touring-analysis/Cargo.toml 2>/dev/null || true
sed -i 's/touring-ast *=/touring-code =/g' crates/touring-assists/Cargo.toml 2>/dev/null || true
sed -i 's/touring-ast *=/touring-code =/g' crates/touring-cognitive/Cargo.toml 2>/dev/null || true
sed -i 's/touring-ast *=/touring-code =/g' crates/touring-cortex/Cargo.toml 2>/dev/null || true
sed -i 's/touring-ast *=/touring-code =/g' crates/touring-generator/Cargo.toml 2>/dev/null || true
sed -i 's/touring-ast *=/touring-code =/g' crates/touring-hooks/Cargo.toml 2>/dev/null || true
sed -i 's/touring-ast *=/touring-code =/g' crates/touring-index/Cargo.toml 2>/dev/null || true
sed -i 's/touring-ast *=/touring-code =/g' crates/touring-learning/Cargo.toml 2>/dev/null || true
sed -i 's/touring-ast *=/touring-code =/g' crates/touring-python/Cargo.toml 2>/dev/null || true
sed -i 's/touring-ast *=/touring-code =/g' crates/touring-semantics/Cargo.toml 2>/dev/null || true
sed -i 's/touring-ast *=/touring-code =/g' crates/touring-server/Cargo.toml 2>/dev/null || true
sed -i 's/touring-ast-polyglot *=/touring-code =/g' crates/touring-ast/Cargo.toml 2>/dev/null || true
sed -i 's/touring-ast-polyglot *=/touring-code =/g' crates/touring-generator/Cargo.toml 2>/dev/null || true
sed -i 's/touring-ast-polyglot *=/touring-code =/g' crates/touring-hooks/Cargo.toml 2>/dev/null || true
sed -i 's/touring-ast-polyglot *=/touring-code =/g' crates/touring-server/Cargo.toml 2>/dev/null || true
sed -i 's/touring-language *=/touring-code =/g' crates/touring-server/Cargo.toml 2>/dev/null || true
sed -i 's/touring-semantics *=/touring-code =/g' crates/touring-hooks/Cargo.toml 2>/dev/null || true
sed -i 's/touring-semantics *=/touring-code =/g' crates/touring-index/Cargo.toml 2>/dev/null || true

echo 'Phase 3: cargo check --workspace'

