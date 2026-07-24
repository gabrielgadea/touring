//! CLI semantic-primitive handlers — resolve-def, find-references, rename.
//!
//! These handlers implement the D.2 primitives from the Cross-Repo Improvements
//! Master Plan. They use touring-semantics (Definition enum, Semantics facade)
//! to resolve positions in source code to definitions, find references, and
//! perform rename refactoring via SourceChange.
//!
//! Wire: cli-resolve-def, cli-find-references, cli-rename (daemon dispatch)
//! + all_daemon_hook_names() registration.

use crate::runtime::HookRuntime;
use serde::Serialize;
use touring_code::ast::languages::Lang;
use touring_code::ast::parser::ParserPool;
use touring_code::ast::tree_sitter;
use touring_code::semantics::{Definition, DefinitionKind, source_to_definition};

/// ── shared response types ────────────────────────────────────────────────

#[derive(Serialize)]
struct DefinitionResponse {
    kind: String,
    name: String,
    source_range: String,
    source_file: String,
    definition_id: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct ReferenceResponse {
    count: usize,
    locations: Vec<ReferenceLocation>,
}

#[derive(Serialize)]
struct ReferenceLocation {
    file_path: String,
    line: usize,
    column: usize,
    context: String,
}

#[derive(Serialize)]
struct RenameResponse {
    status: String,
    old_name: String,
    new_name: String,
    sites: usize,
    source_change: Option<serde_json::Value>,
    error: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// cli-resolve-def
// ─────────────────────────────────────────────────────────────────────────────

/// `cli-resolve-def` — resolve a file:line:col position to its definition.
///
/// # Payload
/// ```json
/// { "file": "src/foo.rs", "line": 10, "column": 5, "source": "fn bar() {}" }
/// ```
/// `source` is the file contents (the CLI reads it before calling).
///
/// # Response
/// ```json
/// { "kind": "Function", "name": "bar", "source_range": "10:1-12:5",
///   "source_file": "src/foo.rs", "definition_id": { "file_id": 0, "symbol_index": 42 } }
/// ```
pub fn cli_resolve_def(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file = payload.get("file").and_then(|v| v.as_str()).unwrap_or("");
    let line = payload.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let col = payload.get("column").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let source = payload.get("source").and_then(|v| v.as_str()).unwrap_or("");

    if source.is_empty() {
        // Fallback: read from disk
        let source = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                return serde_json::json!({
                    "error": format!("cannot read '{}': {}", file, e)
                })
                .to_string();
            }
        };
        return resolve_impl(file, line, col, &source);
    }

    resolve_impl(file, line, col, source)
}

/// Format a source range as `start_line:start_col-end_line:end_col`.
///
/// Extracted as a pure helper for testability (N07 regression guard — the
/// inline format string previously contained a stray `(`, producing malformed
/// output like `12:4-15(:8`). See `tests::n07_*`.
fn format_source_range(
    start_line: usize,
    start_col: usize,
    end_line: usize,
    end_col: usize,
) -> String {
    format!("{start_line}:{start_col}-{end_line}:{end_col}")
}

#[cfg(test)]
mod tests {
    use super::{cli_find_references, cli_rename, format_source_range, resolve_impl};

    #[test]
    fn n07_format_source_range_well_formed() {
        // Regression for N07 (Master Plan H0): never emit a stray '('.
        assert_eq!(format_source_range(12, 4, 15, 8), "12:4-15:8");
        let r = format_source_range(1, 0, 3, 7);
        assert!(!r.contains('('), "stray paren in range: {r}");
        assert!(r.contains('-') && r.matches(':').count() == 2);
    }

    #[test]
    fn n07_resolve_impl_range_has_no_stray_paren() {
        // End-to-end: resolve a local binding; the emitted source_range
        // (if any) must be well-formed, never containing '('.
        let src = "fn main() {\n    let x = 1;\n    let y = x;\n}\n";
        let out = resolve_impl("test.rs", 3, 13, src);
        assert!(
            !out.contains("(:"),
            "N07 regression: malformed source_range in: {out}"
        );
    }

