//! CLI SSR (structural search & replace) handlers (`cli_ssr_*`) — extracted from cli_handlers.rs (A-W2.P4).
//!
//! Wraps `touring_code::ast::ssr` prebuilt-rule introspection and rule application.

use crate::runtime::HookRuntime;

/// Handler: `touring ssr status` — returns prebuilt rule count + supported languages.
pub fn cli_ssr_status(_rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let rules = touring_code::ast::ssr::prebuilt_rules();
    let status = serde_json::json!(
        { "prebuilt_rules" : rules.len(), "supported_languages" : ["rust", "python",
        "javascript", "typescript", "go", "java", "bash"] }
    );
    serde_json::to_string(&status).unwrap_or_default()
}
/// Handler: `touring ssr apply` — applies an SSR rule to stdin source.
///
/// Payload:
/// ```json
/// {
///   "pattern": "console.log($X)",
///   "replacement": "logger.info($X)",
///   "lang": "javascript",
///   "stdin": "console.log('hello');"
/// }
/// ```
pub fn cli_ssr_apply(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let pattern = payload
        .get("pattern")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let replacement = payload
        .get("replacement")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let lang = payload
        .get("lang")
        .and_then(|v| v.as_str())
        .unwrap_or("rust");
    let stdin_src = payload.get("stdin").and_then(|v| v.as_str()).unwrap_or("");
    if pattern.is_empty() {
        return serde_json::json!({ "error" : "missing 'pattern' field" }).to_string();
    }
    if replacement.is_empty() {
        return serde_json::json!({ "error" : "missing 'replacement' field" }).to_string();
    }
    if stdin_src.is_empty() {
        return serde_json::json!({ "error" : "missing 'stdin' field" }).to_string();
    }
    let rule = touring_code::ast::ssr::SsrRule {
        id: "cli-rule".to_string(),
        lang: lang.to_string(),
        pattern: pattern.to_string(),
        replacement: replacement.to_string(),
        file_path: None,
    };
    let lang_str = &rule.lang;
    let ast_lang = match touring_code::ast::Lang::from_path(std::path::Path::new(lang_str)) {
        Some(l) => l,
        None => {
            return serde_json::json!(
                { "error" : format!("unsupported language: {lang_str}") }
            )
            .to_string();
        }
    };
    match touring_code::ast::ssr::apply_ssr_rule(&rule, stdin_src, ast_lang) {
        Ok(result) => serde_json::json!(
            { "rule_id" : result.rule_id, "file_path" : result.file_path, "matches" :
            result.matches, "was_formatted" : result.was_formatted }
        )
        .to_string(),
        Err(e) => serde_json::json!({ "error" : e.to_string() }).to_string(),
    }
}
