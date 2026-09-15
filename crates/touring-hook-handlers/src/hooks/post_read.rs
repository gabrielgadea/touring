//! Post-Read Hook — Learns from files Claude just read.
//!
//! After Claude reads a file, this hook:
//! 1. Reads the file directly from filesystem (fast, independent of stdin size)
//! 2. **AST path** (Python/Rust/TS/JS): Uses touring-ast via `ast_bridge` for
//!    precise symbol extraction, import resolution, and content hashing
//! 3. **Regex fallback** (markdown, JSON, TOML, etc.): Uses fast regex for
//!    languages not supported by tree-sitter
//! 4. Upserts file_knowledge + file_relations + access_log
//!
//! Runs async (non-blocking). Target latency: <15ms.

use std::path::Path;

use super::ast_bridge;
use super::knowledge::{FileKnowledge, FileRelation};
use super::runtime::{HookRuntime, make_relative};
use crate::schemas::validate_payload;

/// Languages supported by touring-ast (tree-sitter path).
/// With v11.0.0, all 11 languages are supported.
fn is_ast_supported(language: &str) -> bool {
    matches!(
        language,
        "python"
            | "rust"
            | "typescript"
            | "javascript"
            | "bash"
            | "html"
            | "css"
            | "markdown"
            | "json"
            | "toml"
            | "yaml"
    )
}

/// Run the post-read hook. Always exits 0 (learning is best-effort).
#[tracing::instrument(skip(runtime, input), fields(hook = "post_read"))]
pub fn run(
    runtime: &HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    // D9: Validate payload with typed schema — fail fast on malformed input.
    let tool_input = match input.get("tool_input") {
        Some(v) => v,
        None => return Ok(()), // Silently skip malformed input (post_read is non-blocking)
    };
    let validated = match validate_payload::<crate::schemas::PostReadPayload>(tool_input) {
        Ok(v) => v,
        Err(_) => return Ok(()), // Silently skip on validation failure
    };
    let file_path = validated.file_path.as_str();

    if file_path.is_empty() {
        return Ok(());
    }

    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Read file directly from filesystem (faster + more reliable than stdin)
    let abs_path = if Path::new(file_path).is_absolute() {
        file_path.to_string()
    } else {
        runtime
            .project_root
            .join(file_path)
            .to_string_lossy()
            .to_string()
    };

    let content = match std::fs::read_to_string(&abs_path) {
        Ok(c) => c,
        Err(_) => return Ok(()), // File not readable — skip silently
    };

    // The walker's admission (cross-audit 14/09/2026, B3): a read of a file the
    // rebuild refuses writes nothing, and a companion file is stored under its
    // key. Without it a read of `~/.claude/skills/…` stored an absolute key.
    let Ok(rel_path) = crate::shared::reindex::admission_refusal(
        runtime,
        &make_relative(file_path, &runtime.project_root),
    ) else {
        return Ok(());
    };
    let language = detect_language(&rel_path);

    // Choose extraction path: AST (precise) or regex (fast fallback)
    let (knowledge, imports_for_relations) = if is_ast_supported(&language) {
        build_knowledge_ast(&rel_path, &content, &abs_path)
    } else {
        build_knowledge_regex(&rel_path, &content, &language)
    };

    // A read is not a change. When the stored knowledge already carries this exact
    // content, every wiring row was derived from it — by the rebuild or by the
    // last edit — and re-deriving here only rewrote them (cross-audit 14/09/2026:
    // the critics' reads alone moved the orphan count during the audit).
    let content_unchanged = runtime
        .ctx
        .knowledge
        .lookup(&rel_path)
        .ok()
        .flatten()
        .and_then(|stored| stored.content_hash)
        .is_some_and(|stored| knowledge.content_hash.as_deref() == Some(stored.as_str()));

    // Upsert file knowledge
    let _ = runtime.ctx.knowledge.upsert(&knowledge);

    // Build and store relations from imports
    let relations: Vec<FileRelation> = imports_for_relations
        .iter()
        .filter_map(|imp| resolve_import_path(imp, &language))
        .map(|target| FileRelation {
            source: rel_path.clone(),
            target,
            relation_type: "imports".to_string(),
        })
        .collect();

    if !relations.is_empty() {
        let _ = runtime
            .ctx
            .knowledge
            .replace_relations_from(&rel_path, &relations);
    }

    // ── Wiring Intelligence: the file's whole wiring, only when its content changed ──
    // (The F9 name-only dispatch pass that lived here is `record_inferred_consumers`
    // inside `refresh_file_wiring` now: capped, provenance `ast_inferred`, with type
    // positions and qualified calls — it used to record guesses as `ast_resolved`.)
    if !content_unchanged {
        populate_wiring_map(&runtime.ctx.knowledge, &rel_path, &knowledge, &content);
    }

    // ── Functional Signature: register module's functional identity for chain detection ──
    // Runs only for AST-supported languages where symbols_json is populated.
    // INSERT OR REPLACE — safe to call on every re-read of the same file.
    if let Some(symbols_json) = knowledge.symbols_json.as_deref()
        && let Some(sig) = crate::functional_wiring::extract_functional_signature(
            &rel_path,
            &content,
            symbols_json,
        )
    {
        let _ = runtime.ctx.knowledge.register_functional_signature(&sig);
    }

    // ── L0 Ecosystem: register module in ecosystem map ──
    // Extract pub_count and import_count from the knowledge we just built
    let pub_count = knowledge
        .symbols_json
        .as_deref()
        .and_then(|j| serde_json::from_str::<Vec<serde_json::Value>>(j).ok())
        .map(|syms| {
            syms.iter()
                .filter(|s| {
                    s.get("is_public")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                })
                .count() as i64
        })
        .unwrap_or(0);
    let import_count = knowledge
        .imports_json
        .as_deref()
        .and_then(|j| serde_json::from_str::<Vec<String>>(j).ok())
        .map(|v| v.len() as i64)
        .unwrap_or(0);
    crate::ecosystem::register_module(
        &runtime.ctx.knowledge,
        &rel_path,
        pub_count,
        import_count,
        0,
    );

    // I-7: Record AST strategy outcome for LearningLoop EMA tracking (fire-and-forget)
    if let Ok(mut ll) = runtime.learning.learning_loop.try_borrow_mut() {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let strategy = if is_ast_supported(&language) {
            "ast_extraction"
        } else {
            "regex_fallback"
        };
        ll.record_event(touring_code::ast::learning_loop::GenerationEvent {
            symbol_name: rel_path.clone(),
            language: language.clone(),
            success: true, // post_read success = file was read and indexed
            strategy_used: strategy.to_string(),
            timestamp_ms,
        });
    }

    // Record access
    let _ = runtime.ctx.knowledge.record_access(&rel_path, session_id);

    Ok(())
}

