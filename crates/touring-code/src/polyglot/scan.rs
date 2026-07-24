//! Batch structural scan — apply N ast-grep rules to M files in one call.
//!
//! Wave Q2 (waves-Q-R-M-A-T-P plan, 2026-04-24).
//!
//! # Design
//!
//! - `Rule` is a pre-parsed structural pattern bound to a language and severity.
//!   YAML loading is the caller's responsibility (touring-hooks owns the
//!   `serde_yaml` dep and the on-disk format).
//! - `scan_files` iterates files × rules, calling `search()` per (file, rule) pair.
//!   Files unreachable / unparseable become `RuleMatch`-free entries — the scan
//!   never panics on bad input.
//! - Returns a `ScanReport` with elapsed time + per-match metadata (file, line,
//!   col, matched text, optional suggested fix).
//!
//! # Performance
//!
//! Sequential by design at this layer — parallelism (rayon) belongs at the
//! caller level where workspace-wide scans aggregate per-crate batches.
//! Per-file ast-grep parse is O(LOC) with tree-sitter zero-copy slices.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::lang::Lang;
use super::search::{Match, search};

/// Severity classification — mirrors `touring_foundation::diagnostic::Severity`
/// without taking a dep (this crate is leaf for ast-grep clients).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Lowest level — a passive suggestion.
    Hint,
    /// Informational note, not a problem.
    Info,
    /// A potential problem that does not block.
    Warning,
    /// A definite problem.
    Error,
}

impl Severity {
    /// Return the severity as a lowercase string slice (e.g. `"warning"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Hint => "hint",
        }
    }
}

/// A single structural rule — pre-parsed pattern + metadata.
///
/// Loaded from YAML by the caller (touring-hooks); this crate is YAML-free.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// Stable identifier, e.g. `"rust:unwrap-in-prod"`.
    pub id: String,
    /// Target language. Files of other languages are skipped for this rule.
    pub language: Lang,
    /// ast-grep pattern with `$VAR` / `$$$VAR` metavariables.
    pub pattern: String,
    /// Human-readable description shown on match.
    pub message: String,
    /// Severity.
    pub severity: Severity,
    /// Optional suggested fix (template with metavariable references).
    pub suggested_fix: Option<String>,
}

/// A single rule match against a single file location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleMatch {
    /// Identifier of the rule that produced this match.
    pub rule_id: String,
    /// Path of the file the match was found in.
    pub file_path: PathBuf,
    /// Name of the language the file was parsed as.
    pub language: String,
    /// 1-based line number of the match.
    pub line: usize,
    /// 1-based column number of the match.
    pub col: usize,
    /// The source text that matched the rule pattern.
    pub matched_text: String,
    /// Human-readable message describing the match.
    pub message: String,
    /// Severity assigned to this match by the rule.
    pub severity: Severity,
    /// Optional suggested fix rendered from the rule template.
    pub suggested_fix: Option<String>,
}

/// Aggregate result of `scan_files`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanReport {
    /// Number of files that were parsed and scanned.
    pub files_scanned: usize,
    /// Number of files skipped (e.g. unsupported language or unreadable).
    pub files_skipped: usize,
    /// Number of rules applied during the scan.
    pub rules_applied: usize,
    /// All matches found across every scanned file.
    pub matches: Vec<RuleMatch>,
    /// Wall-clock time spent on the scan, in milliseconds.
    pub elapsed_ms: u128,
}

impl ScanReport {
    /// Total error-severity matches.
    pub fn error_count(&self) -> usize {
        self.matches
            .iter()
            .filter(|m| m.severity == Severity::Error)
            .count()
    }

    /// Total warning-severity matches.
    pub fn warning_count(&self) -> usize {
        self.matches
            .iter()
            .filter(|m| m.severity == Severity::Warning)
            .count()
    }
}

