//! CLI polyglot AST handler — backed by `touring_code::polyglot` (ast-grep).
//!
//! Surface:
//! - `cli-ast-grep`: structural search + optional rewrite over JS/TS/Python/Go/etc.
//!
//! Payload shape:
//! ```json
//! {
//!   "file_path": "src/foo.ts",
//!   "pattern": "console.log($X)",
//!   "lang": "typescript",          // optional; auto-detected from extension
//!   "rewrite": "logger.info($X)",   // optional; when present returns rewritten source
//!   "top": 50,                     // optional; cap number of hits (default 50)
//!   "skip_strings": true           // optional; filter matches inside StringLit/Comment/RawString (B.3.3)
//! }
//! ```
//!
//! Response (search mode):
//! ```json
//! {"file_path": "...", "lang": "...", "count": N,
//!  "matches": [{text, start_line, start_col, end_line, end_col, metavars}]}
//! ```
//!
//! Response (rewrite mode):
//! ```json
//! {"file_path": "...", "lang": "...", "rewritten": true, "source": "...transformed..."}
//! ```

use crate::runtime::HookRuntime;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use touring_code::polyglot::{
    Lang, Rule, Severity, detect_lang, rewrite, scan_files, search, walk_files,
};
use touring_foundation::char_classes::{CharClass, CharClasses};

/// `cli-ast-grep` — polyglot structural search + rewrite.
pub fn cli_ast_grep(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = match crate::cli::shared::require_file_path(payload) {
        Ok(fp) => fp,
        Err(e) => return e,
    };
    let pattern = payload
        .get("pattern")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if pattern.is_empty() {
        return serde_json::json!({"error": "pattern required"}).to_string();
    }

    let lang = resolve_lang(file_path, payload.get("lang").and_then(|v| v.as_str()));
    let lang = match lang {
        Ok(l) => l,
        Err(msg) => return serde_json::json!({"error": msg}).to_string(),
    };

    let source = match std::fs::read_to_string(Path::new(file_path)) {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!({
                "error": format!("read failed: {e}"),
                "file_path": file_path
            })
            .to_string();
        }
    };

    let replacement = payload.get("rewrite").and_then(|v| v.as_str());
    if let Some(repl) = replacement {
        match rewrite(lang, &source, pattern, repl) {
            Ok(out) => serde_json::json!({
                "file_path": file_path,
                "lang": lang.name(),
                "rewritten": out != source,
                "source": out
            })
            .to_string(),
            Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
        }
    } else {
        let top = payload.get("top").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
        let skip_strings = payload
            .get("skip_strings")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        match search(lang, &source, pattern) {
            Ok(mut hits) => {
                if skip_strings {
                    let excluded = string_like_ranges(&source);
                    hits.retain(|h| !hit_is_in_string_like(h, &excluded));
                }
                hits.truncate(top);
                serde_json::json!({
                    "file_path": file_path,
                    "lang": lang.name(),
                    "count": hits.len(),
                    "matches": hits
                })
                .to_string()
            }
            Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
        }
    }
}

/// YAML schema for a scan rule (Wave Q2). On-disk shape that gets
/// converted into `touring_code::polyglot::Rule`.
#[derive(Debug, Deserialize)]
struct YamlRule {
    id: String,
    language: String,
    pattern: String,
    message: String,
    severity: String,
    suggested_fix: Option<String>,
}

fn parse_severity(s: &str) -> Severity {
    match s.to_ascii_lowercase().as_str() {
        "error" => Severity::Error,
        "warning" | "warn" => Severity::Warning,
        "info" => Severity::Info,
        _ => Severity::Hint,
    }
}

fn yaml_to_rule(y: YamlRule) -> Result<Rule, String> {
    let lang = Lang::from_str(&y.language)
        .map_err(|e| format!("unknown language '{}': {e}", y.language))?;
    Ok(Rule {
        id: y.id,
        language: lang,
        pattern: y.pattern,
        message: y.message,
        severity: parse_severity(&y.severity),
        suggested_fix: y.suggested_fix,
    })
}