// ─── Wiring Intelligence ─────────────────────────────────────────────────────

/// Subprojects inside the touring workspace that are NOT touring crates.
/// These are indexed by the daemon but should NOT contribute to wiring analysis
/// because their symbols have no consumers in the touring crates proper.
const WIRING_SKIP_SUBPROJECTS: &[&str] = &["agent-harness", "holon-wasm-components", "pln2"];

/// Returns true if `rel_path` belongs to a known non-touring subproject.
fn is_non_touring_subproject(rel_path: &str) -> bool {
    WIRING_SKIP_SUBPROJECTS
        .iter()
        .any(|subproject| rel_path.contains(subproject))
}

/// Populate wiring_map: register pub symbols and their consumers.
///
/// NOTE: Skips non-code languages and non-touring subprojects (agent-harness,
/// holon-wasm-components, pln2) to avoid false-positive orphans.
fn populate_wiring_map(
    db: &super::knowledge::FileKnowledgeDB,
    rel_path: &str,
    knowledge: &FileKnowledge,
    content: &str,
) {
    // Skip non-code languages — they don't have real symbol visibility (the one
    // predicate the rebuild and the edit path use).
    if let Some(lang) = knowledge.language.as_deref()
        && !crate::wiring::declares_wireable_api(lang)
    {
        return;
    }

    // Skip subprojects that are not part of the touring crates proper
    if is_non_touring_subproject(rel_path) {
        return;
    }

    // The file's whole wiring through the single hook-path refresh the edit,
    // file-changed and task-output paths use. This body used to clear every
    // consumer row of the file — the rebuild's inferred edges included — and
    // re-record only `use` imports through a resolver of its own, whose `crate::`
    // string fallback minted phantom modules (`src/protocol.rs`, `src/shared.rs`)
    // (cross-audit 14/09/2026, B1).
    crate::wiring::refresh_file_wiring(
        db,
        rel_path,
        knowledge.language.as_deref().unwrap_or(""),
        content,
    );
}

