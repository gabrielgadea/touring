//! VGP Layer 5 — Path Boundary Enforcement.
//!
//! Validates that generated artifact paths comply with the `TaskKind` allowlist
//! defined in `Contracts.path_boundaries`. Runs as a validation layer inside the
//! VGP engine, just before the `Speculated` state is entered.

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

use crate::NormalizedScore;
use crate::plan::contracts::{BoundaryEnforcement, PathBoundaries, TaskKind};
use crate::plan::result::{LayerResult, RenderedFile};

/// Result of a single boundary validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundaryViolation {
    /// Artifact path that violated the boundary allowlist.
    pub file_path: String,
    /// Task kind whose path boundaries were being enforced.
    pub task_kind: TaskKind,
    /// Whether the path was explicitly forbidden or merely not allowed.
    pub violation_kind: ViolationKind,
    /// Glob pattern that triggered the violation (empty if none recorded).
    pub matched_pattern: String,
}

/// Why a path was rejected by the boundary check.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ViolationKind {
    /// Path matched a `forbidden_write` glob pattern.
    ForbiddenWrite,
    /// Path matched no `write` allowlist pattern.
    NotAllowedWrite,
}

/// Outcome of the full boundary layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BoundaryResult {
    /// All artifact paths are within bounds.
    Valid,
    /// Violations occurred under `WarnOnly` enforcement (non-blocking).
    Warnings(Vec<BoundaryViolation>),
    /// Violations occurred under `FailClosed` enforcement (blocking).
    Violations(Vec<BoundaryViolation>),
}

/// Builds and caches `GlobSet` instances per boundary config.
#[derive(Debug, Clone)]
struct GlobCache {
    write_patterns: GlobSet,
    forbidden_patterns: GlobSet,
}

impl GlobCache {
    fn new(boundaries: &PathBoundaries) -> Result<Self, globset::Error> {
        let mut write_builder = GlobSetBuilder::new();
        for pattern in &boundaries.write {
            write_builder.add(Glob::new(pattern)?);
        }
        let write_patterns = write_builder.build()?;

        let mut forbid_builder = GlobSetBuilder::new();
        for pattern in &boundaries.forbidden_write {
            forbid_builder.add(Glob::new(pattern)?);
        }
        let forbidden_patterns = forbid_builder.build()?;

        Ok(Self {
            write_patterns,
            forbidden_patterns,
        })
    }

    fn matches_write(&self, path: &str) -> bool {
        self.write_patterns.is_match(path)
    }

    fn matches_forbidden(&self, path: &str) -> bool {
        self.forbidden_patterns.is_match(path)
    }
}

/// Path boundary validator — VGP Layer 5.
#[derive(Debug, Clone)]
pub struct BoundaryValidator {
    cache: GlobCache,
    task_kind: TaskKind,
    enforcement: BoundaryEnforcement,
}

impl BoundaryValidator {
    /// # Errors
    ///
    /// Returns `globset::Error` if any glob pattern in `boundaries` is invalid.
    pub fn new(boundaries: &PathBoundaries) -> Result<Self, globset::Error> {
        let cache = GlobCache::new(boundaries)?;
        Ok(Self {
            cache,
            task_kind: boundaries.task_kind.clone(),
            enforcement: boundaries.enforcement.clone(),
        })
    }

    fn check_path(&self, path: &str) -> Option<BoundaryViolation> {
        if self.cache.matches_forbidden(path) {
            return Some(BoundaryViolation {
                file_path: path.to_string(),
                task_kind: self.task_kind.clone(),
                violation_kind: ViolationKind::ForbiddenWrite,
                matched_pattern: String::new(),
            });
        }
        if !self.cache.matches_write(path) {
            return Some(BoundaryViolation {
                file_path: path.to_string(),
                task_kind: self.task_kind.clone(),
                violation_kind: ViolationKind::NotAllowedWrite,
                matched_pattern: String::new(),
            });
        }
        None
    }

    /// Check every artifact path against the boundaries and aggregate the result
    /// according to the configured `BoundaryEnforcement` mode.
    #[must_use]
    pub fn validate_artifacts(&self, artifacts: &[RenderedFile]) -> BoundaryResult {
        let mut violations: Vec<BoundaryViolation> = Vec::new();

        for artifact in artifacts {
            if let Some(v) = self.check_path(&artifact.path) {
                violations.push(v);
            }
        }

        match self.enforcement {
            BoundaryEnforcement::FailClosed if !violations.is_empty() => {
                BoundaryResult::Violations(violations)
            }
            BoundaryEnforcement::WarnOnly => {
                if violations.is_empty() {
                    BoundaryResult::Valid
                } else {
                    BoundaryResult::Warnings(violations)
                }
            }
            BoundaryEnforcement::FailClosed => BoundaryResult::Valid,
        }
    }
}

