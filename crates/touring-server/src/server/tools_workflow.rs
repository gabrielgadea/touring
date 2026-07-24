//! Master **workflow** tools (coupling backlog — master tools).
//!
//! A workflow tool fans a *single* MCP call across several Touring detection
//! engines and returns one consolidated, ranked report — collapsing a manual
//! chain (vuln-scan → quality-gate → …) into N→1. This is the agentic
//! "orchestrate a workflow" pattern: fewer round-trips, one coherent verdict.
//!
//! The first such tool is [`TouringServer::touring_audit`] —
//! *failure / error / gap / problem identification*. It orchestrates:
//!
//! 1. the **offensive** CWE/OWASP pattern engine
//!    ([`touring_offensive::vuln::PatternRegistry`], 10 detectors — SQLi, XSS,
//!    command injection, path traversal, SSRF, deserialization, integer/buffer
//!    overflow, LDAP & XML injection); and
//! 2. the **6 P0 BLOCK** quality dimensions ([`touring_quality::score_target`]
//!    on F2.1 OWASP, F2.4 secrets, F2.5 dependency CVEs, F2.6 config security,
//!    F4.3 deprecated APIs, F4.5 package management).
//!
//! Both layers are stateless (regex over file content / file-scoped scoring),
//! so the whole tool is pure-in-process and unit-testable with zero daemon
//! coupling. The pure entry point is [`run_audit`]; the `#[tool]` method is a
//! thin MCP adapter over it.

use super::*;
use serde::Serialize;
use std::path::Path;
use std::sync::OnceLock;
use touring_offensive::vuln::PatternRegistry;
use touring_quality::{DimId, OutputFormat, score_target};

/// The six P0 BLOCK quality dimensions (fail-closed security/maintenance gates).
const P0_BLOCK_DIMS: [DimId; 6] = [
    DimId::F2_1, // OWASP Top 10
    DimId::F2_4, // crypto / secrets
    DimId::F2_5, // dependency CVEs
    DimId::F2_6, // config security
    DimId::F4_3, // deprecated APIs
    DimId::F4_5, // package management
];

/// Severity below which an offensive `VulnMatch` is reported as a warning
/// rather than a hard block (CVSS-style: 7.0+ ≈ high/critical).
const VULN_BLOCK_THRESHOLD: f32 = 7.0;

/// Shared, lazily-built CWE pattern registry (10 detectors). `PatternRegistry`
/// holds `Box<dyn VulnerabilityPattern + Send + Sync>`, so a process-wide
/// `OnceLock` avoids rebuilding 10 regexes on every audit call.
fn vuln_registry() -> &'static PatternRegistry {
    static REGISTRY: OnceLock<PatternRegistry> = OnceLock::new();
    REGISTRY.get_or_init(PatternRegistry::all)
}

/// Severity bucket for an audit finding — drives ranking and the overall verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditSeverity {
    /// Must be fixed before edit/merge (P0 gap or high-severity vulnerability).
    Block,
    /// Should be reviewed (lower-severity finding or non-P0 warning).
    Warn,
    /// Informational — no problem surfaced.
    Info,
}

impl AuditSeverity {
    /// Rank for sorting (lower = more severe → sorts first).
    fn rank(self) -> u8 {
        match self {
            Self::Block => 0,
            Self::Warn => 1,
            Self::Info => 2,
        }
    }
}

/// A single problem surfaced by one of the detection layers.
#[derive(Debug, Clone, Serialize)]
pub struct AuditFinding {
    /// Which engine found it: `"vuln"` (offensive) or `"quality"`.
    pub layer: &'static str,
    /// Severity bucket.
    pub severity: AuditSeverity,
    /// Machine-readable kind — pattern name (`"SQLi"`) or dim id (`"F2.4"`).
    pub kind: String,
    /// CWE identifier when known (offensive layer), else `0`.
    pub cwe_id: u32,
    /// 1-based line in the file (best effort), `0` when not line-scoped.
    pub line: usize,
    /// Human-readable description with remediation context.
    pub message: String,
}

/// Consolidated, ranked audit report across all requested layers.
#[derive(Debug, Clone, Serialize)]
pub struct AuditReport {
    /// Audited file path.
    pub file: String,
    /// Overall verdict (worst severity present).
    pub verdict: AuditSeverity,
    /// Count of `Block`-severity findings.
    pub block_count: usize,
    /// Count of `Warn`-severity findings.
    pub warn_count: usize,
    /// Count of `Info`-severity findings (always 0 unless a layer emits info).
    pub info_count: usize,
    /// Findings, ranked most-severe first.
    pub findings: Vec<AuditFinding>,
}

/// Which detection layers to run, parsed from the tool's `layers` parameter.
#[derive(Debug, Clone, Copy)]
pub struct AuditLayers {
    /// Run the offensive CWE/OWASP pattern engine.
    pub vuln: bool,
    /// Run the 6 P0 BLOCK quality dimensions.
    pub quality: bool,
}