// ─── AST path (tree-sitter via touring-ast) ──────────────────────────────────

/// Build FileKnowledge using touring-ast's tree-sitter parsing.
///
/// Provides: precise symbol extraction (with kind, parent, async, decorators),
/// tree-sitter import extraction, content hash.
fn build_knowledge_ast(
    rel_path: &str,
    content: &str,
    abs_path: &str,
) -> (FileKnowledge, Vec<String>) {
    // Try AST-based enrichment first
    let knowledge = ast_bridge::build_enriched_knowledge_with_quality(abs_path, content);

    // Extract import strings for relation building
    let imports: Vec<String> = ast_bridge::extract_file_imports(content, abs_path)
        .into_iter()
        .map(|(module, _symbols)| module)
        .collect();

    // Override file_path to relative
    let knowledge = FileKnowledge {
        file_path: rel_path.to_string(),
        ..knowledge
    };

    (knowledge, imports)
}

// ─── Regex fallback (non-AST languages) ──────────────────────────────────────

/// Build FileKnowledge using fast regex extraction.
///
/// Used for languages NOT supported by tree-sitter (markdown, JSON, TOML, etc.).
fn build_knowledge_regex(
    rel_path: &str,
    content: &str,
    language: &str,
) -> (FileKnowledge, Vec<String>) {
    let line_count = content.lines().count() as i64;
    let imports = extract_imports_fast(content, language);
    let symbols = extract_symbols_fast(content, language);
    let content_hash = sha256_short(content);

    let knowledge = FileKnowledge {
        file_path: rel_path.to_string(),
        language: Some(language.to_string()),
        line_count,
        symbol_count: symbols.len() as i64,
        content_hash: Some(content_hash),
        imports_json: Some(serde_json::to_string(&imports).unwrap_or_default()),
        symbols_json: Some(serde_json::to_string(&symbols).unwrap_or_default()),
        ..Default::default()
    };

    (knowledge, imports)
}

/// Run the post-read hook and return a `HookResponse`.
///
/// Wraps [`run`] for callers that need a return value rather than a side effect.
/// Post-read is always best-effort (learning), so this always returns Allow.
pub fn run_returning(
    runtime: &HookRuntime,
    input: &serde_json::Value,
) -> crate::runtime::HookResponse {
    let _ = run(runtime, input);
    crate::runtime::HookResponse::Allow
}

// ─── Shared utilities ────────────────────────────────────────────────────────

/// Detect programming language from file extension.
pub fn detect_language(path: &str) -> String {
    match Path::new(path).extension().and_then(|e| e.to_str()) {
        Some("py") => "python".to_string(),
        Some("rs") => "rust".to_string(),
        Some("ts" | "tsx") => "typescript".to_string(),
        Some("js" | "jsx") => "javascript".to_string(),
        Some("md") => "markdown".to_string(),
        Some("json") => "json".to_string(),
        Some("toml") => "toml".to_string(),
        Some("yaml" | "yml") => "yaml".to_string(),
        Some("html") => "html".to_string(),
        Some("css") => "css".to_string(),
        Some("sh" | "bash") => "shell".to_string(),
        Some(ext) => ext.to_string(),
        None => "unknown".to_string(),
    }
}

// ─── Lazy-compiled regexes for import extraction ─────────────────────────────

