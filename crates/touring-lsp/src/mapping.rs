//! Pure, feature-independent mapping between LSP-shaped requests and the
//! Touring backend payloads (`cli_find_references` / `cli_rename`).
//!
//! This module is intentionally free of any `tower-lsp` dependency so that:
//!   1. the DEFAULT workspace build compiles it (no heavy deps pulled), and
//!   2. the request/response translation can be unit-tested *without* a live
//!      stdio LSP server.
//!
//! The actual async `LanguageServer` impl (gated behind `lsp-bridge`) is a thin
//! shell that (a) extracts LSP params into the structs here, (b) calls the
//! backend, and (c) re-inflates the structs here into `lsp_types`. All the
//! non-trivial logic lives here and is therefore covered by fast unit tests.

use serde::Deserialize;

/// A 0-based LSP position (line, character) — mirrors `lsp_types::Position`
/// without depending on it, so the default build stays lean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LspPosition {
    /// 0-based line.
    pub line: u32,
    /// 0-based UTF-16 character offset (LSP convention).
    pub character: u32,
}

/// A resolved location returned by the references backend.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BackendLocation {
    /// Workspace-relative (or absolute) file path the backend reported.
    pub file_path: String,
    /// 1-based line as emitted by the backend's tree-sitter walker.
    pub line: u32,
    /// 1-based column as emitted by the backend.
    pub column: u32,
}

/// Deserialized shape of `cli_find_references` output:
/// `{ "count": N, "locations": [ { file_path, line, column, .. } ] }`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReferencesBackendResponse {
    /// Total reference count reported by the backend.
    #[serde(default)]
    pub count: u64,
    /// Resolved reference locations.
    #[serde(default)]
    pub locations: Vec<BackendLocation>,
}

/// Deserialized shape of one rename edit inside `cli_rename`'s
/// `edits[<file>].edits[]` array.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RenameEdit {
    /// File the edit applies to.
    pub file_path: String,
    /// 1-based line of the identifier to replace.
    pub line: u32,
    /// 1-based column of the identifier to replace.
    pub column: u32,
}

/// Build the JSON payload for `cli_find_references` from an LSP references
/// request. The backend reads `{file, line, column, scope}` where `line`/
/// `column` are 1-based (its tree-sitter walker is 1-based), while LSP is
/// 0-based — so we add 1 on the way in.
///
/// `scope` is forced to `"workspace"` to surface the cross-file capability
/// that this whole crate exists to expose.
pub fn references_request_payload(file: &str, pos: LspPosition) -> serde_json::Value {
    serde_json::json!({
        "file": file,
        "line": pos.line as u64 + 1,
        "column": pos.character as u64 + 1,
        "scope": "workspace",
    })
}

/// Build the JSON payload for `cli_rename`. Same 0-based -> 1-based conversion
/// as references, plus the `new_name`, and `scope="workspace"` for cross-file
/// rename collection.
pub fn rename_request_payload(file: &str, pos: LspPosition, new_name: &str) -> serde_json::Value {
    serde_json::json!({
        "file": file,
        "line": pos.line as u64 + 1,
        "column": pos.character as u64 + 1,
        "new_name": new_name,
        "scope": "workspace",
    })
}

/// Parse the `cli_find_references` JSON string into the typed response.
/// Returns an empty response on parse failure or backend error envelope —
/// the LSP layer treats "no references" and "could not resolve" identically
/// (an empty `Vec<Location>`), which is the correct, fail-open LSP behavior.
pub fn parse_references_response(raw: &str) -> ReferencesBackendResponse {
    serde_json::from_str::<ReferencesBackendResponse>(raw).unwrap_or(ReferencesBackendResponse {
        count: 0,
        locations: Vec::new(),
    })
}

/// Convert a backend 1-based location into a 0-based [`LspPosition`].
/// Saturating subtraction guards against a backend `0` (defensive; the backend
/// is 1-based but we never want to underflow).
pub fn backend_location_to_lsp(loc: &BackendLocation) -> LspPosition {
    LspPosition {
        line: loc.line.saturating_sub(1),
        character: loc.column.saturating_sub(1),
    }
}