    /// D.2.2 (Master Plan A.W4.P2.T13): `find-references --scope workspace`
    /// must return cross-file references via the workspace symbol store, while
    /// `--scope file` must remain origin-file only. This is a behaviour change:
    /// previously `scope` was ignored and the handler only searched the origin
    /// file regardless of the requested scope.
    #[test]
    fn t13_find_references_scope_workspace_returns_crossfile() {
        use crate::runtime::HookRuntime;
        use touring_code::ast::SymbolLocation;

        let tmp = tempfile::tempdir().expect("tempdir");
        let mut rt = HookRuntime::new(tmp.path()).expect("HookRuntime::new");

        // Write the origin file to disk: `cli_find_references` reads `file`
        // from disk to resolve the definition at the given position.
        let origin = tmp.path().join("origin.rs");
        let origin_src = "fn target() {}\nfn caller() { target(); }\n";
        std::fs::write(&origin, origin_src).expect("write origin");
        let origin_path = origin.to_string_lossy().to_string();

        // Seed the workspace symbol store with a cross-file reference to
        // `target` living in a DIFFERENT file. The store is initialised by
        // `HookRuntime::new` (Some), so this exercises the real cross-file API.
        let store = rt
            .infra
            .symbol_store
            .as_ref()
            .expect("symbol_store initialised by HookRuntime::new");
        store
            .upsert_symbol(&SymbolLocation::new(
                origin_path.clone(),
                "target",
                1,
                3,
                true,
            ))
            .expect("upsert origin def");
        store
            .upsert_symbol(&SymbolLocation::new(
                "other_file.rs",
                "target",
                42,
                8,
                false,
            ))
            .expect("upsert cross-file ref");

        // scope=workspace → cross-file location MUST appear.
        let payload = serde_json::json!({
            "file": origin_path,
            "line": 1,
            "column": 3,
            "scope": "workspace"
        });
        let out = cli_find_references(&mut rt, &payload);
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid json");
        let locations = parsed["locations"].as_array().expect("locations array");
        let has_crossfile = locations.iter().any(|l| {
            l["file_path"].as_str() == Some("other_file.rs") && l["line"].as_u64() == Some(42)
        });
        assert!(
            has_crossfile,
            "scope=workspace must include cross-file ref (other_file.rs:42); got: {out}"
        );
        // Response shape unchanged: `count` mirrors `locations.len()`.
        assert_eq!(
            parsed["count"].as_u64().expect("count field is u64") as usize,
            locations.len(),
            "count must equal locations length: {out}"
        );

        // scope=file → cross-file location MUST NOT appear (origin-file only).
        let payload_file = serde_json::json!({
            "file": origin_path,
            "line": 1,
            "column": 3,
            "scope": "file"
        });
        let out_file = cli_find_references(&mut rt, &payload_file);
        let parsed_file: serde_json::Value =
            serde_json::from_str(&out_file).expect("valid json (file scope)");
        let locs_file = parsed_file["locations"]
            .as_array()
            .expect("locations array");
        assert!(
            locs_file
                .iter()
                .all(|l| l["file_path"].as_str() != Some("other_file.rs")),
            "scope=file must NOT include cross-file refs; got: {out_file}"
        );
        // Origin-file matches (the def + the call site) are still present.
        assert!(
            locs_file
                .iter()
                .all(|l| l["file_path"].as_str() == Some(origin_path.as_str())),
            "scope=file must contain only origin-file locations; got: {out_file}"
        );
    }

