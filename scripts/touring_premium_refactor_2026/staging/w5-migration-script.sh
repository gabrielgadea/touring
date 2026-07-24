#!/usr/bin/env bash
# AUTO-GENERATED — W5.x consumer migration
# Generated: 2026-05-12T00:53:39.489775+00:00
set -euo pipefail

touring ast grep 'crates/touring-server/src/tools/utility_tools.rs' 'crate::knowledge' --rewrite 'touring_storage::sqlite::knowledge' --lang rust || true
touring ast grep 'crates/touring-server/src/tools/utility_tools.rs' 'crate::tantivy_index' --rewrite 'touring_storage::tantivy' --lang rust || true
touring ast grep 'crates/touring-server/src/tools/utility_tools.rs' 'crate::rkyv_archive' --rewrite 'touring_storage::rkyv_archive' --lang rust || true
touring ast grep 'crates/touring-server/src/tools/utility_tools.rs' 'crate::persistence' --rewrite 'touring_storage::sqlite::persistence' --lang rust || true
touring ast grep 'crates/touring-hooks/src/tantivy_index.rs' 'crate::knowledge' --rewrite 'touring_storage::sqlite::knowledge' --lang rust || true
touring ast grep 'crates/touring-hooks/src/tantivy_index.rs' 'crate::tantivy_index' --rewrite 'touring_storage::tantivy' --lang rust || true
touring ast grep 'crates/touring-hooks/src/tantivy_index.rs' 'crate::rkyv_archive' --rewrite 'touring_storage::rkyv_archive' --lang rust || true
touring ast grep 'crates/touring-hooks/src/tantivy_index.rs' 'crate::persistence' --rewrite 'touring_storage::sqlite::persistence' --lang rust || true
touring ast grep 'crates/touring-server/src/reasoning/persistence.rs' 'crate::knowledge' --rewrite 'touring_storage::sqlite::knowledge' --lang rust || true
touring ast grep 'crates/touring-server/src/reasoning/persistence.rs' 'crate::tantivy_index' --rewrite 'touring_storage::tantivy' --lang rust || true
touring ast grep 'crates/touring-server/src/reasoning/persistence.rs' 'crate::rkyv_archive' --rewrite 'touring_storage::rkyv_archive' --lang rust || true
touring ast grep 'crates/touring-server/src/reasoning/persistence.rs' 'crate::persistence' --rewrite 'touring_storage::sqlite::persistence' --lang rust || true
touring ast grep 'crates/touring-hooks/src/knowledge.rs' 'crate::knowledge' --rewrite 'touring_storage::sqlite::knowledge' --lang rust || true
touring ast grep 'crates/touring-hooks/src/knowledge.rs' 'crate::tantivy_index' --rewrite 'touring_storage::tantivy' --lang rust || true
touring ast grep 'crates/touring-hooks/src/knowledge.rs' 'crate::rkyv_archive' --rewrite 'touring_storage::rkyv_archive' --lang rust || true
touring ast grep 'crates/touring-hooks/src/knowledge.rs' 'crate::persistence' --rewrite 'touring_storage::sqlite::persistence' --lang rust || true
touring ast grep 'crates/touring-server/src/server/tools_infra.rs' 'crate::knowledge' --rewrite 'touring_storage::sqlite::knowledge' --lang rust || true
touring ast grep 'crates/touring-server/src/server/tools_infra.rs' 'crate::tantivy_index' --rewrite 'touring_storage::tantivy' --lang rust || true
touring ast grep 'crates/touring-server/src/server/tools_infra.rs' 'crate::rkyv_archive' --rewrite 'touring_storage::rkyv_archive' --lang rust || true
touring ast grep 'crates/touring-server/src/server/tools_infra.rs' 'crate::persistence' --rewrite 'touring_storage::sqlite::persistence' --lang rust || true
touring ast grep 'crates/touring-server/src/server/tools_analysis.rs' 'crate::knowledge' --rewrite 'touring_storage::sqlite::knowledge' --lang rust || true
touring ast grep 'crates/touring-server/src/server/tools_analysis.rs' 'crate::tantivy_index' --rewrite 'touring_storage::tantivy' --lang rust || true
touring ast grep 'crates/touring-server/src/server/tools_analysis.rs' 'crate::rkyv_archive' --rewrite 'touring_storage::rkyv_archive' --lang rust || true
touring ast grep 'crates/touring-server/src/server/tools_analysis.rs' 'crate::persistence' --rewrite 'touring_storage::sqlite::persistence' --lang rust || true
touring ast grep 'crates/touring-learning/src/data/checkpoint.rs' 'crate::knowledge' --rewrite 'touring_storage::sqlite::knowledge' --lang rust || true
touring ast grep 'crates/touring-learning/src/data/checkpoint.rs' 'crate::tantivy_index' --rewrite 'touring_storage::tantivy' --lang rust || true
touring ast grep 'crates/touring-learning/src/data/checkpoint.rs' 'crate::rkyv_archive' --rewrite 'touring_storage::rkyv_archive' --lang rust || true
touring ast grep 'crates/touring-learning/src/data/checkpoint.rs' 'crate::persistence' --rewrite 'touring_storage::sqlite::persistence' --lang rust || true
touring ast grep 'crates/touring-server/src/cli/migrate.rs' 'crate::knowledge' --rewrite 'touring_storage::sqlite::knowledge' --lang rust || true
touring ast grep 'crates/touring-server/src/cli/migrate.rs' 'crate::tantivy_index' --rewrite 'touring_storage::tantivy' --lang rust || true
touring ast grep 'crates/touring-server/src/cli/migrate.rs' 'crate::rkyv_archive' --rewrite 'touring_storage::rkyv_archive' --lang rust || true
touring ast grep 'crates/touring-server/src/cli/migrate.rs' 'crate::persistence' --rewrite 'touring_storage::sqlite::persistence' --lang rust || true
touring ast grep 'crates/touring-core/src/checkpoint/fingerprint.rs' 'crate::knowledge' --rewrite 'touring_storage::sqlite::knowledge' --lang rust || true
touring ast grep 'crates/touring-core/src/checkpoint/fingerprint.rs' 'crate::tantivy_index' --rewrite 'touring_storage::tantivy' --lang rust || true
touring ast grep 'crates/touring-core/src/checkpoint/fingerprint.rs' 'crate::rkyv_archive' --rewrite 'touring_storage::rkyv_archive' --lang rust || true
touring ast grep 'crates/touring-core/src/checkpoint/fingerprint.rs' 'crate::persistence' --rewrite 'touring_storage::sqlite::persistence' --lang rust || true
touring ast grep 'crates/touring-server/src/session/manager.rs' 'crate::knowledge' --rewrite 'touring_storage::sqlite::knowledge' --lang rust || true
touring ast grep 'crates/touring-server/src/session/manager.rs' 'crate::tantivy_index' --rewrite 'touring_storage::tantivy' --lang rust || true
touring ast grep 'crates/touring-server/src/session/manager.rs' 'crate::rkyv_archive' --rewrite 'touring_storage::rkyv_archive' --lang rust || true
touring ast grep 'crates/touring-server/src/session/manager.rs' 'crate::persistence' --rewrite 'touring_storage::sqlite::persistence' --lang rust || true
touring ast grep 'crates/touring-ast/src/store.rs' 'crate::knowledge' --rewrite 'touring_storage::sqlite::knowledge' --lang rust || true
touring ast grep 'crates/touring-ast/src/store.rs' 'crate::tantivy_index' --rewrite 'touring_storage::tantivy' --lang rust || true
touring ast grep 'crates/touring-ast/src/store.rs' 'crate::rkyv_archive' --rewrite 'touring_storage::rkyv_archive' --lang rust || true
touring ast grep 'crates/touring-ast/src/store.rs' 'crate::persistence' --rewrite 'touring_storage::sqlite::persistence' --lang rust || true
touring ast grep 'crates/touring-core/src/schema/mod.rs' 'crate::knowledge' --rewrite 'touring_storage::sqlite::knowledge' --lang rust || true
touring ast grep 'crates/touring-core/src/schema/mod.rs' 'crate::tantivy_index' --rewrite 'touring_storage::tantivy' --lang rust || true
touring ast grep 'crates/touring-core/src/schema/mod.rs' 'crate::rkyv_archive' --rewrite 'touring_storage::rkyv_archive' --lang rust || true
touring ast grep 'crates/touring-core/src/schema/mod.rs' 'crate::persistence' --rewrite 'touring_storage::sqlite::persistence' --lang rust || true
touring ast grep 'crates/touring-hooks/src/cli_handlers.rs' 'crate::knowledge' --rewrite 'touring_storage::sqlite::knowledge' --lang rust || true
touring ast grep 'crates/touring-hooks/src/cli_handlers.rs' 'crate::tantivy_index' --rewrite 'touring_storage::tantivy' --lang rust || true
touring ast grep 'crates/touring-hooks/src/cli_handlers.rs' 'crate::rkyv_archive' --rewrite 'touring_storage::rkyv_archive' --lang rust || true
touring ast grep 'crates/touring-hooks/src/cli_handlers.rs' 'crate::persistence' --rewrite 'touring_storage::sqlite::persistence' --lang rust || true
touring ast grep 'crates/touring-server/src/server/tools_metadata.rs' 'crate::knowledge' --rewrite 'touring_storage::sqlite::knowledge' --lang rust || true
touring ast grep 'crates/touring-server/src/server/tools_metadata.rs' 'crate::tantivy_index' --rewrite 'touring_storage::tantivy' --lang rust || true
touring ast grep 'crates/touring-server/src/server/tools_metadata.rs' 'crate::rkyv_archive' --rewrite 'touring_storage::rkyv_archive' --lang rust || true
touring ast grep 'crates/touring-server/src/server/tools_metadata.rs' 'crate::persistence' --rewrite 'touring_storage::sqlite::persistence' --lang rust || true
touring ast grep 'crates/touring-analysis/src/e2e/schema_guard.rs' 'crate::knowledge' --rewrite 'touring_storage::sqlite::knowledge' --lang rust || true
touring ast grep 'crates/touring-analysis/src/e2e/schema_guard.rs' 'crate::tantivy_index' --rewrite 'touring_storage::tantivy' --lang rust || true
touring ast grep 'crates/touring-analysis/src/e2e/schema_guard.rs' 'crate::rkyv_archive' --rewrite 'touring_storage::rkyv_archive' --lang rust || true
touring ast grep 'crates/touring-analysis/src/e2e/schema_guard.rs' 'crate::persistence' --rewrite 'touring_storage::sqlite::persistence' --lang rust || true

echo 'Phase 2: cargo check'
cargo check --workspace 2>&1 | tail -20
