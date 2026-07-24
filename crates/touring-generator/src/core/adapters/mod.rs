//! Adapters for cross-crate integration.
//!
//! Provides typed adapters that wrap external crate dependencies (e.g.,
//! `touring-offensive::concolic::ConcolicExecutor`) into focused closure interfaces
//! suitable for injection into `GeneratorContext`.

pub mod concolic_pre_tool_adapter;

pub use concolic_pre_tool_adapter::{
    ConcolicAnalyzeFn, ConcolicDetectFn, ConcolicPreToolAdapter, ConcolicSatisfiableFn,
};