impl From<(BoundaryResult, std::time::Instant)> for LayerResult {
    fn from((result, started): (BoundaryResult, std::time::Instant)) -> Self {
        let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

        match &result {
            BoundaryResult::Valid => LayerResult {
                name: crate::validate::pipeline::ValidationLayer::L5PathBoundary
                    .name()
                    .to_string(),
                score: NormalizedScore::ONE,
                passed: true,
                issues: vec![],
                elapsed_ms,
            },
            BoundaryResult::Warnings(w) => {
                let issues: Vec<String> = w
                    .iter()
                    .map(|v| {
                        format!(
                            "{}: {}",
                            v.file_path,
                            match v.violation_kind {
                                ViolationKind::ForbiddenWrite => "forbidden_write",
                                ViolationKind::NotAllowedWrite => "not_allowed_write",
                            }
                        )
                    })
                    .collect();
                LayerResult {
                    name: crate::validate::pipeline::ValidationLayer::L5PathBoundary
                        .name()
                        .to_string(),
                    score: NormalizedScore::ONE,
                    passed: true,
                    issues,
                    elapsed_ms,
                }
            }
            BoundaryResult::Violations(v) => {
                let issues: Vec<String> = v
                    .iter()
                    .map(|violation| {
                        format!(
                            "{}: {}",
                            violation.file_path,
                            match violation.violation_kind {
                                ViolationKind::ForbiddenWrite => "forbidden_write",
                                ViolationKind::NotAllowedWrite => "not_allowed_write",
                            }
                        )
                    })
                    .collect();
                LayerResult {
                    name: crate::validate::pipeline::ValidationLayer::L5PathBoundary
                        .name()
                        .to_string(),
                    score: NormalizedScore::ZERO,
                    passed: false,
                    issues,
                    elapsed_ms,
                }
            }
        }
    }
}

// Default presets per TaskKind
fn make_boundaries(
    kind: TaskKind,
    read: Vec<String>,
    write: Vec<String>,
    forbidden_write: Vec<String>,
    enforcement: BoundaryEnforcement,
) -> PathBoundaries {
    PathBoundaries {
        task_kind: kind,
        read,
        write,
        forbidden_write,
        enforcement,
    }
}

