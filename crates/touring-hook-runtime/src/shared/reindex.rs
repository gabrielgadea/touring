// Shared file re-indexing for touring-hooks.
//
// Centralizes `reindex_file` which was duplicated in post_edit and post_write.

use std::path::Path;

use crate::runtime::HookRuntime;

use super::feature_flags::extract_features_auto;

/// Error from [`reindex_file`] / [`reindex_file_with_old`] (F-8 / RBP-03: typed
/// in place of `String`). `From<String>` lets existing `?`-propagated `format!`
/// messages convert transparently.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ReindexError(pub String);

impl From<String> for ReindexError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

/// Compute byte offsets for an incremental edit from old→new content.
///
/// Returns `(start_byte, old_end_byte)` where:
/// - `start_byte` is the first position where `old` and `new` differ
/// - `old_end_byte` is the position in `old` where the common suffix begins
fn compute_edit_offsets(old: &str, new: &str) -> (usize, usize) {
    let old_bytes = old.as_bytes();
    let new_bytes = new.as_bytes();

    let start_byte = old_bytes
        .iter()
        .zip(new_bytes.iter())
        .take_while(|(o, n)| o == n)
        .count();

    let old_len = old_bytes.len();
    let new_len = new_bytes.len();
    let max_suffix = old_len
        .saturating_sub(start_byte)
        .min(new_len.saturating_sub(start_byte));
    let mut suffix_len = 0;
    while suffix_len < max_suffix {
        if old_bytes[old_len - 1 - suffix_len] == new_bytes[new_len - 1 - suffix_len] {
            suffix_len += 1;
        } else {
            break;
        }
    }

    let old_end_byte = old_len - suffix_len;
    (start_byte, old_end_byte)
}

/// Extract symbols using IncrementalPipeline (process_edit or process_file).
///
/// Returns `(symbols_json, symbol_count)` or falls back to `extract_symbols_fallback`
/// on any pipeline error.
///
/// # 2026-05-10 fix — incremental write to `symbols.db`
///
/// Before this revision, the pipeline result was only serialized into
/// `FileKnowledge.symbols_json` (knowledge.db). The actual `symbols` table in
/// `symbols.db` was **never updated** by the incremental path — only by the
/// full `cli_index_rebuild` walker, which explicitly calls
/// `store.replace_file_symbols(...)` after each `process_file`.
///
/// Consequence: `touring index find <NewSymbol>` returned 0 hits after
/// every edit, even though `post_edit` hooks reported success. The
/// daemon-level cache stayed warm but the queryable symbol store drifted
/// from reality after the very first edit.
///
/// Fix: when a `symbol_store` is available, mirror `cli_index_rebuild`'s
/// behaviour and atomically replace the file's symbol rows with
/// `result.symbols_added` immediately after the pipeline succeeds.
/// Mirrors `cli_index_rebuild`'s references branch (B 2026-06-21) on the
/// INCREMENTAL path: appends call-sites as `is_definition=false, kind="call"`
/// rows so find-references and wiring learn consumers from edits/ingest, not
/// only from full rebuilds. Root cause closed (cross-audit 2026-08-12): any
/// file only ever *edited* — never rebuilt — recorded zero references, so a
/// symbol used cross-crate via `module::symbol(...)` (e.g. `tags::derive_tags`)
/// read as 0 consumers and surfaced as a false wiring orphan.
/// Same kill-switch as the rebuild: `TOURING_INDEX_REFERENCES=0` disables.
fn with_call_sites(
    rel_path: &str,
    content: &str,
    mut symbols: Vec<touring_code::ast::SymbolLocation>,
) -> Vec<touring_code::ast::SymbolLocation> {
    let references_on = std::env::var("TOURING_INDEX_REFERENCES")
        .map(|v| v != "0" && v != "false")
        .unwrap_or(true);
    if !references_on {
        return symbols;
    }
    let Some(lang) = touring_code::ast::Lang::from_path(Path::new(rel_path)) else {
        return symbols;
    };
    for cs in touring_code::ast::build_call_graph(content, lang).sites {
        symbols.push(
            touring_code::ast::SymbolLocation::new(rel_path, cs.callee, cs.line, 0, false)
                .with_kind(Some("call".to_string())),
        );
    }
    symbols
}

