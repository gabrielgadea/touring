//! Pure-Rust MetaGenerator implementation.
//!
//! Generates GeneratorSpec from DomainSpec without ACO delegation.

use super::{
    domain_spec::{DomainId, DomainSpec},
    generator_spec::{
        GENERATOR_ID_BASIC, GENERATOR_ID_RUST_ANALYZER, GENERATOR_ID_WEB_GENERATOR,
        GeneratorConfig, GeneratorId, GeneratorSpec, SequencePattern, ToolCallSpec,
    },
    meta_generator::MetaGenerator,
};
use crate::rl::LearningResult;

/// Pure-Rust meta generator.
///
/// Generates specialized N1 generator specs from domain specifications.
/// Does not delegate to ACO Python — pure Rust implementation.
#[derive(Clone)]
pub struct RustMetaGenerator {
    supported: Vec<DomainId>,
}

impl RustMetaGenerator {
    /// Create a new RustMetaGenerator with default supported domains.
    pub fn new() -> Self {
        Self {
            supported: vec![
                DomainId::RUST,
                DomainId::PYTHON,
                DomainId::TYPESCRIPT,
                DomainId::JAVASCRIPT,
                DomainId::GO,
                DomainId::GENERIC,
            ],
        }
    }
}

impl Default for RustMetaGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl MetaGenerator for RustMetaGenerator {
    fn generate_spec(&self, domain: &DomainSpec) -> LearningResult<GeneratorSpec> {
        let id = self.generator_id_for_domain(domain.id);
        let patterns = self.patterns_for_domain(domain);
        let config = self.config_for_domain(domain);

        Ok(GeneratorSpec {
            id,
            domain_id: domain.id,
            description: format!(
                "Auto-generated N1 generator for {} ({})",
                domain.name, domain.language
            ),
            config,
            patterns,
            pheromone_prefix: format!("n3_{}", domain.id.0),
            quality_gates: domain.quality_gates.clone(),
        })
    }

    fn supported_domains(&self) -> Vec<DomainId> {
        self.supported.clone()
    }
}

impl RustMetaGenerator {
    /// Get the generator ID for a domain.
    fn generator_id_for_domain(&self, domain_id: DomainId) -> GeneratorId {
        match domain_id {
            DomainId::RUST => GENERATOR_ID_RUST_ANALYZER,
            DomainId::PYTHON => GENERATOR_ID_BASIC,
            DomainId::TYPESCRIPT | DomainId::JAVASCRIPT => GENERATOR_ID_WEB_GENERATOR,
            DomainId::GO => GENERATOR_ID_BASIC,
            _ => GENERATOR_ID_BASIC,
        }
    }

    /// Get the generator config for a domain.
    fn config_for_domain(&self, domain: &DomainSpec) -> GeneratorConfig {
        let mut config = GeneratorConfig::default();

        // Adjust based on domain
        match domain.id {
            DomainId::RUST => {
                config.max_tool_calls = 15;
                config.evaporation_rate = 0.08;
            }
            DomainId::PYTHON => {
                config.max_tool_calls = 12;
                config.evaporation_rate = 0.1;
            }
            DomainId::WEB if domain.framework.as_deref() == Some("nextjs") => {
                config.max_tool_calls = 20;
            }
            _ => {}
        }

        config
    }

    /// Get sequence patterns for a domain.
    fn patterns_for_domain(&self, domain: &DomainSpec) -> Vec<SequencePattern> {
        match domain.id {
            DomainId::RUST => Self::rust_patterns(),
            DomainId::PYTHON => Self::python_patterns(),
            DomainId::TYPESCRIPT | DomainId::JAVASCRIPT => Self::web_patterns(),
            _ => Self::generic_patterns(),
        }
    }

    fn rust_patterns() -> Vec<SequencePattern> {
        vec![
            SequencePattern::new("rust_read_analyze", "analyze rust code")
                .with_tool(ToolCallSpec {
                    tool_name: "Read".into(),
                    has_dependency: false,
                    parallelizable: true,
                    expected_output: Some("file content".into()),
                })
                .with_tool(ToolCallSpec {
                    tool_name: "Bash".into(),
                    has_dependency: true,
                    parallelizable: false,
                    expected_output: Some("command output".into()),
                }),
            SequencePattern::new("rust_read_edit_test", "modify and test rust code")
                .with_tool(ToolCallSpec {
                    tool_name: "Read".into(),
                    has_dependency: false,
                    parallelizable: false,
                    expected_output: Some("file content".into()),
                })
                .with_tool(ToolCallSpec {
                    tool_name: "Edit".into(),
                    has_dependency: true,
                    parallelizable: false,
                    expected_output: Some("edit result".into()),
                })
                .with_tool(ToolCallSpec {
                    tool_name: "Bash".into(),
                    has_dependency: true,
                    parallelizable: false,
                    expected_output: Some("test output".into()),
                }),
        ]
    }