impl AuditLayers {
    /// Parse the optional `layers` list. `None`, empty, or `["all"]` enables
    /// every layer; otherwise each named layer is opt-in.
    pub fn from_param(layers: &Option<Vec<String>>) -> Self {
        match layers {
            None => Self {
                vuln: true,
                quality: true,
            },
            Some(v) if v.is_empty() => Self {
                vuln: true,
                quality: true,
            },
            Some(v) => {
                let all = v.iter().any(|s| s.eq_ignore_ascii_case("all"));
                let has = |name: &str| all || v.iter().any(|s| s.eq_ignore_ascii_case(name));
                Self {
                    vuln: has("vuln"),
                    quality: has("quality"),
                }
            }
        }
    }
}

/// Convert a byte offset into a 1-based line number within `content`.
fn byte_to_line(content: &str, byte_off: usize) -> usize {
    if byte_off == 0 {
        return 1;
    }
    let upto = content
        .get(..byte_off.min(content.len()))
        .unwrap_or(content);
    upto.bytes().filter(|&b| b == b'\n').count() + 1
}

/// Offensive layer — run all CWE/OWASP detectors over the file content.
fn run_vuln_layer(content: &str) -> Vec<AuditFinding> {
    vuln_registry()
        .detect_all(content)
        .into_iter()
        .map(|m| {
            let line = byte_to_line(content, m.span.0);
            let severity = if m.severity >= VULN_BLOCK_THRESHOLD {
                AuditSeverity::Block
            } else {
                AuditSeverity::Warn
            };
            let message = format!(
                "{} vulnerability (CWE-{}, severity {:.1})",
                m.pattern_name, m.cwe_id, m.severity
            );
            AuditFinding {
                layer: "vuln",
                severity,
                kind: m.pattern_name,
                cwe_id: m.cwe_id,
                line,
                message,
            }
        })
        .collect()
}

/// Quality layer — score the file on the 6 P0 BLOCK dims and surface any that
/// blocked or warned. Scoring failure is graceful (returns no findings rather
/// than erroring the whole audit).
fn run_quality_layer(path: &Path) -> Vec<AuditFinding> {
    let Ok(report) = score_target(path, &P0_BLOCK_DIMS, OutputFormat::Json) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (dim, score) in &report.dimensions {
        let is_block = report.blockers.contains(dim);
        let is_warn = report.warnings.contains(dim);
        if !is_block && !is_warn {
            continue;
        }
        out.push(AuditFinding {
            layer: "quality",
            severity: if is_block {
                AuditSeverity::Block
            } else {
                AuditSeverity::Warn
            },
            kind: dim.as_str().to_string(),
            cwe_id: 0,
            line: 0,
            message: format!(
                "{}: {} (score {:.2})",
                dim.short_name(),
                score.evidence,
                score.value
            ),
        });
    }
    out
}

/// Pure entry point: audit `path` across the requested `layers` and return a
/// ranked, consolidated [`AuditReport`]. Errors only on file-read failure.
pub fn run_audit(path: &Path, layers: &AuditLayers) -> std::io::Result<AuditReport> {
    let content = std::fs::read_to_string(path)?;
    let mut findings = Vec::new();
    if layers.vuln {
        findings.extend(run_vuln_layer(&content));
    }
    if layers.quality {
        findings.extend(run_quality_layer(path));
    }
    // Rank: most-severe first, then by line for stable, readable output.
    findings.sort_by(|a, b| {
        a.severity
            .rank()
            .cmp(&b.severity.rank())
            .then(a.line.cmp(&b.line))
    });

    let block_count = findings
        .iter()
        .filter(|f| f.severity == AuditSeverity::Block)
        .count();
    let warn_count = findings
        .iter()
        .filter(|f| f.severity == AuditSeverity::Warn)
        .count();
    let info_count = findings
        .iter()
        .filter(|f| f.severity == AuditSeverity::Info)
        .count();
    let verdict = if block_count > 0 {
        AuditSeverity::Block
    } else if warn_count > 0 {
        AuditSeverity::Warn
    } else {
        AuditSeverity::Info
    };

    Ok(AuditReport {
        file: path.display().to_string(),
        verdict,
        block_count,
        warn_count,
        info_count,
        findings,
    })
}