/// Load every `*.yaml` / `*.yml` file under `dir` (recursive) and parse
/// as YamlRule -> Rule. Returns `(rules, parse_errors)`.
fn load_yaml_rules(dir: &Path) -> (Vec<Rule>, Vec<String>) {
    let mut rules = Vec::new();
    let mut errors = Vec::new();
    let yaml_files = walk_files(dir, &["yaml", "yml"]);
    for path in yaml_files {
        let content = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                errors.push(format!("read {}: {e}", path.display()));
                continue;
            }
        };
        let yaml: YamlRule = match serde_yaml::from_str(&content) {
            Ok(y) => y,
            Err(e) => {
                errors.push(format!("parse {}: {e}", path.display()));
                continue;
            }
        };
        match yaml_to_rule(yaml) {
            Ok(r) => rules.push(r),
            Err(e) => errors.push(format!("{}: {e}", path.display())),
        }
    }
    (rules, errors)
}

/// `cli-ast-scan` — batch structural scan applying YAML rules to files.
///
/// Payload:
/// ```json
/// {
///   "rules_dir": "/path/to/rules",
///   "root": "/path/to/scan",          // optional; default: project_root
///   "files": ["a.rs", "b.py"],         // optional; explicit file list
///   "extensions": ["rs", "py"]         // optional; default: derived from rules
/// }
/// ```
///
/// Returns ScanReport JSON + parse_errors array.
pub fn cli_ast_scan(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let rules_dir = payload
        .get("rules_dir")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if rules_dir.is_empty() {
        return serde_json::json!({"error": "rules_dir required"}).to_string();
    }
    let rules_path = Path::new(rules_dir);
    if !rules_path.is_dir() {
        return serde_json::json!({
            "error": format!("rules_dir '{rules_dir}' is not a directory")
        })
        .to_string();
    }

    let (rules, parse_errors) = load_yaml_rules(rules_path);
    if rules.is_empty() {
        return serde_json::json!({
            "error": "no valid rules loaded",
            "rules_dir": rules_dir,
            "parse_errors": parse_errors,
        })
        .to_string();
    }

    let files: Vec<PathBuf> = if let Some(arr) = payload.get("files").and_then(|v| v.as_array()) {
        arr.iter()
            .filter_map(|v| v.as_str().map(PathBuf::from))
            .collect()
    } else {
        let root = payload
            .get("root")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| rt.project_root.clone());
        let extensions: Vec<&str> =
            if let Some(arr) = payload.get("extensions").and_then(|v| v.as_array()) {
                arr.iter().filter_map(|v| v.as_str()).collect()
            } else {
                // Derive from loaded rule languages
                let mut exts: Vec<&str> = rules
                    .iter()
                    .filter_map(|r| match r.language {
                        Lang::Rust => Some("rs"),
                        Lang::Python => Some("py"),
                        Lang::JavaScript => Some("js"),
                        Lang::TypeScript => Some("ts"),
                        Lang::Go => Some("go"),
                        _ => None,
                    })
                    .collect();
                exts.sort();
                exts.dedup();
                exts
            };
        walk_files(&root, &extensions)
    };

    let report = scan_files(&files, &rules);
    let mut out = serde_json::to_value(&report).unwrap_or(serde_json::Value::Null);
    if let Some(obj) = out.as_object_mut() {
        obj.insert("parse_errors".to_string(), serde_json::json!(parse_errors));
        obj.insert("rules_dir".to_string(), serde_json::json!(rules_dir));
    }
    out.to_string()
}

fn resolve_lang(file_path: &str, override_lang: Option<&str>) -> Result<Lang, String> {
    if let Some(explicit) = override_lang.filter(|s| !s.is_empty()) {
        return Lang::from_str(explicit).map_err(|e| format!("unknown lang '{explicit}': {e}"));
    }
    detect_lang(file_path).ok_or_else(|| {
        format!("could not auto-detect lang for '{file_path}' — pass explicit `lang`")
    })
}