// Wave R+C I2 (2026-06-10): the fast extractors + import-path resolution moved
// to `touring_hooks_core::symbol_extractors` (pure engines). Re-exported so
// `crate::post_read::extract_*` / `resolve_import_path*` paths are unchanged.
pub use crate::symbol_extractors::{
    extract_imports_fast, extract_symbols_fast, resolve_import_path,
    resolve_import_path_with_source,
};

// ─── Lazy-compiled regexes for symbol extraction ─────────────────────────────

/// Compute short SHA-256 hash (first 16 hex chars).
fn sha256_short(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let hash = hasher.finalize();
    // SAFETY: SHA-256 always produces 32 bytes; slicing first 8 is always in bounds.
    #[allow(clippy::indexing_slicing)]
    let short = &hash[..8];
    hex::encode(short)
}

/// Resolve a module-path + imported-symbols list to `(module_file, symbol_name)` pairs.
///
/// Used by `cli_handlers` to populate consumer relationships from import analysis.
/// Returns an owned `Vec<(String, String)>` so callers can iterate freely.
pub fn resolve_import_for_language_pub(
    module_path: &str,
    imported_symbols: &[String],
    language: &str,
) -> Vec<(String, String)> {
    let resolved_file = match resolve_import_path(module_path, language) {
        Some(f) => f,
        None => return Vec::new(),
    };
    if imported_symbols.is_empty() {
        // Wildcard import — record the module itself as consumer with a placeholder
        return vec![(resolved_file, "*".to_string())];
    }
    imported_symbols
        .iter()
        .map(|sym| (resolved_file.clone(), sym.clone()))
        .collect()
}

/// Encode bytes as hex string.
mod hex {
    pub(crate) fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language() {
        assert_eq!(detect_language("src/main.py"), "python");
        assert_eq!(detect_language("src/lib.rs"), "rust");
        assert_eq!(detect_language("src/app.ts"), "typescript");
        assert_eq!(detect_language("src/index.js"), "javascript");
        assert_eq!(detect_language("README.md"), "markdown");
        assert_eq!(detect_language("noext"), "unknown");
    }

    #[test]
    fn test_is_non_touring_subproject() {
        assert!(is_non_touring_subproject(
            "agent-harness/cli_anything/touring/__init__.py"
        ));
        assert!(is_non_touring_subproject(
            "holon-wasm-components/runner/src/main.rs"
        ));
        assert!(is_non_touring_subproject("pln2/src/lib.rs"));
        assert!(!is_non_touring_subproject(
            "crates/touring-hooks/src/lib.rs"
        ));
        assert!(!is_non_touring_subproject("src/main.rs"));
    }

    #[test]
    fn test_extract_imports_python() {
        let content =
            "from os import path\nimport sys\nfrom pathlib import Path\n\ndef main():\n    pass";
        let imports = extract_imports_fast(content, "python");
        assert!(imports.contains(&"os".to_string()));
        assert!(imports.contains(&"sys".to_string()));
        assert!(imports.contains(&"pathlib".to_string()));
    }

    #[test]
    fn test_extract_imports_rust() {
        let content = "use std::path::Path;\nuse crate::hooks::classifier;\n\nfn main() {}";
        let imports = extract_imports_fast(content, "rust");
        assert!(imports.contains(&"std::path::Path".to_string()));
        assert!(imports.contains(&"crate::hooks::classifier".to_string()));
    }

    #[test]
    fn test_extract_imports_typescript() {
        let content = r#"import { useState } from 'react';
import { Config } from './config';
const x = require('lodash');"#;
        let imports = extract_imports_fast(content, "typescript");
        assert!(imports.contains(&"react".to_string()));
        assert!(imports.contains(&"./config".to_string()));
        assert!(imports.contains(&"lodash".to_string()));
    }

    #[test]
    fn test_extract_symbols_python() {
        let content = "class Foo:\n    pass\n\ndef bar():\n    pass\n\nasync def baz():\n    pass";
        let symbols = extract_symbols_fast(content, "python");
        assert!(symbols.contains(&"Foo".to_string()));
        assert!(symbols.contains(&"bar".to_string()));
        assert!(symbols.contains(&"baz".to_string()));
    }

