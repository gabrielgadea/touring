//! Plugin runtime with lock-free runtime swap support.
//!
//! This module provides a plugin registry and traits that enable
//! runtime-swappable backends via `ArcSwap` for lock-free hot-path access.
//!
//! # Example
//!
//! ```
//! use touring_foundation::plugin::{PluginRegistry, PluginFamily};
//! use std::sync::Arc;
//!
//! let registry = PluginRegistry::new();
//! let families: Vec<PluginFamily> = registry.families();
//! assert!(families.is_empty());
//! ```

pub mod error;
pub mod registry;
pub mod r#trait;

pub use error::PluginError;
pub use registry::{PluginRegistry, global_registry, populate_global_registry};
pub use r#trait::{Plugin, PluginFamily, ProviderPlugin};
