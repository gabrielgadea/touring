//! MetaGenerator trait — N3 interface for generating N1 generators.
//!
//! N3 generates specialized ToolSequenceGenerator implementations
//! from DomainSpec inputs.

use super::{DomainSpec, GeneratorSpec};
use crate::rl::LearningResult;

/// Trait for N3 meta-generators.
///
/// Generates specialized N1 ToolSequenceGenerator configurations
/// (GeneratorSpec) from domain specifications.
pub trait MetaGenerator: Send + Sync {
    /// Generate a generator spec for a given domain.
    fn generate_spec(&self, domain: &DomainSpec) -> LearningResult<GeneratorSpec>;

    /// Get the list of supported domains.
    fn supported_domains(&self) -> Vec<super::DomainId>;

    /// Check if a domain is supported.
    fn supports(&self, domain_id: super::DomainId) -> bool {
        self.supported_domains().contains(&domain_id)
    }
}
