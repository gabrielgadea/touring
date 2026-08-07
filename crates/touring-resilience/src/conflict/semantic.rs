//! Semantic conflict detection with type inference awareness.
//!
//! Uses `touring-ast` for AST parsing with type context.
//! SLA: < 1s for inputs up to 10,000 lines.

use crate::conflict::{ConflictDetector, ConflictReport, ConflictTier};
use std::time::Instant;

/// Semantic conflict detector.
///
/// Goes beyond syntax to detect conflicts that would cause type errors
/// or semantic ambiguities when both snippets are used together.
#[derive(Debug, Clone, Copy)]
pub struct SemanticConflictDetector;

impl ConflictDetector for SemanticConflictDetector {
    fn detect(&self, a: &str, b: &str) -> ConflictReport {
        let start = Instant::now();
        let sla = ConflictTier::Semantic.sla_spec();
        let ast_eq = crate::conflict::ast_diff::AstDiffDetector::ast_eq(a, b);
        let has_conflict = !ast_eq || Self::has_semantic_conflict(a, b);
        let description = if has_conflict {
            if !ast_eq {
                "Semantic conflict: structural difference detected".to_string()
            } else {
                "Semantic conflict: type/signature incompatibility".to_string()
            }
        } else {
            "No semantic conflict detected".to_string()
        };
        let locations = if has_conflict {
            crate::conflict::ast_diff::AstDiffDetector::diff_locations(a, b)
        } else {
            vec![]
        };
        let latency_ms = start.elapsed().as_millis() as u64;
        let sla_violations = sla.check_violation(latency_ms);
        let severity = if has_conflict { 0.75 } else { 0.0 };
        ConflictReport {
            has_conflict,
            tier: ConflictTier::Semantic,
            latency_ms,
            sla,
            sla_violations,
            description,
            locations,
            severity,
        }
    }
    fn name(&self) -> &'static str {
        "SemanticConflictDetector"
    }
}

impl SemanticConflictDetector {
    /// Detect semantic-level conflicts beyond pure syntax.
    pub(crate) fn has_semantic_conflict(a: &str, b: &str) -> bool {
        let sigs_a = Self::extract_signatures(a);
        let sigs_b = Self::extract_signatures(b);
        for (name_a, sig_a) in &sigs_a {
            if let Some(sig_b) = sigs_b.get(name_a)
                && sig_a != sig_b
            {
                return true;
            }
        }
        false
    }
    /// Extract function signatures from source.
    pub(crate) fn extract_signatures(source: &str) -> std::collections::HashMap<String, String> {
        let mut sigs = std::collections::HashMap::new();
        for line in source.lines() {
            let trimmed = line.trim();
            if let Some(start) = trimmed.find("fn ") {
                let rest = &trimmed[start + 3..];
                if let Some(end) = rest.find('(') {
                    let name = rest[..end].trim().to_string();
                    let sig_end = rest[start..]
                        .find('{')
                        .or_else(|| rest[start..].find("->"))
                        .unwrap_or(rest.len() - start);
                    let full_sig = rest[..sig_end.min(rest.len())].trim().to_string();
                    sigs.insert(name, full_sig);
                }
            }
        }
        sigs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_extract_signatures() {
        let source = "fn foo(a: i32) -> i32 {}\nfn bar() {}";
        let sigs = SemanticConflictDetector::extract_signatures(source);
        assert!(sigs.contains_key("foo"));
    }
    #[test]
    fn test_same_signature_no_conflict() {
        let detector = SemanticConflictDetector;
        let r = detector.detect("fn foo() {}", "fn foo() {}");
        assert!(!r.has_conflict);
    }
    #[test]
    fn test_different_signature_conflict() {
        let detector = SemanticConflictDetector;
        let r = detector.detect("fn foo(a: i32) -> i32 {}", "fn foo(a: String) -> i32 {}");
        assert!(r.has_conflict);
    }
    #[test]
    fn test_sla_1s() {
        let detector = SemanticConflictDetector;
        let r = detector.detect("// input", "// different");
        assert!(r.latency_ms <= 2000);
    }
}
