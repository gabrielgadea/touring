//! Wiring Configuration Generator — generates ACO wiring configurations from GeneratorSpec.
//!
//! Produces optimized ACO wiring configurations per domain, specifying:
//! - Pheromone key prefixes
//! - Trail persistence parameters
//! - Evaporation rates per tool pattern
//! - Sequence scoring weights
//!
//! ## Example output
//!
//! For a Rust domain spec, generates a wiring config like:
//! ```yaml
//! domain: rust
//! pheromone_prefix: n3_rust
//! evaporation_rate: 0.08
//! tool_patterns:
//!   - tool: Read
//!     weight: 1.2
//!     decay: 0.05
//! ```

use std::collections::HashMap;

use crate::rl::n3::domain_spec::DomainSpec;
use crate::rl::n3::generator_spec::{GeneratorSpec, SequencePattern};

/// Wiring configuration for a domain.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WiringConfig {
    /// Domain identifier.
    pub domain: String,
    /// Pheromone key prefix for this domain.
    pub pheromone_prefix: String,
    /// Global evaporation rate.
    pub evaporation_rate: f64,
    /// Tool-specific weights.
    pub tool_weights: Vec<ToolWeight>,
    /// Sequence pattern configurations.
    pub sequence_patterns: Vec<SequenceConfig>,
    /// Quality gate thresholds.
    pub quality_gates: QualityGateConfig,
    /// Max tool calls per sequence.
    pub max_tool_calls: usize,
    /// Whether parallel execution is allowed.
    pub allow_parallel: bool,
}

/// Weight configuration for a specific tool.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolWeight {
    /// Tool name (e.g., "Read", "Edit", "Bash").
    pub tool: String,
    /// Pheromone trail weight (higher = more attractive).
    pub weight: f64,
    /// Trail decay rate per cycle.
    pub decay: f64,
    /// Whether this tool is commonly used in sequences.
    pub common: bool,
}

/// Sequence pattern configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SequenceConfig {
    /// Pattern name.
    pub name: String,
    /// Trigger condition description.
    pub trigger: String,
    /// Pheromone strength for this pattern.
    pub pheromone_strength: f64,
    /// Whether this pattern allows parallel execution.
    pub allow_parallel: bool,
}

/// Quality gate configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QualityGateConfig {
    /// Minimum confidence threshold.
    pub min_confidence: f64,
    /// Maximum tool calls before suggesting split.
    pub max_tool_calls: usize,
    /// Required validation criteria.
    pub required_validations: Vec<String>,
}

impl Default for WiringConfig {
    fn default() -> Self {
        Self {
            domain: "generic".to_string(),
            pheromone_prefix: "n3_generic".to_string(),
            evaporation_rate: 0.1,
            tool_weights: Vec::new(),
            sequence_patterns: Vec::new(),
            quality_gates: QualityGateConfig::default(),
            max_tool_calls: 10,
            allow_parallel: true,
        }
    }
}

impl Default for QualityGateConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.5,
            max_tool_calls: 15,
            required_validations: vec![
                "syntax_check".to_string(),
                "dependency_resolution".to_string(),
            ],
        }
    }
}

/// Generate a wiring configuration from a GeneratorSpec.
pub fn generate_wiring_config(spec: &GeneratorSpec, domain: &DomainSpec) -> WiringConfig {
    let tool_weights = generate_tool_weights(domain);
    let sequence_patterns = generate_sequence_configs(&spec.patterns);

    WiringConfig {
        domain: spec.domain_id.0.to_string(),
        pheromone_prefix: spec.pheromone_prefix.clone(),
        evaporation_rate: spec.config.evaporation_rate,
        tool_weights,
        sequence_patterns,
        quality_gates: QualityGateConfig {
            min_confidence: spec.config.min_confidence,
            max_tool_calls: spec.config.max_tool_calls,
            required_validations: vec![
                "syntax_check".to_string(),
                "dependency_resolution".to_string(),
            ],
        },
        max_tool_calls: spec.config.max_tool_calls,
        allow_parallel: spec.config.allow_parallel,
    }
}

