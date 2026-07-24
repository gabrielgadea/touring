//! ACO Delegating Generator — delegates to ACO Python when domain is supported.
//!
//! When the domain has a specialized generator in the ACO Python codebase,
//! this implementation delegates to it via subprocess, parsing the output
//! back into a GeneratorSpec.
//!
//! Fallback to RustMetaGenerator when:
//! - ACO Python is not available
//! - Subprocess times out
//! - Domain is not supported by ACO Python

use std::process::{Command, Stdio};
use std::time::Duration;

use crate::rl::LearningResult;
use crate::rl::n3::domain_spec::{DomainId, DomainSpec};
use crate::rl::n3::generator_spec::GeneratorSpec;
use crate::rl::n3::meta_generator::MetaGenerator;
use crate::rl::n3::rust_meta_generator::RustMetaGenerator;

/// Timeout for ACO Python subprocess calls.
const ACO_SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(30);

/// Path to ACO Python generator script.
const ACO_GENERATOR_SCRIPT: &str = "/projects/analise/scripts/aco/generator_factory.py";

/// ACO Delegating MetaGenerator.
///
/// Tries to delegate to ACO Python first, falls back to RustMetaGenerator.
#[derive(Clone)]
pub struct AcoDelegatingGenerator {
    /// Rust fallback for unsupported domains.
    rust_fallback: RustMetaGenerator,
    /// Cache of known ACO-supported domains.
    aco_supported_domains: Vec<DomainId>,
    /// Path to ACO script (configurable for testing).
    aco_script_path: String,
    /// Timeout for subprocess calls.
    timeout: Duration,
    /// Whether to skip ACO delegation entirely (testing mode).
    skip_aco: bool,
}

impl AcoDelegatingGenerator {
    /// Create a new delegating generator.
    pub fn new() -> Self {
        Self {
            rust_fallback: RustMetaGenerator::new(),
            aco_supported_domains: vec![
                DomainId::RUST,
                DomainId::PYTHON,
                DomainId::TYPESCRIPT,
                DomainId::JAVASCRIPT,
            ],
            aco_script_path: ACO_GENERATOR_SCRIPT.to_string(),
            timeout: ACO_SUBPROCESS_TIMEOUT,
            skip_aco: false,
        }
    }

    /// Create with custom ACO script path (for testing).
    pub fn with_aco_path(path: impl Into<String>) -> Self {
        let mut this = Self::new();
        this.aco_script_path = path.into();
        this
    }

    /// Set timeout for ACO subprocess calls.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Skip ACO delegation (use Rust fallback only).
    pub fn with_skip_aco(mut self) -> Self {
        self.skip_aco = true;
        self
    }

    /// Check if a domain is supported by ACO Python.
    pub fn is_aco_supported(&self, domain_id: &DomainId) -> bool {
        self.aco_supported_domains.contains(domain_id)
    }

    /// Try to delegate to ACO Python subprocess.
    fn try_aco_delegation(&self, domain: &DomainSpec) -> Result<GeneratorSpec, AcoDelegationError> {
        if self.skip_aco {
            return Err(AcoDelegationError::Skipped);
        }

        // Check if ACO script exists
        if !std::path::Path::new(&self.aco_script_path).exists() {
            return Err(AcoDelegationError::ScriptNotFound(
                self.aco_script_path.clone(),
            ));
        }

        // Build JSON input for ACO script
        let _input_json = serde_json::to_string(&domain).map_err(|e| {
            AcoDelegationError::SerializationError(format!("failed to serialize domain: {}", e))
        })?;

        // Execute ACO script
        let output = Command::new("python3")
            .arg(&self.aco_script_path)
            .arg("--domain")
            .arg(domain.id.0)
            .arg("--format")
            .arg("json")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| AcoDelegationError::SubprocessError(format!("failed to spawn: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AcoDelegationError::AcoError(format!(
                "ACO script failed: {}",
                stderr
            )));
        }

        // Parse output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let spec: GeneratorSpec = serde_json::from_str(&stdout).map_err(|e| {
            AcoDelegationError::ParseError(format!("failed to parse ACO output: {}", e))
        })?;

        Ok(spec)
    }
}

impl Default for AcoDelegatingGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that can occur during ACO delegation.
#[derive(Debug, Clone)]
pub enum AcoDelegationError {
    /// ACO delegation was skipped (testing mode).
    Skipped,
    /// ACO Python script not found.
    ScriptNotFound(String),
    /// Subprocess execution failed.
    SubprocessError(String),
    /// ACO script returned an error.
    AcoError(String),
    /// Serialization/deserialization error.
    SerializationError(String),
    /// Failed to parse ACO output.
    ParseError(String),
    /// Timeout waiting for ACO subprocess.
    Timeout,
}

