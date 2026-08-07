//! CLI skip-region handlers (`cli_skip_*`) — extracted from cli_handlers.rs (A-W2.P4).
//!
//! Self-contained skip-region parser (mirrors `post_edit::parse_skip_regions`)
//! to avoid a circular dependency on `touring-generator`. The `SkipRegionRaw`
//! struct stays in cli_handlers.rs (promoted `pub(crate)`) and is imported.

use crate::cli_handlers::SkipRegionRaw;
use crate::runtime::HookRuntime;

/// Outcome of resolving the `file_path` payload field and reading that file —
/// the prelude both `cli_skip_*` handlers share.
///
/// The failure travels as **data** rather than a pre-serialized envelope
/// because the two handlers report it differently on purpose: `cli_skip_list`
/// answers with a bare `error`, while `cli_skip_validate` answers with a
/// `valid: false` verdict that still names the file.
enum PayloadSource<'a> {
    Loaded { file_path: &'a str, source: String },
    MissingPath,
    Unreadable {
        file_path: &'a str,
        error: std::io::Error,
    },
}

/// Resolve `payload.file_path` and read it. See [`PayloadSource`].
fn load_payload_source(payload: &serde_json::Value) -> PayloadSource<'_> {
    let file_path = payload
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if file_path.is_empty() {
        return PayloadSource::MissingPath;
    }
    match std::fs::read_to_string(file_path) {
        Ok(source) => PayloadSource::Loaded { file_path, source },
        Err(error) => PayloadSource::Unreadable { file_path, error },
    }
}

/// Parse skip regions from a file and return them as JSON.
///
/// Payload: `{"file_path": "..."}` — relative or absolute path.
/// Uses the same self-contained parser as `post_edit::parse_skip_regions`
/// to avoid a circular dependency on `touring-generator`.
pub fn cli_skip_list(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let (file_path, source) = match load_payload_source(payload) {
        PayloadSource::Loaded { file_path, source } => (file_path, source),
        PayloadSource::MissingPath => {
            return serde_json::json!({ "error" : "missing 'file_path' field" }).to_string();
        }
        PayloadSource::Unreadable { error, .. } => {
            return serde_json::json!({ "error" : format!("read error: {}", error) }).to_string();
        }
    };
    let regions = parse_skip_regions_raw(&source);
    serde_json::json!(
        { "file" : file_path, "region_count" : regions.len(), "regions" : regions, }
    )
    .to_string()
}
/// Validate whether a file can be parsed for skip regions.
///
/// Payload: `{"file_path": "..."}` — returns validation result as JSON.
pub fn cli_skip_validate(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let (file_path, source) = match load_payload_source(payload) {
        PayloadSource::Loaded { file_path, source } => (file_path, source),
        PayloadSource::MissingPath => {
            return serde_json::json!({ "error" : "missing 'file_path' field" }).to_string();
        }
        PayloadSource::Unreadable { file_path, error } => {
            return serde_json::json!(
                { "file" : file_path, "valid" : false, "error" :
                format!("read error: {}", error) }
            )
            .to_string();
        }
    };
    let is_rust = file_path.ends_with(".rs")
        || source.lines().take(10).any(|l| {
            let t = l.trim();
            t.starts_with("use ")
                || t.starts_with("fn ")
                || t.starts_with("struct ")
                || t.starts_with("enum ")
                || t.starts_with("mod ")
                || t.starts_with("impl ")
                || t.starts_with("pub ")
                || t.starts_with("//!")
                || t.starts_with("/*")
        });
    let regions = parse_skip_regions_raw(&source);
    serde_json::json!(
        { "file" : file_path, "valid" : is_rust, "is_rust_file" : file_path
        .ends_with(".rs"), "has_skip_regions" : ! regions.is_empty(), "region_count" :
        regions.len(), }
    )
    .to_string()
}
fn parse_skip_regions_raw(source: &str) -> Vec<SkipRegionRaw> {
    let mut regions = Vec::new();
    let mut in_region = false;
    let mut region_start: Option<u64> = None;
    let mut line_cursor: u64 = 0;
    for line in source.lines() {
        let line_start = line_cursor;
        let line_end = line_cursor + line.len() as u64 + 1;
        let trimmed = line.trim();
        if trimmed.starts_with('#')
            && (trimmed.contains("touring::skip") || trimmed.contains("touring(skip)"))
        {
            regions.push(SkipRegionRaw {
                start: line_start,
                end: line_end,
                style: "RustAttribute",
            });
            line_cursor = line_end;
            continue;
        }
        if trimmed.starts_with("//")
            && trimmed.contains("touring:skip-region")
            && !trimmed.contains("touring:skip-end")
        {
            region_start = Some(line_end);
            in_region = true;
        } else if trimmed.starts_with("//") && trimmed.contains("touring:skip-end") && in_region {
            if let Some(start) = region_start.take() {
                regions.push(SkipRegionRaw {
                    start,
                    end: line_start,
                    style: "LineComment",
                });
            }
            in_region = false;
        }
        line_cursor = line_end;
    }
    regions
}