/// Generate tool weights from domain tool patterns.
fn generate_tool_weights(domain: &DomainSpec) -> Vec<ToolWeight> {
    domain
        .tool_patterns
        .iter()
        .map(|tp| {
            let weight = if tp.common { 1.2 } else { 0.8 };
            let decay = 0.05;
            ToolWeight {
                tool: tp.tool_name.clone(),
                weight,
                decay,
                common: tp.common,
            }
        })
        .collect()
}

/// Generate sequence configurations from sequence patterns.
fn generate_sequence_configs(patterns: &[SequencePattern]) -> Vec<SequenceConfig> {
    patterns
        .iter()
        .map(|p| {
            let allow_parallel = p.tool_sequence.iter().all(|tc| tc.parallelizable);

            SequenceConfig {
                name: p.name.clone(),
                trigger: p.trigger.clone(),
                pheromone_strength: p.pheromone_strength,
                allow_parallel,
            }
        })
        .collect()
}

/// Serialize a wiring config to YAML string (manual, no serde_yaml dep).
pub fn to_yaml(config: &WiringConfig) -> String {
    let mut out = String::new();
    out.push_str(&format!("domain: {}\n", config.domain));
    out.push_str(&format!("pheromone_prefix: {}\n", config.pheromone_prefix));
    out.push_str(&format!("evaporation_rate: {}\n", config.evaporation_rate));
    out.push_str(&format!("max_tool_calls: {}\n", config.max_tool_calls));
    out.push_str(&format!("allow_parallel: {}\n", config.allow_parallel));
    if !config.tool_weights.is_empty() {
        out.push_str("tool_weights:\n");
        for tw in &config.tool_weights {
            out.push_str(&format!(
                "  - tool: {}\n    weight: {}\n    decay: {}\n    common: {}\n",
                tw.tool, tw.weight, tw.decay, tw.common
            ));
        }
    }
    if !config.sequence_patterns.is_empty() {
        out.push_str("sequence_patterns:\n");
        for sp in &config.sequence_patterns {
            out.push_str(&format!(
                "  - name: {}\n    trigger: {}\n    pheromone_strength: {}\n    allow_parallel: {}\n",
                sp.name, sp.trigger, sp.pheromone_strength, sp.allow_parallel
            ));
        }
    }
    out.push_str(&format!(
        "quality_gates:\n  min_confidence: {}\n  max_tool_calls: {}\n",
        config.quality_gates.min_confidence, config.quality_gates.max_tool_calls
    ));
    out
}

/// Serialize a wiring config to JSON string.
pub fn to_json(config: &WiringConfig) -> String {
    serde_json::to_string_pretty(config)
        .unwrap_or_else(|e| format!("// JSON serialization error: {}", e))
}

/// Generate TOML configuration string (for integration with ACO).
pub fn to_toml(config: &WiringConfig) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "# Wiring configuration for domain: {}\n\n",
        config.domain
    ));
    output.push_str(&format!(
        "pheromone_prefix = \"{}\"\n",
        config.pheromone_prefix
    ));
    output.push_str(&format!("evaporation_rate = {}\n", config.evaporation_rate));
    output.push_str(&format!("max_tool_calls = {}\n", config.max_tool_calls));
    output.push_str(&format!("allow_parallel = {}\n\n", config.allow_parallel));

    if !config.tool_weights.is_empty() {
        output.push_str("[tool_weights]\n");
        for tw in &config.tool_weights {
            output.push_str(&format!(
                "{} = {{ weight = {}, decay = {}, common = {} }}\n",
                tw.tool, tw.weight, tw.decay, tw.common
            ));
        }
        output.push('\n');
    }

    if !config.sequence_patterns.is_empty() {
        output.push_str("[[sequence_patterns]]\n");
        for sp in &config.sequence_patterns {
            output.push_str(&format!("name = \"{}\"\n", sp.name));
            output.push_str(&format!("trigger = \"{}\"\n", sp.trigger));
            output.push_str(&format!("pheromone_strength = {}\n", sp.pheromone_strength));
            output.push_str(&format!("allow_parallel = {}\n", sp.allow_parallel));
            output.push('\n');
        }
    }

    output.push_str("[quality_gates]\n");
    output.push_str(&format!(
        "min_confidence = {}\n",
        config.quality_gates.min_confidence
    ));
    output.push_str(&format!(
        "max_tool_calls = {}\n",
        config.quality_gates.max_tool_calls
    ));

    output
}