    #[test]
    fn test_extract_symbols_rust() {
        let content = "pub fn main() {}\nstruct Config {}\nenum Level { A, B }\ntrait Handler {}";
        let symbols = extract_symbols_fast(content, "rust");
        assert!(symbols.contains(&"main".to_string()));
        assert!(symbols.contains(&"Config".to_string()));
        assert!(symbols.contains(&"Level".to_string()));
        assert!(symbols.contains(&"Handler".to_string()));
    }

    #[test]
    fn test_sha256_short() {
        let hash = sha256_short("hello world");
        assert_eq!(hash.len(), 16); // 8 bytes = 16 hex chars
    }

    /// A dotted Python module resolves to a file that EXISTS, or to nothing.
    ///
    /// This used to assert `Some("packages/kazuba_core/models.py")` for a path
    /// that need not exist anywhere — the assertion pinned the defect in place.
    /// Under it, `import pathlib` produced a producer row for `pathlib.py`:
    /// 88 such phantoms measured in `analise` on 2026-08-19, 76 of them with
    /// `symbol_kind='unknown'`, none reachable by any JOIN. The Rust arm two
    /// tests down has probed the filesystem since the phantom-`super.rs` fix;
    /// the Python arm never did.
    #[test]
    fn test_resolve_import_python() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let pkg = tmp.path().join("packages").join("kazuba_core");
        std::fs::create_dir_all(&pkg).expect("mkdir");
        std::fs::write(pkg.join("models.py"), "class User: pass\n").expect("write");
        let importer = tmp.path().join("app.py");
        std::fs::write(&importer, "from packages.kazuba_core.models import User\n").expect("write");

        let path = touring_hooks_core::symbol_extractors::resolve_import_path_with_source(
            "packages.kazuba_core.models",
            "python",
            importer.to_str(),
        );
        assert_eq!(
            path.as_deref(),
            Some(pkg.join("models.py").to_string_lossy().as_ref()),
            "an existing module resolves to its file"
        );

