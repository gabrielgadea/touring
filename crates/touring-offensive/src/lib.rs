//! touring-offensive — Cap II Offensive Engine
//!
//! Provides offensive security primitives: bug bounty tracking with CVSS scoring,
//! concolic execution engine for symbolic path exploration, Erickson NLP
//! for argument mining (Claims/Evidence/Warrant extraction), and vulnerability
//! pattern detection (CWEx E12 — SQLi, XSS, CMDi, PathTraversal, etc.).
//!
//! # Architecture
//!
//! This crate is organized into four independent modules:
//!
//! - [`bug_bounty`] — Bug bounty tracker with CVE references and CVSS scoring
//! - [`concolic`] — Concolic executor with path exploration and constraint solving
//! - [`erickson`] — Erickson NLP argument mining via pattern matching
//! - [`vuln`] — Vulnerability pattern detection via CWE taxonomy
//!
//! # Example
//!
//! ```rust
//! use touring_offensive::{bug_bounty::BugBountyTracker, erickson::{extract, NLPPattern}};
//!
//! // Track a vulnerability
//! let mut tracker = BugBountyTracker::new("CVE-2024-1234", 9.8);
//! tracker.add_affected_module("touring-core::config");
//!
//! // Extract arguments from text
//! let args = extract("We should upgrade serde because it has a critical CVE");
//! assert!(args.iter().any(|a| matches!(a.pattern, NLPPattern::Claim)));
//! ```

#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free (10 `Regex::new(<static
// literal>).unwrap()` → `.expect("valid static regex")` — infallible compile-time
// patterns) — lock against future bare unwrap in non-test code.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod bug_bounty;
pub mod concolic;
pub mod erickson;
pub mod solver;
pub mod vuln;

pub use bug_bounty::{BugBountyTracker, BugStatus};
pub use concolic::{
    ConcolicExecutor, ConcolicResult, Constraint, ConstraintExpr, ConstraintSolver, PathExplorer,
    PathExplorerStrategy, SymbolExpr, SymbolKind,
};
pub use erickson::{
    EricksonElement, EricksonExtractor, NLPPattern, PopulationResult, QualifierPattern,
    RelationContext, RelationType, compute_qualifier, extract, extract_with_relations,
    get_relation_context,
};
pub use solver::{
    ClaimContext, ClaimEncodeError, ClaimKind, ConstraintTranslator, ProofReport, ProofStatus,
    SolverBackend, SolverBackendKind, constraint_to_smtlib, encode_claim, prove_claim,
    symbol_to_smtlib,
};
pub use vuln::cwe_patterns::{
    BufferOverflowPattern, CmdInjectionPattern, DeserializationPattern, IntegerOverflowPattern,
    LdapInjectionPattern, PathTraversalPattern, PatternRegistry, SqlInjectionPattern, SsrfPattern,
    XmlInjectionPattern, XssPattern,
};
pub use vuln::{VulnMatch, VulnerabilityPattern};