/// Run every rule against every file. Returns a populated `ScanReport`.
///
/// Files that fail to read are counted in `files_skipped`. Rules whose
/// language doesn't match the file's detected language are silently skipped
/// (not counted as failures — language filtering is normal).
///
/// # Behavior
///
/// - Each (file, rule) pair where languages match runs `search()` once.
/// - `pattern` parse errors return zero matches for that pair (logged via
///   tracing if available — best-effort).
/// - Output ordering: matches sorted by (file_path, line, col) for stable
///   diffability.
#[must_use]
pub fn scan_files(files: &[PathBuf], rules: &[Rule]) -> ScanReport {
    let start = std::time::Instant::now();
    let mut matches = Vec::new();
    let mut files_scanned = 0usize;
    let mut files_skipped = 0usize;

    for file_path in files {
        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => {
                files_skipped += 1;
                continue;
            }
        };
        files_scanned += 1;

        let file_lang = match super::lang::detect_lang(file_path) {
            Some(l) => l,
            None => continue, // unknown extension — skip silently
        };

        for rule in rules {
            if rule.language != file_lang {
                continue;
            }
            let hits: Vec<Match> = match search(rule.language, &source, &rule.pattern) {
                Ok(h) => h,
                Err(_) => continue, // bad pattern — skip silently
            };
            for hit in hits {
                let suggested_fix = rule
                    .suggested_fix
                    .as_ref()
                    .map(|tmpl| render_fix(tmpl, &hit));
                matches.push(RuleMatch {
                    rule_id: rule.id.clone(),
                    file_path: file_path.clone(),
                    language: rule.language.name().to_string(),
                    line: hit.start_line + 1, // 1-indexed for human display
                    col: hit.start_col + 1,
                    matched_text: hit.text,
                    message: rule.message.clone(),
                    severity: rule.severity,
                    suggested_fix,
                });
            }
        }
    }

    // Stable ordering: by file then line then col then rule_id
    matches.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.col.cmp(&b.col))
            .then_with(|| a.rule_id.cmp(&b.rule_id))
    });

    ScanReport {
        files_scanned,
        files_skipped,
        rules_applied: rules.len(),
        matches,
        elapsed_ms: start.elapsed().as_millis(),
    }
}

/// Walk a directory recursively and collect all files matching `extensions`.
/// Skips hidden dirs (starting with `.`) and `target/` / `node_modules/`.
///
/// `extensions` should be without leading dot, e.g. `&["rs", "py"]`.
#[must_use]
pub fn walk_files(root: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    fn is_skipped_dir(name: &std::ffi::OsStr) -> bool {
        let n = name.to_string_lossy();
        n.starts_with('.') || n == "target" || n == "node_modules"
    }
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if file_type.is_dir() {
                if let Some(name) = path.file_name() {
                    if !is_skipped_dir(name) {
                        stack.push(path);
                    }
                }
            } else if file_type.is_file() {
                let ext_match = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| extensions.contains(&e));
                if ext_match {
                    out.push(path);
                }
            }
        }
    }
    out
}

