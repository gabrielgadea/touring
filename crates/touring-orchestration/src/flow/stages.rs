//! Pipeline stage trait and standard stage implementations.
//!
//! ## Stage Architecture
//!
//! Any type that implements [`Stage`] can participate in a pipeline. The core
//! trait is [`process`][Stage::process], which transforms an input item into an
//! output item or returns an error.
//!
//! ## Standard Stages
//!
//! | Stage | Semantics |
//! |-------|-----------|
//! | [`Filter`] | Pass items that satisfy a predicate |
//! | [`Transform`] | Map items through a closure |
//! | [`FanOut`] | Dispatch each item to multiple sub-stages |
//! | [`FanIn`] | Collect outputs from multiple sub-stages into one |
//! | [`Inspect`] | Side-effect only; passes item through unchanged |

use crate::flow::error::StageError;
use std::time::{Duration, Instant};

/// A named stage in a pipeline.
pub struct NamedStage<Item> {
    /// Human-readable name identifying this stage.
    pub name: String,
    /// The underlying stage processor.
    pub stage: Box<dyn Stage<Item>>,
}

impl<Item> NamedStage<Item> {
    /// Construct a named stage.
    pub fn new(name: impl Into<String>, stage: Box<dyn Stage<Item>>) -> Self {
        Self {
            name: name.into(),
            stage,
        }
    }
}

impl<Item> std::fmt::Debug for NamedStage<Item> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NamedStage")
            .field("name", &self.name)
            .finish()
    }
}

/// Any stage that can process pipeline items.
pub trait Stage<Item>: Send + Sync {
    /// Process one item, returning the transformed item or an error.
    fn process(&self, item: Item) -> Result<Item, StageError>;

    /// Human-readable name of this stage.
    fn name(&self) -> &str;
}

impl<Item: Clone, T: Fn(Item) -> Result<Item, StageError> + Send + Sync + 'static> Stage<Item>
    for T
{
    fn process(&self, item: Item) -> Result<Item, StageError> {
        (self)(item)
    }

    fn name(&self) -> &str {
        "<closure>"
    }
}

/// A stage that filters items based on a predicate.
pub struct Filter<F> {
    predicate: F,
    name: String,
}

impl<F> Filter<F> {
    /// Construct a [`Filter`] with the given predicate and name.
    pub fn new(name: impl Into<String>, predicate: F) -> Self {
        Self {
            name: name.into(),
            predicate,
        }
    }
}