fn extract_symbols_via_pipeline(
    pipeline: &touring_code::ast::incremental_pipeline::SharedPipeline,
    symbol_store: Option<&touring_code::ast::store::SymbolStore>,
    rel_path: &str,
    content: &str,
    old_content: Option<&str>,
    full_path: &str,
) -> (String, i64) {
    // Try process_edit first when we have cached tree + old content.
    if let Some(old) = old_content
        && pipeline.has_cached_tree(rel_path)
    {
        // Sprint 4.7 (2026-05-24) — upstream defense for ropey OOB panic:
        //
        // `compute_edit_offsets(old, content)` returns `old_end_byte ≤ old.len()`,
        // valid for the `old` snapshot. But if the cached pipeline doc has
        // drifted (cross-actor race, queued-after-mutation, file watcher
        // racing CC edit, full reparse intervening), `old.len()` may exceed
        // the cached rope's `len_bytes`. In that case `old_end_byte` is
        // out-of-bounds for the cached rope, and `Document::edit` would
        // historically panic in `Rope::byte_to_char`.
        //
        // Sprint 4.6 added a clamp+warn defense inside `Document::edit`
        // so the daemon no longer dies, but the resulting "edit" applied
        // to a divergent rope state produces a meaningless InputEdit. Far
        // better to detect the drift here and bail to a full reparse —
        // which is what cli_index_rebuild does and is known-good.
        //
        // Forensic anchor: Sprint 4.6 captured panic at byte_idx=84877
        // vs cached rope len=84279 (drift = 598 bytes) on thread
        // `touring-project-actor`.
        let cached_len = pipeline.with_read(|p| p.get_document(rel_path).map(|d| d.len_bytes()));
        if cached_len != Some(old.len()) {
            tracing::debug!(
                target: "touring::reindex",
                file = %rel_path,
                expected_old_len = old.len(),
                cached_doc_len = ?cached_len,
                "old content drift vs cached pipeline doc — bailing to full reparse \
                 (avoids stale-offset OOB in Document::edit; Sprint 4.7 upstream defense)"
            );
            // Skip the incremental block — fall through to full reparse below.
        } else {
            let (start_byte, old_end_byte) = compute_edit_offsets(old, content);
            if let Ok(result) =
                pipeline.with_write(|p| p.process_edit(rel_path, start_byte, old_end_byte, content))
            {
                if let Some(store) = symbol_store
                    && let Err(e) = store.replace_file_symbols(
                        rel_path,
                        &with_call_sites(rel_path, content, result.symbols_added.clone()),
                    )
                {
                    tracing::warn!(
                        target: "touring::reindex",
                        file = %rel_path,
                        error = %e,
                        "process_edit succeeded but replace_file_symbols failed — \
                         symbols.db will drift; try `touring index rebuild`",
                    );
                }
                let symbols_json = serde_json::to_string(&result.symbols_added)
                    .unwrap_or_else(|_| "[]".to_string());
                return (symbols_json, result.symbols_added.len() as i64);
            }
        } // close `} else {` block of Sprint 4.7 drift check
    }

    // Fall back to full reparse.
    match pipeline.with_write(|p| p.process_file(rel_path, content)) {
        Ok(result) => {
            if let Some(store) = symbol_store
                && let Err(e) = store.replace_file_symbols(
                    rel_path,
                    &with_call_sites(rel_path, content, result.symbols_added.clone()),
                )
            {
                tracing::warn!(
                    target: "touring::reindex",
                    file = %rel_path,
                    error = %e,
                    "process_file succeeded but replace_file_symbols failed — \
                     symbols.db will drift; try `touring index rebuild`",
                );
            }
            let symbols_json =
                serde_json::to_string(&result.symbols_added).unwrap_or_else(|_| "[]".to_string());
            (symbols_json, result.symbols_added.len() as i64)
        }
        Err(e) => {
            tracing::debug!("pipeline failed for {rel_path}: {e}, using fallback");
            extract_symbols_fallback(symbol_store, rel_path, content, full_path)
        }
    }
}