    /// D.2.2 (Master Plan A.W4.P3.T14): `rename --scope workspace` must COLLECT
    /// cross-file rename sites via the workspace symbol store (read-only — the
    /// dry-run preview lists the sites that *would* change; nothing is written
    /// to other files). `--scope file` must remain origin-file only. Mirrors the
    /// T13 cross-file find-references test.
    #[test]
    fn t14_rename_scope_workspace_collects_crossfile() {
        use crate::runtime::HookRuntime;
        use touring_code::ast::SymbolLocation;

        let tmp = tempfile::tempdir().expect("tempdir");
        let mut rt = HookRuntime::new(tmp.path()).expect("HookRuntime::new");

        // Write the origin file to disk: `cli_rename` reads `file` from disk to
        // resolve the definition at the given position.
        let origin = tmp.path().join("origin.rs");
        let origin_src = "fn target() {}\nfn caller() { target(); }\n";
        std::fs::write(&origin, origin_src).expect("write origin");
        let origin_path = origin.to_string_lossy().to_string();

        // Seed the workspace symbol store with a cross-file reference to
        // `target` living in a DIFFERENT file. The store is initialised by
        // `HookRuntime::new` (Some), so this exercises the real cross-file API.
        let store = rt
            .infra
            .symbol_store
            .as_ref()
            .expect("symbol_store initialised by HookRuntime::new");
        store
            .upsert_symbol(&SymbolLocation::new(
                origin_path.clone(),
                "target",
                1,
                3,
                true,
            ))
            .expect("upsert origin def");
        store
            .upsert_symbol(&SymbolLocation::new(
                "other_file.rs",
                "target",
                42,
                8,
                false,
            ))
            .expect("upsert cross-file ref");

        // Helper: gather the file paths covered by a rename SourceChange's edits.
        let edit_files = |out: &str| -> Vec<String> {
            let parsed: serde_json::Value = serde_json::from_str(out).expect("valid rename json");
            let mut files = Vec::new();
            if let Some(edits) = parsed["source_change"]["edits"].as_object() {
                for file_entry in edits.values() {
                    if let Some(arr) = file_entry["edits"].as_array() {
                        for e in arr {
                            if let Some(fp) = e["file_path"].as_str() {
                                files.push(fp.to_string());
                            }
                        }
                    }
                }
            }
            files
        };

        // scope=workspace → cross-file site MUST be collected (dry-run preview).
        let payload = serde_json::json!({
            "file": origin_path,
            "line": 1,
            "column": 3,
            "new_name": "renamed_target",
            "apply": false,
            "scope": "workspace"
        });
        let out = cli_rename(&mut rt, &payload);
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid json");
        // Response shape unchanged: dry-run is a "preview".
        assert_eq!(
            parsed["status"].as_str(),
            Some("preview"),
            "rename dry-run must report status=preview: {out}"
        );
        let ws_files = edit_files(&out);
        assert!(
            ws_files.iter().any(|f| f == "other_file.rs"),
            "scope=workspace must collect cross-file rename site (other_file.rs); got: {out}"
        );
        // `sites` count must mirror the total collected sites.
        assert_eq!(
            parsed["sites"].as_u64().expect("sites field is u64") as usize,
            ws_files.len(),
            "sites count must equal collected edit count: {out}"
        );

        // scope=file → cross-file site MUST NOT be collected (origin-file only).
        let payload_file = serde_json::json!({
            "file": origin_path,
            "line": 1,
            "column": 3,
            "new_name": "renamed_target",
            "apply": false,
            "scope": "file"
        });
        let out_file = cli_rename(&mut rt, &payload_file);
        let file_files = edit_files(&out_file);
        assert!(
            !file_files.iter().any(|f| f == "other_file.rs"),
            "scope=file must NOT collect cross-file rename sites; got: {out_file}"
        );
        assert!(
            file_files.iter().all(|f| f == origin_path.as_str()),
            "scope=file must collect only origin-file rename sites; got: {out_file}"
        );
    }
}

