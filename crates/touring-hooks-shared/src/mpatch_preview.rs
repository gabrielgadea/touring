//! `mpatch_preview` — fuzzy patch preview via mpatch library
//!
//! Provides dry-run fuzzy patching that can detect what would change
//! before applying. Used by pre_write hook and plan_commit pipeline.

/// Dry-run result of a fuzzy patch attempt: whether it matched and what it would produce.
#[derive(Debug, Clone)]
pub struct PatchPreview {
    /// Whether the patch could be located and applied in dry-run.
    pub matched: bool,
    /// The matching strategy that succeeded.
    pub method: PatchMethod,
    /// Match confidence in `[0.0, 1.0]` reported by the apply engine.
    pub confidence: f32,
    /// The proposed post-patch content (dry-run only; not written to disk).
    pub preview: String,
}

/// Strategy used to locate the patch target, from strictest to most lenient.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchMethod {
    /// Byte-for-byte exact match of the context.
    Exact,
    /// Match ignoring leading/trailing whitespace differences.
    Whitespace,
    /// Approximate match via fuzzy similarity scoring.
    Fuzzy,
}

/// Feature-gated impl — only available when `mpatch-fuzzy` is enabled.
#[cfg(feature = "mpatch-fuzzy")]
impl PatchPreview {
    /// Compute a dry-run patch preview without applying.
    ///
    /// Parses the diff content, then applies it in dry-run mode to get
    /// the proposed result and confidence metrics from the apply report.
    pub fn dry_run(source: &str, diff: &str) -> Option<Self> {
        use mpatch::{ApplyOptions, parse_auto, try_apply_patch_to_content};

        // Parse the diff content to extract patches.
        let patches = parse_auto(diff).ok()?;
        if patches.len() != 1 {
            return None;
        }
        let patch = patches.into_iter().next()?;

        // Apply in dry-run mode with fuzz factor to get confidence.
        let options = ApplyOptions {
            dry_run: true,
            fuzz_factor: 0.7,
        };
        let result = try_apply_patch_to_content(&patch, Some(source), &options).ok()?;

        let report = &result.report;
        let hunk_results = &report.hunk_results;

        // Calculate success metrics from hunk results.
        let total = hunk_results.len();
        let failures = hunk_results
            .iter()
            .filter(|r| matches!(r, mpatch::HunkApplyStatus::Failed(_)))
            .count();

        let (method, confidence) = if failures == 0 && total > 0 {
            // Check if any fuzzy matching was used by examining match_type.
            let has_fuzzy = hunk_results.iter().any(|r| {
                if let mpatch::HunkApplyStatus::Applied { match_type, .. } = r {
                    matches!(match_type, mpatch::MatchType::Fuzzy { .. })
                } else {
                    false
                }
            });
            if has_fuzzy {
                (PatchMethod::Fuzzy, 0.85) // Fuzzy used but all succeeded
            } else {
                (PatchMethod::Exact, 1.0)
            }
        } else {
            let success_rate = if total > 0 {
                (total - failures) as f32 / total as f32
            } else {
                0.0
            };
            (PatchMethod::Fuzzy, success_rate)
        };

        Some(PatchPreview {
            matched: true,
            method,
            confidence,
            preview: result.new_content,
        })
    }

    /// Returns true if patch can be applied with confidence >= threshold.
    pub fn is_confident(&self, threshold: f32) -> bool {
        self.matched && self.confidence >= threshold
    }
}

/// Stub impl — always available when feature is off.
#[cfg(not(feature = "mpatch-fuzzy"))]
impl PatchPreview {
    /// Stub: dry_run is not available without mpatch.
    pub fn dry_run(_source: &str, _diff: &str) -> Option<Self> {
        None
    }

    /// Stub: always returns false since mpatch is not available.
    pub fn is_confident(&self, _threshold: f32) -> bool {
        false
    }
}

/// Feature-gated: only available when `mpatch-fuzzy` feature is enabled.
#[cfg(feature = "mpatch-fuzzy")]
pub fn preview_patch(source: &str, diff: &str) -> Option<PatchPreview> {
    PatchPreview::dry_run(source, diff)
}

/// Stub when feature is off — always returns None.
#[cfg(not(feature = "mpatch-fuzzy"))]
pub fn preview_patch(_source: &str, _diff: &str) -> Option<PatchPreview> {
    None
}
