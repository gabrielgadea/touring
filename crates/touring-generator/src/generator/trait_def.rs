//! `Generator` trait and `DynGenerator` type alias.
//!
//! Uses native async fn in traits (Rust 1.75+ stable) for the `Generator` trait,
//! with a separate object-safe `ErasedGenerator` for dynamic dispatch.

use crate::error::GenerateError;
use crate::plan::result::RenderedFile;
use crate::plan::schema::GeneratorPlan;
use std::collections::HashMap;
use std::sync::Arc;

/// Core trait implemented by every artifact generator.
///
/// Uses native async fn in traits (Rust 1.75+ stable — no async-trait macro overhead).
pub trait Generator: Send + Sync {
    /// Returns the generator's unique identifier.
    fn id(&self) -> &'static str;

    /// Render the plan into zero or more files.
    ///
    /// This is a pure render step — no I/O side effects. The executor
    /// is responsible for committing the output via atomic write.
    fn render(
        &self,
        plan: &GeneratorPlan,
        vars: &HashMap<String, serde_json::Value>,
    ) -> impl std::future::Future<Output = Result<Vec<RenderedFile>, GenerateError>> + Send;
}

/// Type-erased generator for storage in the registry.
///
/// Uses `Arc` for cheap clone across the registry and executor.
pub type DynGenerator = Arc<dyn ErasedGenerator>;

/// Object-safe erased version of `Generator` (async fn in traits is not object-safe).
pub trait ErasedGenerator: Send + Sync {
    /// Returns the generator's unique identifier.
    fn id(&self) -> &'static str;

    /// Render the plan into zero or more files (object-safe, boxed future).
    ///
    /// Lifetime `'a` ties the future to both `&self` and `vars` to avoid
    /// the lifetime mismatch that occurs when only `self` is tied.
    fn render_boxed<'a>(
        &'a self,
        plan: &'a GeneratorPlan,
        vars: &'a HashMap<String, serde_json::Value>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<RenderedFile>, GenerateError>> + Send + 'a>,
    >;
}

/// Blanket impl: any `Generator` becomes an `ErasedGenerator`.
impl<G: Generator> ErasedGenerator for G {
    fn id(&self) -> &'static str {
        Generator::id(self)
    }

    fn render_boxed<'a>(
        &'a self,
        plan: &'a GeneratorPlan,
        vars: &'a HashMap<String, serde_json::Value>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<RenderedFile>, GenerateError>> + Send + 'a>,
    > {
        Box::pin(Generator::render(self, plan, vars))
    }
}