/// Fallback symbol extraction when pipeline is unavailable or fails.
///
/// Follow-up F-1 (cross-audit 2026-08-12): the old fallback returned
/// `(json, count)` for the knowledge DB but never wrote to the symbol
/// store — a file indexed through this path had NO rows in symbols.db at
/// all (not even definitions), so it was invisible to find-references and
/// its consumers never appeared in the wiring map. Now the fallback parses
/// once via `touring_code::ast::extract_symbols` and persists definitions +
/// call-sites through the same `replace_file_symbols` + `with_call_sites`
/// pair the pipeline paths use — all three reindex paths keep the index.
fn extract_symbols_fallback(
    symbol_store: Option<&touring_code::ast::store::SymbolStore>,
    rel_path: &str,
    content: &str,
    full_path: &str,
) -> (String, i64) {
    let Some(lang) = touring_code::ast::Lang::from_path(Path::new(full_path)) else {
        return (String::new(), 0);
    };
    let symbols = match touring_code::ast::extract_symbols(content, lang) {
        Ok(v) => v,
        Err(_) => return (String::new(), 0),
    };
    let count = symbols.len();
    // Same compact shape `enrich_file_knowledge` always produced — the
    // knowledge.symbols_json contract is unchanged.
    let compact: Vec<serde_json::Value> = symbols
        .iter()
        .map(|s| {
            serde_json::json!({
                "name": s.name,
                "kind": s.kind.as_str(),
                "is_public": s.is_public,
                "line": s.line,
            })
        })
        .collect();
    let json = serde_json::to_string(&compact).unwrap_or_else(|_| "[]".to_string());

    if let Some(store) = symbol_store {
        let locations: Vec<touring_code::ast::SymbolLocation> = symbols
            .iter()
            .map(|s| {
                touring_code::ast::SymbolLocation::new(
                    rel_path,
                    s.name.clone(),
                    s.line,
                    s.column,
                    true,
                )
                .with_kind(Some(s.kind.as_str().to_string()))
            })
            .collect();
        let with_refs = with_call_sites(rel_path, content, locations);
        if let Err(e) = store.replace_file_symbols(rel_path, &with_refs) {
            tracing::warn!(
                target: "touring::reindex",
                file = %rel_path,
                error = %e,
                "fallback replace_file_symbols failed — symbols.db will drift;                  try `touring index rebuild`",
            );
        }
    }
    (json, count as i64)
}

/// Re-index a file after edit/write (update knowledge DB with current content).
///
/// Reads the file from disk, extracts symbols and imports via tree-sitter
/// (falling back to regex), upserts into the knowledge DB, updates relations,
/// and refreshes the wiring map. Also wires Pln2 extended data:
///
/// - Feature flags (Cargo.toml, pyproject.toml, package.json, shell scripts)
/// - BLAKE3 content hash
/// - TODOs/FIXMEs extracted from content
///
/// When `old_content` is provided AND the pipeline has a cached tree for this
/// file, uses `process_edit` (O(edit_region)) instead of `process_file` (O(file))
/// — up to 6.9× faster for cached-tree edits.
pub fn reindex_file(
    runtime: &HookRuntime,
    abs_path: &str,
    rel_path: &str,
) -> Result<(), ReindexError> {
    reindex_file_with_old(runtime, abs_path, rel_path, None)
}

