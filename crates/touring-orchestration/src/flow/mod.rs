//! touring_flow — Declarative dataflow pipeline crate with FlowBuilder, FlowPipeline,
//! filter DSL, and types for orchestrating multi-step analysis workflows.
//!
//! ## Module structure
//!
//!   - [`error`]: typed errors via `thiserror`.
//!   - [`flow_pipeline`]: runtime execution engine for configured pipelines.
//!   - [`flow_result`]: execution results, stage outcomes, and output targets.
//!   - [`new_flow_builder`]: fluent declarative builder for `FlowPipeline`.
//!   - [`stages`]: `Stage<Item>` trait and standard stage implementations.
//!   - [`types`]: public data types (`Item`).

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod error;
pub mod flow_pipeline;
pub mod flow_result;
pub mod new_flow_builder;
pub mod stages;
pub mod types;

pub use error::{Error, Result};
pub use flow_pipeline::FlowPipeline;
pub use flow_result::{FlowResult, OutputTarget, StageOutcome};
pub use new_flow_builder::TouringFlowBuilder;
pub use stages::{FanIn, FanOut, Filter, Inspect, NamedStage, Stage, Timed, Transform};
pub use types::Item;