/// Return the built-in default `PathBoundaries` preset for a given `TaskKind`.
#[must_use]
pub fn default_boundary(kind: &TaskKind) -> PathBoundaries {
    match kind {
        TaskKind::Spec => make_boundaries(
            TaskKind::Spec,
            vec!["docs/".into(), "spec/".into(), "*.md".into()],
            vec![
                "docs/".into(),
                "docs/**".into(),
                "spec/".into(),
                "spec/**".into(),
                "*.md".into(),
            ],
            vec![
                "crates/**".into(),
                "src/**".into(),
                "tests/**".into(),
                "benches/**".into(),
            ],
            BoundaryEnforcement::FailClosed,
        ),
        TaskKind::Impl => make_boundaries(
            TaskKind::Impl,
            vec!["crates/**".into(), "src/**".into(), "tests/**".into()],
            vec!["crates/**".into(), "src/**".into(), "tests/**".into()],
            vec!["docs/**".into(), "spec/**".into()],
            BoundaryEnforcement::FailClosed,
        ),
        TaskKind::QA => make_boundaries(
            TaskKind::QA,
            vec![
                "crates/**".into(),
                "src/**".into(),
                "tests/**".into(),
                "benches/**".into(),
            ],
            vec!["tests/**".into(), "benches/**".into()],
            vec!["crates/*/src/**".into(), "docs/**".into(), "spec/**".into()],
            BoundaryEnforcement::FailClosed,
        ),
        TaskKind::Doc => make_boundaries(
            TaskKind::Doc,
            vec![
                "crates/**".into(),
                "src/**".into(),
                "docs/**".into(),
                "*.md".into(),
            ],
            vec!["docs/**".into(), "*.md".into()],
            vec![
                "crates/**".into(),
                "src/**".into(),
                "tests/**".into(),
                "benches/**".into(),
            ],
            BoundaryEnforcement::FailClosed,
        ),
        TaskKind::Audit => make_boundaries(
            TaskKind::Audit,
            vec![
                "crates/**".into(),
                "src/**".into(),
                "tests/**".into(),
                "docs/**".into(),
            ],
            vec!["docs/audit/**".into()],
            vec!["crates/**".into(), "src/**".into(), "tests/**".into()],
            BoundaryEnforcement::WarnOnly,
        ),
        TaskKind::Hotfix => make_boundaries(
            TaskKind::Hotfix,
            vec!["crates/**".into(), "src/**".into(), "tests/**".into()],
            vec!["crates/**".into(), "src/**".into()],
            vec!["docs/**".into(), "spec/**".into()],
            BoundaryEnforcement::FailClosed,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impl_boundary() -> PathBoundaries {
        default_boundary(&TaskKind::Impl)
    }

    #[test]
    fn impl_write_allowed_for_crate_file() {
        let bv = BoundaryValidator::new(&impl_boundary()).unwrap();
        let result = bv.validate_artifacts(&[RenderedFile::new(
            "crates/touring-foo/src/lib.rs",
            "pub fn foo() {}",
            crate::plan::result::FileAction::Created,
        )]);
        assert!(
            matches!(result, BoundaryResult::Valid),
            "impl should allow crates/ write"
        );
    }

    #[test]
    fn spec_forbidden_for_crate_file() {
        let mut b = default_boundary(&TaskKind::Spec);
        b.enforcement = BoundaryEnforcement::FailClosed;
        let bv = BoundaryValidator::new(&b).unwrap();
        let result = bv.validate_artifacts(&[RenderedFile::new(
            "crates/foo/src/lib.rs",
            "// should be blocked",
            crate::plan::result::FileAction::Created,
        )]);
        assert!(matches!(result, BoundaryResult::Violations(_)));
    }

    #[test]
    fn audit_warn_only_enforcement() {
        // Audit uses WarnOnly enforcement, so forbidden writes produce Warnings, not Violations.
        let bv = BoundaryValidator::new(&default_boundary(&TaskKind::Audit)).unwrap();
        let result = bv.validate_artifacts(&[RenderedFile::new(
            "crates/foo/src/lib.rs",
            "// audit reading is fine, writing not",
            crate::plan::result::FileAction::Created,
        )]);
        assert!(matches!(result, BoundaryResult::Warnings(_)));
    }

    #[test]
    fn warn_only_enforcement_no_block() {
        let mut b = impl_boundary();
        b.enforcement = BoundaryEnforcement::WarnOnly;
        let bv = BoundaryValidator::new(&b).unwrap();
        let result = bv.validate_artifacts(&[RenderedFile::new(
            "docs/changes.md",
            "# Changelog",
            crate::plan::result::FileAction::Created,
        )]);
        assert!(matches!(result, BoundaryResult::Warnings(_)));
    }

    #[test]
    fn fail_closed_blocks_violation() {
        let mut b = default_boundary(&TaskKind::Spec);
        b.enforcement = BoundaryEnforcement::FailClosed;
        let bv = BoundaryValidator::new(&b).unwrap();
        let result = bv.validate_artifacts(&[RenderedFile::new(
            "src/lib.rs",
            "pub struct Foo;",
            crate::plan::result::FileAction::Created,
        )]);
        assert!(matches!(result, BoundaryResult::Violations(_)));
    }

    #[test]
    fn layer_result_from_valid_result() {
        let started = std::time::Instant::now();
        let layer: LayerResult = (BoundaryResult::Valid, started).into();
        assert_eq!(
            layer.name,
            crate::validate::pipeline::ValidationLayer::L5PathBoundary.name()
        );
        assert_eq!(layer.score, NormalizedScore::ONE);
        assert!(layer.passed);
    }

    #[test]
    fn layer_result_from_violations() {
        let started = std::time::Instant::now();
        let violations = vec![BoundaryViolation {
            file_path: "crates/foo/src/lib.rs".into(),
            task_kind: TaskKind::Spec,
            violation_kind: ViolationKind::ForbiddenWrite,
            matched_pattern: String::new(),
        }];
        let layer: LayerResult = (BoundaryResult::Violations(violations), started).into();
        assert_eq!(
            layer.name,
            crate::validate::pipeline::ValidationLayer::L5PathBoundary.name()
        );
        assert!(!layer.passed);
    }

    #[test]
    fn default_boundary_task_kind_coverage() {
        for kind in [
            TaskKind::Spec,
            TaskKind::Impl,
            TaskKind::QA,
            TaskKind::Doc,
            TaskKind::Audit,
            TaskKind::Hotfix,
        ] {
            let b = default_boundary(&kind);
            assert_eq!(b.task_kind, kind);
            assert!(!b.write.is_empty());
            assert!(!b.forbidden_write.is_empty());
        }
    }

    #[test]
    fn prefix_match_semantics_crates() {
        let mut b = impl_boundary();
        b.enforcement = BoundaryEnforcement::FailClosed;
        let bv = BoundaryValidator::new(&b).unwrap();
        let result = bv.validate_artifacts(&[RenderedFile::new(
            "crates/foo/src/lib.rs",
            "pub mod foo;",
            crate::plan::result::FileAction::Created,
        )]);
        assert!(matches!(result, BoundaryResult::Valid));
    }

    #[test]
    fn glob_meta_chars() {
        let b = PathBoundaries {
            task_kind: TaskKind::Impl,
            read: vec![],
            write: vec!["**/*.rs".into()],
            forbidden_write: vec!["**/target/**".into()],
            enforcement: BoundaryEnforcement::FailClosed,
        };
        let bv = BoundaryValidator::new(&b).unwrap();
        let result = bv.validate_artifacts(&[RenderedFile::new(
            "crates/bar/src/main.rs",
            "fn main() {}",
            crate::plan::result::FileAction::Created,
        )]);
        assert!(matches!(result, BoundaryResult::Valid));
    }
}