/// Like `reindex_file` but accepts the old file content to enable incremental
/// re-parsing when a cached tree exists in the pipeline.
pub fn reindex_file_with_old(
    runtime: &HookRuntime,
    abs_path: &str,
    rel_path: &str,
    old_content: Option<&str>,
) -> Result<(), ReindexError> {
    let full_path = if std::path::Path::new(abs_path).is_absolute() {
        abs_path.to_string()
    } else {
        runtime
            .project_root
            .join(abs_path)
            .to_string_lossy()
            .to_string()
    };

    let content = match std::fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };

    let path = Path::new(rel_path);
    let language = crate::shared::detect_language::detect_language_owned(rel_path);
    let line_count = content.lines().count() as i64;

    #[cfg(feature = "post-hooks")]
    let imports = crate::symbol_extractors::extract_imports_fast(&content, &language);
    #[cfg(not(feature = "post-hooks"))]
    let imports: Vec<String> = Vec::new();

    // P2-C / P4.3: Use IncrementalPipeline when available.
    //
    // Priority order:
    // 1. process_edit — O(edit_region) when we have a cached tree + old content.
    //    This is the hot path after the first reindex populates the tree cache.
    // 2. process_file — O(file) full reparse for cache misses or no old content.
    //    Also used on first edit after daemon startup (tree not yet cached).
    // 3. extract_symbols_fallback — regex-based when pipeline unavailable.
    let (symbols_json, symbol_count) = if let Some(ref pipeline) = runtime.infra.pipeline {
        extract_symbols_via_pipeline(
            pipeline,
            runtime.infra.symbol_store.as_ref(),
            rel_path,
            &content,
            old_content,
            &full_path,
        )
    } else {
        extract_symbols_fallback(
            runtime.infra.symbol_store.as_ref(),
            rel_path,
            &content,
            &full_path,
        )
    };

    let knowledge = crate::knowledge::FileKnowledge {
        file_path: rel_path.to_string(),
        language: Some(language.clone()),
        line_count,
        symbol_count,
        imports_json: Some(serde_json::to_string(&imports).unwrap_or_default()),
        symbols_json: Some(symbols_json),
        ..Default::default()
    };

    let _ = runtime.ctx.knowledge.upsert(&knowledge);

    // Update relations.
    #[cfg(feature = "post-hooks")]
    {
        let relations: Vec<crate::knowledge::FileRelation> = imports
            .iter()
            .filter_map(|imp| {
                crate::symbol_extractors::resolve_import_path(imp, &language).map(|target| {
                    crate::knowledge::FileRelation {
                        source: rel_path.to_string(),
                        target,
                        relation_type: "imports".to_string(),
                    }
                })
            })
            .collect();

        if !relations.is_empty() {
            let _ = runtime
                .ctx
                .knowledge
                .replace_relations_from(rel_path, &relations);
        }
    }

    // Wiring Intelligence: update wiring_map after edit/write.
    crate::wiring::update_wiring_after_edit(&runtime.ctx.knowledge, rel_path);

    // Direct-path consumer edges (FIX-2): detect `crate::mod::fn(...)` /
    // `super::mod::fn(...)` call sites that don't appear as `use` imports.
    // Without this pass, call-path consumers (e.g. `hook_registry.rs`
    // invoking `crate::lifecycle::handle_*`) were invisible to the wiring
    // analyzer — leaving modules mis-flagged as 100% orphaned.
    crate::wiring::record_direct_path_consumers(&runtime.ctx.knowledge, rel_path, &content);

    // ── Pln2: Wire feature flags into file_feature_flags table ─────────────
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let is_config_file = matches!(ext, "toml" | "pyproject" | "json" | "sh" | "bash" | "zsh")
        || rel_path.contains("package.json")
        || rel_path.ends_with("Cargo.toml")
        || rel_path.ends_with("pyproject.toml");
    if is_config_file || ext == "rs" {
        let features = extract_features_auto(path, &content);
        if !features.is_empty() {
            let feature_pairs: Vec<(&str, &str)> = features
                .iter()
                .map(|f| {
                    (
                        f.as_str(),
                        if ext == "py" || ext == "pyproject" {
                            "python"
                        } else {
                            "rust"
                        },
                    )
                })
                .collect();
            let _ = runtime
                .ctx
                .knowledge
                .upsert_feature_flags_batch(rel_path, &feature_pairs);
        }
    }

    // ── Pln2: Wire BLAKE3 hash into file_blake3_registry table ───────────
    //
    // Fast pre-filter: use AES-NI accelerated fast_content_hash (via
    // touring_analysis::quality) to skip the blake3 computation entirely when
    // old_content is available and the quick hash confirms the content is
    // unchanged.  The fast hash is ~3–10× faster than blake3 and covers the
    // common case where post_edit fires but the file bytes are identical
    // (e.g. whitespace-only edits or no-op saves).
    //
    // Collision probability is ~1/2^64 per pair — negligible for this use case.
    // When hashes agree we still update the symbol_count via a lightweight path
    // that skips the heavier blake3 I/O but preserves correctness.
    let skip_blake3 = if let Some(old) = old_content {
        !crate::shared::quality::quick_content_changed(old, &content)
    } else {
        false
    };

    if !skip_blake3 {
        use blake3::Hasher;
        let mut hasher = Hasher::new();
        hasher.update(content.as_bytes());
        let hash = hasher.finalize().to_hex().to_string();
        let _ = runtime
            .ctx
            .knowledge
            .upsert_blake3_registry(rel_path, &hash, symbol_count, None);
    }

    // ── Pln2: Wire TODOs/FIXMEs into file_todos table ────────────────────
    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        let kind = if trimmed.starts_with("TODO") {
            "TODO"
        } else if trimmed.starts_with("FIXME") {
            "FIXME"
        } else if trimmed.starts_with("XXX") {
            "XXX"
        } else {
            continue;
        };
        // Extract the actual content after the tag
        let content_part = trimmed
            .find(':')
            .map(|p| trimmed[p + 1..].trim())
            .unwrap_or("");
        if !content_part.is_empty() {
            let _ = runtime.ctx.knowledge.insert_todo(
                rel_path,
                (line_idx + 1) as i64,
                kind,
                content_part,
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod fallback_store_tests {
    /// F-1 (cross-audit 2026-08-12): the fallback path must persist
    /// definitions AND call-sites to the symbol store — a file indexed
    /// through the fallback is no longer invisible to symbols.db.
    #[test]
    fn fallback_writes_defs_and_calls_to_store() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db = tmp.path().join("sym.db");
        let store = touring_code::ast::store::SymbolStore::new(&db).unwrap();
        let src = "pub fn producer() {}\nfn caller() {\n    producer();\n    tags::derive_tags();\n}\n";
        let (json, count) = super::extract_symbols_fallback(
            Some(&store),
            "src/demo.rs",
            src,
            "/abs/proj/src/demo.rs",
        );
        assert!(count >= 2, "defs extracted: {json}");
        let defs = store.find_symbol("producer").unwrap();
        assert!(defs.iter().any(|l| l.is_definition), "def row persisted");
        let refs = store.find_references("derive_tags").unwrap();
        assert!(
            refs.iter().any(|l| !l.is_definition && l.kind.as_deref() == Some("call")),
            "call-site row persisted: {refs:?}"
        );
    }
}

#[cfg(test)]
mod call_site_tests {
    use super::with_call_sites;

    /// Cross-audit 2026-08-12 regression guard: the incremental path must
    /// append call-site rows (kind="call", is_definition=false) for scoped
    /// calls — the gap that made cross-crate consumers read as orphans.
    #[test]
    fn with_call_sites_appends_scoped_calls() {
        let src = "fn f() {\n    tags::derive_tags(k);\n    plain_call();\n}\n";
        let out = with_call_sites("src/rlm.rs", src, Vec::new());
        let names: Vec<&str> = out.iter().map(|s| s.symbol_name.as_str()).collect();
        assert!(names.contains(&"derive_tags"), "scoped call: {names:?}");
        assert!(names.contains(&"plain_call"), "direct call: {names:?}");
        assert!(out.iter().all(|s| s.kind.as_deref() == Some("call")));
        assert!(out.iter().all(|s| !s.is_definition));
    }

    /// The kill-switch mirrors the rebuild: TOURING_INDEX_REFERENCES=0 keeps
    /// the incremental path definitions-only. Env access serialized (the
    /// Rust-2024 discipline, same as the crate-wide locks elsewhere).
    #[test]
    fn with_call_sites_respects_kill_switch() {
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe { std::env::set_var("TOURING_INDEX_REFERENCES", "0") };
        let out = with_call_sites("src/a.rs", "fn f() { g::h(); }", Vec::new());
        unsafe { std::env::remove_var("TOURING_INDEX_REFERENCES") };
        drop(_guard);
        assert!(out.is_empty());
    }

    /// Non-source files yield no call rows (Lang::from_path → None).
    #[test]
    fn with_call_sites_skips_unknown_languages() {
        let out = with_call_sites("docs/guide.txt", "anything()", Vec::new());
        assert!(out.is_empty());
    }
}
