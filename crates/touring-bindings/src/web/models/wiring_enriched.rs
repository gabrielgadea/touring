//! Enriched wiring model — adds consumer list to WiringModule.
//!
//! S-3 deliverable: `WiringModuleWithConsumers` extends `WiringModule`
//! with the list of pub symbols that consume this module.

use super::wiring::WiringModule;
use serde::{Deserialize, Serialize};

/// A consumer of a wiring module — identifies a pub symbol that
/// imports or uses types from the parent module.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WiringConsumer {
    /// Full symbol path (e.g. "touring_hooks::pre_tool_use::HookRuntime").
    #[serde(rename = "symbol", default)]
    pub symbol: String,
    /// File path where the consumer is defined.
    #[serde(rename = "file_path", default)]
    pub file_path: String,
    /// Line number in the file where the consumer appears.
    #[serde(rename = "line", default)]
    pub line: u64,
}

/// A wiring module with its list of consumers (S-3 enrichment).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WiringModuleWithConsumers {
    /// Base wiring module data.
    #[serde(flatten)]
    pub base: WiringModule,
    /// List of symbols that consume this module.
    #[serde(rename = "consumers", default)]
    pub consumers: Vec<WiringConsumer>,
}

impl WiringModuleWithConsumers {
    /// Returns true if this module has no consumers (orphan module).
    pub fn is_orphan(&self) -> bool {
        self.consumers.is_empty()
    }

    /// Returns the number of consumers.
    pub fn consumer_count(&self) -> usize {
        self.consumers.len()
    }
}

/// Enrichment for wiring modules — each module augmented with its consumers.
/// This is the S-3 deliverable for the touring-web wiring graph viewer.
pub type WiringModulesWithConsumersReport = Vec<WiringModuleWithConsumers>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wiring_module_with_consumers_flatten() {
        let module = WiringModuleWithConsumers {
            base: WiringModule {
                file_path: "crates/touring-hooks/src/lib.rs".to_string(),
                integration_score: 1.0,
                orphan_count: 0,
            },
            consumers: vec![WiringConsumer {
                symbol: "touring_bindings::web::app::App".to_string(),
                file_path: "crates/touring-web/src/app.rs".to_string(),
                line: 42,
            }],
        };
        // Verify flattened serialization includes base fields
        let json = serde_json::to_string(&module).unwrap();
        assert!(json.contains("\"file_path\""));
        assert!(json.contains("\"integration_score\""));
        assert!(json.contains("\"consumers\""));
    }

    #[test]
    fn test_is_orphan() {
        let orphan = WiringModuleWithConsumers::default();
        assert!(orphan.is_orphan());

        let wired = WiringModuleWithConsumers {
            base: WiringModule::default(),
            consumers: vec![WiringConsumer {
                symbol: "test::func".to_string(),
                file_path: "test.rs".to_string(),
                line: 1,
            }],
        };
        assert!(!wired.is_orphan());
    }
}
