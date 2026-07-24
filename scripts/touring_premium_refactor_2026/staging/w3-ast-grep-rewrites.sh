#!/usr/bin/env bash
# AUTO-GENERATED — W3.1 ast-grep rewrites
# Generated: 2026-05-11T23:50:22.986911+00:00
# Idempotent: safe to re-run.
set -euo pipefail

echo 'Phase 1: Rewriting use statements...'

touring ast grep 'crates/touring-analysis/src/pipeline.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-analysis/src/cache.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-analysis/src/blast_radius/warning.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-analysis/src/wiring/finding.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-analysis/src/wiring/orphan.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-analysis/src/wiring/cycle_detection.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-analysis/src/wiring/functional_chains.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-analysis/src/wiring/fingerprints.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-analysis/src/wiring/mod.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-analysis/src/learning/mod.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-analysis/src/knowledge/mod.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-analysis/src/temporal/trends.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-analysis/src/quality/quality_finding.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-analysis/src/quality/tdg.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-analysis/src/e2e/schema_guard.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-antt/src/rlm_integration.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-ast/src/symbols.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-ast/src/surgery.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-ast-polyglot/src/scan.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-capnp-server/src/generator_health.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-cortex/src/circuit_breaker.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-cortex/src/runtime.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-cortex/src/handlers/lifecycle.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-cortex/src/handlers/learning.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-cortex/src/handlers/intelligence.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-cortex/src/handlers/enrichment.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-embeddings/src/adapter.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-generator/src/error.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/capnp_embed.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/post_edit.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/pre_read.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/rfc100_emission.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/stop.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/post_write.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/pre_write.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/cli_handlers_polyglot.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/memory_finding.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/hook_runtime.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/pre_tool_use.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/tantivy_index.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/health_delta.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/instructions_loaded.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/pre_bash.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/cli_handlers.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/knowledge.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/post_tool_rl.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/pre_edit.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/cli_handlers_session.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/post_bash.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/daemon.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/session_hooks.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/post_tool_failure.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/cli_e2e.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/cli_handlers_repo_score.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/lifecycle/pre_compact.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/runtime/impls_cognitive.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-hooks/src/runtime/impls_knowledge.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-offensive/src/erickson.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-offensive/src/erickson/rl_feedback.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-offensive/src/vuln/mod.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-search-fusion/src/hybrid/pipeline.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/graph_service.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/memory_store.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/main.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/server/tools_metadata.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/server/tools_infra.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/server/mod.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/tools/utility_tools.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/tools/file_tools.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/tools/cluster_tools.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/tools/ast_tools.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/tools/generator_tools.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/tools/project_tools.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/tools/memory_tools.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/ingest/watcher.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/ingest/parser.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/cli/migrate.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/cli/diagnostics.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/cli/status.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/cli/highlight.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true
touring ast grep 'crates/touring-server/src/cli/synergy.rs' 'touring_core' --rewrite 'touring_foundation' --lang rust || true

echo 'Phase 1 done.'
echo 'Phase 2: cargo check --workspace'
cargo check --workspace --message-format=short 2>&1 | tail -20