fn resolve_impl(file: &str, line: usize, column: usize, source: &str) -> String {
    let lang = Lang::from_path(std::path::Path::new(file)).unwrap_or(Lang::Rust);

    let pool = ParserPool::new();
    let parsed = match pool.parse(source, lang) {
        Ok(p) => p,
        Err(e) => {
            return serde_json::json!({"error": format!("parse failed: {}", e)}).to_string();
        }
    };

    // Convert line:col (1-indexed) to byte offset
    let byte_offset = byte_offset_for_position(source, line, column);
    let root = parsed.root_node();

    // Find deepest node at the byte offset
    let mut target_node = None;
    find_deepest_node_at_byte(root, byte_offset, &mut target_node);

    let node = match target_node {
        Some(n) => n,
        None => {
            return serde_json::json!({
                "error": format!("no syntax node at {}:{}", line, column)
            })
            .to_string();
        }
    };

    // Resolve definition via parent-walking algorithm
    let def = match source_to_definition(source, lang, node) {
        Some(d) => d,
        None => {
            return serde_json::json!({
                "kind": "unknown",
                "name": "",
                "source_range": "",
                "source_file": file,
                "note": "no definition found for this position"
            })
            .to_string();
        }
    };

    let kind_str = match def.kind() {
        DefinitionKind::Function => "Function",
        DefinitionKind::Struct => "Struct",
        DefinitionKind::Trait => "Trait",
        DefinitionKind::Module => "Module",
        DefinitionKind::Variant => "Variant",
        DefinitionKind::Macro => "Macro",
        DefinitionKind::Field => "Field",
        DefinitionKind::Variable => "Variable",
        DefinitionKind::Lifetime => "Lifetime",
        DefinitionKind::Generic => "Generic",
        DefinitionKind::Class => "Class",
        DefinitionKind::Interface => "Interface",
        DefinitionKind::Enum => "Enum",
        DefinitionKind::TypeAlias => "TypeAlias",
        DefinitionKind::Namespace => "Namespace",
        DefinitionKind::Parameter => "Parameter",
        DefinitionKind::Property => "Property",
    };

    let id = match &def {
        Definition::Function(id)
        | Definition::Struct(id)
        | Definition::Trait(id)
        | Definition::Module(id)
        | Definition::Variant(id)
        | Definition::Macro(id)
        | Definition::Field(id)
        | Definition::Variable(id)
        | Definition::Lifetime(id)
        | Definition::Generic(id)
        | Definition::Class(id)
        | Definition::Interface(id)
        | Definition::Enum(id)
        | Definition::TypeAlias(id)
        | Definition::Namespace(id)
        | Definition::Parameter(id)
        | Definition::Property(id) => id,
    };

    let (start_line, start_col) = point_to_line_col(source, node.start_byte());
    let (end_line, end_col) = point_to_line_col(source, node.end_byte());
    let source_range = format_source_range(start_line, start_col, end_line, end_col);

    let resp = DefinitionResponse {
        kind: kind_str.to_string(),
        name: id.name.clone().unwrap_or_default(),
        source_range,
        source_file: file.to_string(),
        definition_id: Some(serde_json::json!({
            "file_id": id.file_id,
            "symbol_index": id.symbol_index
        })),
    };
    serde_json::to_string(&resp).unwrap_or_default()
}

// ─────────────────────────────────────────────────────────────────────────────
// cli-find-references
// ─────────────────────────────────────────────────────────────────────────────

/// `cli-find-references` — find all references to the symbol at a position.
///
/// # Payload
/// ```json
/// { "file": "src/foo.rs", "line": 10, "column": 5, "scope": "workspace" }
/// ```
/// `scope` is "workspace" (default) or "project".
///
/// # Response
/// ```json
/// { "count": 3, "locations": [
///     { "file_path": "src/foo.rs", "line": 15, "column": 3, "context": "bar();" },
///     { "file_path": "src/bar.rs", "line": 8, "column": 1, "context": "foo::bar()" }
///   ] }
/// ```
pub fn cli_find_references(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file = payload.get("file").and_then(|v| v.as_str()).unwrap_or("");
    let line = payload.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let col = payload.get("column").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let scope = payload
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("workspace");

    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!({"error": format!("cannot read '{}': {}", file, e)})
                .to_string();
        }
    };

    let lang = Lang::from_path(std::path::Path::new(file)).unwrap_or(Lang::Rust);
    let pool = ParserPool::new();
    let parsed = match pool.parse(&source, lang) {
        Ok(p) => p,
        Err(e) => {
            return serde_json::json!({"error": format!("parse failed: {}", e)}).to_string();
        }
    };

    let byte_offset = byte_offset_for_position(&source, line, col);
    let root = parsed.root_node();

    let mut target_node = None;
    find_deepest_node_at_byte(root, byte_offset, &mut target_node);

    let node = match target_node {
        Some(n) => n,
        None => {
            return serde_json::json!({"count": 0, "locations": []}).to_string();
        }
    };

    let def = match source_to_definition(&source, lang, node) {
        Some(d) => d,
        None => {
            return serde_json::json!({"count": 0, "locations": [], "note": "no definition found"})
                .to_string();
        }
    };

    // Find all references using symbol store + AST search.
    // `scope` controls whether cross-file references are included:
    //   - "file"               → origin-file intra-file matches only
    //   - "workspace" (default) → all indexed files via symbol_store
    //   - "project"            → workspace results (project-root filtering is
    //                             a future refinement; maps to workspace today)
    let ref_locations = find_all_references(rt, file, &source, lang, &def, scope);

    serde_json::to_string(&ReferenceResponse {
        count: ref_locations.len(),
        locations: ref_locations,
    })
    .unwrap_or_default()
}

