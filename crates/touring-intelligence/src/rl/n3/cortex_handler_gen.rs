//! Cortex Handler Generator — generates boilerplate cortex handlers from GeneratorSpec.
//!
//! Given a `GeneratorSpec`, produces Rust source code for a cortex handler that:
//! - Integrates with the N1 generator for the given domain
//! - Follows the existing Handler trait pattern
//! - Uses pheromone-guided tool selection
//!
//! ## Example output
//!
//! For a Rust generator spec, generates a handler like:
//! ```ignore
//! pub struct RustAnalyzerHandler { /* ... */ }
//! impl Handler for RustAnalyzerHandler {
//!     fn name(&self) -> &str { "HXXX_rust_analyzer" }
//!     fn events(&self) -> &[HookEvent] { &[HookEvent::PreToolUse] }
//!     fn execute(&self, ctx: &mut CortexContext) -> HandlerResult { /* ... */ }
//! }
//! ```

use crate::rl::n3::domain_spec::DomainId;
use crate::rl::n3::generator_spec::{GeneratorId, GeneratorSpec};
use std::collections::HashMap;

/// Configuration for generating a cortex handler.
#[derive(Debug, Clone)]
pub struct HandlerGenConfig {
    /// Handler number (H##) for naming.
    pub handler_number: u16,
    /// Priority for the handler (default 140).
    pub priority: u8,
    /// Whether this handler is async.
    pub is_async: bool,
    /// Tool matcher pattern (e.g., "Read|Edit" or None for all).
    pub tool_matcher: Option<&'static str>,
    /// Dependency tier (0-3).
    pub dependency_tier: u8,
    /// Custom template variables.
    pub custom_vars: HashMap<String, String>,
}

impl Default for HandlerGenConfig {
    fn default() -> Self {
        Self {
            handler_number: 150,
            priority: 140,
            is_async: false,
            tool_matcher: None,
            dependency_tier: 0,
            custom_vars: HashMap::new(),
        }
    }
}

impl HandlerGenConfig {
    /// Set handler number.
    pub fn with_handler_number(mut self, n: u16) -> Self {
        self.handler_number = n;
        self
    }

    /// Set priority.
    pub fn with_priority(mut self, p: u8) -> Self {
        self.priority = p;
        self
    }

    /// Set tool matcher.
    pub fn with_tool_matcher(mut self, m: &'static str) -> Self {
        self.tool_matcher = Some(m);
        self
    }

    /// Set dependency tier.
    pub fn with_dependency_tier(mut self, t: u8) -> Self {
        self.dependency_tier = t;
        self
    }

    /// Add custom variable.
    pub fn with_custom_var(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.custom_vars.insert(k.into(), v.into());
        self
    }
}

/// Generated handler source code with metadata.
#[derive(Debug, Clone)]
pub struct GeneratedHandler {
    /// The Rust source code.
    pub source: String,
    /// Handler name (e.g., "H150_rust_analyzer").
    pub handler_name: String,
    /// Events this handler responds to.
    pub events: Vec<&'static str>,
    /// Whether async.
    pub is_async: bool,
    /// Priority.
    pub priority: u8,
}

