//! `touring scan-pii` — PII detection hook (PreToolUse Write).

use super::read_stdin_safe;

/// Known tool_input field names that may contain user-authored text.
pub(crate) const SCANNABLE_FIELDS: &[&str] = &[
    "content",
    "command",
    "file_path",
    "new_string",
    "old_string",
];

/// Extract scannable text fields from a tool_input JSON object.
pub(crate) fn extract_texts_to_scan(data: &serde_json::Value) -> Vec<&str> {
    let tool_input = match data.get("tool_input").and_then(|v| v.as_object()) {
        Some(obj) => obj,
        None => return Vec::new(),
    };

    let mut texts = Vec::new();
    for key in SCANNABLE_FIELDS {
        if let Some(s) = tool_input.get(*key).and_then(|v| v.as_str())
            && !s.is_empty()
        {
            texts.push(s);
        }
    }
    texts
}

/// Entry point for the `touring scan-pii` CLI handler — reads the `PreToolUse`
/// payload from stdin, extracts scannable text fields, runs the [`PIIScanner`]
/// over them, and (when any findings exist) prints a `PreToolUse`
/// `additionalContext` warning reporting the finding count.
///
/// [`PIIScanner`]: crate::hooks::PIIScanner
pub fn run() -> anyhow::Result<()> {
    let data = read_stdin_safe();
    let texts_to_scan = extract_texts_to_scan(&data);

    if texts_to_scan.is_empty() {
        return Ok(());
    }

    let scanner = crate::hooks::PIIScanner::new();
    let mut all_findings = Vec::new();
    for text in &texts_to_scan {
        all_findings.extend(scanner.scan_text(text));
    }

    if all_findings.is_empty() {
        return Ok(());
    }

    let output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "additionalContext": format!("⚠️ PII detected: {} finding(s)", all_findings.len()),
        }
    });
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_texts_empty_when_no_tool_input() {
        let data = serde_json::json!({});
        assert!(extract_texts_to_scan(&data).is_empty());
    }

    #[test]
    fn extract_texts_empty_when_tool_input_is_null() {
        let data = serde_json::json!({"tool_input": null});
        assert!(extract_texts_to_scan(&data).is_empty());
    }

    #[test]
    fn extract_texts_empty_when_tool_input_has_no_known_fields() {
        let data = serde_json::json!({"tool_input": {"unknown_field": "value"}});
        assert!(extract_texts_to_scan(&data).is_empty());
    }

    #[test]
    fn extract_texts_skips_empty_strings() {
        let data = serde_json::json!({"tool_input": {"content": "", "command": "ls"}});
        let texts = extract_texts_to_scan(&data);
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0], "ls");
    }

    #[test]
    fn extract_texts_collects_all_non_empty_fields() {
        let data = serde_json::json!({
            "tool_input": {
                "content": "hello world",
                "command": "echo test",
                "file_path": "/tmp/foo.txt",
                "new_string": "replacement",
                "old_string": "original"
            }
        });
        let texts = extract_texts_to_scan(&data);
        assert_eq!(texts.len(), 5);
    }

    #[test]
    fn extract_texts_ignores_non_string_values() {
        let data = serde_json::json!({
            "tool_input": {
                "content": 42,
                "command": true,
                "file_path": "/real/path"
            }
        });
        let texts = extract_texts_to_scan(&data);
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0], "/real/path");
    }

    #[test]
    fn scannable_fields_has_five_entries() {
        assert_eq!(SCANNABLE_FIELDS.len(), 5);
        assert!(SCANNABLE_FIELDS.contains(&"content"));
        assert!(SCANNABLE_FIELDS.contains(&"command"));
        assert!(SCANNABLE_FIELDS.contains(&"file_path"));
        assert!(SCANNABLE_FIELDS.contains(&"new_string"));
        assert!(SCANNABLE_FIELDS.contains(&"old_string"));
    }
}