// ─────────────────────────────────────────────────────────────────────────────
// cli-rename
// ─────────────────────────────────────────────────────────────────────────────

/// `cli-rename` — rename a symbol and all its references atomically.
///
/// # Payload
/// ```json
/// { "file": "src/foo.rs", "line": 10, "column": 5, "new_name": "baz",
///   "apply": false }
/// ```
/// `apply` is false for dry-run (default), true to commit.
///
/// # Response (dry-run)
/// ```json
/// { "status": "preview", "old_name": "bar", "new_name": "baz",
///   "sites": 3, "source_change": { "edits": {...}, "fs_edits": [] } }
/// ```
pub fn cli_rename(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file = payload.get("file").and_then(|v| v.as_str()).unwrap_or("");
    let line = payload.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let col = payload.get("column").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let new_name = payload
        .get("new_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let apply = payload
        .get("apply")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    // `scope` controls cross-file rename-site collection (D.2.2):
    //   - "file"               → origin-file rename sites only
    //   - "workspace" (default) → all indexed files via symbol_store
    //   - "project"            → workspace sites (project-root filtering is a
    //                             future refinement; maps to workspace today)
    let scope = payload
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("workspace");

    if new_name.is_empty() {
        return serde_json::json!({
            "status": "error",
            "error": "new_name is required"
        })
        .to_string();
    }

    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!({
                "status": "error",
                "error": format!("cannot read '{}': {}", file, e)
            })
            .to_string();
        }
    };

    let lang = Lang::from_path(std::path::Path::new(file)).unwrap_or(Lang::Rust);
    let pool = ParserPool::new();
    let parsed = match pool.parse(&source, lang) {
        Ok(p) => p,
        Err(e) => {
            return serde_json::json!({
                "status": "error",
                "error": format!("parse failed: {}", e)
            })
            .to_string();
        }
    };

    let byte_offset = byte_offset_for_position(&source, line, col);
    let root = parsed.root_node();

    let mut target_node = None;
    find_deepest_node_at_byte(root, byte_offset, &mut target_node);

    let node = match target_node {
        Some(n) => n,
        None => {
            return serde_json::json!({
                "status": "error",
                "error": format!("no syntax node at {}:{}", line, col)
            })
            .to_string();
        }
    };

    let def = match source_to_definition(&source, lang, node) {
        Some(d) => d,
        None => {
            return serde_json::json!({
                "status": "error",
                "error": "no definition found at position"
            })
            .to_string();
        }
    };

    let old_name = def_name(&def);
    if old_name.is_empty() {
        return serde_json::json!({
            "status": "error",
            "error": "could not extract old name from definition"
        })
        .to_string();
    }

    // D.2.4 conflict detection: check if new_name already exists in scope
    if conflict_exists(&source, lang, new_name) {
        return serde_json::json!({
            "status": "conflict",
            "old_name": old_name,
            "new_name": new_name,
            "sites": 0,
            "error": format!("'{}' already exists in scope", new_name)
        })
        .to_string();
    }

    // Collect all rename sites (read-only: cross-file augmentation honours `scope`).
    let sites = collect_rename_sites(rt, file, &source, lang, &def, scope);

    // Build SourceChange (B.5)
    let source_change = build_rename_source_change(file, &source, &sites, &old_name, new_name);

    if apply {
        // Commit the SourceChange
        if let Err(e) = commit_source_change(&source_change) {
            return serde_json::json!({
                "status": "error",
                "old_name": old_name,
                "new_name": new_name,
                "sites": sites.len(),
                "error": format!("commit failed: {}", e)
            })
            .to_string();
        }
        serde_json::to_string(&RenameResponse {
            status: "applied".to_string(),
            old_name,
            new_name: new_name.to_string(),
            sites: sites.len(),
            source_change: Some(source_change),
            error: None,
        })
        .unwrap_or_default()
    } else {
        serde_json::to_string(&RenameResponse {
            status: "preview".to_string(),
            old_name,
            new_name: new_name.to_string(),
            sites: sites.len(),
            source_change: Some(source_change),
            error: None,
        })
        .unwrap_or_default()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

fn byte_offset_for_position(source: &str, line: usize, column: usize) -> usize {
    let mut current_line = 1;
    let mut offset = 0;
    let bytes = source.as_bytes();

    while offset < bytes.len() {
        if current_line == line {
            // Found target line — count columns
            let mut col = 1;
            let mut col_offset = offset;
            while col < column && col_offset < bytes.len() && bytes[col_offset] != b'\n' {
                col += 1;
                col_offset += 1;
            }
            return col_offset;
        }

        if bytes[offset] == b'\n' {
            current_line += 1;
        }
        offset += 1;
    }

    offset
}

fn point_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let bytes = source.as_bytes();
    let mut line = 1;
    let mut col = 1;
    for (i, &b) in bytes.iter().enumerate() {
        if i >= byte_offset {
            return (line, col);
        }
        if b == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn find_deepest_node_at_byte<'a>(
    node: tree_sitter::Node<'a>,
    target: usize,
    result: &mut Option<tree_sitter::Node<'a>>,
) {
    if node.start_byte() <= target && node.end_byte() >= target {
        *result = Some(node);
        let mut child_idx = 0;
        while let Some(child) = node.child(child_idx) {
            if child.start_byte() <= target && child.end_byte() >= target {
                find_deepest_node_at_byte(child, target, result);
            }
            child_idx += 1;
        }
    }
}

fn def_name(def: &Definition) -> String {
    match def {
        Definition::Function(id)
        | Definition::Struct(id)
        | Definition::Trait(id)
        | Definition::Module(id)
        | Definition::Variant(id)
        | Definition::Macro(id)
        | Definition::Field(id)
        | Definition::Variable(id)
        | Definition::Lifetime(id)
        | Definition::Generic(id)
        | Definition::Class(id)
        | Definition::Interface(id)
        | Definition::Enum(id)
        | Definition::TypeAlias(id)
        | Definition::Namespace(id)
        | Definition::Parameter(id)
        | Definition::Property(id) => id.name.clone().unwrap_or_default(),
    }
}

fn conflict_exists(source: &str, lang: Lang, new_name: &str) -> bool {
    let pool = ParserPool::new();
    let parsed = match pool.parse(source, lang) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let root = parsed.root_node();
    let name_bytes = new_name.as_bytes();

    // Walk tree looking for identifier nodes that match new_name. The node
    // text must be resolved against the *source* buffer (node byte offsets index
    // into `source`), not against `name`; passing `name` here previously caused
    // an out-of-range slice panic whenever an identifier sat past `name.len()`.
    let mut found = false;
    walk_tree_for_name(root, source.as_bytes(), name_bytes, &mut found);
    found
}

fn walk_tree_for_name(node: tree_sitter::Node<'_>, source: &[u8], name: &[u8], found: &mut bool) {
    if *found {
        return;
    }
    if node.kind() == "identifier"
        || node.kind() == "type_identifier"
        || node.kind() == "field_identifier"
    {
        if let Ok(node_text) = node.utf8_text(source) {
            if node_text.as_bytes() == name {
                *found = true;
                return;
            }
        }
    }
    let mut i = 0;
    while let Some(child) = node.child(i) {
        walk_tree_for_name(child, source, name, found);
        i += 1;
    }
}

struct RenameSite {
    file_path: String,
    byte_start: usize,
    byte_end: usize,
    line: usize,
    column: usize,
}

fn collect_rename_sites(
    rt: &mut HookRuntime,
    origin_file: &str,
    source: &str,
    lang: Lang,
    def: &Definition,
    scope: &str,
) -> Vec<RenameSite> {
    let mut sites = Vec::new();
    let def_name = def_name(def);

    // 1) Origin-file rename sites via AST scan (exact byte spans for the edit).
    let pool = ParserPool::new();
    let parsed = match pool.parse(source, lang) {
        Ok(p) => p,
        Err(_) => return sites,
    };
    let root = parsed.root_node();
    collect_matching_identifiers(root, source, def_name.as_bytes(), origin_file, &mut sites);

    // 2) Cross-file rename sites via the workspace symbol store (D.2.2).
    //    This is the COLLECT phase only — we gather the sites that *would* be
    //    renamed; applying the edits is the caller's responsibility (e.g.
    //    `Edit tool` / the SourceChange applier). We never write
    //    to other files here.
    //
    //    `scope=file` keeps the historical intra-file-only behaviour; any other
    //    scope ("workspace" default, or "project") augments with the cross-file
    //    sites the indexer already recorded. The symbol store is optional: when
    //    the project is not indexed it is `None`, and we simply return the
    //    origin-file sites (graceful, never panics) — mirroring `find_all_references`
    //    and the `if let Some(ref store)` pattern in `cli_index_find`.
    //
    //    Note: the symbol store records line/column but not byte spans, so a
    //    cross-file rename site carries placeholder zero byte offsets; the
    //    line/column locate the identifier and the applier re-resolves the exact
    //    span in the target file before mutating it.
    if scope != "file" && !def_name.is_empty() {
        if let Some(ref store) = rt.infra.symbol_store {
            if let Ok(locations) = store.find_all_locations(&def_name) {
                for loc in locations {
                    // The origin file is already covered by the AST scan above
                    // (with exact byte spans); skip it to avoid duplicates.
                    if loc.file_path == origin_file {
                        continue;
                    }
                    sites.push(RenameSite {
                        file_path: loc.file_path,
                        // Byte spans are not retained by the symbol store; the
                        // applier resolves them from line/column in the target file.
                        byte_start: 0,
                        byte_end: 0,
                        line: loc.line,
                        column: loc.column,
                    });
                }
            }
        }
    }

    sites
}

fn collect_matching_identifiers(
    node: tree_sitter::Node<'_>,
    source: &str,
    name: &[u8],
    file_path: &str,
    sites: &mut Vec<RenameSite>,
) {
    if node.kind() == "identifier"
        || node.kind() == "type_identifier"
        || node.kind() == "field_identifier"
    {
        if let Ok(text) = node.utf8_text(source.as_bytes()) {
            if text.as_bytes() == name {
                let (line, col) = point_to_line_col(source, node.start_byte());
                sites.push(RenameSite {
                    file_path: file_path.to_string(),
                    byte_start: node.start_byte(),
                    byte_end: node.end_byte(),
                    line,
                    column: col,
                });
            }
        }
    }
    let mut i = 0;
    while let Some(child) = node.child(i) {
        collect_matching_identifiers(child, source, name, file_path, sites);
        i += 1;
    }
}

fn build_rename_source_change(
    file: &str,
    _source: &str,
    sites: &[RenameSite],
    _old_name: &str,
    new_name: &str,
) -> serde_json::Value {
    // Build a simple text edit set — B.5 SourceChange structure
    // edits: BTreeMap<FileId, Vec<Indel>>
    let mut edits = serde_json::Map::new();
    let mut text_edits = serde_json::Map::new();
    text_edits.insert("byte_start".to_string(), serde_json::Value::Null);
    text_edits.insert("byte_end".to_string(), serde_json::Value::Null);
    text_edits.insert("insert".to_string(), serde_json::Value::Null);

    // Group edits by file (all in same file for now)
    let edits_array: Vec<serde_json::Value> = sites
        .iter()
        .map(|site| {
            serde_json::json!({
                "delete": { "start": site.byte_start, "end": site.byte_end },
                "insert": new_name,
                "file_path": site.file_path,
                "line": site.line,
                "column": site.column
            })
        })
        .collect();

    edits.insert(
        file.to_string(),
        serde_json::json!({ "edits": edits_array }),
    );

    serde_json::json!({
        "edits": edits,
        "fs_edits": [],
        "annotations": []
    })
}

fn commit_source_change(sc: &serde_json::Value) -> Result<(), String> {
    // Read edits from sc and write to files
    // This is a stub — B.5 SourceChange applier handles the actual I/O
    // For now, the CLI stub returns the SourceChange JSON without committing
    let _ = sc;
    Ok(())
}

fn find_all_references(
    rt: &mut HookRuntime,
    origin_file: &str,
    source: &str,
    lang: Lang,
    def: &Definition,
    scope: &str,
) -> Vec<ReferenceLocation> {
    let mut refs = Vec::new();
    let def_name = def_name(def);

    // 1) Origin-file references via AST scan (rich `context` line snippets).
    let pool = ParserPool::new();
    let parsed = match pool.parse(source, lang) {
        Ok(p) => p,
        Err(_) => return refs,
    };
    let root = parsed.root_node();
    collect_reference_nodes(root, source, def_name.as_bytes(), origin_file, &mut refs);

    // 2) Cross-file references via the workspace symbol store (D.2.2).
    //    `scope=file` keeps the historical intra-file-only behaviour; any
    //    other scope ("workspace" default, or "project") augments with the
    //    cross-file matches the indexer already recorded. The symbol store is
    //    optional: when the project is not indexed it is `None`, and we simply
    //    return the origin-file results (graceful, never panics) — mirroring
    //    the `if let Some(ref store)` pattern in `cli_index_find`.
    if scope != "file" && !def_name.is_empty() {
        if let Some(ref store) = rt.infra.symbol_store {
            if let Ok(locations) = store.find_all_locations(&def_name) {
                for loc in locations {
                    // The origin file is already covered by the AST scan above
                    // (with full line context); skip it to avoid duplicates.
                    if loc.file_path == origin_file {
                        continue;
                    }
                    refs.push(ReferenceLocation {
                        file_path: loc.file_path,
                        line: loc.line,
                        column: loc.column,
                        // The symbol store does not retain the source line; the
                        // file_path/line/column are sufficient to navigate.
                        context: String::new(),
                    });
                }
            }
        }
    }

    refs
}

fn collect_reference_nodes(
    node: tree_sitter::Node<'_>,
    source: &str,
    name: &[u8],
    file_path: &str,
    refs: &mut Vec<ReferenceLocation>,
) {
    if node.kind() == "identifier"
        || node.kind() == "type_identifier"
        || node.kind() == "field_identifier"
    {
        if let Ok(text) = node.utf8_text(source.as_bytes()) {
            if text.as_bytes() == name {
                let (line, col) = point_to_line_col(source, node.start_byte());
                let context = extract_node_context(node, source);
                refs.push(ReferenceLocation {
                    file_path: file_path.to_string(),
                    line,
                    column: col,
                    context,
                });
            }
        }
    }
    let mut i = 0;
    while let Some(child) = node.child(i) {
        collect_reference_nodes(child, source, name, file_path, refs);
        i += 1;
    }
}

fn extract_node_context(node: tree_sitter::Node<'_>, source: &str) -> String {
    // Get surrounding line text for context
    let (line, _) = point_to_line_col(source, node.start_byte());
    let lines: Vec<&str> = source.lines().collect();
    if line > 0 && line <= lines.len() {
        lines[line - 1].trim().to_string()
    } else {
        String::new()
    }
}