/// Generate cortex handler source from a GeneratorSpec.
pub fn generate_handler(spec: &GeneratorSpec, config: HandlerGenConfig) -> GeneratedHandler {
    let handler_name = format!(
        "H{:03}_{}",
        config.handler_number,
        spec.id.0.replace('-', "_")
    );

    let tool_matcher_str = config
        .tool_matcher
        .map(|m| format!(r#"Some("{}")"#, m))
        .unwrap_or_else(|| "None".to_string());

    let events = events_for_domain(&spec.domain_id);
    let events_str = format!(
        "&[{}]",
        events
            .iter()
            .map(|e| format!("HookEvent::{}", e))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let execute_body = generate_execute_body(spec, &events);

    let source = format!(
        r#"//! Generated handler: {handler_name}
//!
//! Auto-generated from GeneratorSpec for domain `{domain_id}`.
//! Do not edit by hand — regenerate with `cortex_handler_gen`.

use std::sync::OnceLock;

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::types_{{}}
    {{HandlerResult, HookEvent}};

{extra_imports}

/// Process-global generator state for `{handler_name}`.
static GLOBAL_STATE: OnceLock<{struct_name}> = OnceLock::new();

fn global_state() -> &'static {struct_name} {{
    GLOBAL_STATE.get_or_init({struct_name}::new)
}}

/// {description}
#[derive(Clone)]
pub struct {struct_name} {{
    config: {config_name},
}}

impl {struct_name} {{
    pub fn new() -> Self {{
        Self {{
            config: {config_name} {{
                max_tool_calls: {max_tool_calls},
                evaporation_rate: {evaporation_rate},
                min_confidence: {min_confidence},
                allow_parallel: {allow_parallel},
            }},
        }}
    }}
}}

impl Handler for {struct_name} {{
    fn name(&self) -> &str {{
        "{handler_name}"
    }}

    fn events(&self) -> &[HookEvent] {{
        {events_str}
    }}

    fn tool_matcher(&self) -> Option<&str> {{
        {tool_matcher_str}
    }}

    fn is_async(&self) -> bool {{
        {is_async}
    }}

    fn priority(&self) -> u8 {{
        {priority}
    }}

    fn dependency_tier(&self) -> u8 {{
        {dependency_tier}
    }}

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {{
{execute_body}    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn test_handler_name() {{
        let handler = {struct_name}::new();
        assert_eq!(handler.name(), "{handler_name}");
    }}

    #[test]
    fn test_handler_events() {{
        let handler = {struct_name}::new();
        assert!(!handler.events().is_empty());
    }}
}}
"#,
        handler_name = handler_name,
        domain_id = spec.domain_id.0,
        struct_name = struct_name_for(&spec.id),
        config_name = config_name_for(&spec.id),
        description = spec.description,
        max_tool_calls = spec.config.max_tool_calls,
        evaporation_rate = spec.config.evaporation_rate,
        min_confidence = spec.config.min_confidence,
        allow_parallel = spec.config.allow_parallel,
        events_str = events_str,
        tool_matcher_str = tool_matcher_str,
        is_async = config.is_async,
        priority = config.priority,
        dependency_tier = config.dependency_tier,
        extra_imports = extra_imports_for(&spec.domain_id),
        execute_body = execute_body.trim(),
    );

    GeneratedHandler {
        source,
        handler_name,
        events,
        is_async: config.is_async,
        priority: config.priority,
    }
}

fn struct_name_for(id: &GeneratorId) -> String {
    let base = id.0.replace('-', "_").replace("gen", "");
    format!("{}Handler", to_camel_case(&base))
}

fn config_name_for(id: &GeneratorId) -> String {
    let base = id.0.replace('-', "_").replace("gen", "");
    format!("{}Config", to_camel_case(&base))
}

fn to_camel_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;
    for c in s.chars() {
        if c == '_' || c == '-' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

fn events_for_domain(domain_id: &DomainId) -> Vec<&'static str> {
    match domain_id.0 {
        "rust" | "python" => vec!["PreToolUse", "PostToolUse"],
        "typescript" | "javascript" => vec!["PreToolUse", "PostToolUse"],
        "web" => vec!["PreToolUse", "PostToolUse"],
        _ => vec!["PreToolUse"],
    }
}

fn extra_imports_for(domain_id: &DomainId) -> String {
    match domain_id.0 {
        "rust" => r#"use crate::n1::BasicGenerator;
use crate::n1::ObjectiveSpec;
use crate::n1::GeneratedSequence;"#
            .to_string(),
        "python" | "typescript" | "javascript" => r#"use crate::n1::BasicGenerator;
use crate::n1::ObjectiveSpec;
use crate::n1::GeneratedSequence;"#
            .to_string(),
        _ => r#"use crate::n1::BasicGenerator;
use crate::n1::ObjectiveSpec;"#
            .to_string(),
    }
}

fn generate_execute_body(spec: &GeneratorSpec, events: &[&'static str]) -> String {
    let has_pre = events.contains(&"PreToolUse");
    let has_post = events.contains(&"PostToolUse");

    let mut body = String::new();

    if has_pre {
        body.push_str(
            "        // PreToolUse: generate tool sequence based on pheromone guidance\n",
        );
        body.push_str(&generate_pre_body(spec));
    }

    if has_post {
        if has_pre {
            body.push('\n');
        }
        body.push_str("        // PostToolUse: record pheromone update from outcome\n");
        body.push_str(&generate_post_body(spec));
    }

    if body.is_empty() {
        body.push_str("        // Handler executed but produced no output\n");
        body.push_str("        Ok(Default::default())");
    }

    body
}

fn generate_pre_body(spec: &GeneratorSpec) -> String {
    let mut body = String::new();
    body.push_str(
        r#"        let tool_name = ctx.input.tool_name.as_deref().unwrap_or("");
"#,
    );
    body.push_str(
        r#"        let description = ctx.input.description.as_deref().unwrap_or("");
"#,
    );
    body.push_str(
        r#"        let file_path = ctx.input.file_path.as_deref().unwrap_or("");
"#,
    );
    body.push('\n');
    body.push_str(
        r#"        // Build objective from current context
        let objective = ObjectiveSpec {
            description: description.to_string(),
            file_path: file_path.to_string(),
            is_complex: ctx.input.cila_level.map(|l| l >= 4).unwrap_or(false),
            quality_gates: vec![],
        };
"#,
    );
    body.push('\n');
    // Use spec.domain_id.0 directly in the format string
    body.push_str(&format!(
        r#"        // Generate sequence using domain-specific generator
        let generator = BasicGenerator::with_domain("{}");
        match generator.generate(&objective) {{
            Ok(seq) => {{
                // Inject generated sequence as context hint
                ctx.add_enrichment("n1_generator", &format!("n1_sequence: {{}} tool(s)", seq.tool_calls.len()));
                Ok(Default::default())
            }}
            Err(e) => {{
                // Fallback: let default behavior handle it
                ctx.add_enrichment("n1_generator", &format!("generation_failed: {{}}", e));
                Ok(Default::default())
            }}
        }}
"#,
        spec.domain_id.0
    ));
    body
}

fn generate_post_body(_spec: &GeneratorSpec) -> String {
    let mut body = String::new();
    body.push_str(
        r#"        // Record outcome for pheromone learning
        let success = ctx.input.error.is_none();
        let quality = if success { 1.0 } else { 0.0 };
"#,
    );
    body.push('\n');
    body.push_str(
        r#"        // Update pheromone trails based on outcome
        // This is handled by the global pheromone integrator
        ctx.add_enrichment(
            "pheromone_update",
            &format!("pheromone_update: quality={}", quality),
        );
"#,
    );
    body.push('\n');
    body.push_str(
        r#"        Ok(Default::default())
"#,
    );
    body
}

/// Generate all handlers for a list of specs.
pub fn generate_handlers(specs: &[(GeneratorSpec, HandlerGenConfig)]) -> Vec<GeneratedHandler> {
    specs
        .iter()
        .map(|(spec, config)| generate_handler(spec, config.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_struct_name_generation() {
        let id = GeneratorId("rust_analyzer_gen");
        assert_eq!(struct_name_for(&id), "RustAnalyzerHandler");
    }

    #[test]
    fn test_camel_case() {
        assert_eq!(to_camel_case("rust_analyzer"), "RustAnalyzer");
        assert_eq!(to_camel_case("web-generator"), "WebGenerator");
        assert_eq!(to_camel_case("basic_gen"), "BasicGen");
    }

    #[test]
    fn test_handler_name_format() {
        let id = GeneratorId("rust_analyzer_gen");
        let name = format!("H{:03}_{}", 150, id.0.replace('-', "_"));
        assert_eq!(name, "H150_rust_analyzer_gen");
    }
}