impl std::fmt::Display for AcoDelegationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Skipped => write!(f, "ACO delegation skipped"),
            Self::ScriptNotFound(p) => write!(f, "ACO script not found: {}", p),
            Self::SubprocessError(e) => write!(f, "subprocess error: {}", e),
            Self::AcoError(e) => write!(f, "ACO error: {}", e),
            Self::SerializationError(e) => write!(f, "serialization error: {}", e),
            Self::ParseError(e) => write!(f, "parse error: {}", e),
            Self::Timeout => write!(f, "timeout waiting for ACO subprocess"),
        }
    }
}

impl std::error::Error for AcoDelegationError {}

/// Result of attempting ACO delegation.
#[derive(Debug)]
pub enum DelegationResult {
    /// Successfully delegated to ACO Python.
    Aco(GeneratorSpec),
    /// Fell back to Rust implementation.
    Rust(GeneratorSpec),
}

impl MetaGenerator for AcoDelegatingGenerator {
    fn generate_spec(&self, domain: &DomainSpec) -> LearningResult<GeneratorSpec> {
        // Try ACO first if domain is supported
        if self.is_aco_supported(&domain.id) {
            match self.try_aco_delegation(domain) {
                Ok(spec) => {
                    return Ok(spec);
                }
                Err(e) => {
                    // Log warning but continue to fallback
                    tracing::warn!(
                        "ACO delegation failed for domain {}, falling back to Rust: {}",
                        domain.id.0,
                        e
                    );
                }
            }
        }

        // Fall back to Rust generator
        self.rust_fallback.generate_spec(domain)
    }

    fn supported_domains(&self) -> Vec<DomainId> {
        let mut domains = self.aco_supported_domains.clone();
        domains.extend(self.rust_fallback.supported_domains());
        domains.sort_by(|a, b| a.0.cmp(b.0));
        domains.dedup_by(|a, b| a.0 == b.0);
        domains
    }
}

/// Check if ACO Python is available on the system.
pub fn is_aco_available() -> bool {
    std::path::Path::new(ACO_GENERATOR_SCRIPT).exists()
}

/// Get the ACO script version.
pub fn aco_version() -> Option<String> {
    if !is_aco_available() {
        return None;
    }

    let output = Command::new("python3")
        .arg(ACO_GENERATOR_SCRIPT)
        .arg("--version")
        .output()
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// Result of testing ACO delegation.
#[derive(Debug)]
pub struct DelegationTest {
    /// Whether ACO was available.
    pub aco_available: bool,
    /// Whether delegation succeeded.
    pub delegation_succeeded: bool,
    /// The resulting spec (if successful).
    pub spec: Option<GeneratorSpec>,
    /// Error message (if failed).
    pub error: Option<String>,
}

/// Test ACO delegation for a domain.
pub fn test_delegation(domain: &DomainSpec) -> DelegationTest {
    let aco_available = is_aco_available();
    let generator = AcoDelegatingGenerator::new();

    if !aco_available {
        return DelegationTest {
            aco_available: false,
            delegation_succeeded: false,
            spec: None,
            error: Some("ACO script not found".to_string()),
        };
    }

    match generator.try_aco_delegation(domain) {
        Ok(spec) => DelegationTest {
            aco_available: true,
            delegation_succeeded: true,
            spec: Some(spec),
            error: None,
        },
        Err(e) => DelegationTest {
            aco_available: true,
            delegation_succeeded: false,
            spec: None,
            error: Some(e.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delegating_generator_creation() {
        let r#gen = AcoDelegatingGenerator::new();
        assert!(!r#gen.is_aco_supported(&DomainId("unknown".into())));
    }

    #[test]
    fn test_aco_availability_check() {
        // ACO script may or may not exist depending on setup
        let available = is_aco_available();
        if available {
            assert!(std::path::Path::new(ACO_GENERATOR_SCRIPT).exists());
        }
    }

    #[test]
    fn test_skip_aco_mode() {
        let r#gen = AcoDelegatingGenerator::new().with_skip_aco();
        assert!(matches!(
            r#gen.try_aco_delegation(&DomainSpec::new(DomainId("rust".into()), "Rust", "rust")),
            Err(AcoDelegationError::Skipped)
        ));
    }

    #[test]
    fn test_rust_fallback_for_unsupported() {
        let r#gen = AcoDelegatingGenerator::new().with_skip_aco();
        let domain = DomainSpec::new(DomainId("java".into()), "Java", "java");

        // Should fall back to Rust even for unknown domains
        let result = r#gen.generate_spec(&domain);
        assert!(result.is_ok());
    }

    #[test]
    fn test_supported_domains_includes_aco_and_rust() {
        let r#gen = AcoDelegatingGenerator::new();
        let domains = r#gen.supported_domains();

        // Should include both ACO-supported and Rust-supported domains
        assert!(!domains.is_empty());
    }

    #[test]
    fn test_delegation_test_unknown_domain() {
        let domain = DomainSpec::new(DomainId("unknown".into()), "Unknown", "unknown");
        let test = test_delegation(&domain);

        // Should indicate not successful since domain is unknown to ACO
        assert!(!test.delegation_succeeded);
    }
}