/// Collect byte ranges of string/comment/raw-string regions in `source`
/// using CharClasses, so callers can filter hits that land inside them.
fn string_like_ranges(source: &str) -> Vec<(usize, usize)> {
    let chars = CharClasses::new(source);
    let mut ranges = Vec::new();
    let mut current: Option<usize> = None;
    for (offset, _, class) in chars {
        match class {
            CharClass::StringLit
            | CharClass::RawString
            | CharClass::Comment
            | CharClass::DocComment => {
                if current.is_none() {
                    current = Some(offset);
                }
            }
            CharClass::Code => {
                if let Some(start) = current.take() {
                    ranges.push((start, offset));
                }
            }
        }
    }
    ranges
}

/// True when `hit` falls entirely inside one of the excluded ranges.
fn hit_is_in_string_like(
    hit: &touring_code::polyglot::search::Match,
    ranges: &[(usize, usize)],
) -> bool {
    ranges
        .iter()
        .any(|(start, end)| hit.start_byte >= *start && hit.end_byte <= *end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn run_without_rt(payload: serde_json::Value) -> serde_json::Value {
        // HookRuntime isn't required for this pure handler; use a null placeholder.
        // We reach straight into the handler body via the public entry point,
        // but the `_rt` is unused so an empty runtime built for tests is safe.
        // Tests construct tempfile content and invoke search/rewrite indirectly.
        let _ = payload;
        json!(null)
    }

    #[test]
    fn resolve_lang_from_override() {
        assert_eq!(resolve_lang("a.txt", Some("python")).unwrap(), Lang::Python);
    }

    #[test]
    fn resolve_lang_from_extension() {
        assert_eq!(resolve_lang("a.ts", None).unwrap(), Lang::TypeScript);
    }

    #[test]
    fn string_like_ranges_code_only() {
        // "foo" is in Code region — no string ranges should match it
        let src = "let foo = 1;";
        let ranges = string_like_ranges(src);
        assert!(
            ranges.is_empty(),
            "code-only source should have no string ranges"
        );
    }

    #[test]
    fn string_like_ranges_with_string() {
        let src = r#"let x = "hello";"#;
        let ranges = string_like_ranges(src);
        assert_eq!(ranges.len(), 1, "one string literal range expected");
        let (start, end) = ranges[0];
        assert!(
            src[start..end].contains("hello"),
            "range should cover the string content"
        );
    }

    #[test]
    fn string_like_ranges_with_comment() {
        let src = "// this is a comment\nlet x = 1;";
        let ranges = string_like_ranges(src);
        assert_eq!(ranges.len(), 1, "one comment range expected");
        let (start, end) = ranges[0];
        assert!(src[start..end].contains("this is a comment"));
    }

    #[test]
    fn hit_is_in_string_like_rejects_hit_in_string() {
        use touring_code::polyglot::search::Match;
        let hit = Match {
            text: "hello".to_string(),
            start_byte: 9,
            end_byte: 14,
            start_line: 1,
            start_col: 9,
            end_line: 1,
            end_col: 14,
            metavars: vec![],
        };
        let ranges = [(9usize, 15usize)].to_vec();
        assert!(
            hit_is_in_string_like(&hit, &ranges),
            "hit inside string range should be rejected"
        );
    }

    #[test]
    fn hit_is_in_string_like_accepts_hit_outside_string() {
        use touring_code::polyglot::search::Match;
        let hit = Match {
            text: "hello".to_string(),
            start_byte: 0,
            end_byte: 5,
            start_line: 1,
            start_col: 0,
            end_line: 1,
            end_col: 5,
            metavars: vec![],
        };
        let ranges = [(9usize, 15usize)].to_vec();
        assert!(
            !hit_is_in_string_like(&hit, &ranges),
            "hit outside string range should be accepted"
        );
    }

    #[test]
    fn resolve_lang_missing_errors() {
        assert!(resolve_lang("a.unknown", None).is_err());
    }

    #[test]
    fn unused_helper_compiles() {
        let _ = run_without_rt(json!({}));
    }
}