/// Extract the flat list of rename edit sites from a `cli_rename` response.
///
/// `cli_rename` returns `{ status, old_name, new_name, sites, edits }` where
/// `edits` is an object keyed by file path, each value `{ "edits": [ {
/// delete, insert, file_path, line, column } ] }`. We flatten every nested
/// edit into a [`RenameEdit`]. Only `status == "preview"` yields edits; any
/// `conflict`/`error` status yields an empty vec (the LSP layer then returns
/// `None`, signalling "rename not applicable").
pub fn parse_rename_edits(raw: &str) -> Vec<RenameEdit> {
    let value: serde_json::Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    if value.get("status").and_then(|s| s.as_str()) != Some("preview") {
        return Vec::new();
    }
    let mut out = Vec::new();
    if let Some(edits_obj) = value.get("edits").and_then(|e| e.as_object()) {
        for file_entry in edits_obj.values() {
            if let Some(arr) = file_entry.get("edits").and_then(|a| a.as_array()) {
                for edit in arr {
                    if let Ok(parsed) = serde_json::from_value::<RenameEdit>(edit.clone()) {
                        out.push(parsed);
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_payload_is_1based_workspace_scoped() {
        // LSP sends 0-based (line=9, char=4); backend expects 1-based (10, 5).
        let payload = references_request_payload(
            "src/foo.rs",
            LspPosition {
                line: 9,
                character: 4,
            },
        );
        assert_eq!(payload["file"], "src/foo.rs");
        assert_eq!(payload["line"], 10);
        assert_eq!(payload["column"], 5);
        assert_eq!(payload["scope"], "workspace");
    }

    #[test]
    fn rename_payload_carries_new_name_and_scope() {
        let payload = rename_request_payload(
            "src/bar.rs",
            LspPosition {
                line: 0,
                character: 0,
            },
            "renamed",
        );
        assert_eq!(payload["file"], "src/bar.rs");
        assert_eq!(payload["line"], 1);
        assert_eq!(payload["column"], 1);
        assert_eq!(payload["new_name"], "renamed");
        assert_eq!(payload["scope"], "workspace");
    }

    #[test]
    fn parse_references_roundtrip() {
        // Exactly the shape cli_find_references emits (cli_handlers_semantics.rs).
        let raw = r#"{"count":2,"locations":[
            {"file_path":"src/foo.rs","line":10,"column":5,"context":"bar();"},
            {"file_path":"src/bar.rs","line":8,"column":1,"context":"foo::bar()"}
        ]}"#;
        let resp = parse_references_response(raw);
        assert_eq!(resp.count, 2);
        assert_eq!(resp.locations.len(), 2);
        assert_eq!(resp.locations[1].file_path, "src/bar.rs");
        // 1-based (8,1) -> 0-based (7,0).
        let pos = backend_location_to_lsp(&resp.locations[1]);
        assert_eq!(
            pos,
            LspPosition {
                line: 7,
                character: 0
            }
        );
    }

    #[test]
    fn parse_references_handles_error_envelope() {
        // cli_find_references can return {"error": "..."} — must degrade to empty.
        let resp = parse_references_response(r#"{"error":"cannot read 'x': nope"}"#);
        assert_eq!(resp.count, 0);
        assert!(resp.locations.is_empty());
    }

    #[test]
    fn parse_references_handles_no_definition() {
        let resp =
            parse_references_response(r#"{"count":0,"locations":[],"note":"no definition found"}"#);
        assert_eq!(resp.count, 0);
        assert!(resp.locations.is_empty());
    }

    #[test]
    fn parse_rename_edits_flattens_preview() {
        // Exact cli_rename preview shape: edits keyed by file, nested edits[].
        let raw = r#"{
            "status":"preview","old_name":"foo","new_name":"baz","sites":2,
            "edits":{
                "src/foo.rs":{"edits":[
                    {"delete":{"start":100,"end":103},"insert":"baz","file_path":"src/foo.rs","line":10,"column":5},
                    {"delete":{"start":200,"end":203},"insert":"baz","file_path":"src/bar.rs","line":8,"column":1}
                ]}
            }
        }"#;
        let edits = parse_rename_edits(raw);
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0].file_path, "src/foo.rs");
        assert_eq!(edits[1].line, 8);
    }

    #[test]
    fn parse_rename_edits_conflict_yields_none() {
        let raw = r#"{"status":"conflict","old_name":"foo","new_name":"baz","sites":0,"error":"'baz' already exists in scope"}"#;
        assert!(parse_rename_edits(raw).is_empty());
    }

    #[test]
    fn parse_rename_edits_malformed_yields_empty() {
        assert!(parse_rename_edits("not json").is_empty());
    }
}
