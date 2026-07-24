//! Fascicles — functional decomposition units for touring-cortex
//!
//! This module contains the core functional units:
//! - `evidence`: Evidence tracking and accumulation
//! - `channels`: Inter-fascicle communication channels
//! - `dispatcher`: Request dispatch and routing
//! - `registry`: Fascicle registration and lifecycle management

pub mod channels;
pub mod dispatcher;
pub mod evidence;
pub mod evidence_adapter;
pub mod planning;
pub mod registry;

// Re-exported types for external crate consumers
pub use dispatcher::{DispatchError, FascicleDispatcher, HandlerChannelHandle};
pub use evidence_adapter::{EvidenceAdaptError, EvidenceAdapter};
pub use planning::{MctsPlanningFascicle, MctsPlanningRequest};
pub use registry::HandlerRegistry;
