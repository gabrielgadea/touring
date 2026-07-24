//! Vulnerability detection primitives
//!
//! Provides the [`VulnerabilityPattern`] trait and [`VulnMatch`] struct
//! for detecting security vulnerabilities in source code and text inputs.
//!
//! ## Architecture
//!
//! These types live in [`touring_foundation::security::vulnerability`] as
//! foundation types (zero dependencies on touring crates), allowing both
//! `touring-offensive` (which implements concrete patterns) and
//! `touring-analysis` (which composes security analysis) to use them
//! without creating a cyclic dependency.

pub use touring_foundation::security::vulnerability::{VulnMatch, VulnerabilityPattern};

pub mod cwe_patterns;

pub use cwe_patterns::{
    BufferOverflowPattern, CmdInjectionPattern, DeserializationPattern, IntegerOverflowPattern,
    LdapInjectionPattern, PathTraversalPattern, PatternRegistry, SqlInjectionPattern, SsrfPattern,
    XmlInjectionPattern, XssPattern,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vuln_match_creation() {
        let m = VulnMatch::new("SQLi".into(), (5, 20), 9.5, 89);
        assert_eq!(m.pattern_name, "SQLi");
        assert_eq!(m.span, (5, 20));
        assert!((m.severity - 9.5).abs() < f32::EPSILON);
        assert_eq!(m.cwe_id, 89);
    }
}