        // And a stdlib import, which is a file in nobody's project, resolves to
        // nothing rather than to a producer row for a file that is not there.
        assert_eq!(
            resolve_import_path("pathlib", "python"),
            None,
            "a stdlib import must not become a phantom producer"
        );
    }

    #[test]
    fn test_resolve_import_rust() {
        let path = resolve_import_path("crate::hooks::classifier", "rust");
        assert_eq!(path, Some("src/hooks/classifier.rs".to_string()));
    }

    #[test]
    fn test_resolve_import_cross_crate_touring_analysis() {
        // S-1.2 fix: cross-crate paths like touring_analysis::pipeline::Builder
        let path = resolve_import_path(
            "touring_analysis::pipeline::AnalysisPipelineBuilder",
            "rust",
        );
        assert_eq!(
            path,
            Some("crates/touring-analysis/src/pipeline.rs".to_string())
        );
    }

    #[test]
    fn test_resolve_import_cross_crate_touring_ast() {
        let path = resolve_import_path("touring_code::ast::semantic::CosineComputer", "rust");
        // semantic.rs does not exist; only semantic_search.rs does
        assert_eq!(path, None);
    }

    #[test]
    fn test_resolve_import_cross_crate_touring_learning() {
        let path = resolve_import_path(
            "touring_intelligence::rl::metacognitive::MetacognitivePipeline",
            "rust",
        );
        // metacognitive.rs does not exist; only metacognitive_pipeline.rs does
        assert_eq!(path, None);
    }

    #[test]
    fn test_resolve_import_cross_crate_alias() {
        // Alias without touring_ prefix should also work
        let path = resolve_import_path("analysis::pipeline::Builder", "rust");
        assert_eq!(
            path,
            Some("crates/touring-analysis/src/pipeline.rs".to_string())
        );
    }

    #[test]
    fn test_resolve_import_ts_relative() {
        // Without a source file, a relative specifier cannot be resolved to a
        // project path: the TS/JS arm needs the importing file's directory to
        // join against + filesystem probing (it was refactored from the old
        // naive `"./x" -> "x"` strip to real Node/TS module resolution). None is
        // the correct, conservative result — skip rather than fabricate a phantom
        // path. Resolution WITH a source file is covered in symbol_extractors.rs.
        assert_eq!(resolve_import_path("./config", "typescript"), None);
        // External package (bare specifier, node_modules) — also not resolved.
        assert_eq!(resolve_import_path("react", "typescript"), None);
    }

    /// Regression test for the phantom `super.rs` bug.
    ///
    /// Before the keyword-guard fix, `resolve_import_path("super", "rust")`
    /// returned `Some("super.rs")` because the final fallback applied a naive
    /// `replace("::", "/")` to bare keywords. That phantom file path then
    /// flowed into `record_consumer(...)` and surfaced in
    /// `/api/viz/workspace` as 7 pseudo-nodes with 708 outgoing edges and 0
    /// incoming — the classic vortex signature of an unresolved keyword.
    ///
    /// Self-imports / parent-module imports require module-hierarchy
    /// resolution which this resolver does not perform. Returning `None` is
    /// the correct, conservative behaviour: skip rather than fabricate.
    #[test]
    fn test_resolve_import_rejects_rust_scope_keywords() {
        // Bare keywords have no concrete file path.
        assert_eq!(resolve_import_path("super", "rust"), None);
        assert_eq!(resolve_import_path("self", "rust"), None);
        assert_eq!(resolve_import_path("Self", "rust"), None);

        // Keyword-prefixed paths without source context cannot be resolved.
        assert_eq!(resolve_import_path("super::Foo", "rust"), None);
        assert_eq!(resolve_import_path("super::module::Bar", "rust"), None);
        assert_eq!(resolve_import_path("self::helper", "rust"), None);
        assert_eq!(resolve_import_path("Self::associated", "rust"), None);

        // `crate::` retains its legacy project-root-relative fallback —
        // it is intentionally NOT in the keyword guard because workspace
        // crate-relative paths are still useful even without source context.
        assert_eq!(
            resolve_import_path("crate::hooks::classifier", "rust"),
            Some("src/hooks/classifier.rs".to_string())
        );

        // With source_file context, `super::module` resolves correctly via
        // crate_src_root (the existing source-aware branch handles this).
        // NOTE: The "module" segment is a keyword-like segment. When we
        // correctly resolve relative to workspace root, module.rs does NOT
        // exist in touring-hooks/src, so we correctly return None.
        assert_eq!(
            resolve_import_path_with_source(
                "super::module",
                "rust",
                Some("crates/touring-hooks/src/lib.rs"),
            ),
            None
        );

        // Bare keyword still returns None even with source context — there is
        // no concrete module path to resolve to.
        assert_eq!(
            resolve_import_path_with_source(
                "super",
                "rust",
                Some("crates/touring-hooks/src/lib.rs"),
            ),
            None
        );

        // Result-side guard: any path whose basename is a scope keyword is
        // rejected, even if the input was a valid-looking compound path.
        // E.g. "crate::super" → "crates/X/src/super.rs" basename = "super"
        // (variant observed as 6 phantom nodes in /api/viz/workspace).
        assert_eq!(
            resolve_import_path_with_source(
                "crate::super",
                "rust",
                Some("/abs/crates/touring-hooks/src/lib.rs"),
            ),
            None
        );
        // Same for crate::self / crate::Self / fallback paths.
        assert_eq!(resolve_import_path("crate::super", "rust"), None);
        assert_eq!(resolve_import_path("crate::self", "rust"), None);
    }

    #[test]
    fn test_empty_content() {
        let imports = extract_imports_fast("", "python");
        assert!(imports.is_empty());
        let symbols = extract_symbols_fast("", "python");
        assert!(symbols.is_empty());
    }

    // ── AST path tests ──────────────────────────────────────────────────

    #[test]
    fn test_is_ast_supported() {
        // Code languages
        assert!(is_ast_supported("python"));
        assert!(is_ast_supported("rust"));
        assert!(is_ast_supported("typescript"));
        assert!(is_ast_supported("javascript"));
        assert!(is_ast_supported("bash"));
        // Markup/data languages (all supported since v11.0.0)
        assert!(is_ast_supported("markdown"));
        assert!(is_ast_supported("json"));
        assert!(is_ast_supported("toml"));
        assert!(is_ast_supported("html"));
        assert!(is_ast_supported("css"));
        assert!(is_ast_supported("yaml"));
        // Still unsupported
        assert!(!is_ast_supported("unknown"));
    }

    #[test]
    fn test_ast_path_python() {
        let content = "import os\nfrom pathlib import Path\n\ndef hello():\n    pass\n\nclass Foo:\n    def bar(self):\n        return 42\n";
        let (knowledge, imports) = build_knowledge_ast("test.py", content, "test.py");

        assert_eq!(knowledge.language.as_deref(), Some("python"));
        assert!(
            knowledge.symbol_count >= 2,
            "Should find >= 2 symbols via AST, got {}",
            knowledge.symbol_count
        );
        assert!(knowledge.content_hash.is_some());
        assert!(!imports.is_empty(), "Should extract imports via AST");
    }

    #[test]
    fn test_ast_path_rust() {
        let content = "use std::path::Path;\n\npub fn main() {}\n\nstruct Config { x: i32 }\n";
        let (knowledge, imports) = build_knowledge_ast("main.rs", content, "main.rs");

        assert_eq!(knowledge.language.as_deref(), Some("rust"));
        assert!(knowledge.symbol_count >= 2);
        assert!(!imports.is_empty());
    }

    #[test]
    fn test_regex_fallback_for_markdown() {
        let content = "# Title\n\nSome content\n";
        let (knowledge, imports) = build_knowledge_regex("README.md", content, "markdown");

        assert_eq!(knowledge.language.as_deref(), Some("markdown"));
        assert_eq!(knowledge.line_count, 3);
        assert!(imports.is_empty());
    }

    #[test]
    fn test_extract_imports_fast_still_works() {
        // Existing extract_imports_fast must still work for non-AST languages
        let content = "use std::path::Path;\nuse crate::foo::Bar;\nfn main() {}";
        let imports = extract_imports_fast(content, "rust");
        assert!(imports.contains(&"std::path::Path".to_string()));
        assert!(imports.contains(&"crate::foo::Bar".to_string()));
    }

    #[test]
    fn test_populate_wiring_map_registers_pub_symbols() {
        use super::super::knowledge::FileKnowledgeDB;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = FileKnowledgeDB::new(&db_path).unwrap();

        let knowledge = FileKnowledge {
            file_path: "src/tfidf.rs".into(),
            language: Some("rust".into()),
            line_count: 100,
            symbol_count: 3,
            read_count: 1,
            last_read_at: None,
            imports_json: Some("[]".into()),
            symbols_json: None,
            content_hash: None,
            notes: None,
        };

        let content =
            "pub struct TfIdfVectorizer;\nfn internal_fn() {}\npub fn compute_scores() {}\n";
        populate_wiring_map(&db, "src/tfidf.rs", &knowledge, content);

        let orphans = db.orphan_symbols().unwrap();
        assert_eq!(
            orphans.len(),
            2,
            "Should have 2 pub orphan symbols (TfIdfVectorizer + compute_scores)"
        );
        let names: Vec<&str> = orphans.iter().map(|o| o.symbol_name.as_str()).collect();
        assert!(names.contains(&"TfIdfVectorizer"));
        assert!(names.contains(&"compute_scores"));
    }

    #[test]
    fn test_populate_wiring_map_records_consumers() {
        use super::super::knowledge::FileKnowledgeDB;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = FileKnowledgeDB::new(&db_path).unwrap();

        // First, register a pub symbol in the target module
        db.register_pub_symbol("src/tfidf.rs", "TfIdfVectorizer", "struct", "public")
            .unwrap();

        // Then process a consumer file that imports it
        let knowledge = FileKnowledge {
            file_path: "src/nexus.rs".into(),
            language: Some("rust".into()),
            line_count: 50,
            symbol_count: 1,
            read_count: 1,
            last_read_at: None,
            imports_json: Some(r#"["crate::tfidf::TfIdfVectorizer"]"#.into()),
            symbols_json: Some("[]".into()),
            content_hash: None,
            notes: None,
        };

        // Production upserts the knowledge before refreshing the wiring (`run`);
        // the refresh reads the stored imports, as the edit path does.
        db.upsert(&knowledge).unwrap();
        populate_wiring_map(&db, "src/nexus.rs", &knowledge, "");

        // The TfIdfVectorizer should now have a consumer entry
        let score = db.integration_score("src/tfidf.rs").unwrap();
        assert!(
            score > 0.0,
            "Integration score should be > 0 after recording consumer"
        );
    }

    #[test]
    fn test_ast_vs_regex_parity() {
        // For supported languages, AST should find at least as many symbols as regex
        let content =
            "def hello():\n    pass\n\nclass Foo:\n    def bar(self):\n        return 42\n";

        let regex_symbols = extract_symbols_fast(content, "python");
        let (ast_knowledge, _) = build_knowledge_ast("test.py", content, "test.py");

        assert!(
            ast_knowledge.symbol_count as usize >= regex_symbols.len(),
            "AST ({}) should find >= regex ({}) symbols",
            ast_knowledge.symbol_count,
            regex_symbols.len()
        );
    }

    /// Cross-audit 14/09/2026 (B1): a read is not a change. Re-reading a file
    /// whose stored knowledge carries the same content leaves its wiring rows —
    /// including edges only a rebuild derives — untouched; the old path cleared
    /// them on every read.
    #[test]
    fn a_read_of_a_file_the_walker_refuses_writes_nothing() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".venv/lib")).unwrap();
        let refused = tmp.path().join(".venv/lib/site.py");
        std::fs::write(&refused, "def vendored():\n    pass\n").unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        let stray = outside.path().join("stray.py");
        std::fs::write(&stray, "def stray():\n    pass\n").unwrap();
        let rt = HookRuntime::new(tmp.path()).unwrap();
        for file in [&refused, &stray] {
            let input = serde_json::json!({
                "session_id": "t",
                "tool_input": { "file_path": file.to_string_lossy() }
            });
            run(&rt, &input).expect("read");
        }
        let rows: i64 = rt
            .ctx
            .knowledge
            .conn_ref()
            .query_row("SELECT COUNT(*) FROM file_knowledge", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            rows, 0,
            "neither the excluded nor the outside file is stored"
        );
    }

    #[test]
    fn a_re_read_of_unchanged_content_leaves_the_wiring_alone() {
        use touring_hook_runtime::knowledge_wiring::WiringOrigin;

        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        let file = tmp.path().join("src/lib.rs");
        std::fs::write(&file, "pub fn exported() {}\nfn body() { helper(); }\n").unwrap();
        let rt = HookRuntime::new(tmp.path()).unwrap();
        let input = serde_json::json!({
            "session_id": "t",
            "tool_input": { "file_path": file.to_string_lossy() }
        });

        run(&rt, &input).expect("first read");
        // An edge a rebuild derived and the hook path cannot re-derive.
        rt.ctx
            .knowledge
            .register_pub_symbol("src/extra.rs", "rebuild_only", "function", "public")
            .unwrap();
        rt.ctx
            .knowledge
            .record_consumer_with_origin(
                "src/extra.rs",
                "rebuild_only",
                "src/lib.rs",
                None,
                WiringOrigin::AstInferred,
            )
            .unwrap();
        let edges = |rt: &HookRuntime| -> i64 {
            rt.ctx
                .knowledge
                .conn_ref()
                .query_row(
                    "SELECT COUNT(*) FROM wiring_map WHERE consumer_file = 'src/lib.rs' AND symbol_name = 'rebuild_only'",
                    [],
                    |r| r.get(0),
                )
                .unwrap()
        };
        assert_eq!(edges(&rt), 1);

        run(&rt, &input).expect("unchanged re-read");
        assert_eq!(edges(&rt), 1, "an unchanged read left the edge in place");

        std::fs::write(&file, "pub fn exported() {}\nfn body() {}\n").unwrap();
        run(&rt, &input).expect("read after an external change");
        assert_eq!(
            edges(&rt),
            0,
            "a changed file is refreshed: the edge it no longer supports is gone"
        );
    }
}