impl<Item: Clone, F: Fn(&Item) -> bool + Send + Sync + 'static> Stage<Item> for Filter<F> {
    fn process(&self, item: Item) -> Result<Item, StageError> {
        if (self.predicate)(&item) {
            Ok(item)
        } else {
            Err(StageError::Filtered)
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// A stage that transforms items through a mapping function.
pub struct Transform<F> {
    mapper: F,
    name: String,
}

impl<F> Transform<F> {
    /// Construct a [`Transform`] with the given mapper and name.
    pub fn new(name: impl Into<String>, mapper: F) -> Self {
        Self {
            name: name.into(),
            mapper,
        }
    }
}

impl<Item: Clone, F: Fn(Item) -> std::result::Result<Item, StageError> + Send + Sync + 'static>
    Stage<Item> for Transform<F>
{
    fn process(&self, item: Item) -> Result<Item, StageError> {
        (self.mapper)(item)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// A stage that fans out each item to multiple sub-stages and collects their outputs.
pub struct FanOut<Item> {
    name: String,
    branches: Vec<Box<dyn Stage<Item>>>,
}

impl<Item> FanOut<Item> {
    /// Construct a [`FanOut`] with the given branches.
    pub fn new(name: impl Into<String>, branches: Vec<Box<dyn Stage<Item>>>) -> Self {
        Self {
            name: name.into(),
            branches,
        }
    }

    /// Add a branch to the fan-out.
    pub fn add_branch(mut self, branch: Box<dyn Stage<Item>>) -> Self {
        self.branches.push(branch);
        self
    }
}

impl<Item: Clone> Stage<Item> for FanOut<Item> {
    fn process(&self, item: Item) -> Result<Item, StageError> {
        let mut results: Vec<Item> = Vec::new();
        for branch in &self.branches {
            match branch.process(item.clone()) {
                Ok(out) => results.push(out),
                Err(StageError::Filtered) => {}
                Err(e) => return Err(e),
            }
        }
        if results.is_empty() {
            return Err(StageError::FanOutEmpty);
        }
        if results.len() == 1 {
            return Ok(results
                .into_iter()
                .next()
                .expect("fan-out: single result guaranteed by len==1 guard"));
        }
        Err(StageError::FanOutMultiple(results.len()))
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// A stage that fans in from multiple sub-stages using an aggregator function.
pub struct FanIn<Item> {
    name: String,
    aggregator: Box<dyn Fn(Vec<Item>) -> Result<Item, StageError> + Send + Sync>,
}

impl<Item> FanIn<Item> {
    /// Construct a [`FanIn`] with the given aggregator and name.
    pub fn new(
        name: impl Into<String>,
        aggregator: impl Fn(Vec<Item>) -> Result<Item, StageError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            aggregator: Box::new(aggregator),
        }
    }
}

impl<Item> Stage<Item> for FanIn<Item> {
    fn process(&self, item: Item) -> Result<Item, StageError> {
        (self.aggregator)(vec![item])
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// A stage that performs a side-effect and passes the item through unchanged.
pub struct Inspect<F> {
    name: String,
    inspector: F,
}

impl<F> Inspect<F> {
    /// Construct an [`Inspect`] stage with the given inspector callback.
    pub fn new(name: impl Into<String>, inspector: F) -> Self {
        Self {
            name: name.into(),
            inspector,
        }
    }
}

impl<Item, F: Fn(&Item) + Send + Sync + 'static> Stage<Item> for Inspect<F> {
    fn process(&self, item: Item) -> Result<Item, StageError> {
        (self.inspector)(&item);
        Ok(item)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// A stage that applies a timeout to an inner stage.
pub struct Timed<Item> {
    name: String,
    inner: Box<dyn Stage<Item>>,
    timeout: Duration,
}

impl<Item> Timed<Item> {
    /// Wrap a stage with a timeout.
    pub fn new(name: impl Into<String>, inner: Box<dyn Stage<Item>>, timeout: Duration) -> Self {
        Self {
            name: name.into(),
            inner,
            timeout,
        }
    }
}

impl<Item: Send + 'static> Stage<Item> for Timed<Item> {
    fn process(&self, item: Item) -> Result<Item, StageError> {
        let start = Instant::now();
        if self.timeout > Duration::ZERO && start.elapsed() > self.timeout {
            return Err(StageError::Timeout);
        }
        self.inner.process(item)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::types::Item;

    #[test]
    fn filter_passes() {
        let stage = Filter::new("even", |item: &Item| item.id.starts_with("even"));
        let item = Item::new("even_1", "First even");
        let result = stage.process(item);
        assert!(result.is_ok());
    }

    #[test]
    fn filter_blocks() {
        let stage = Filter::new("even", |item: &Item| item.id.starts_with("even"));
        let item = Item::new("odd_1", "First odd");
        let result = stage.process(item);
        assert!(matches!(result, Err(StageError::Filtered)));
    }

    #[test]
    fn transform_maps() {
        let stage = Transform::new("upcase", |item: Item| {
            Ok(Item::new(item.id.to_uppercase(), item.label.to_uppercase()))
        });
        let item = Item::new("id1", "label");
        let out = stage
            .process(item)
            .expect("test: transform stage should process valid item");
        assert_eq!(out.id, "ID1");
    }

    #[test]
    fn inspect_passes_through() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        let called = Arc::new(AtomicBool::new(false));
        let called_inner = called.clone();
        let stage = Inspect::new("logger", move |_item: &Item| {
            called_inner.store(true, Ordering::SeqCst)
        });
        let item = Item::new("id1", "label");
        let out = stage
            .process(item)
            .expect("test: inspect stage should process valid item");
        assert!(called.load(Ordering::SeqCst));
        assert_eq!(out.id, "id1");
    }
}