/// Merge multiple wiring configs (for multi-domain scenarios).
pub fn merge_configs(configs: &[WiringConfig]) -> WiringConfig {
    if configs.is_empty() {
        return WiringConfig::default();
    }

    if configs.len() == 1 {
        return configs.first().cloned().unwrap_or_default();
    }

    // Use first config as base, merge tool weights and patterns
    let mut merged = configs.first().cloned().unwrap_or_default();
    let mut all_tools: HashMap<String, ToolWeight> = HashMap::new();
    let mut all_patterns: Vec<SequenceConfig> = Vec::new();

    for config in configs {
        for tw in &config.tool_weights {
            all_tools.insert(tw.tool.clone(), tw.clone());
        }
        for sp in &config.sequence_patterns {
            all_patterns.push(sp.clone());
        }
    }

    merged.tool_weights = all_tools.into_values().collect();
    merged.sequence_patterns = all_patterns;

    merged
}

/// Validate a wiring config for consistency.
pub fn validate_config(config: &WiringConfig) -> Result<Vec<String>, Vec<String>> {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    // Check evaporation rate range
    if !(0.0..=1.0).contains(&config.evaporation_rate) {
        errors.push(format!(
            "evaporation_rate {} out of range [0.0, 1.0]",
            config.evaporation_rate
        ));
    }

    // Check tool weights
    for tw in &config.tool_weights {
        if !(0.0..=2.0).contains(&tw.weight) {
            warnings.push(format!(
                "tool '{}' weight {} outside typical range [0.0, 2.0]",
                tw.tool, tw.weight
            ));
        }
        if !(0.0..=1.0).contains(&tw.decay) {
            errors.push(format!(
                "tool '{}' decay {} out of range [0.0, 1.0]",
                tw.tool, tw.decay
            ));
        }
    }

    // Check pheromone strengths
    for sp in &config.sequence_patterns {
        if !(0.0..=1.0).contains(&sp.pheromone_strength) {
            errors.push(format!(
                "sequence '{}' pheromone_strength {} out of range [0.0, 1.0]",
                sp.name, sp.pheromone_strength
            ));
        }
    }

    // Check min_confidence
    if !(0.0..=1.0).contains(&config.quality_gates.min_confidence) {
        errors.push(format!(
            "min_confidence {} out of range [0.0, 1.0]",
            config.quality_gates.min_confidence
        ));
    }

    if errors.is_empty() {
        Ok(warnings)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_wiring_config() {
        let config = WiringConfig::default();
        assert_eq!(config.domain, "generic");
        assert_eq!(config.evaporation_rate, 0.1);
    }

    #[test]
    fn test_quality_gate_defaults() {
        let qg = QualityGateConfig::default();
        assert_eq!(qg.min_confidence, 0.5);
        assert_eq!(qg.max_tool_calls, 15);
    }

    #[test]
    fn test_validate_config_valid() {
        let config = WiringConfig::default();
        let result = validate_config(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_config_errors() {
        let mut config = WiringConfig::default();
        config.evaporation_rate = 1.5; // Invalid
        let result = validate_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_configs() {
        let config1 = WiringConfig {
            domain: "rust".to_string(),
            tool_weights: vec![ToolWeight {
                tool: "Read".to_string(),
                weight: 1.0,
                decay: 0.05,
                common: true,
            }],
            ..WiringConfig::default()
        };

        let config2 = WiringConfig {
            domain: "python".to_string(),
            tool_weights: vec![ToolWeight {
                tool: "Bash".to_string(),
                weight: 1.5,
                decay: 0.03,
                common: true,
            }],
            ..WiringConfig::default()
        };

        let merged = merge_configs(&[config1, config2]);
        assert_eq!(merged.tool_weights.len(), 2);
    }

    #[test]
    fn test_to_json() {
        let config = WiringConfig::default();
        let json = to_json(&config);
        assert!(json.contains("generic"));
    }

    #[test]
    fn test_to_toml() {
        let config = WiringConfig::default();
        let toml_str = to_toml(&config);
        assert!(toml_str.contains("pheromone_prefix"));
    }
}