#[tool_router(router = router_workflow, vis = "pub(crate)")]
impl TouringServer {
    #[tool(
        name = "touring_audit",
        description = "Audit a source file for problems in ONE call — orchestrates the \
                       offensive CWE/OWASP vulnerability engine (10 detectors: SQL/command/LDAP/XML \
                       injection, XSS, path traversal, SSRF, deserialization, integer/buffer overflow) \
                       AND the 6 P0 BLOCK quality gates (secrets, dependency CVEs, config security, \
                       deprecated APIs, package management). Returns one ranked report — each finding \
                       has layer, severity, CWE id, line and message — plus a Block|Warn|Info verdict. \
                       Use before editing or merging to catch failures, security gaps and quality \
                       blockers without chaining separate scans. Read-only.",
        annotations(title = "Audit file for failures & gaps", read_only_hint = true)
    )]
    async fn touring_audit(
        &self,
        params: Parameters<crate::server::params::AuditParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let path_str = p.path.trim();
        if path_str.is_empty() {
            return Err(McpError::invalid_params(
                "path must not be empty".to_string(),
                None,
            ));
        }
        let path = Path::new(path_str);
        let layers = AuditLayers::from_param(&p.layers);
        let report = run_audit(path, &layers).map_err(|e| {
            McpError::internal_error(format!("audit could not read {path_str}: {e}"), None)
        })?;
        let mut json = serde_json::to_value(&report)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        crate::server::params::apply_detail_level(&mut json, p.detail);
        let text = serde_json::to_string_pretty(&json)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_file(name: &str, content: &str) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("touring_audit_test_{}_{name}", std::process::id()));
        let mut f = std::fs::File::create(&dir).expect("create temp");
        f.write_all(content.as_bytes()).expect("write temp");
        dir
    }

    #[test]
    fn layers_from_param_defaults_to_all() {
        let l = AuditLayers::from_param(&None);
        assert!(l.vuln && l.quality);
        let l = AuditLayers::from_param(&Some(vec![]));
        assert!(l.vuln && l.quality);
        let l = AuditLayers::from_param(&Some(vec!["all".into()]));
        assert!(l.vuln && l.quality);
    }

    #[test]
    fn layers_from_param_is_opt_in() {
        let l = AuditLayers::from_param(&Some(vec!["vuln".into()]));
        assert!(l.vuln && !l.quality);
        let l = AuditLayers::from_param(&Some(vec!["QUALITY".into()]));
        assert!(!l.vuln && l.quality);
    }

    #[test]
    fn byte_to_line_counts_newlines() {
        assert_eq!(byte_to_line("abc", 0), 1);
        assert_eq!(byte_to_line("abc", 2), 1);
        assert_eq!(byte_to_line("a\nb\nc", 4), 3);
        // out-of-range offset clamps to content length
        assert_eq!(byte_to_line("a\nb", 999), 2);
    }

    #[test]
    fn vuln_layer_flags_sql_injection_as_block() {
        // `UNION SELECT` is a canonical CWE-89 payload (touring-offensive corpus).
        let findings = run_vuln_layer("let q = \"UNION SELECT password FROM users\";");
        let sqli = findings
            .iter()
            .find(|f| f.cwe_id == 89)
            .expect("SQLi finding");
        assert_eq!(sqli.layer, "vuln");
        assert_eq!(sqli.severity, AuditSeverity::Block); // severity 9.8 ≥ 7.0
        assert_eq!(sqli.kind, "SQLi");
    }

    #[test]
    fn vuln_layer_clean_content_has_no_findings() {
        let findings = run_vuln_layer("let total = a + b; // arithmetic only\n");
        assert!(
            findings.is_empty(),
            "clean code must not flag: {findings:?}"
        );
    }

    #[test]
    fn run_audit_vuln_only_blocks_on_xss() {
        // `<script>` is a canonical CWE-79 payload.
        let path = temp_file("xss.txt", "render(\"<script>alert(1)</script>\")");
        let report = run_audit(
            &path,
            &AuditLayers {
                vuln: true,
                quality: false,
            },
        )
        .expect("audit");
        assert_eq!(report.verdict, AuditSeverity::Block);
        assert!(report.block_count >= 1);
        assert!(report.findings.iter().any(|f| f.cwe_id == 79));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn run_audit_clean_file_is_info_verdict() {
        let path = temp_file("clean.txt", "fn add(a: i32, b: i32) -> i32 { a + b }\n");
        let report = run_audit(
            &path,
            &AuditLayers {
                vuln: true,
                quality: false,
            },
        )
        .expect("audit");
        assert_eq!(report.verdict, AuditSeverity::Info);
        assert_eq!(report.block_count, 0);
        assert!(report.findings.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn run_audit_missing_file_errors() {
        let res = run_audit(
            Path::new("/nonexistent/touring_audit/x.rs"),
            &AuditLayers {
                vuln: true,
                quality: true,
            },
        );
        assert!(res.is_err());
    }

    #[test]
    fn quality_layer_is_graceful_on_temp_file() {
        // Must not panic regardless of scoring availability.
        let path = temp_file("q.rs", "pub fn f() {}\n");
        let _ = run_quality_layer(&path); // returns Vec (possibly empty) without panic
        let _ = std::fs::remove_file(&path);
    }
}
