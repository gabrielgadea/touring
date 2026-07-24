//! Domain specification for N3 meta-generation.
//!
//! Defines what domain a generator should specialize in.

use serde::{Deserialize, Serialize};

/// Specification of a domain for generator specialization.
///
/// Captures the language, framework, file patterns, and quality requirements
/// that a generated N1 generator should handle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainSpec {
    /// Unique identifier for this domain.
    pub id: DomainId,
    /// Human-readable name.
    pub name: String,
    /// Primary language (e.g., "rust", "python", "typescript").
    pub language: String,
    /// File patterns for this domain (e.g., "*.rs", "*.py").
    pub file_patterns: Vec<String>,
    /// Common tool patterns used in this domain.
    pub tool_patterns: Vec<ToolPattern>,
    /// Quality gates relevant to this domain.
    pub quality_gates: Vec<super::QualityGate>,
    /// Optional framework (e.g., "actix", "django", "nextjs").
    pub framework: Option<String>,
}

impl DomainSpec {
    /// Create a new domain spec.
    pub fn new(id: DomainId, name: impl Into<String>, language: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            language: language.into(),
            file_patterns: Vec::new(),
            tool_patterns: Vec::new(),
            quality_gates: Vec::new(),
            framework: None,
        }
    }

    /// Add a file pattern.
    pub fn with_file_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.file_patterns.push(pattern.into());
        self
    }

    /// Add a tool pattern.
    pub fn with_tool_pattern(mut self, pattern: ToolPattern) -> Self {
        self.tool_patterns.push(pattern);
        self
    }

    /// Set the framework.
    pub fn with_framework(mut self, framework: impl Into<String>) -> Self {
        self.framework = Some(framework.into());
        self
    }
}

/// Unique identifier for a domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DomainId(pub &'static str);

impl DomainId {
    /// Rust language domain.
    pub const RUST: Self = Self("rust");
    /// Python language domain.
    pub const PYTHON: Self = Self("python");
    /// TypeScript language domain.
    pub const TYPESCRIPT: Self = Self("typescript");
    /// JavaScript language domain.
    pub const JAVASCRIPT: Self = Self("javascript");
    /// Go language domain.
    pub const GO: Self = Self("go");
    /// Java language domain.
    pub const JAVA: Self = Self("java");
    /// Language-agnostic generic domain.
    pub const GENERIC: Self = Self("generic");
    /// Web platform domain (TypeScript/JavaScript frameworks like nextjs).
    pub const WEB: Self = Self("web");
}

impl Serialize for DomainId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0)
    }
}

impl<'de> Deserialize<'de> for DomainId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <String as Deserialize<'de>>::deserialize(deserializer)?;
        Ok(match s.as_str() {
            "rust" => DomainId::RUST,
            "python" => DomainId::PYTHON,
            "typescript" => DomainId::TYPESCRIPT,
            "javascript" => Self::JAVASCRIPT,
            "go" => Self::GO,
            "java" => Self::JAVA,
            _ => Self::GENERIC,
        })
    }
}

/// A tool pattern used in a domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPattern {
    /// Tool name (e.g., "Read", "Edit", "Bash").
    pub tool_name: String,
    /// Whether this tool is commonly used in sequences.
    pub common: bool,
    /// Typical arguments for this tool in this domain.
    pub typical_args: Vec<String>,
    /// Known sequence patterns involving this tool.
    pub sequence_hints: Vec<String>,
}

impl ToolPattern {
    /// Create a new tool pattern.
    pub fn new(tool_name: impl Into<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            common: true,
            typical_args: Vec::new(),
            sequence_hints: Vec::new(),
        }
    }

    /// Mark as uncommon.
    pub fn uncommon(mut self) -> Self {
        self.common = false;
        self
    }

    /// Add a sequence hint.
    pub fn with_sequence_hint(mut self, hint: impl Into<String>) -> Self {
        self.sequence_hints.push(hint.into());
        self
    }

    /// Add typical arguments.
    pub fn with_typical_args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.typical_args.extend(args.into_iter().map(Into::into));
        self
    }
}

/// Predefined domain specs for common languages.
pub mod predefined {
    use super::*;

    /// Rust domain specification.
    pub fn rust() -> DomainSpec {
        let mut spec = DomainSpec::new(DomainId::RUST, "Rust", "rust")
            .with_file_pattern("*.rs")
            .with_framework("cargo");

        let mut read_pattern = ToolPattern::new("Read");
        read_pattern.sequence_hints = vec!["Read -> Edit".into(), "Read -> Bash -> Edit".into()];
        spec.tool_patterns.push(read_pattern);

        let edit_pattern = ToolPattern::new("Edit");
        spec.tool_patterns.push(edit_pattern);

        let mut bash_pattern = ToolPattern::new("Bash");
        bash_pattern.sequence_hints = vec!["Bash (cargo check)".into(), "Bash (cargo test)".into()];
        spec.tool_patterns.push(bash_pattern);

        spec
    }

    /// Python domain specification.
    pub fn python() -> DomainSpec {
        let mut spec = DomainSpec::new(DomainId::PYTHON, "Python", "python")
            .with_file_pattern("*.py")
            .with_framework("pip");

        spec.tool_patterns.push(ToolPattern::new("Read"));

        spec.tool_patterns.push(ToolPattern::new("Edit"));

        let mut bash_pattern = ToolPattern::new("Bash");
        bash_pattern.sequence_hints = vec!["Bash (ruff check)".into(), "Bash (pytest)".into()];
        spec.tool_patterns.push(bash_pattern);

        spec
    }

    /// TypeScript domain specification.
    pub fn typescript() -> DomainSpec {
        let mut spec = DomainSpec::new(DomainId::TYPESCRIPT, "TypeScript", "typescript")
            .with_file_pattern("*.ts")
            .with_file_pattern("*.tsx")
            .with_framework("npm");
        spec.tool_patterns.push(ToolPattern::new("Read"));
        spec.tool_patterns.push(ToolPattern::new("Edit"));
        spec.tool_patterns.push(ToolPattern::new("Bash"));
        spec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_spec_builder() {
        let spec = DomainSpec::new(DomainId::RUST, "Rust", "rust")
            .with_file_pattern("*.rs")
            .with_framework("cargo");

        assert_eq!(spec.name, "Rust");
        assert_eq!(spec.language, "rust");
        assert!(spec.framework.as_ref().is_some());
    }

    #[test]
    fn test_predefined_rust() {
        let spec = predefined::rust();
        assert_eq!(spec.id, DomainId::RUST);
        assert!(!spec.file_patterns.is_empty());
        assert!(!spec.tool_patterns.is_empty());
    }
}