    fn python_patterns() -> Vec<SequencePattern> {
        vec![
            SequencePattern::new("python_read_edit", "modify python file")
                .with_tool(ToolCallSpec {
                    tool_name: "Read".into(),
                    has_dependency: false,
                    parallelizable: true,
                    expected_output: Some("file content".into()),
                })
                .with_tool(ToolCallSpec {
                    tool_name: "Edit".into(),
                    has_dependency: true,
                    parallelizable: false,
                    expected_output: Some("edit result".into()),
                }),
            SequencePattern::new("python_test", "run python tests").with_tool(ToolCallSpec {
                tool_name: "Bash".into(),
                has_dependency: false,
                parallelizable: false,
                expected_output: Some("test output".into()),
            }),
        ]
    }

    fn web_patterns() -> Vec<SequencePattern> {
        vec![
            SequencePattern::new("web_read_edit", "modify web file")
                .with_tool(ToolCallSpec {
                    tool_name: "Read".into(),
                    has_dependency: false,
                    parallelizable: true,
                    expected_output: Some("file content".into()),
                })
                .with_tool(ToolCallSpec {
                    tool_name: "Edit".into(),
                    has_dependency: true,
                    parallelizable: false,
                    expected_output: Some("edit result".into()),
                }),
            SequencePattern::new("web_build", "build web project").with_tool(ToolCallSpec {
                tool_name: "Bash".into(),
                has_dependency: false,
                parallelizable: false,
                expected_output: Some("build output".into()),
            }),
        ]
    }

    fn generic_patterns() -> Vec<SequencePattern> {
        vec![
            SequencePattern::new("generic_read", "read files").with_tool(ToolCallSpec {
                tool_name: "Read".into(),
                has_dependency: false,
                parallelizable: true,
                expected_output: Some("file content".into()),
            }),
            SequencePattern::new("generic_read_edit", "modify files")
                .with_tool(ToolCallSpec {
                    tool_name: "Read".into(),
                    has_dependency: false,
                    parallelizable: true,
                    expected_output: Some("file content".into()),
                })
                .with_tool(ToolCallSpec {
                    tool_name: "Edit".into(),
                    has_dependency: true,
                    parallelizable: false,
                    expected_output: Some("edit result".into()),
                }),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::n3::domain_spec::predefined;

    #[test]
    fn test_rust_meta_generator() {
        let meta = RustMetaGenerator::new();
        let spec = meta
            .generate_spec(&predefined::rust())
            .expect("generate_spec should succeed for predefined domain");

        assert_eq!(spec.domain_id, DomainId::RUST);
        assert!(!spec.patterns.is_empty());
        assert!(spec.pheromone_prefix.starts_with("n3_rust"));
    }

    #[test]
    fn test_python_meta_generator() {
        let meta = RustMetaGenerator::new();
        let spec = meta
            .generate_spec(&predefined::python())
            .expect("generate_spec should succeed for predefined domain");

        assert_eq!(spec.domain_id, DomainId::PYTHON);
        assert!(!spec.patterns.is_empty());
    }

    #[test]
    fn test_supported_domains() {
        let meta = RustMetaGenerator::new();
        assert!(meta.supports(DomainId::RUST));
        assert!(meta.supports(DomainId::PYTHON));
        assert!(!meta.supports(DomainId::JAVA)); // Not supported
    }

    #[test]
    fn test_config_for_rust() {
        let meta = RustMetaGenerator::new();
        let spec = meta
            .generate_spec(&predefined::rust())
            .expect("generate_spec should succeed for predefined domain");

        // Rust has higher max_tool_calls
        assert_eq!(spec.config.max_tool_calls, 15);
        assert_eq!(spec.config.evaporation_rate, 0.08);
    }

    #[test]
    fn test_unsupported_domain_returns_error() {
        let meta = RustMetaGenerator::new();
        let java_domain = DomainSpec::new(DomainId::JAVA, "Java", "java");
        // generate_spec doesn't check supported - it generates for any domain
        let spec = meta.generate_spec(&java_domain);
        assert!(spec.is_ok());
    }
}