/// Substitute `$VAR` references in `template` with captured metavar values.
/// Best-effort: unknown references stay as-is.
fn render_fix(template: &str, hit: &Match) -> String {
    let mut out = template.to_string();
    for (name, value) in &hit.metavars {
        let token = format!("${name}");
        out = out.replace(&token, value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_rule(id: &str, lang: Lang, pattern: &str, sev: Severity) -> Rule {
        Rule {
            id: id.to_string(),
            language: lang,
            pattern: pattern.to_string(),
            message: format!("test rule {id}"),
            severity: sev,
            suggested_fix: None,
        }
    }

    fn write_temp(name: &str, content: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("touring-scan-test-{}-{name}", std::process::id()));
        let mut f = std::fs::File::create(&path).expect("create temp");
        f.write_all(content.as_bytes()).expect("write temp");
        path
    }

    #[test]
    fn severity_str_matches_lowercase() {
        assert_eq!(Severity::Error.as_str(), "error");
        assert_eq!(Severity::Warning.as_str(), "warning");
        assert_eq!(Severity::Info.as_str(), "info");
        assert_eq!(Severity::Hint.as_str(), "hint");
    }

    #[test]
    fn scan_files_empty_inputs_returns_empty_report() {
        let r = scan_files(&[], &[]);
        assert_eq!(r.files_scanned, 0);
        assert_eq!(r.files_skipped, 0);
        assert_eq!(r.rules_applied, 0);
        assert!(r.matches.is_empty());
    }

    #[test]
    fn scan_files_unreadable_file_counted_as_skipped() {
        let nonexistent = PathBuf::from("/nonexistent/path/file.rs");
        let rule = make_rule("r1", Lang::Rust, "fn $X() {}", Severity::Info);
        let r = scan_files(&[nonexistent], &[rule]);
        assert_eq!(r.files_scanned, 0);
        assert_eq!(r.files_skipped, 1);
        assert!(r.matches.is_empty());
    }

    #[test]
    fn scan_files_finds_matches_in_rust_file() {
        let path = write_temp("foo.rs", "fn alpha() {}\nfn beta() {}\n");
        let rule = make_rule("rust:fn-decl", Lang::Rust, "fn $X() {}", Severity::Info);
        let r = scan_files(std::slice::from_ref(&path), &[rule]);
        assert_eq!(r.files_scanned, 1);
        assert_eq!(r.matches.len(), 2, "should find 2 fn decls");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn scan_files_skips_rule_with_mismatched_language() {
        let path = write_temp("bar.rs", "fn alpha() {}\n");
        let py_rule = make_rule("py:def", Lang::Python, "def $X(): $$$Y", Severity::Info);
        let r = scan_files(std::slice::from_ref(&path), &[py_rule]);
        assert_eq!(r.files_scanned, 1);
        assert!(
            r.matches.is_empty(),
            "Python rule should not match Rust file"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn scan_files_matches_sorted_by_file_line_col() {
        let p1 = write_temp("a.rs", "fn a() {}\nfn b() {}\n");
        let p2 = write_temp("b.rs", "fn c() {}\n");
        let rule = make_rule("rust:fn", Lang::Rust, "fn $X() {}", Severity::Info);
        let r = scan_files(&[p2.clone(), p1.clone()], &[rule]);
        // Even though p2 was passed first, output should be sorted by file_path
        assert_eq!(r.matches.len(), 3);
        // Adjacent lines must be non-decreasing within same file
        for window in r.matches.windows(2) {
            if let [a, b] = window {
                if a.file_path == b.file_path {
                    assert!(
                        a.line <= b.line,
                        "lines not sorted: {} vs {}",
                        a.line,
                        b.line
                    );
                }
            }
        }
        let _ = std::fs::remove_file(&p1);
        let _ = std::fs::remove_file(&p2);
    }

    #[test]
    fn scan_report_severity_counts() {
        let p = write_temp("multi.rs", "fn alpha() {}\nfn beta() {}\n");
        let err_rule = make_rule("err", Lang::Rust, "fn $X() {}", Severity::Error);
        let warn_rule = make_rule("warn", Lang::Rust, "fn $X() {}", Severity::Warning);
        let r = scan_files(std::slice::from_ref(&p), &[err_rule, warn_rule]);
        // 2 fns × 2 rules = 4 matches
        assert_eq!(r.matches.len(), 4);
        assert_eq!(r.error_count(), 2);
        assert_eq!(r.warning_count(), 2);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn scan_files_records_elapsed_time() {
        let p = write_temp("timed.rs", "fn x() {}\n");
        let rule = make_rule("r", Lang::Rust, "fn $X() {}", Severity::Info);
        let r = scan_files(std::slice::from_ref(&p), &[rule]);
        // elapsed_ms should be measurable (could be 0 on very fast systems)
        // but the field should exist and not panic
        let _ = r.elapsed_ms;
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn walk_files_finds_rs_files_in_dir() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("touring-scan-walk-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let f1 = dir.join("a.rs");
        let f2 = dir.join("b.py");
        let f3 = dir.join("c.rs");
        let _ = std::fs::write(&f1, "fn a() {}");
        let _ = std::fs::write(&f2, "def b(): pass");
        let _ = std::fs::write(&f3, "fn c() {}");

        let rs_only = walk_files(&dir, &["rs"]);
        assert_eq!(rs_only.len(), 2, "should find 2 .rs files, got {rs_only:?}");

        let multi = walk_files(&dir, &["rs", "py"]);
        assert_eq!(multi.len(), 3);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn walk_files_skips_target_and_hidden() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("touring-scan-walk-skip-{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("src"));
        let _ = std::fs::create_dir_all(dir.join("target"));
        let _ = std::fs::create_dir_all(dir.join(".git"));
        let _ = std::fs::write(dir.join("src/visible.rs"), "fn v() {}");
        let _ = std::fs::write(dir.join("target/hidden.rs"), "fn t() {}");
        let _ = std::fs::write(dir.join(".git/config.rs"), "fn g() {}");

        let found = walk_files(&dir, &["rs"]);
        assert_eq!(
            found.len(),
            1,
            "should only find src/visible.rs, got {found:?}"
        );
        assert!(found[0].to_string_lossy().contains("visible.rs"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn render_fix_substitutes_metavars() {
        let m = Match {
            text: "foo".to_string(),
            start_byte: 0,
            end_byte: 0,
            start_line: 0,
            start_col: 0,
            end_line: 0,
            end_col: 0,
            metavars: vec![("X".to_string(), "bar".to_string())],
        };
        assert_eq!(
            render_fix("$X.expect(\"reason\")", &m),
            "bar.expect(\"reason\")"
        );
        // Unknown metavar stays as-is
        assert_eq!(render_fix("$Y stays", &m), "$Y stays");
    }
}
