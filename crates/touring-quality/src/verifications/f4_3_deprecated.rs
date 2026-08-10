//! F4.3 — Deprecated APIs verifier.
//!
//! W0 (2026-06-21): corrected measure. The MVP stub counted `#[deprecated`
//! *declarations* and penalised them — inverted. Declaring a deprecation on
//! your own item is good hygiene (D42); the real defect is *consuming* a
//! deprecated API and silencing the compiler. We now score that instead.

use crate::DimId;
use crate::verifications::{Verification, strip_rust_comments_and_strings};
use anyhow::Result;
use std::path::Path;

/// F4.3 verifier — Deprecated APIs.
#[allow(non_camel_case_types)]
pub struct F4_3_Deprecated;

impl Verification for F4_3_Deprecated {
    fn id(&self) -> DimId {
        DimId::F4_3
    }

    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        let raw = crate::verifications::read_target_source(target)?;

        // F4.3 measures CONSUMPTION of deprecated APIs, not their declaration.
        // Declaring `#[deprecated(..)]` on your own item is *good* hygiene (D42):
        // it hands consumers a migration path, so it is NOT penalised. The real
        // defect is *using* a deprecated API and silencing the compiler — visible
        // at file scope as the lint-suppression attribute. Needles are split via
        // `concat!` so this verifier never matches its own source text.
        //
        // 2026-06-26: strip comments and string literals first so docs that
        // *mention* `#[allow(deprecated)]` in a doc comment are not flagged as
        // suppressions. Same pattern as f1_8_dep_cycles.
        let code_only = strip_rust_comments_and_strings(&raw);
        let supp_needle = concat!("allow(", "deprecated");
        let decl_needle = concat!("#[", "deprecated");
        let suppressions = code_only.matches(supp_needle).count();
        let declarations = code_only.matches(decl_needle).count();

        let value = if suppressions == 0 {
            1.0
        } else {
            (1.0 - suppressions as f32 * 0.25).max(0.0)
        };

        let evidence = format!(
            "Deprecated-API suppressions={suppressions} (penalised), own deprecation declarations={declarations} (good hygiene, not penalised); score={value:.3}"
        );
        Ok((value, evidence))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    #[test]
    fn test_deprecated_returns_valid_score() {
        let f = write_temp("fn example() {}\n");
        let s = F4_3_Deprecated.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_deprecated_empty_file() {
        let f = write_temp("");
        let s = F4_3_Deprecated.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// W0 fixture: DECLARING a deprecation is good hygiene → must score 1.0.
    /// (The old stub inverted this and returned 0.3.) Trigger built via
    /// `concat!` so this test's own source never carries the contiguous needle.
    #[test]
    fn test_declaring_deprecation_is_not_penalised() {
        let content = concat!(
            "#[",
            "deprecated(note = \"use bar\")]\n",
            "pub fn foo() {}\n"
        );
        let f = write_temp(content);
        let s = F4_3_Deprecated.check(f.path()).expect("check");
        assert_eq!(
            s.value, 1.0,
            "declaring #[deprecated] must NOT be penalised"
        );
    }

    /// W0 fixture: CONSUMING a deprecated API (suppression attr) is the real
    /// defect → must score below 1.0.
    #[test]
    fn test_suppressing_deprecation_warning_is_penalised() {
        let content = concat!("#[", "allow(", "deprecated)]\n", "fn uses_old() {}\n");
        let f = write_temp(content);
        let s = F4_3_Deprecated.check(f.path()).expect("check");
        assert!(
            s.value < 1.0,
            "suppressing the deprecation lint must be penalised, got {}",
            s.value
        );
    }
}
