//! FamilyRouter for MCP tool `touring_status` (W1.2 of task_1780763041476850005).
//!
//! Consolidates 9 historical `*_status` MCP tools into 1 unified tool with an
//! enum `family` discriminator. Migrates the user from a 102-tool surface to a
//! 22-tool curated surface by absorbing the 9 status tools without losing any
//! data they previously surfaced.
//!
//! Replaces (when `mcp-curated` is enabled and these are gated behind
//! `mcp-legacy`):
//!   - touring_ctx_lsp_status         (family="integration", subsystem="lsp")
//!   - touring_ctx_otlp_status        (family="integration", subsystem="otlp")
//!   - touring_ctx_graphql_status     (family="integration", subsystem="graphql")
//!   - touring_ctx_cloud_sync_status  (family="integration", subsystem="cloud")
//!   - touring_ctx_shared_cache_status(family="integration", subsystem="cache")
//!   - touring_ctx_web_status         (family="integration", subsystem="web")
//!   - touring_evolution_status       (family="evolution")
//!   - touring_generator_registry_status(family="generator")
//!   - touring_index_status           (family="index")
//!
//! When `family` is `None`, returns the composite dashboard (same as before
//! — backward compatible).
//!
//! Token cost: ~80-150 tokens for typical `family="integration"` response vs
//! 9 × ~80 = ~720 tokens for 9 separate calls. ~5× compression.

// W1.3 (task_1780763041476850005): this entire module is gated behind
// `mcp-curated` so it only compiles when the curated tool set is enabled.
// During the 30-day migration window (W1-W3) the historical 102 tools
// remain on by default (`mcp-legacy`); opt in to the curated set via:
//   cargo build -p touring-server --features mcp-curated
// After W4, `mcp-curated` becomes the default and `mcp-legacy` is removed.
#![cfg(feature = "mcp-curated")]

// NOTE: the `rmcp` symbols (ErrorData, Parameters, model::*, tool) for the
// `#[tool]` router land in W2.5 together with the `touring_status` handler;
// importing them now would be dead until then. The skeleton below is the
// tested enum/struct contract those handlers will consume.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Top-level status family. Maps to the underlying subsystem the caller is
/// asking about. `None` (omitted from JSON) returns the composite dashboard.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema, Serialize, PartialEq, Eq)]
pub enum StatusFamily {
    /// Composite dashboard (daemon + index + wiring + sessions + composite_health_score).
    /// Default when `family` is absent.
    #[default]
    #[serde(rename = "composite")]
    Composite,
    /// LSP integration health (lsp crate connection state).
    #[serde(rename = "integration.lsp")]
    IntegrationLsp,
    /// OpenTelemetry OTLP exporter status.
    #[serde(rename = "integration.otlp")]
    IntegrationOtlp,
    /// GraphQL endpoint status.
    #[serde(rename = "integration.graphql")]
    IntegrationGraphql,
    /// Cloud sync session status.
    #[serde(rename = "integration.cloud")]
    IntegrationCloud,
    /// Multi-agent shared cache status.
    #[serde(rename = "integration.cache")]
    IntegrationCache,
    /// Web UI dashboard status.
    #[serde(rename = "integration.web")]
    IntegrationWeb,
    /// Evolution drift + learning engine state.
    #[serde(rename = "evolution")]
    Evolution,
    /// Generator plan registry status.
    #[serde(rename = "generator")]
    Generator,
    /// Tantivy FTS index health.
    #[serde(rename = "index")]
    Index,
}

/// Optional subsystem discriminator for fine-grained routing.
/// Reserved for future use; for now the `StatusFamily` enum is sufficient.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct StatusInput {
    /// Which subsystem's status to return. Omit for the composite dashboard.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<StatusFamily>,
    /// Reserved — ignored for now, kept for forward compatibility with
    /// `touring status <family> [subsystem]` CLI style.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subsystem: Option<String>,
}

impl StatusFamily {
    /// Stable string identifier (used as a JSON discriminator + CLI flag).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Composite => "composite",
            Self::IntegrationLsp => "integration.lsp",
            Self::IntegrationOtlp => "integration.otlp",
            Self::IntegrationGraphql => "integration.graphql",
            Self::IntegrationCloud => "integration.cloud",
            Self::IntegrationCache => "integration.cache",
            Self::IntegrationWeb => "integration.web",
            Self::Evolution => "evolution",
            Self::Generator => "generator",
            Self::Index => "index",
        }
    }

    /// All variants as (str, doc) — for `touring status --help` and JSON schema.
    pub const ALL: &'static [(&'static str, &'static str)] = &[
        (
            "composite",
            "composite dashboard (daemon + index + wiring + sessions)",
        ),
        ("integration.lsp", "LSP integration health"),
        ("integration.otlp", "OpenTelemetry OTLP exporter status"),
        ("integration.graphql", "GraphQL endpoint status"),
        ("integration.cloud", "Cloud sync session status"),
        ("integration.cache", "Multi-agent shared cache status"),
        ("integration.web", "Web UI dashboard status"),
        ("evolution", "Evolution drift + learning engine state"),
        ("generator", "Generator plan registry status"),
        ("index", "Tantivy FTS index health"),
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_default_is_composite() {
        assert_eq!(StatusFamily::default(), StatusFamily::Composite);
    }

    #[test]
    fn family_as_str_round_trip() {
        for (s, _) in StatusFamily::ALL {
            let parsed: StatusFamily = serde_json::from_str(&format!("\"{}\"", s))
                .unwrap_or_else(|_| panic!("parse failed for {}", s));
            assert_eq!(parsed.as_str(), *s, "round-trip mismatch for {}", s);
        }
    }

    #[test]
    fn all_families_have_unique_strs() {
        let strs: std::collections::HashSet<&str> =
            StatusFamily::ALL.iter().map(|(s, _)| *s).collect();
        assert_eq!(strs.len(), StatusFamily::ALL.len());
    }
}
